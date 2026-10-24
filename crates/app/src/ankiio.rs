//! Anki interchange: Anki's plain-text (TSV) and `.apkg` files to and from `{{cN::text}}` notes.
//! Pure except the `.apkg` functions, which run `unzip`, `zip` and `sqlite3`.

use crate::cards;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What an import found: per deck, one cloze paragraph per Anki note.
#[derive(Debug, Default, PartialEq)]
pub struct Imported {
    pub decks: Vec<(String, Vec<String>)>,
    pub skipped: usize,
}

impl Imported {
    pub fn add(&mut self, deck: &str, text: String) {
        match self.decks.iter_mut().find(|d| d.0 == deck) {
            Some(d) => d.1.push(text),
            None => self.decks.push((deck.to_string(), vec![text])),
        }
    }

    pub fn notes(&self) -> usize {
        self.decks.iter().map(|d| d.1.len()).sum()
    }
}

fn decode_entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(j) = rest.find(';').filter(|&j| j <= 8) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..j];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => ent.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()).and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[j + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// An Anki HTML field as plain text: `<br>` and closing `div`/`p`/`li` become newlines, other tags
/// are dropped, entities decoded. A `<` that does not open a tag stays.
pub fn html_to_text(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '<' {
            out.push(c);
            continue;
        }
        let mut tag = String::new();
        let mut closed = false;
        for d in it.by_ref() {
            if d == '>' {
                closed = true;
                break;
            }
            tag.push(d);
        }
        let close = tag.starts_with('/');
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
        if !closed || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            out.push('<');
            out.push_str(&tag);
            if closed {
                out.push('>');
            }
            continue;
        }
        let block = matches!(name.as_str(), "div" | "p" | "li");
        if name == "br" || (close && block) || (!close && block && !out.is_empty() && !out.ends_with('\n')) {
            out.push('\n');
        }
    }
    decode_entities(&out).trim().to_string()
}

/// Drop Anki's `::hint` from every `{{cN::text::hint}}`.
fn strip_hints(s: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?s)\{\{(c\d+)::(.*?)(?:::.*?)?\}\}").unwrap()).replace_all(s, "{{$1::$2}}").into_owned()
}

/// Lines without the blank ones, so a note stays one paragraph.
fn one_block(s: &str) -> String {
    s.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().join("\n")
}

fn field_text(s: &str, html: bool) -> String {
    if html {
        html_to_text(s)
    } else {
        s.to_string()
    }
}

/// A Cloze-type field as one paragraph; None when it holds no valid cloze.
pub(crate) fn cloze_note(field: &str, html: bool) -> Option<String> {
    let t = one_block(&strip_hints(&field_text(field, html)));
    (!cards::parse(&t).is_empty()).then_some(t)
}

/// A Basic note (front, back) as `front {{c1::back}}`; None when a part is empty or holds braces
/// that would parse differently (same rule as `cards::new_card`).
pub(crate) fn basic_note(front: &str, back: &str, html: bool) -> Option<String> {
    let q = one_block(&field_text(front, html));
    let a = one_block(&field_text(back, html)).replace('\n', " ");
    let bad = |s: &str| s.contains("{{") || s.contains("}}");
    if q.is_empty() || a.is_empty() || bad(&q) || bad(&a) || a.starts_with('{') || a.ends_with('}') {
        return None;
    }
    Some(format!("{q} {{{{c1::{a}}}}}"))
}

/// A note text as an Anki HTML field: escaped, newlines as `<br>`, no tabs.
pub(crate) fn to_field(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\t', " ").replace('\r', "").replace('\n', "<br>")
}

