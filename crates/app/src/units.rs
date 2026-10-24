//! Units of measurement in text: find `5 miles`, `12kg` or `98.6F` at the caret, say what units
//! they could mean (`5m` is metres, miles or minutes), and convert them.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dim {
    Length,
    Mass,
    Volume,
    Temperature,
    Time,
    Speed,
    Data,
}

#[derive(Debug)]
pub struct Unit {
    pub name: &'static str,
    pub plural: &'static str,
    pub abbr: &'static str,
    /// Lowercase spellings that mean this unit. A one-letter spelling must touch the number.
    pub aliases: &'static [&'static str],
    pub dim: Dim,
    /// Value in the dimension's base unit = value * factor + offset.
    pub factor: f64,
    pub offset: f64,
}

macro_rules! u {
    ($n:expr, $p:expr, $a:expr, [$($al:expr),*], $d:ident, $f:expr) => {
        Unit { name: $n, plural: $p, abbr: $a, aliases: &[$($al),*], dim: Dim::$d, factor: $f, offset: 0.0 }
    };
}

pub static UNITS: &[Unit] = &[
    // length, base metre
    u!("metre", "metres", "m", ["m", "metre", "metres", "meter", "meters"], Length, 1.0),
    u!("kilometre", "kilometres", "km", ["km", "kilometre", "kilometres", "kilometer", "kilometers"], Length, 1000.0),
    u!("centimetre", "centimetres", "cm", ["cm", "centimetre", "centimetres", "centimeter", "centimeters"], Length, 0.01),
    u!("millimetre", "millimetres", "mm", ["mm", "millimetre", "millimetres", "millimeter", "millimeters"], Length, 0.001),
    u!("mile", "miles", "mi", ["mi", "m", "mile", "miles"], Length, 1609.344),
    u!("yard", "yards", "yd", ["yd", "yds", "yard", "yards"], Length, 0.9144),
    u!("foot", "feet", "ft", ["ft", "foot", "feet"], Length, 0.3048),
    u!("inch", "inches", "in", ["in", "inch", "inches"], Length, 0.0254),
    u!("nautical mile", "nautical miles", "nmi", ["nmi"], Length, 1852.0),
    // mass, base gram
    u!("gram", "grams", "g", ["g", "gram", "grams", "gramme", "grammes"], Mass, 1.0),
    u!("kilogram", "kilograms", "kg", ["kg", "kgs", "kilogram", "kilograms", "kilo", "kilos"], Mass, 1000.0),
    u!("milligram", "milligrams", "mg", ["mg", "milligram", "milligrams"], Mass, 0.001),
    u!("tonne", "tonnes", "t", ["t", "tonne", "tonnes"], Mass, 1_000_000.0),
    u!("pound", "pounds", "lb", ["lb", "lbs", "pound", "pounds"], Mass, 453.592_37),
    u!("ounce", "ounces", "oz", ["oz", "ounce", "ounces"], Mass, 28.349_523_125),
    // volume, base litre
    u!("litre", "litres", "l", ["l", "litre", "litres", "liter", "liters"], Volume, 1.0),
    u!("millilitre", "millilitres", "ml", ["ml", "millilitre", "millilitres", "milliliter", "milliliters"], Volume, 0.001),
    u!("gallon", "gallons", "gal", ["gal", "gallon", "gallons"], Volume, 3.785_411_784),
    u!("quart", "quarts", "qt", ["qt", "quart", "quarts"], Volume, 0.946_352_946),
    u!("pint", "pints", "pt", ["pt", "pint", "pints"], Volume, 0.473_176_473),
    u!("cup", "cups", "cup", ["cup", "cups"], Volume, 0.236_588_236_5),
    u!("fluid ounce", "fluid ounces", "fl oz", ["floz"], Volume, 0.029_573_529_562_5),
    u!("tablespoon", "tablespoons", "tbsp", ["tbsp", "tablespoon", "tablespoons"], Volume, 0.014_786_764_78),
    u!("teaspoon", "teaspoons", "tsp", ["tsp", "teaspoon", "teaspoons"], Volume, 0.004_928_921_59),
    // temperature, base celsius
    Unit { name: "degree Celsius", plural: "degrees Celsius", abbr: "\u{b0}C", aliases: &["c", "\u{b0}c", "celsius"], dim: Dim::Temperature, factor: 1.0, offset: 0.0 },
    Unit { name: "degree Fahrenheit", plural: "degrees Fahrenheit", abbr: "\u{b0}F", aliases: &["f", "\u{b0}f", "fahrenheit"], dim: Dim::Temperature, factor: 5.0 / 9.0, offset: -32.0 * 5.0 / 9.0 },
    Unit { name: "kelvin", plural: "kelvin", abbr: "K", aliases: &["kelvin"], dim: Dim::Temperature, factor: 1.0, offset: -273.15 },
    // time, base second
    u!("second", "seconds", "s", ["s", "sec", "secs", "second", "seconds"], Time, 1.0),
    u!("millisecond", "milliseconds", "ms", ["ms", "millisecond", "milliseconds"], Time, 0.001),
    u!("minute", "minutes", "min", ["m", "min", "mins", "minute", "minutes"], Time, 60.0),
    u!("hour", "hours", "h", ["h", "hr", "hrs", "hour", "hours"], Time, 3600.0),
    u!("day", "days", "d", ["d", "day", "days"], Time, 86_400.0),
    u!("week", "weeks", "wk", ["wk", "wks", "week", "weeks"], Time, 604_800.0),
    // speed, base metre per second
    u!("metre per second", "metres per second", "m/s", ["m/s", "mps"], Speed, 1.0),
    u!("kilometre per hour", "kilometres per hour", "km/h", ["km/h", "kph", "kmh", "kmph"], Speed, 1000.0 / 3600.0),
    u!("mile per hour", "miles per hour", "mph", ["mph", "mi/h"], Speed, 0.447_04),
    u!("foot per second", "feet per second", "ft/s", ["ft/s", "fps"], Speed, 0.3048),
    u!("knot", "knots", "kn", ["knot", "knots", "kn"], Speed, 1852.0 / 3600.0),
    // data, base byte
    u!("byte", "bytes", "B", ["byte", "bytes"], Data, 1.0),
    u!("kilobyte", "kilobytes", "KB", ["kb", "kilobyte", "kilobytes"], Data, 1e3),
    u!("megabyte", "megabytes", "MB", ["mb", "megabyte", "megabytes"], Data, 1e6),
    u!("gigabyte", "gigabytes", "GB", ["gb", "gigabyte", "gigabytes"], Data, 1e9),
    u!("terabyte", "terabytes", "TB", ["tb", "terabyte", "terabytes"], Data, 1e12),
    u!("kibibyte", "kibibytes", "KiB", ["kib"], Data, 1024.0),
    u!("mebibyte", "mebibytes", "MiB", ["mib"], Data, 1_048_576.0),
    u!("gibibyte", "gibibytes", "GiB", ["gib"], Data, 1_073_741_824.0),
    u!("tebibyte", "tebibytes", "TiB", ["tib"], Data, 1_099_511_627_776.0),
];

