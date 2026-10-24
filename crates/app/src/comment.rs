//! Toggle comments: pick the comment syntax from the file's name (or shebang), then comment or
//! uncomment the touched lines.

use std::path::Path;

use meridian_buffer::{Buffer, Edit, Range};

/// How a language writes comments: a line prefix, else a per-line block pair (`<!-- -->`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Style {
    Line(&'static str),
    Block(&'static str, &'static str),
}

pub fn style_for(path: Option<&Path>, first_line: &str) -> Option<Style> {
    let name = path.and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("");
    let ext = path.and_then(|p| p.extension()).and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let by_name = match name {
        "Makefile" | "makefile" | "GNUmakefile" | "Dockerfile" | "CMakeLists.txt" | ".gitignore" | ".bashrc"
        | ".zshrc" | ".profile" | ".bash_profile" | ".env" => Some(Style::Line("#")),
        _ => None,
    };
    let by_ext = match ext.as_str() {
        "sh" | "bash" | "zsh" | "fish" | "py" | "rb" | "pl" | "pm" | "r" | "yml" | "yaml" | "toml" | "conf"
        | "mk" | "tcl" | "nix" | "ps1" | "ex" | "exs" | "jl" | "cr" | "gitignore" | "dockerfile" => {
            Style::Line("#")
        }
        "rs" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx"
        | "java" | "go" | "swift" | "kt" | "kts" | "scala" | "cs" | "php" | "dart" | "zig" | "proto" | "sol"
        | "groovy" | "gradle" | "glsl" | "json5" | "jsonc" | "scss" | "less" | "v" => Style::Line("//"),
        "lua" | "sql" | "hs" | "elm" | "ada" | "adb" | "vhdl" => Style::Line("--"),
        "lisp" | "cl" | "clj" | "el" | "scm" | "ini" | "asm" | "s" => Style::Line(";"),
        "tex" | "erl" | "m" => Style::Line("%"),
        "vim" => Style::Line("\""),
        "bat" | "cmd" => Style::Line("REM"),
        "html" | "htm" | "xhtml" | "xml" | "svg" | "md" | "markdown" | "vue" => Style::Block("<!--", "-->"),
        "css" => Style::Block("/*", "*/"),
        _ => {
            return first_line.starts_with("#!").then_some(Style::Line("#")).or(by_name);
        }
    };
    Some(by_name.unwrap_or(by_ext))
}

/// Lines (inclusive) touched by the selections; a selection ending at a line start excludes that line.
fn touched_lines(buf: &Buffer) -> Vec<usize> {
    let mut v = Vec::new();
    for r in buf.sels() {
        let a = buf.line_of(r.start());
        let mut b = buf.line_of(r.end());
        if b > a && r.end() == buf.line_start(b) {
            b -= 1;
        }
        v.extend(a..=b);
    }
    v.dedup();
    v
}

struct Line {
    start: usize,
    /// Leading whitespace length, in chars.
    indent: usize,
    end: usize,
    text: String,
}

/// Non-blank touched lines, with their text after the indentation.
fn lines(buf: &Buffer) -> Vec<Line> {
    touched_lines(buf)
        .into_iter()
        .filter_map(|l| {
            let (start, end) = (buf.line_start(l), buf.line_end(l));
            let s: String = buf.rope().slice(start..end).chars().collect();
            let body = s.trim_start();
            (!body.trim_end().is_empty()).then(|| Line {
                start,
                indent: s.chars().count() - body.chars().count(),
                end,
                text: body.trim_end().to_string(),
            })
        })
        .collect()
}

fn is_commented(l: &Line, st: Style) -> bool {
    match st {
        Style::Line(p) => l.text.starts_with(p),
        Style::Block(o, c) => l.text.starts_with(o) && l.text.ends_with(c) && l.text.len() >= o.len() + c.len(),
    }
}