/// Anki "Notes in plain text". Header lines start `#`; only `#separator:tab` is accepted and
/// `#html:true` turns on HTML reduction. A first column with a cloze is a cloze note; otherwise
/// columns 1 and 2 are a basic note.
pub fn parse_tsv(text: &str, stem: &str) -> Result<Imported, String> {
    let mut html = false;
    let mut meta: Vec<usize> = Vec::new();
    let mut deck_col = None;
    let mut out = Imported::default();
    for line in text.trim_start_matches('\u{feff}').lines() {
        if let Some(h) = line.strip_prefix('#') {
            if let Some(v) = h.strip_prefix("separator:") {
                if !v.trim().eq_ignore_ascii_case("tab") {
                    return Err(format!("separator \"{}\" is not supported: export from Anki with tabs", v.trim()));
                }
            } else if let Some(v) = h.strip_prefix("html:") {
                html = v.trim().eq_ignore_ascii_case("true");
            } else if let Some((name, n)) = h.split_once(" column:") {
                if let Ok(n) = n.trim().parse::<usize>().map(|n| n.saturating_sub(1)) {
                    meta.push(n);
                    if name == "deck" {
                        deck_col = Some(n);
                    }
                }
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let all: Vec<String> = line.split('\t').map(csv_unquote).collect();
        let deck = deck_col.and_then(|n| all.get(n)).filter(|d| !d.trim().is_empty()).map_or(stem, String::as_str);
        let cols: Vec<&str> = all.iter().enumerate().filter(|(i, _)| !meta.contains(i)).map(|(_, c)| c.as_str()).collect();
        let note = match cols.first() {
            Some(c0) if c0.contains("{{c") => cloze_note(c0, html),
            Some(c0) if cols.len() >= 2 => basic_note(c0, cols[1], html),
            _ => None,
        };
        match note {
            Some(n) => out.add(deck, n),
            None => out.skipped += 1,
        }
    }
    Ok(out)
}

/// Undo the CSV quoting Anki puts around a field holding `"`: outer quotes off, `""` to `"`.
fn csv_unquote(f: &str) -> String {
    match f.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        Some(inner) => inner.replace("\"\"", "\""),
        None => f.to_string(),
    }
}

/// The paragraphs of a note that hold clozes, leading `#` heading lines removed.
pub fn blocks(text: &str) -> Vec<String> {
    let mut paras: Vec<Vec<&str>> = vec![Vec::new()];
    for l in text.lines() {
        if l.trim().is_empty() {
            if !paras.last().is_some_and(Vec::is_empty) {
                paras.push(Vec::new());
            }
        } else if let Some(p) = paras.last_mut() {
            p.push(l);
        }
    }
    paras.into_iter().map(|p| p.into_iter().skip_while(|l| l.starts_with('#')).collect::<Vec<_>>().join("\n")).filter(|t| !cards::parse(t).is_empty()).collect()
}

/// TSV text for Anki's importer: one Cloze note per block, `Text` then an empty `Back Extra`.
pub fn export_tsv(blocks: &[String]) -> String {
    let mut out = String::from("#separator:tab\n#html:true\n#notetype:Cloze\n");
    for b in blocks {
        out.push_str(&to_field(b));
        out.push_str("\t\n");
    }
    out
}

/// A deck name as a file stem: `::` becomes ` - `, path characters `_`.
pub fn deck_file_name(deck: &str) -> String {
    let s: String = deck.replace("::", " - ").chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let s = s.trim().trim_matches('.').trim().to_string();
    if s.is_empty() {
        "deck".to_string()
    } else {
        s
    }
}

/// The note file for one deck: a title, then one paragraph per Anki note.
pub fn render_deck(deck: &str, notes: &[String]) -> String {
    format!("# {deck}\n\n{}\n", notes.join("\n\n"))
}

/// (file name, content) per deck; decks that map to the same name get ` 2`, ` 3`, ...
pub fn deck_files(imp: &Imported) -> Vec<(String, String)> {
    let mut used: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (deck, notes) in &imp.decks {
        let base = deck_file_name(deck);
        let mut name = base.clone();
        let mut n = 2;
        while used.contains(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        used.push(name.clone());
        out.push((format!("{name}.md"), render_deck(deck, notes)));
    }
    out
}


/// Run an external tool; its error names the tool when it is missing.
fn run(tool: &str, args: &[&str], dir: Option<&Path>, stdin: Option<&str>) -> Result<String, String> {
    let mut c = Command::new(tool);
    c.args(args).stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(d) = dir {
        c.current_dir(d);
    }
    let mut child = c.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { format!("{tool} is not installed (needed for .apkg files)") } else { format!("{tool}: {e}") })?;
    if let (Some(s), Some(mut si)) = (stdin, child.stdin.take()) {
        use std::io::Write;
        si.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("{tool} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
fn tools_present() -> bool {
    ["sqlite3 --version", "zip -v", "unzip -v"].iter().all(|c| {
        let mut p = c.split(' ');
        Command::new(p.next().unwrap_or("")).args(p).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    })
}

/// A scratch folder removed on drop.
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Result<Tmp, String> {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!("meridian-anki-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(Tmp(p))
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unhex(h: &str) -> String {
    let b: Vec<u8> = (0..h.len() / 2).filter_map(|i| u8::from_str_radix(h.get(i * 2..i * 2 + 2)?, 16).ok()).collect();
    String::from_utf8_lossy(&b).into_owned()
}

/// Read an Anki `.apkg` (legacy collection formats only).
pub fn read_apkg(path: &Path) -> Result<Imported, String> {
    let tmp = Tmp::new()?;
    let dir = tmp.0.display().to_string();
    run("unzip", &["-o", "-q", &path.display().to_string(), "-d", &dir], None, None)?;
    let has = |n: &str| tmp.0.join(n).exists();
    let db = if has("collection.anki21") {
        "collection.anki21"
    } else if has("collection.anki21b") {
        return Err("this .apkg uses Anki's new format: export it again with \"Support older Anki versions\" ticked".into());
    } else if has("collection.anki2") {
        "collection.anki2"
    } else {
        return Err("no Anki collection found in that file".into());
    };
    let dbp = tmp.0.join(db).display().to_string();
    let q = |sql: &str| run("sqlite3", &["-readonly", &dbp, sql], None, None);
    let decks: std::collections::HashMap<String, String> = q("select key||'|'||hex(json_extract(value,'$.name')) from col, json_each(col.decks)")?
        .lines()
        .filter_map(|l| {
            let (k, h) = l.split_once('|')?;
            Some((k.to_string(), unhex(h)))
        })
        .collect();
    let cloze: std::collections::HashSet<String> = q("select key||'|'||json_extract(value,'$.type') from col, json_each(col.models)")?
        .lines()
        .filter_map(|l| {
            let (k, t) = l.split_once('|')?;
            (t.trim() == "1").then(|| k.to_string())
        })
        .collect();
    let mut out = Imported::default();
    for l in q("select hex(n.flds)||'|'||n.mid||'|'||min(c.did) from notes n join cards c on c.nid=n.id group by n.id order by n.id")?.lines() {
        let mut p = l.splitn(3, '|');
        let (Some(h), Some(mid), Some(did)) = (p.next(), p.next(), p.next()) else { continue };
        let flds = unhex(h);
        let f: Vec<&str> = flds.split('\u{1f}').collect();
        let deck = decks.get(did.trim()).map_or("Default", String::as_str);
        let note = if cloze.contains(mid) {
            cloze_note(f[0], true)
        } else if f.len() >= 2 {
            basic_note(f[0], f[1], true)
        } else {
            None
        };
        match note {
            Some(n) => out.add(deck, n),
            None => out.skipped += 1,
        }
    }
    Ok(out)
}

const MODEL_ID: i64 = 1_700_000_000_001;
const DECK_ID: i64 = 1_700_000_000_002;
const SCHEMA: &str = "CREATE TABLE col (id integer primary key, crt integer not null, mod integer not null, scm integer not null, ver integer not null, dty integer not null, usn integer not null, ls integer not null, conf text not null, models text not null, decks text not null, dconf text not null, tags text not null);
CREATE TABLE notes (id integer primary key, guid text not null, mid integer not null, mod integer not null, usn integer not null, tags text not null, flds text not null, sfld integer not null, csum integer not null, flags integer not null, data text not null);
CREATE TABLE cards (id integer primary key, nid integer not null, did integer not null, ord integer not null, mod integer not null, usn integer not null, type integer not null, queue integer not null, due integer not null, ivl integer not null, factor integer not null, reps integer not null, lapses integer not null, left integer not null, odue integer not null, odid integer not null, flags integer not null, data text not null);
CREATE TABLE revlog (id integer primary key, cid integer not null, usn integer not null, ease integer not null, ivl integer not null, lastIvl integer not null, factor integer not null, time integer not null, type integer not null);
CREATE TABLE graves (usn integer not null, type integer not null, oid integer not null);
CREATE INDEX ix_notes_usn on notes (usn);
CREATE INDEX ix_cards_nid on cards (nid);
CREATE INDEX ix_cards_sched on cards (did, queue, due);
";
const DCONF: &str = r#"{"1":{"id":1,"name":"Default","mod":0,"usn":-1,"maxTaken":60,"autoplay":true,"timer":0,"replayq":true,"new":{"bury":false,"delays":[1,10],"initialFactor":2500,"ints":[1,4,0],"order":1,"perDay":20},"rev":{"bury":false,"ease4":1.3,"ivlFct":1,"maxIvl":36500,"perDay":200,"hardFactor":1.2},"lapse":{"delays":[10],"leechAction":1,"leechFails":8,"minInt":1,"mult":0},"dyn":false}}"#;

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn sql_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn model_json() -> String {
    format!(
        r#"{{"{MODEL_ID}":{{"id":{MODEL_ID},"name":"Cloze","type":1,"mod":0,"usn":-1,"sortf":0,"did":{DECK_ID},"tmpls":[{{"name":"Cloze","ord":0,"qfmt":"{{{{cloze:Text}}}}","afmt":"{{{{cloze:Text}}}}<br>{{{{Back Extra}}}}","bqfmt":"","bafmt":"","did":null,"bfont":"","bsize":0}}],"flds":[{{"name":"Text","ord":0,"sticky":false,"rtl":false,"font":"Arial","size":20,"media":[]}},{{"name":"Back Extra","ord":1,"sticky":false,"rtl":false,"font":"Arial","size":20,"media":[]}}],"css":".card {{ font-family: arial; font-size: 20px; text-align: center; }} .cloze {{ font-weight: bold; color: blue; }}","latexPre":"","latexPost":"","latexsvg":false,"req":[[0,"any",[0]]],"tags":[],"vers":[]}}}}"#
    )
}

fn decks_json(name: &str) -> String {
    let d = |id: i64, n: &str| {
        format!(r#"{{"id":{id},"name":{},"mod":0,"usn":-1,"lrnToday":[0,0],"revToday":[0,0],"newToday":[0,0],"timeToday":[0,0],"collapsed":false,"browserCollapsed":false,"desc":"","dyn":0,"conf":1,"extendedNew":0,"extendedRev":0}}"#, json_str(n))
    };
    format!(r#"{{"1":{},"{DECK_ID}":{}}}"#, d(1, "Default"), d(DECK_ID, name))
}

/// Write `blocks` as an Anki `.apkg` (legacy collection): one Cloze note per block, one card per
/// cloze number, all in deck `deck`. Replaces `out`.
pub fn write_apkg(blocks: &[String], deck: &str, out: &Path) -> Result<(), String> {
    let mut sql = String::from(SCHEMA);
    sql.push_str(&format!("INSERT INTO col VALUES(1,0,0,0,11,0,-1,0,'{{}}',{},{},{},'{{}}');\n", sql_str(&model_json()), sql_str(&decks_json(deck)), sql_str(DCONF)));
    let (mut nid, mut cid, mut due) = (1_700_000_001_000_i64, 1_700_000_900_000_i64, 1);
    for b in blocks {
        let nums: std::collections::BTreeSet<u32> = cards::parse(b).iter().map(|z| z.num).collect();
        if nums.is_empty() {
            continue;
        }
        let text = to_field(b);
        sql.push_str(&format!("INSERT INTO notes VALUES({nid},{},{MODEL_ID},0,-1,'',{}||char(31)||'',{},0,0,'');\n", sql_str(&format!("{:x}", cards::hash(&[b]))), sql_str(&text), sql_str(&text)));
        for n in nums {
            sql.push_str(&format!("INSERT INTO cards VALUES({cid},{nid},{DECK_ID},{},0,-1,0,0,{due},0,0,0,0,0,0,0,0,'');\n", n - 1));
            cid += 1;
            due += 1;
        }
        nid += 1;
    }
    let tmp = Tmp::new()?;
    run("sqlite3", &[&tmp.0.join("collection.anki2").display().to_string()], None, Some(&sql))?;
    std::fs::write(tmp.0.join("media"), "{}").map_err(|e| e.to_string())?;
    let abs = std::path::absolute(out).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&abs);
    run("zip", &["-q", &abs.display().to_string(), "collection.anki2", "media"], Some(&tmp.0), None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_text() {
        assert_eq!(html_to_text("a<br>b<div>c</div>d &amp; e &lt;f&gt; &#65; &nbsp;g"), "a\nb\nc\nd & e <f> A  g");
        assert_eq!(html_to_text("1 < 2 and 3 > 2"), "1 < 2 and 3 > 2");
        assert_eq!(html_to_text("<b>bold</b> <i>it</i>"), "bold it");
    }

    #[test]
    fn cloze_hints_dropped_and_blank_lines_collapsed() {
        assert_eq!(cloze_note("The {{c1::heart::organ}} pumps<br><br>blood {{c2::fast}}", true).unwrap(), "The {{c1::heart}} pumps\nblood {{c2::fast}}");
        assert!(cloze_note("no cloze here", true).is_none());
    }

    #[test]
    fn basic_note_becomes_one_cloze() {
        assert_eq!(basic_note("Capital of France?", "Paris", true).unwrap(), "Capital of France? {{c1::Paris}}");
        assert_eq!(basic_note("Q", "line1<br>line2", true).unwrap(), "Q {{c1::line1 line2}}");
        assert!(basic_note("Q", "", true).is_none());
        assert!(basic_note("", "A", true).is_none());
        assert!(basic_note("Q {{x", "A", true).is_none());
        assert!(basic_note("Q", "}}A", true).is_none());
        assert!(basic_note("Q", "{A", true).is_none());
    }

    #[test]
    fn tsv_import_mixes_cloze_and_basic_and_counts_skips() {
        let t = "\u{feff}#separator:tab\r\n#html:true\r\n{{c1::Paris}} is the capital\t\r\nWhat is 2+2?\t4\r\nbad {{\tx\r\nonly one column\r\n";
        let i = parse_tsv(t, "geo").unwrap();
        assert_eq!(i.decks, vec![("geo".to_string(), vec!["{{c1::Paris}} is the capital".to_string(), "What is 2+2? {{c1::4}}".to_string()])]);
        assert_eq!(i.skipped, 2);
        assert_eq!(i.notes(), 2);
    }

    #[test]
    fn tsv_import_refuses_other_separators_and_plain_text_is_not_html() {
        assert!(parse_tsv("#separator:comma\na,b\n", "x").unwrap_err().contains("comma"));
        let i = parse_tsv("a <b> c\td\n", "x").unwrap();
        assert_eq!(i.decks[0].1[0], "a <b> c {{c1::d}}");
    }

    #[test]
    fn tsv_import_undoes_csv_quoting() {
        let i = parse_tsv("#separator:tab\n#html:true\n\"Who said \"\"hi\"\"?\"\tPlato\n", "x").unwrap();
        assert_eq!(i.decks[0].1[0], "Who said \"hi\"? {{c1::Plato}}");
    }

    #[test]
    fn tsv_import_skips_metadata_columns_and_uses_the_deck_column() {
        let t = "#separator:tab\n#html:true\n#guid column:1\n#notetype column:2\n#deck column:3\nabc$Xy\tBasic\tBio::Cells\tWhat is 2+2?\t4\nq9\tCloze\tHist\tThe {{c1::war}} began\t\n";
        let i = parse_tsv(t, "stem").unwrap();
        assert_eq!(i.decks, vec![("Bio::Cells".to_string(), vec!["What is 2+2? {{c1::4}}".to_string()]), ("Hist".to_string(), vec!["The {{c1::war}} began".to_string()])]);
    }

    #[test]
    fn tsv_round_trip() {
        let blocks = vec!["Line one {{c1::a}}\nline two {{c2::b}}".to_string(), "x < y {{c1::&}}".to_string()];
        let tsv = export_tsv(&blocks);
        assert!(tsv.starts_with("#separator:tab\n#html:true\n#notetype:Cloze\n"));
        assert!(tsv.contains("Line one {{c1::a}}<br>line two {{c2::b}}\t\n"));
        let back = parse_tsv(&tsv, "d").unwrap();
        assert_eq!(back.decks[0].1, blocks);
    }

    #[test]
    fn blocks_are_cloze_paragraphs_without_headings() {
        let t = "# Title\n\nplain paragraph\n\n## Sec\nthe {{c1::cell}} divides\nsecond line\n\n\nend {{c1::x}}\n";
        assert_eq!(blocks(t), vec!["the {{c1::cell}} divides\nsecond line".to_string(), "end {{c1::x}}".to_string()]);
    }

    #[test]
    fn deck_names_become_file_names_and_never_collide() {
        assert_eq!(deck_file_name("Bio::Cells"), "Bio - Cells");
        assert_eq!(deck_file_name("a/b:c*d"), "a_b_c_d");
        assert_eq!(deck_file_name("  "), "deck");
        let mut i = Imported::default();
        i.add("A::B", "x {{c1::y}}".into());
        i.add("A - B", "z {{c1::w}}".into());
        let f = deck_files(&i);
        assert_eq!(f[0].0, "A - B.md");
        assert_eq!(f[1].0, "A - B 2.md");
        assert_eq!(f[0].1, "# A::B\n\nx {{c1::y}}\n");
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("meridian-ankitest-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn apkg_round_trip() {
        if !tools_present() {
            eprintln!("skipped: sqlite3/zip/unzip missing");
            return;
        }
        let d = tmp("rt");
        let out = d.join("deck.apkg");
        let blocks = vec!["Heart {{c1::pumps}} and {{c2::beats}}".to_string(), "a & b < c {{c1::x}}\nsecond".to_string()];
        write_apkg(&blocks, "My::Deck", &out).unwrap();
        let back = read_apkg(&out).unwrap();
        assert_eq!(back.decks, vec![("My::Deck".to_string(), blocks)]);
        assert_eq!(back.skipped, 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn apkg_reads_basic_and_cloze_models() {
        if !tools_present() {
            return;
        }
        let d = tmp("basic");
        let sql = r#"
CREATE TABLE col (models text, decks text);
CREATE TABLE notes (id integer, mid integer, flds text);
CREATE TABLE cards (id integer, nid integer, did integer);
INSERT INTO col VALUES ('{"10":{"type":0,"name":"Basic"},"11":{"type":1,"name":"Cloze"}}', '{"1":{"name":"Default"},"5":{"name":"Bio::Cells"}}');
INSERT INTO notes VALUES (1, 10, 'Capital?<br>x' || char(31) || 'Paris &amp; co');
INSERT INTO notes VALUES (2, 11, 'The {{c1::cell::hint}} divides' || char(31) || '');
INSERT INTO notes VALUES (3, 10, 'Bad {{' || char(31) || 'x');
INSERT INTO cards VALUES (1, 1, 1);
INSERT INTO cards VALUES (2, 2, 5);
INSERT INTO cards VALUES (3, 3, 1);
"#;
        run("sqlite3", &[&d.join("collection.anki2").display().to_string()], None, Some(sql)).unwrap();
        run("zip", &["-q", "t.apkg", "collection.anki2"], Some(&d), None).unwrap();
        let i = read_apkg(&d.join("t.apkg")).unwrap();
        assert_eq!(i.decks, vec![("Default".to_string(), vec!["Capital?\nx {{c1::Paris & co}}".to_string()]), ("Bio::Cells".to_string(), vec!["The {{c1::cell}} divides".to_string()])]);
        assert_eq!(i.skipped, 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn apkg_errors_are_clear() {
        if !tools_present() {
            return;
        }
        let d = tmp("err");
        std::fs::write(d.join("junk.apkg"), "not a zip").unwrap();
        assert!(read_apkg(&d.join("junk.apkg")).is_err());
        assert!(read_apkg(&d.join("missing.apkg")).is_err());
        std::fs::write(d.join("collection.anki21b"), "x").unwrap();
        std::fs::write(d.join("collection.anki2"), "stub").unwrap();
        run("zip", &["-q", "new.apkg", "collection.anki21b", "collection.anki2"], Some(&d), None).unwrap();
        let e = read_apkg(&d.join("new.apkg")).unwrap_err();
        assert!(e.contains("Support older Anki versions"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_tool_is_named() {
        let e = run("meridian-no-such-tool", &[], None, None).unwrap_err();
        assert!(e.contains("meridian-no-such-tool") && e.contains("not installed"), "{e}");
    }
}
