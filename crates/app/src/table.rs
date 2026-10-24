//! CSV/TSV table view: the buffer stays plain text; this module parses it into cells that know
//! their char range in the buffer, so an edited cell is written back as a single replacement.

use meridian_buffer::cell_width;

/// Display columns of `s`.
pub fn width(s: &str) -> usize {
    s.chars().fold(0, |w, c| w + cell_width(c, w, 4))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Char range of the raw text (quotes included) in the buffer.
    pub start: usize,
    pub end: usize,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub rows: Vec<Vec<Cell>>,
    pub delim: char,
}

/// Delimiter for a file extension: tab for `tsv`/`tab`, comma for `csv`.
pub fn delim_for_ext(ext: &str) -> Option<char> {
    match ext.to_ascii_lowercase().as_str() {
        "csv" => Some(','),
        "tsv" | "tab" => Some('\t'),
        _ => None,
    }
}

/// Parse `text`. Fields may be quoted with `"` (CSV only); inside quotes the delimiter, newlines
/// and doubled quotes are literal. A trailing newline does not make an extra row.
pub fn parse(text: &str, delim: char) -> Table {
    let quotes = delim != '\t';
    let chars: Vec<char> = text.chars().collect();
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut row: Vec<Cell> = Vec::new();
    let mut i = 0;
    let n = chars.len();
    while i < n {
        let start = i;
        let mut value = String::new();
        if quotes && chars[i] == '"' {
            i += 1;
            while i < n {
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        value.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                value.push(chars[i]);
                i += 1;
            }
            // Stray text after the closing quote stays part of the raw field.
            while i < n && chars[i] != delim && chars[i] != '\n' {
                value.push(chars[i]);
                i += 1;
            }
        } else {
            while i < n && chars[i] != delim && chars[i] != '\n' {
                value.push(chars[i]);
                i += 1;
            }
        }
        // `\r` before a newline belongs to the line ending, not the field.
        let mut end = i;
        if i < n && chars[i] == '\n' && end > start && chars[end - 1] == '\r' && value.ends_with('\r') {
            value.pop();
            end -= 1;
        }
        row.push(Cell { start, end, value });
        if i >= n {
            break;
        }
        if chars[i] == delim {
            i += 1;
            if i >= n {
                // A trailing delimiter ends the file with one more, empty, field.
                row.push(Cell { start: n, end: n, value: String::new() });
            }
        } else {
            // newline
            i += 1;
            rows.push(std::mem::take(&mut row));
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    Table { rows, delim }
}

/// The raw text for a value: quoted (CSV) when it holds the delimiter, a quote or a newline.
pub fn encode(value: &str, delim: char) -> String {
    if delim == '\t' {
        return value.replace(['\t', '\n', '\r'], " ");
    }
    if value.contains([delim, '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

impl Table {
    pub fn cols(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    pub fn cell(&self, r: usize, c: usize) -> Option<&Cell> {
        self.rows.get(r)?.get(c)
    }

    /// Display widths per column, capped at `cap`; `over` widens one cell (the one being edited).
    pub fn widths(&self, cap: usize, over: Option<(usize, usize, usize)>) -> Vec<usize> {
        let mut w = vec![1usize; self.cols()];
        for (r, row) in self.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let mut cw = width(&flat(&cell.value)).min(cap);
                if let Some((or, oc, ow)) = over {
                    if (or, oc) == (r, c) {
                        cw = ow;
                    }
                }
                w[c] = w[c].max(cw);
            }
        }
        w
    }
}

/// One-line form of a value for display (newlines and tabs shown as spaces).
pub fn flat(s: &str) -> String {
    s.replace(['\n', '\r', '\t'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(t: &Table) -> Vec<Vec<&str>> {
        t.rows.iter().map(|r| r.iter().map(|c| c.value.as_str()).collect()).collect()
    }

    #[test]
    fn plain_csv() {
        let t = parse("a,b,c\n1,2,3\n", ',');
        assert_eq!(vals(&t), vec![vec!["a", "b", "c"], vec!["1", "2", "3"]]);
        assert_eq!((t.rows[1][1].start, t.rows[1][1].end), (8, 9));
    }

    #[test]
    fn quoted_fields_keep_their_raw_range() {
        let text = "x,\"a,\"\"b\"\"\",y\n";
        let t = parse(text, ',');
        assert_eq!(vals(&t), vec![vec!["x", "a,\"b\"", "y"]]);
        let c = &t.rows[0][1];
        let raw: String = text.chars().skip(c.start).take(c.end - c.start).collect();
        assert_eq!(raw, "\"a,\"\"b\"\"\"");
    }

    #[test]
    fn newline_inside_quotes_stays_in_the_cell() {
        let t = parse("\"a\nb\",c\nd,e", ',');
        assert_eq!(vals(&t), vec![vec!["a\nb", "c"], vec!["d", "e"]]);
    }

    #[test]
    fn empty_fields_and_trailing_delimiter() {
        let t = parse("a,,c\n,\n1,", ',');
        assert_eq!(vals(&t), vec![vec!["a", "", "c"], vec!["", ""], vec!["1", ""]]);
        assert_eq!(t.rows[2][1].start, 9);
    }

    #[test]
    fn tsv_ignores_quotes() {
        let t = parse("a\t\"b\nc\td\n", '\t');
        assert_eq!(vals(&t), vec![vec!["a", "\"b"], vec!["c", "d"]]);
    }

    #[test]
    fn crlf_is_not_part_of_the_value() {
        let t = parse("a,b\r\nc,d\r\n", ',');
        assert_eq!(vals(&t), vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn no_phantom_row_after_the_last_newline() {
        assert_eq!(parse("a\n", ',').rows.len(), 1);
        assert_eq!(parse("a\n\n", ',').rows.len(), 2);
        assert_eq!(parse("", ',').rows.len(), 0);
    }

    #[test]
    fn encode_quotes_only_when_needed() {
        assert_eq!(encode("plain", ','), "plain");
        assert_eq!(encode("a,b", ','), "\"a,b\"");
        assert_eq!(encode("say \"hi\"", ','), "\"say \"\"hi\"\"\"");
        assert_eq!(encode("a,b", '\t'), "a,b");
        assert_eq!(encode("a\tb", '\t'), "a b");
    }

    #[test]
    fn round_trip() {
        let text = "n,\"q,1\",\"he said \"\"x\"\"\"\n";
        let t = parse(text, ',');
        for c in &t.rows[0] {
            let raw: String = text.chars().skip(c.start).take(c.end - c.start).collect();
            assert_eq!(parse(&raw, ',').rows[0][0].value, c.value);
            assert_eq!(parse(&encode(&c.value, ','), ',').rows[0][0].value, c.value);
        }
    }

    #[test]
    fn widths_cap_and_widen() {
        let t = parse("aaaa,b\nc,dd\n", ',');
        assert_eq!(t.widths(10, None), vec![4, 2]);
        assert_eq!(t.widths(2, None), vec![2, 2]);
        assert_eq!(t.widths(10, Some((1, 0, 7))), vec![7, 2]);
    }
}