/// Edits that comment every touched line at the shallowest indent, or uncomment them all when
/// every non-blank one already is commented. Empty when there is nothing to do.
pub fn toggle_edits(buf: &Buffer, st: Style) -> Vec<Edit> {
    let ls = lines(buf);
    let ins = |at, s: String| Edit { at, del: 0, ins: s };
    let mut out = Vec::new();
    if ls.iter().all(|l| is_commented(l, st)) {
        for l in &ls {
            let at = l.start + l.indent;
            let (open, close) = match st {
                Style::Line(p) => (p, ""),
                Style::Block(o, c) => (o, c),
            };
            let space = usize::from(l.text[open.len()..].starts_with(' '));
            out.push(Edit { at, del: open.chars().count() + space, ins: String::new() });
            if !close.is_empty() {
                let body = &l.text[open.len() + space..l.text.len() - close.len()];
                let pad = usize::from(body.ends_with(' '));
                let from = l.start + l.indent + l.text.chars().count() - close.chars().count() - pad;
                out.push(Edit { at: from, del: close.chars().count() + pad, ins: String::new() });
            }
        }
    } else {
        let col = ls.iter().map(|l| l.indent).min().unwrap_or(0);
        for l in &ls {
            match st {
                Style::Line(p) => out.push(ins(l.start + col, format!("{p} "))),
                Style::Block(o, c) => {
                    out.push(ins(l.start + col, format!("{o} ")));
                    out.push(ins(l.end, format!(" {c}")));
                }
            }
        }
    }
    out
}

/// Selections after the edits. An insertion exactly at a position pushes it along only when it
/// is an opener (ends in a space), so a caret at the end of a line stays before ` -->`.
pub fn shifted(sels: &[Range], edits: &[Edit]) -> Vec<Range> {
    let map = |p: usize| {
        let mut q = p as isize;
        for e in edits {
            if e.at < p || (e.at == p && e.del == 0 && e.ins.ends_with(' ')) {
                q += e.ins.chars().count() as isize - e.del.min(p - e.at) as isize;
            }
        }
        q.max(0) as usize
    };
    sels.iter().map(|r| Range { anchor: map(r.anchor), head: map(r.head) }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_buffer::EditKind;

    fn toggle(text: &str, st: Style, sel: (usize, usize)) -> String {
        let mut b = Buffer::from_str(text);
        b.set_sels(vec![Range { anchor: sel.0, head: sel.1 }]);
        let e = toggle_edits(&b, st);
        b.edit(EditKind::Other, e);
        b.rope().to_string()
    }

    #[test]
    fn picks_syntax_by_name_and_shebang() {
        let p = |s: &str| style_for(Some(Path::new(s)), "");
        assert_eq!(p("a.rs"), Some(Style::Line("//")));
        assert_eq!(p("a.PY"), Some(Style::Line("#")));
        assert_eq!(p("index.html"), Some(Style::Block("<!--", "-->")));
        assert_eq!(p("Makefile"), Some(Style::Line("#")));
        assert_eq!(p("data.json"), None);
        assert_eq!(style_for(Some(Path::new("run")), "#!/bin/sh\n"), Some(Style::Line("#")));
        assert_eq!(style_for(None, "plain"), None);
    }

    #[test]
    fn comments_at_the_shallowest_indent_and_back() {
        let s = Style::Line("//");
        let c = toggle("    a\n      b\n\n    c\n", s, (0, 22));
        assert_eq!(c, "    // a\n    //   b\n\n    // c\n");
        assert_eq!(toggle(&c, s, (0, 28)), "    a\n      b\n\n    c\n");
    }

    #[test]
    fn mixed_lines_get_commented() {
        assert_eq!(toggle("// a\nb\n", Style::Line("//"), (0, 6)), "// // a\n// b\n");
    }

    #[test]
    fn block_style_wraps_each_line() {
        let s = Style::Block("<!--", "-->");
        let c = toggle("<p>x</p>\n  <b/>\n", s, (0, 15));
        assert_eq!(c, "<!-- <p>x</p> -->\n<!--   <b/> -->\n");
        assert_eq!(toggle(&c, s, (0, 33)), "<p>x</p>\n  <b/>\n");
    }

    #[test]
    fn caret_stays_put_relative_to_text() {
        let mut b = Buffer::from_str("ab\n");
        b.set_sels(vec![Range::caret(1)]);
        let e = toggle_edits(&b, Style::Line("#"));
        let sels = shifted(b.sels(), &e);
        b.edit(EditKind::Other, e);
        assert_eq!((sels[0].anchor, sels[0].head), (3, 3));
        assert_eq!(b.rope().to_string(), "# ab\n");
    }
}
