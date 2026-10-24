//! Natural command palette options: typing "sort every 2 lines" or "wrap in try/catch" offers a
//! text transform on the selection (the whole file when nothing is selected). A small rule-based
//! parser, plus user phrases from `natural.conf` that pipe the text through a shell command.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Case {
    Upper,
    Lower,
    Title,
    Sentence,
    Invert,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Wrap {
    TryCatch,
    /// Open and close text around the whole text, or around each line.
    Pair(String, String, bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Sort lines, or blocks of `every` lines; `col` is a 1-based whitespace-separated column.
    Sort { every: usize, col: Option<usize>, desc: bool, numeric: bool, unique: bool, nocase: bool },
    Reverse,
    Dedupe,
    Join(String),
    Wrap(Wrap),
    Case(Case),
    TrimTrailing,
    DropBlank,
    Number,
    Shuffle,
    /// Tabs to spaces (all tabs, to tab stops) or leading spaces to tabs; width 0 = the tab width setting.
    Tabs { to_spaces: bool, width: usize },
    FinalNewline,
    /// Replace each line (an arithmetic expression) with its value.
    Calc,
    /// Reflow paragraphs to this many columns; 0 = the first ruler.
    Rewrap(usize),
    /// A user phrase: the shell command, with `{n}` already filled in.
    Shell(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Nat {
    pub label: String,
    pub op: Op,
}

/// A line of `natural.conf`: `phrase words = shell command`.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub phrase: String,
    pub cmd: String,
}

pub fn parse_rules(conf: &str) -> Vec<Rule> {
    conf.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let (p, c) = l.split_once('=')?;
            let (p, c) = (p.trim().to_lowercase(), c.trim());
            (!p.is_empty() && !c.is_empty()).then(|| Rule { phrase: p, cmd: c.to_string() })
        })
        .collect()
}

const NUMBERS: [&str; 10] = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"];
const CARDINALS: [&str; 10] = ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];

/// A word as a number: `2`, `two`, `2nd`, `second`.
fn number(w: &str) -> Option<usize> {
    let digits = w.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    if let Ok(n) = digits.parse::<usize>() {
        return Some(n);
    }
    CARDINALS.iter().position(|c| *c == w).or_else(|| NUMBERS.iter().position(|c| *c == w)).map(|i| i + 1)
}

fn has(words: &[&str], any: &[&str]) -> bool {
    words.iter().any(|w| any.iter().any(|a| w.starts_with(a)))
}

/// The word after `marker` (or the first of `markers`) as a number.
fn number_after(words: &[&str], markers: &[&str]) -> Option<usize> {
    let i = words.iter().position(|w| markers.contains(w))?;
    words.get(i + 1).and_then(|w| number(w))
}

fn wrap_pair(what: &str) -> Option<Wrap> {
    let w = what.trim();
    let pair = |a: &str, b: &str| Some(Wrap::Pair(a.into(), b.into(), false));
    match w {
        "try/catch" | "try catch" | "try-catch" | "try" => Some(Wrap::TryCatch),
        "quotes" | "quote" | "double quotes" => pair("\"", "\""),
        "single quotes" | "single quote" => pair("'", "'"),
        "backticks" | "backtick" => pair("`", "`"),
        "parens" | "parentheses" | "brackets ()" => pair("(", ")"),
        "brackets" | "square brackets" => pair("[", "]"),
        "braces" | "curly braces" | "curlies" => pair("{", "}"),
        _ => {
            if let Some(t) = w.strip_prefix('<').map(|t| t.trim_end_matches('>')).filter(|t| !t.is_empty()) {
                return pair(&format!("<{t}>"), &format!("</{}>", t.split_whitespace().next().unwrap_or(t)));
            }
            // `wrap in x...y`: anything with an ellipsis.
            let (a, b) = w.split_once("...")?;
            (!a.is_empty() || !b.is_empty()).then(|| Wrap::Pair(a.into(), b.into(), false))
        }
    }
}