/// A number with a unit as it stands in the text.
#[derive(Debug, PartialEq)]
pub struct Quantity {
    /// Char range of the whole `5 miles` and of just the unit part (line-relative).
    pub start: usize,
    pub end: usize,
    pub unit_start: usize,
    pub value: f64,
    pub number: String,
    pub spaced: bool,
    /// The unit text as typed (`miles`, `km/h`).
    pub unit_text: String,
    /// Indices into `UNITS` the unit text can mean (more than one = ambiguous).
    pub candidates: Vec<usize>,
}

impl Quantity {
    /// Written in full (`miles`) rather than abbreviated (`mi`)?
    pub fn long_form(&self) -> bool {
        self.unit_text.chars().count() > 3 && !self.unit_text.contains('/') || self.unit_text.eq_ignore_ascii_case("mins")
    }
}

fn is_unit_char(c: char) -> bool {
    c.is_alphabetic() || c == '\u{b0}'
}

/// The quantity whose text touches char column `col` of `line`.
pub fn find(line: &str, col: usize) -> Option<Quantity> {
    let ch: Vec<char> = line.chars().collect();
    let n = ch.len();
    let mut i = 0;
    while i < n {
        if !ch[i].is_ascii_digit() || (i > 0 && (ch[i - 1].is_alphanumeric() || ch[i - 1] == '.' && i > 1 && ch[i - 2].is_ascii_digit())) {
            i += 1;
            continue;
        }
        let ns = i;
        while i < n && ch[i].is_ascii_digit() {
            i += 1;
        }
        if i + 1 < n && ch[i] == '.' && ch[i + 1].is_ascii_digit() {
            i += 1;
            while i < n && ch[i].is_ascii_digit() {
                i += 1;
            }
        }
        let ne = i;
        let mut start = ns;
        if ns > 0 && ch[ns - 1] == '-' && (ns == 1 || !ch[ns - 2].is_alphanumeric()) {
            start = ns - 1;
        }
        let spaced = i < n && ch[i] == ' ';
        let us = if spaced { i + 1 } else { i };
        let mut ue = us;
        while ue < n && (is_unit_char(ch[ue]) || (ch[ue] == '/' && ue + 1 < n && is_unit_char(ch[ue + 1]) && ue > us)) {
            ue += 1;
        }
        if ue == us {
            continue;
        }
        let text: String = ch[us..ue].iter().collect();
        let low = text.to_lowercase();
        let short = low.chars().filter(|c| *c != '\u{b0}').count() <= 1 || low == "in";
        if (short && spaced) || (spaced && low == "in") {
            i = ue;
            continue;
        }
        let candidates: Vec<usize> = UNITS.iter().enumerate().filter(|(_, u)| u.aliases.contains(&low.as_str())).map(|(k, _)| k).collect();
        if candidates.is_empty() {
            i = ue;
            continue;
        }
        let number: String = ch[start..ne].iter().collect();
        let Ok(value) = number.parse::<f64>() else {
            i = ue;
            continue;
        };
        if col >= start && col <= ue {
            return Some(Quantity { start, end: ue, unit_start: us, value, number, spaced, unit_text: text, candidates });
        }
        i = ue;
    }
    None
}