/// What the typed phrase can do, best guess first. Empty when it is not a phrase we know.
pub fn parse(input: &str, rules: &[Rule]) -> Vec<Nat> {
    let low = input.trim().to_lowercase();
    let mut out = Vec::new();
    if low.chars().count() < 3 {
        return out;
    }
    let words: Vec<&str> = low.split_whitespace().collect();
    let n_in_input = words.iter().find_map(|w| number(w).filter(|_| w.chars().next().is_some_and(|c| c.is_ascii_digit())));

    for r in rules {
        let want: Vec<&str> = r.phrase.split_whitespace().collect();
        let hit = want.iter().all(|p| words.iter().any(|w| w.starts_with(p) || (w.len() >= 3 && p.starts_with(w))));
        if hit {
            let cmd = r.cmd.replace("{n}", &n_in_input.map_or(String::new(), |n| n.to_string()));
            out.push(Nat { label: format!("{} (shell)", r.phrase), op: Op::Shell(cmd) });
        }
    }

    if has(&words, &["sort"]) {
        let every = number_after(&words, &["every", "each"]).filter(|&n| n > 1).unwrap_or(1);
        let col = words
            .iter()
            .position(|w| matches!(*w, "column" | "col" | "field" | "word"))
            .and_then(|i| {
                words.get(i + 1).and_then(|w| number(w)).or_else(|| i.checked_sub(1).and_then(|j| number(words[j])))
            });
        let desc = has(&words, &["desc", "revers", "backward", "z-a", "z to a", "largest"]);
        let numeric = has(&words, &["numer", "number"]);
        let unique = has(&words, &["uniq", "dedup"]) || low.contains("remove dup");
        let nocase = low.contains("ignore case") || low.contains("case insens") || low.contains("case-insens");
        let mut label = String::from("Sort");
        if every > 1 {
            label.push_str(&format!(" every {every} lines"));
        } else {
            label.push_str(" lines");
        }
        if let Some(c) = col {
            label.push_str(&format!(" by column {c}"));
        }
        for (on, s) in [(numeric, " numerically"), (desc, " descending"), (unique, ", unique"), (nocase, ", ignoring case")] {
            if on {
                label.push_str(s);
            }
        }
        out.push(Nat { label, op: Op::Sort { every, col, desc, numeric, unique, nocase } });
    }
    if low.contains("revers") && has(&words, &["line"]) && !has(&words, &["sort"]) {
        out.push(Nat { label: "Reverse the order of the lines".into(), op: Op::Reverse });
    }
    if has(&words, &["dedup", "uniq"]) || low.contains("remove dup") || low.contains("duplicate") {
        if !has(&words, &["sort"]) {
            out.push(Nat { label: "Remove duplicate lines".into(), op: Op::Dedupe });
        }
    }
    if has(&words, &["join"]) {
        let sep = low
            .split_once(" with ")
            .map(|(_, s)| s.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
            .map(|s| match s.as_str() {
                "comma" | "commas" => ",".to_string(),
                "space" | "spaces" => " ".to_string(),
                "nothing" | "none" => String::new(),
                _ => s,
            })
            .unwrap_or_else(|| " ".into());
        out.push(Nat { label: format!("Join the lines with \"{sep}\""), op: Op::Join(sep) });
    }
    if has(&words, &["wrap", "surround", "enclose", "put"]) {
        if let Some((_, what)) = low.split_once(" in ").or_else(|| low.split_once(" with ")) {
            let each = low.contains("each line") || low.contains("every line") || low.contains("per line");
            let what = what.trim();
            if let Some(w) = wrap_pair(what) {
                let w = match w {
                    Wrap::Pair(a, b, _) => Wrap::Pair(a, b, each),
                    t => t,
                };
                out.push(Nat { label: format!("Wrap {} in {what}", if each { "each line" } else { "the selection" }), op: Op::Wrap(w) });
            }
        }
    }
    if has(&words, &["shuffl", "randomi"]) && has(&words, &["line"]) {
        out.push(Nat { label: "Shuffle the lines".into(), op: Op::Shuffle });
    }
    if low.contains("tab") && (low.contains("space") || low.contains("indent")) {
        let at = |p: &str| low.find(p);
        let to_spaces = match (at("tab"), at("space")) {
            (Some(t), Some(sp)) => t < sp,
            (None, _) => true,
            (_, None) => false,
        };
        let label = if to_spaces { "Convert tabs to spaces" } else { "Convert leading spaces to tabs" };
        out.push(Nat { label: label.into(), op: Op::Tabs { to_spaces, width: 0 } });
    }
    if low.contains("newline") && has(&words, &["ensure", "add", "final", "end", "eof"]) {
        out.push(Nat { label: "Ensure a final newline".into(), op: Op::FinalNewline });
    }
    if has(&words, &["calc", "eval", "compute", "solve", "math"]) {
        out.push(Nat { label: "Replace with the calculated result".into(), op: Op::Calc });
    }
    let rewrap = has(&words, &["reflow", "rewrap", "hard"]) || (has(&words, &["wrap"]) && (low.contains("ruler") || low.contains("column")));
    let at_n = number_after(&words, &["at", "to", "width", "column"]).filter(|&n| n >= 10);
    if rewrap || (has(&words, &["wrap"]) && at_n.is_some() && !low.contains(" in ")) {
        let n = at_n.unwrap_or(0);
        let label = if n == 0 { "Wrap paragraphs to the ruler".to_string() } else { format!("Wrap paragraphs at {n} columns") };
        out.push(Nat { label, op: Op::Rewrap(n) });
    }
    for (keys, c, label) in [
        (&["upper", "capital", "caps"][..], Case::Upper, "Make UPPERCASE"),
        (&["lower"][..], Case::Lower, "Make lowercase"),
        (&["title"][..], Case::Title, "Make Title Case"),
        (&["sentence"][..], Case::Sentence, "Make Sentence case"),
        (&["invert", "swap", "toggle", "flip"][..], Case::Invert, "Invert case"),
    ] {
        if has(&words, keys) && !has(&words, &["sort"]) && (c != Case::Invert || low.contains("case")) {
            out.push(Nat { label: label.into(), op: Op::Case(c) });
        }
    }
    if low.contains("trailing") && (has(&words, &["trim", "strip", "remove"])) {
        out.push(Nat { label: "Trim trailing whitespace".into(), op: Op::TrimTrailing });
    }
    if (low.contains("blank") || low.contains("empty")) && has(&words, &["remove", "delete", "drop", "strip"]) {
        out.push(Nat { label: "Remove blank lines".into(), op: Op::DropBlank });
    }
    if has(&words, &["number"]) && has(&words, &["line"]) && !has(&words, &["sort"]) {
        out.push(Nat { label: "Number the lines".into(), op: Op::Number });
    }
    out
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '\'' {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}

fn sentence_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if start { out.extend(c.to_uppercase()) } else { out.extend(c.to_lowercase()) }
            start = false;
        } else {
            out.push(c);
            if matches!(c, '.' | '!' | '?') || c == '\n' && out.ends_with("\n\n") {
                start = true;
            }
        }
    }
    out
}

/// Tabs to spaces on tab stops, or leading runs of whitespace to tabs.
fn tabs(text: &str, to_spaces: bool, width: usize) -> String {
    let (lines, end) = lines_of(text);
    let v: Vec<String> = lines
        .iter()
        .map(|l| {
            if to_spaces {
                let mut out = String::new();
                let mut col = 0;
                for c in l.chars() {
                    if c == '\t' {
                        let n = width - col % width;
                        out.push_str(&" ".repeat(n));
                        col += n;
                    } else {
                        out.push(c);
                        col += 1;
                    }
                }
                out
            } else {
                let lead: String = l.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                let cols = lead.chars().fold(0, |col, c| if c == '\t' { col + width - col % width } else { col + 1 });
                format!("{}{}{}", "\t".repeat(cols / width), " ".repeat(cols % width), &l[lead.len()..])
            }
        })
        .collect();
    join_lines(&v, end)
}

/// Greedy reflow of each paragraph to `width` columns, keeping its indent and `>` quote prefix.
fn rewrap(text: &str, width: usize) -> String {
    let (lines, end) = lines_of(text);
    let mut out: Vec<String> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut Vec<String>| {
        let Some(first) = para.first() else { return };
        let ws = first.len() - first.trim_start().len();
        let mut prefix = first[..ws].to_string();
        let rest = first[ws..].trim_start_matches(|c: char| c == '>' || c == ' ');
        prefix.push_str(&first[ws..first.len() - rest.len()]);
        let mut line = String::new();
        for l in para.iter() {
            let body = l.trim_start().trim_start_matches(|c: char| c == '>' || c == ' ');
            for w in body.split_whitespace() {
                if !line.is_empty() && line.chars().count() + 1 + w.chars().count() > width {
                    out.push(std::mem::take(&mut line));
                }
                if line.is_empty() {
                    line = format!("{prefix}{w}");
                } else {
                    line.push(' ');
                    line.push_str(w);
                }
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
        para.clear();
    };
    for l in lines {
        if l.trim().is_empty() {
            flush(&mut para, &mut out);
            out.push(String::new());
        } else {
            para.push(l);
        }
    }
    flush(&mut para, &mut out);
    join_lines(&out, end)
}

/// Evaluate each non-blank line as arithmetic, keeping its indentation.
fn calc_lines(text: &str) -> Result<String, String> {
    let (lines, end) = lines_of(text);
    let mut v = Vec::new();
    for l in lines {
        if l.trim().is_empty() {
            v.push(l.to_string());
            continue;
        }
        let ws = l.len() - l.trim_start().len();
        let expr = l.trim().trim_end_matches('=').trim();
        v.push(format!("{}{}", &l[..ws], fmt_num(calc(expr)?)));
    }
    Ok(join_lines(&v, end))
}

fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    let s = format!("{x:.10}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `+ - * / % ^ **`, unary minus, parentheses, `,` and `_` thousands separators ignored.
fn calc(expr: &str) -> Result<f64, String> {
    let chars: Vec<char> = expr.chars().filter(|c| !c.is_whitespace() && *c != ',' && *c != '_').collect();
    let mut p = 0;
    let v = calc_sum(&chars, &mut p)?;
    if p < chars.len() {
        return Err(format!("cannot calculate \"{expr}\""));
    }
    if v.is_finite() { Ok(v) } else { Err("result is not a number".into()) }
}

fn calc_sum(c: &[char], p: &mut usize) -> Result<f64, String> {
    let mut v = calc_prod(c, p)?;
    while let Some(&op) = c.get(*p).filter(|o| **o == '+' || **o == '-') {
        *p += 1;
        let r = calc_prod(c, p)?;
        if op == '+' { v += r } else { v -= r }
    }
    Ok(v)
}

fn calc_prod(c: &[char], p: &mut usize) -> Result<f64, String> {
    let mut v = calc_pow(c, p)?;
    loop {
        match c.get(*p) {
            Some('*') if c.get(*p + 1) != Some(&'*') => {
                *p += 1;
                v *= calc_pow(c, p)?;
            }
            Some(&o) if o == '/' || o == '%' => {
                *p += 1;
                let r = calc_pow(c, p)?;
                if r == 0.0 {
                    return Err("division by zero".into());
                }
                v = if o == '/' { v / r } else { v % r };
            }
            _ => return Ok(v),
        }
    }
}

fn calc_pow(c: &[char], p: &mut usize) -> Result<f64, String> {
    let base = calc_unary(c, p)?;
    if c.get(*p) == Some(&'^') {
        *p += 1;
    } else if c.get(*p) == Some(&'*') && c.get(*p + 1) == Some(&'*') {
        *p += 2;
    } else {
        return Ok(base);
    }
    Ok(base.powf(calc_pow(c, p)?))
}

fn calc_unary(c: &[char], p: &mut usize) -> Result<f64, String> {
    match c.get(*p) {
        Some('-') => {
            *p += 1;
            Ok(-calc_unary(c, p)?)
        }
        Some('+') => {
            *p += 1;
            calc_unary(c, p)
        }
        Some('(') => {
            *p += 1;
            let v = calc_sum(c, p)?;
            if c.get(*p) != Some(&')') {
                return Err("missing )".into());
            }
            *p += 1;
            Ok(v)
        }
        _ => {
            let start = *p;
            while c.get(*p).is_some_and(|d| d.is_ascii_digit() || *d == '.') {
                *p += 1;
            }
            c[start..*p].iter().collect::<String>().parse().map_err(|_| "not an arithmetic expression".to_string())
        }
    }
}

/// Fill in what only the app knows: the tab width and the first ruler.
pub fn resolve(op: &Op, tab_width: usize, ruler: Option<usize>) -> Op {
    match op {
        Op::Tabs { to_spaces, width: 0 } => Op::Tabs { to_spaces: *to_spaces, width: tab_width },
        Op::Rewrap(0) => Op::Rewrap(ruler.unwrap_or(0)),
        o => o.clone(),
    }
}

/// Split into lines keeping whether the text ended in a newline.
fn lines_of(text: &str) -> (Vec<&str>, bool) {
    let end = text.ends_with('\n');
    let body = text.strip_suffix('\n').unwrap_or(text);
    (if text.is_empty() { Vec::new() } else { body.split('\n').collect() }, end)
}

fn join_lines(lines: &[String], end: bool) -> String {
    let mut s = lines.join("\n");
    if end && !lines.is_empty() {
        s.push('\n');
    }
    s
}

fn column<'a>(line: &'a str, col: Option<usize>) -> &'a str {
    match col {
        Some(c) => line.split_whitespace().nth(c - 1).unwrap_or(""),
        None => line,
    }
}

fn leading_number(s: &str) -> f64 {
    let t = s.trim_start();
    let mut end = 0;
    for (i, c) in t.char_indices() {
        if c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+')) {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    t[..end].parse().unwrap_or(f64::INFINITY)
}

fn sort(text: &str, every: usize, col: Option<usize>, desc: bool, numeric: bool, unique: bool, nocase: bool) -> String {
    let (lines, end) = lines_of(text);
    let key = |s: &str| -> String {
        let k = column(s, col);
        if nocase { k.to_lowercase() } else { k.to_string() }
    };
    let mut blocks: Vec<Vec<&str>> = lines.chunks(every.max(1)).map(|c| c.to_vec()).collect();
    blocks.sort_by(|a, b| {
        let (ka, kb) = (key(a[0]), key(b[0]));
        let o = if numeric {
            leading_number(&ka).partial_cmp(&leading_number(&kb)).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            ka.cmp(&kb)
        };
        if desc { o.reverse() } else { o }
    });
    if unique {
        blocks.dedup_by(|a, b| a == b);
        let mut seen = std::collections::HashSet::new();
        blocks.retain(|b| seen.insert(b.clone()));
    }
    let flat: Vec<String> = blocks.into_iter().flatten().map(str::to_string).collect();
    join_lines(&flat, end)
}

/// The text with the lines each prefixed and suffixed, keeping their indentation.
fn wrap(text: &str, w: &Wrap, ext: &str) -> String {
    match w {
        Wrap::Pair(a, b, true) => {
            let (lines, end) = lines_of(text);
            let v: Vec<String> = lines.iter().map(|l| if l.is_empty() { String::new() } else { format!("{a}{l}{b}") }).collect();
            join_lines(&v, end)
        }
        Wrap::Pair(a, b, false) => format!("{a}{text}{b}"),
        Wrap::TryCatch => {
            let (lines, _) = lines_of(text);
            let indent: String =
                lines.iter().find(|l| !l.trim().is_empty()).map_or(String::new(), |l| l[..l.len() - l.trim_start().len()].to_string());
            let step = if indent.contains('\t') { "\t" } else { "    " };
            // (opening line, handler lines, closing line) per language.
            let (open, handler, close): (&str, &[&str], Option<&str>) = match ext {
                "py" => ("try:", &["except Exception:", "raise"], None),
                "rb" => ("begin", &["rescue => e", "raise"], Some("end")),
                "java" | "kt" | "cs" | "scala" | "php" => ("try {", &["} catch (Exception e) {", "throw e;"], Some("}")),
                _ => ("try {", &["} catch (e) {", "throw e;"], Some("}")),
            };
            let mut out = format!("{indent}{open}\n");
            for l in &lines {
                if l.is_empty() {
                    out.push('\n');
                } else {
                    out.push_str(&format!("{step}{l}\n"));
                }
            }
            out.push_str(&format!("{indent}{}\n{indent}{step}{}\n", handler[0], handler[1]));
            if let Some(c) = close {
                out.push_str(&format!("{indent}{c}\n"));
            }
            out
        }
    }
}

/// Run the operation over `text`. `ext` is the file extension (for language-specific wraps).
pub fn apply(op: &Op, text: &str, ext: &str) -> Result<String, String> {
    Ok(match op {
        Op::Sort { every, col, desc, numeric, unique, nocase } => sort(text, *every, *col, *desc, *numeric, *unique, *nocase),
        Op::Reverse => {
            let (l, end) = lines_of(text);
            let v: Vec<String> = l.iter().rev().map(|s| s.to_string()).collect();
            join_lines(&v, end)
        }
        Op::Dedupe => {
            let (l, end) = lines_of(text);
            let mut seen = std::collections::HashSet::new();
            let v: Vec<String> = l.iter().filter(|s| seen.insert(**s)).map(|s| s.to_string()).collect();
            join_lines(&v, end)
        }
        Op::Join(sep) => {
            let (l, end) = lines_of(text);
            let mut s = l.iter().map(|s| s.trim_end()).collect::<Vec<_>>().join(sep);
            if end {
                s.push('\n');
            }
            s
        }
        Op::Wrap(w) => wrap(text, w, ext),
        Op::Case(Case::Upper) => text.to_uppercase(),
        Op::Case(Case::Lower) => text.to_lowercase(),
        Op::Case(Case::Title) => title_case(text),
        Op::Case(Case::Sentence) => sentence_case(text),
        Op::Case(Case::Invert) => text.chars().flat_map(|c| if c.is_lowercase() { c.to_uppercase().collect::<Vec<_>>() } else { c.to_lowercase().collect() }).collect(),
        Op::Shuffle => {
            let (l, end) = lines_of(text);
            let mut v: Vec<String> = l.iter().map(|s| s.to_string()).collect();
            let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
            for i in (1..v.len()).rev() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                v.swap(i, (x % (i as u64 + 1)) as usize);
            }
            join_lines(&v, end)
        }
        Op::Tabs { to_spaces, width } => tabs(text, *to_spaces, (*width).max(1)),
        Op::FinalNewline => if text.is_empty() || text.ends_with('\n') { text.to_string() } else { format!("{text}\n") },
        Op::Calc => calc_lines(text)?,
        Op::Rewrap(0) => return Err("no ruler set: try \"wrap at 72\"".into()),
        Op::Rewrap(n) => rewrap(text, *n),
        Op::TrimTrailing => {
            let (l, end) = lines_of(text);
            let v: Vec<String> = l.iter().map(|s| s.trim_end().to_string()).collect();
            join_lines(&v, end)
        }
        Op::DropBlank => {
            let (l, end) = lines_of(text);
            let v: Vec<String> = l.iter().filter(|s| !s.trim().is_empty()).map(|s| s.to_string()).collect();
            join_lines(&v, end)
        }
        Op::Number => {
            let (l, end) = lines_of(text);
            let w = l.len().to_string().len();
            let v: Vec<String> = l.iter().enumerate().map(|(i, s)| format!("{:>w$}  {s}", i + 1)).collect();
            join_lines(&v, end)
        }
        Op::Shell(cmd) => shell(cmd, text)?,
    })
}

const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Pipe `text` through `sh -c cmd`; its stdout replaces the text.
fn shell(cmd: &str, text: &str) -> Result<String, String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("sh: {e}"))?;
    // Feed stdin from a thread so a command that fills its output pipe first cannot deadlock.
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let input = text.to_string();
    std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let (mut so, mut se) = (child.stdout.take().ok_or("no stdout")?, child.stderr.take().ok_or("no stderr")?);
    let out = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = std::io::Read::read_to_end(&mut so, &mut s);
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = std::io::Read::read_to_end(&mut se, &mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() > SHELL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("command timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    if !status.success() {
        let e = String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned();
        return Err(e.lines().next().map_or_else(|| format!("exit {}", status.code().unwrap_or(-1)), str::to_string));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(input: &str) -> Op {
        parse(input, &[]).into_iter().next().unwrap_or_else(|| panic!("no parse for {input}")).op
    }

    #[test]
    fn sort_every_two_lines_sorts_blocks() {
        assert_eq!(first("sort every 2 lines"), Op::Sort { every: 2, col: None, desc: false, numeric: false, unique: false, nocase: false });
        let out = apply(&first("sort every two lines"), "b\n1\na\n2\n", "").unwrap();
        assert_eq!(out, "a\n2\nb\n1\n");
    }

    #[test]
    fn sort_by_column() {
        let op = first("sort these lines by second column");
        let out = apply(&op, "x 3\ny 1\nz 2\n", "").unwrap();
        assert_eq!(out, "y 1\nz 2\nx 3\n");
        assert!(matches!(first("sort by column 2"), Op::Sort { col: Some(2), .. }));
    }

    #[test]
    fn sort_numeric_descending_unique() {
        let op = first("sort numerically descending unique");
        assert_eq!(apply(&op, "10\n9\n10\n100\n", "").unwrap(), "100\n10\n9\n");
    }

    #[test]
    fn wrap_pairs_and_try_catch() {
        let op = first("wrap selection in quotes");
        assert_eq!(apply(&op, "hi", "").unwrap(), "\"hi\"");
        let op = first("wrap each line in parens");
        assert_eq!(apply(&op, "a\nb\n", "").unwrap(), "(a)\n(b)\n");
        let op = first("wrap selection in try/catch");
        assert_eq!(apply(&op, "  foo();\n", "js").unwrap(), "  try {\n      foo();\n  } catch (e) {\n      throw e;\n  }\n");
        assert_eq!(apply(&first("wrap in <div class=x>"), "t", "").unwrap(), "<div class=x>t</div>");
    }

    #[test]
    fn small_transforms() {
        assert_eq!(apply(&first("reverse the lines"), "a\nb\nc", "").unwrap(), "c\nb\na");
        assert_eq!(apply(&first("remove duplicate lines"), "a\nb\na\n", "").unwrap(), "a\nb\n");
        assert_eq!(apply(&first("join lines with comma"), "a\nb\nc\n", "").unwrap(), "a,b,c\n");
        assert_eq!(apply(&first("title case"), "the quick fox", "").unwrap(), "The Quick Fox");
        assert_eq!(apply(&first("trim trailing whitespace"), "a  \nb\t\n", "").unwrap(), "a\nb\n");
        assert_eq!(apply(&first("remove blank lines"), "a\n\n \nb\n", "").unwrap(), "a\nb\n");
        assert_eq!(apply(&first("number the lines"), "x\ny", "").unwrap(), "1  x\n2  y");
    }

    #[test]
    fn new_case_and_line_ops() {
        assert_eq!(apply(&first("sentence case"), "hELLO there. how ARE you? fine", "").unwrap(), "Hello there. How are you? Fine");
        assert_eq!(apply(&first("invert case"), "Hello", "").unwrap(), "hELLO");
        let out = apply(&first("shuffle the lines"), "a\nb\nc\nd\n", "").unwrap();
        let mut l: Vec<&str> = out.lines().collect();
        l.sort();
        assert_eq!(l, ["a", "b", "c", "d"]);
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn tabs_and_final_newline() {
        let op = resolve(&first("tabs to spaces"), 4, None);
        assert_eq!(apply(&op, "\tx\n  \ty\n", "").unwrap(), "    x\n    y\n");
        let op = resolve(&first("convert spaces to tabs"), 4, None);
        assert_eq!(apply(&op, "        x\n      y  z\n", "").unwrap(), "\t\tx\n\t  y  z\n");
        assert_eq!(apply(&first("ensure final newline"), "a", "").unwrap(), "a\n");
        assert_eq!(apply(&first("ensure final newline"), "a\n", "").unwrap(), "a\n");
    }

    #[test]
    fn calculate_expressions() {
        let c = |s: &str| apply(&first("calculate"), s, "").unwrap();
        assert_eq!(c("12*4.5"), "54");
        assert_eq!(c("2^3^2"), "512");
        assert_eq!(c("-(1+2)*3 ="), "-9");
        assert_eq!(c("  10/4\n7%4\n"), "  2.5\n3\n");
        assert_eq!(c("1/3"), "0.3333333333");
        assert!(apply(&Op::Calc, "1/0", "").is_err());
        assert!(apply(&Op::Calc, "hello", "").is_err());
    }

    #[test]
    fn rewrap_paragraphs() {
        let op = first("wrap at 20");
        assert_eq!(op, Op::Rewrap(20));
        let out = apply(&op, "the quick brown fox jumps\nover the lazy dog\n\n  > a b c d e f g h i j k l m n o\n", "").unwrap();
        assert_eq!(out, "the quick brown fox\njumps over the lazy\ndog\n\n  > a b c d e f g h\n  > i j k l m n o\n");
        assert_eq!(first("wrap to ruler"), Op::Rewrap(0));
        assert!(apply(&Op::Rewrap(0), "x", "").is_err());
        assert_eq!(resolve(&Op::Rewrap(0), 4, Some(72)), Op::Rewrap(72));
        // an ordinary wrap-in phrase is still a wrap
        assert!(matches!(first("wrap selection in quotes"), Op::Wrap(_)));
    }

    #[test]
    fn unknown_or_short_input_offers_nothing() {
        assert!(parse("so", &[]).is_empty());
        assert!(parse("open file", &[]).is_empty());
    }

    #[test]
    fn user_rules_match_by_word_and_run_in_shell() {
        let rules = parse_rules("# c\nshout = tr a-z A-Z\nfirst lines = head -n {n}\nbad line\n");
        assert_eq!(rules.len(), 2);
        let hits = parse("shout", &rules);
        assert_eq!(hits[0].op, Op::Shell("tr a-z A-Z".into()));
        assert_eq!(apply(&hits[0].op, "abc\n", "").unwrap(), "ABC\n");
        let hits = parse("first 2 lines", &rules);
        assert_eq!(hits[0].op, Op::Shell("head -n 2".into()));
        assert_eq!(apply(&hits[0].op, "a\nb\nc\n", "").unwrap(), "a\nb\n");
        assert!(apply(&Op::Shell("exit 3".into()), "", "").is_err());
    }
}