/// `value` in `from` expressed in `to` (same dimension).
pub fn convert(value: f64, from: &Unit, to: &Unit) -> f64 {
    let base = value * from.factor + from.offset;
    (base - to.offset) / to.factor
}

/// A number for text: about four significant figures, no trailing zeros.
pub fn fmt_num(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let mag = v.abs().log10().floor() as i32;
    let dec = (3 - mag).clamp(0, 4) as usize;
    let s = format!("{v:.dec$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

/// A converted quantity as text in the style of the original (`5 miles` -> `8.047 kilometres`).
pub fn render(value: f64, to: &Unit, spaced: bool, long: bool) -> String {
    let n = fmt_num(value);
    let name: String = if long {
        if n == "1" { to.name } else { to.plural }.to_string()
    } else {
        to.abbr.to_string()
    };
    // Degrees and short forms sit against the number when it was written that way.
    let space = spaced || (long && !name.starts_with('\u{b0}'));
    format!("{n}{}{name}", if space { " " } else { "" })
}

/// Units worth offering for `q` read as `UNITS[idx]`, nearest in size first: results between
/// 0.01 and a million, every other unit of the dimension otherwise.
pub fn conversions(q: &Quantity, idx: usize) -> Vec<(usize, f64)> {
    let from = &UNITS[idx];
    let all: Vec<(usize, f64)> = UNITS
        .iter()
        .enumerate()
        .filter(|(k, u)| *k != idx && u.dim == from.dim)
        .map(|(k, u)| (k, convert(q.value, from, u)))
        .collect();
    let sane: Vec<(usize, f64)> = all.iter().copied().filter(|(_, v)| v.abs() >= 0.01 && v.abs() < 1_000_000.0 || *v == 0.0).collect();
    let mut out = if sane.is_empty() { all } else { sane };
    // The most readable results first: closest to a one- or two-digit number.
    let score = |v: f64| if v == 0.0 { 9.0 } else { (v.abs().log10() - 1.0).abs() };
    out.sort_by(|a, b| score(a.1).total_cmp(&score(b.1)));
    out.truncate(6);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(i: usize) -> &'static str {
        UNITS[i].abbr
    }

    #[test]
    fn finds_spaced_and_attached_units() {
        let q = find("go 5 miles now", 5).unwrap();
        assert_eq!((q.start, q.end, q.unit_start), (3, 10, 5));
        assert_eq!(q.value, 5.0);
        assert!(q.spaced);
        assert_eq!(q.candidates.iter().map(|&i| name(i)).collect::<Vec<_>>(), vec!["mi"]);
        let q = find("12kg", 4).unwrap();
        assert_eq!((q.start, q.end), (0, 4));
        assert!(!q.spaced);
        assert!(find("12 kg", 0).is_some());
        assert!(find("12 kg", 3).is_some());
        assert!(find("go 5 miles now", 1).is_none());
    }

    #[test]
    fn five_m_is_ambiguous() {
        let q = find("5m", 1).unwrap();
        let names: Vec<_> = q.candidates.iter().map(|&i| UNITS[i].name).collect();
        assert_eq!(names, vec!["metre", "mile", "minute"]);
    }

    #[test]
    fn plain_text_and_ordinals_are_not_units() {
        assert!(find("5 minutes of 1st place", 3).is_some());
        assert!(find("1st place", 1).is_none());
        assert!(find("I have 5 in the box", 8).is_none(), "a spaced `in` is the word");
        assert!(find("5 m", 2).is_none(), "a spaced single letter is not a unit");
        assert!(find("room 12b", 7).is_none());
        assert!(find("v2.5kg", 5).is_none(), "a number glued to letters is part of a name");
        assert!(find("2026-10-05", 6).is_none());
    }

    #[test]
    fn decimals_and_signs() {
        let q = find("temp -3.5c today", 8).unwrap();
        assert_eq!(q.number, "-3.5");
        assert_eq!(q.value, -3.5);
        assert_eq!(q.candidates.iter().map(|&i| name(i)).collect::<Vec<_>>(), vec!["\u{b0}C"]);
        assert_eq!(find("98.6F", 4).unwrap().value, 98.6);
        assert_eq!(find("8 km/h", 4).unwrap().unit_text, "km/h");
    }

    #[test]
    fn conversions_are_right() {
        let u = |a: &str| UNITS.iter().find(|u| u.abbr == a).unwrap();
        assert!((convert(5.0, u("mi"), u("km")) - 8.04672).abs() < 1e-9);
        assert!((convert(100.0, u("\u{b0}C"), u("\u{b0}F")) - 212.0).abs() < 1e-9);
        assert!((convert(98.6, u("\u{b0}F"), u("\u{b0}C")) - 37.0).abs() < 1e-9);
        assert!((convert(0.0, u("\u{b0}C"), u("K")) - 273.15).abs() < 1e-9);
        assert!((convert(1.0, u("GiB"), u("MiB")) - 1024.0).abs() < 1e-9);
        assert!((convert(60.0, u("mph"), u("km/h")) - 96.56064).abs() < 1e-6);
    }

    #[test]
    fn number_and_text_formatting() {
        assert_eq!(fmt_num(8.04672), "8.047");
        assert_eq!(fmt_num(26400.0), "26400");
        assert_eq!(fmt_num(0.012345), "0.0123");
        assert_eq!(fmt_num(1.0), "1");
        assert_eq!(fmt_num(-0.00001), "0");
        let km = UNITS.iter().find(|u| u.abbr == "km").unwrap();
        assert_eq!(render(8.04672, km, true, true), "8.047 kilometres");
        assert_eq!(render(1.0, km, true, true), "1 kilometre");
        assert_eq!(render(8.04672, km, false, false), "8.047km");
        let f = UNITS.iter().find(|u| u.abbr == "\u{b0}F").unwrap();
        assert_eq!(render(212.0, f, false, false), "212\u{b0}F");
    }

    #[test]
    fn offers_sensible_conversions() {
        let q = find("5 miles", 3).unwrap();
        let c: Vec<_> = conversions(&q, q.candidates[0]).iter().map(|c| name(c.0)).collect();
        assert!(c.contains(&"km") && c.contains(&"m") && c.contains(&"ft"));
        assert!(!c.contains(&"mi"));
        assert!(!c.contains(&"mm"), "8 million mm is not useful: {c:?}");
    }
}
