//! The splash drawn over an empty, unnamed, untouched buffer (after Spacemacs' startup page).

use crate::pane::Rect;
use crate::skin::{self, Role};
use crate::chrome_model::InfoBlock;
use meridian_term::{Screen, Style, BOLD};

pub const TAG: &str = "your new notes system";
/// Repeated for the faint ornament rows above and below the logo (shapes the bundled GUI font has).
const ORNAMENT: [char; 4] = ['◇', '◆', '◈', '○'];
pub const TABS: [&str; 4] = ["About", "Features", "FAQ", "Keys"];
pub const BLURB: &str = "Meridian is your notes system: notebooks, sections and pages in plain files, linked together, \
with study cards, a graph of your notes and an endless canvas, quick to open and fully themable.";
enum Page {
	Text(&'static [&'static str]),
	/// (question, answer) pairs; the question is drawn in the accent colour.
	Faq(&'static [(&'static str, &'static str)]),
	/// Bulleted (name, description) lists under a heading per mode.
	Sections(&'static [(&'static str, &'static [(&'static str, &'static str)])]),
	/// (left, right) pairs laid out in aligned columns; the left side is drawn in the accent colour.
	Table(&'static [(&'static str, &'static str)]),
}
const PAGES: [Page; 4] = [
	Page::Text(&[
		"Meridian is intended to bring together the best ideas into one program. It keeps your notes as plain text as notebooks, sections and pages.",
		"You can link to other documents like [[this]], and embed documents like {{this}}. Doing this will connect them in the graph view if you have that enabled.",
		"Still a WIP, so ignore the odd bits and bobs there.",
		"Send bug reports/feature requests here (https://discord.gg/RhE8GavPUT), or drop a pull request. Have fun!",
		"- Ben",
	]),
	Page::Sections(&[
		(
			"Every mode",
			&[
				("Cursors", "multiple carets, column select, move and duplicate lines"),
				("Search", "regex, regex builder, find in files, replace"),
				("Navigation", "go to anything, symbols, markers, jump history"),
				("Layout", "splits, file tree, minimap, sticky scroll, tabs"),
				("History", "scrubber over every edit, save and git commit"),
				("Automation", "macros, snippets, templates, plugins in any language"),
				("Safety", "swap files, encodings, line endings, huge files"),
				("Look", "themes, panel borders, fullscreen, optional Vim keys"),
			],
		),
		(
			"Writing mode",
			&[
				("Prose lint", "adverbs, passive voice, hard-to-read sentences"),
				("Word stats", "counts and reading time in the status bar"),
				("Speed reader", "flash a document word by word"),
				("Wrap width", "wrap at 72, 80, 100, 120 or a custom column"),
				("Thesaurus", "synonyms for the word at the caret (plugin)"),
			],
		),
		(
			"Coding mode",
			&[
				("Table view", "CSV and TSV as an aligned grid, edit by cell"),
				("Data views", "JSON, YAML and TOML checked as you type"),
				("Folding", "collapse blocks, fold all, unfold all"),
				("Comments", "toggle by language for the touched lines"),
				("LSP", "diagnostics, hover, completion, definitions, rename"),
				("Builds", "run cargo, make, npm or go and jump to errors"),
				("Eval blocks", "run fenced code in a document (plugin)"),
			],
		),
		(
			"Study mode",
			&[
				("Marking", "wrap words in `code`, _italic_ or __bold__"),
				("Bookmarks", "drop [[bookmark]] markers and jump between them"),
				("Word select", "move and select a word at a time"),
				("Read-only typing", "annotate without changing the text"),
			],
		),
	]),
	Page::Faq(&[
		("How do I open a file?", "Ctrl+O for the file dialog, or Ctrl+P to go to anything."),
		("Where is the config?", "~/.config/meridian."),
		("Can I use Vim keys?", "Yes, turn on Vim mode in the settings."),
		("How do I switch modes?", "Alt+M opens the mode picker: Coding, Study and Writing."),
		("How do I rebind a key?", "Edit keys.conf; every command is in the palette."),
		("How do I write a plugin?", "See PLUGINS.md; a plugin is any executable."),
		("Will I lose work on a crash?", "Swap files and history guard unsaved changes."),
	]),
	Page::Table(&[
		("Ctrl+P", "go to anything"),
		("Ctrl+Shift+P", "command palette"),
		("Ctrl+O", "open a file"),
		("Ctrl+G", "go to line"),
		("Ctrl+R", "symbol in file"),
		("Ctrl+Shift+F", "find in files"),
		("Ctrl+D", "add next occurrence"),
		("Alt+D", "select all whole words"),
		("Ctrl+Alt+Up/Down", "add caret above/below"),
		("Ctrl+L", "select line"),
		("Ctrl+Shift+Up/Down", "move line"),
		("Ctrl+/", "toggle comment"),
		("Ctrl+] / Ctrl+[", "indent / unindent"),
		("Alt+Up/Down", "previous / next marker"),
		("Alt+- / Alt+=", "jump back / forward"),
		("F9", "fold block"),
		("Alt+U", "list URLs and emoji"),
		("Ctrl+H", "history scrubber"),
		("Ctrl+E", "wrap width"),
		("Alt+M", "pick a mode and view"),
		("Alt+X / Alt+Shift+X", "record / play macro"),
		("Alt+P", "add this file's folder to the project"),
		("Ctrl+B", "build"),
		("Ctrl+K", "convert a unit, swap a word, pick a #hex colour; else insert menu"),
		("Alt+K", "same as Ctrl+K; else hover docs"),
		("Ctrl+.", "tree / emoji toggle"),
		("Tab", "next friday / +10 days becomes a date"),
		("Alt+Enter", "follow [[link]]"),
		("Ctrl+'", "right sidebar (backlinks, words)"),
		("F11", "fullscreen"),
		("Alt+.", "hide interface"),
	]),
];
/// The info page `tab` as blocks for the GUI, which wraps and draws them.
pub fn blocks(tab: usize) -> Vec<InfoBlock> {
	let mut out = Vec::new();
	match &PAGES[tab.min(PAGES.len() - 1)] {
		Page::Text(ps) => out.extend(ps.iter().map(|p| InfoBlock::Para(p.to_string()))),
		Page::Faq(qs) => {
			for (q, a) in qs.iter() {
				out.push(InfoBlock::Question(q.to_string()));
				out.push(InfoBlock::Answer(a.to_string()));
			}
		}
		Page::Sections(secs) => {
			for (head, items) in secs.iter() {
				out.push(InfoBlock::Heading(head.to_string()));
				out.extend(items.iter().map(|(k, d)| InfoBlock::Item(k.to_string(), d.to_string())));
			}
		}
		Page::Table(rows) => out.extend(rows.iter().map(|(k, d)| InfoBlock::Key(k.to_string(), d.to_string()))),
	}
	out
}

/// Colour of a run of text on the info page.
#[derive(Clone, Copy)]
enum Kind {
	Text,
	Accent,
	Label,
	Dim,
}
type Line = Vec<(String, Kind)>;
/// What the home screen's bottom line says the first time it is ever shown.
pub const FIRST_TIP: &str = "Ctrl+O open  ·  Ctrl+P go to files";

/// Short tips, one shown at random on each later home screen.
pub const TIPS: &[&str] = &[
	"Alt+M switches mode, and the view inside Coding: table, JSON, YAML, TOML",
	"Ctrl+K on a month, weekday, number or time swaps it for its neighbours",
	"Type next friday or +10 days and press Tab to get the date",
	"Ctrl+K on a unit like 5 miles or 12kg converts it",
	"Ctrl+K or Alt+K on a #hex code opens a colour picker",
	"Ctrl+Shift+P lists the commands you use most first",
	"Ctrl+H scrubs back through every edit, save and commit of a file",
	"Ctrl+D adds the next match as a caret; Alt+D selects every whole word",
	"Ctrl+P jumps to any file; start with : for a line, @ for a symbol",
	"Ctrl+Shift+F searches every file in the project; Alt+A replaces in all",
	"Undo says what it undid: Undid: replace all [14 changes]",
	"[[ opens a picker of notes to link; Alt+Enter follows the link",
	"{{note}} on its own line shows that note inside this one",
	"Alt+R flashes the text word by word in the speed reader",
	"F8 dims everything but the current paragraph or sentence",
	"Ctrl+E picks a wrap width: 72, 80, 100, 120 or the window",
	"Alt+X records a macro, Alt+Shift+X plays it back; Alt+P adds this file's folder to the project",
	"F9 folds a block; Shift+F9 folds all, Alt+F9 unfolds all",
	"Ctrl+N starts a new file from a template; Settings manages them",
	"Alt+U lists the URLs and emoji in the file",
	"Alt+K shows docs for the word under the caret when a language server runs",
	"Ctrl+B builds the project; F4 jumps through the errors",
	"Settings has a Key bindings row: pick a command, press its new key",
	"Ctrl+/ comments the touched lines in the file's language",
	"Alt+Up and Alt+Down jump between markers and headings",
	"Ctrl+' shows the notes that link to this one",
	"Alt+. hides everything but the text",
	"Alt+I shows the picture under the caret, or the image file",
	"Ctrl+Shift+Up and Down move the lines under the caret",
	"The Settings search finds a setting by what it does, not just by name",
];

/// The bottom line of the home screen: the key hint the first time, a random tip afterwards.
pub fn tip(first: bool, seed: u64) -> &'static str {
	if first {
		return FIRST_TIP;
	}
	// A small mix so close seeds (start times in ms) do not pick neighbouring tips.
	let h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 33;
	TIPS[h as usize % TIPS.len()]
}

/// Where the clickable parts landed: the recent files and folders.
pub struct Hits {
	pub recent: Vec<Rect>,
	/// How many of `recent` are files (the rest are folders).
	pub files: usize,
	/// The "Due today" line.
	pub due: Option<Rect>,
}

/// The width from which the home screen shows two columns.
const TWO_COLS: usize = 90;

/// Draws centred in `r`, skipping whatever does not fit, with the title at heading-1 size `h1` (None in a terminal) and
/// the `files` then `dirs` labels listed below (the `rsel`-th of both together highlighted, as far
/// as they fit). `Hits::recent` lists the shown files followed by the shown folders.
#[allow(clippy::too_many_arguments)]
pub fn draw(sc: &mut Screen, r: Rect, bg: Option<u32>, h1: Option<f32>, files: &[String], dirs: &[String], rsel: Option<usize>, tip: &str, due: Option<&str>) -> Hits {
	let none = || Hits { recent: Vec::new(), files: 0, due: None };
	let dim = Style::new(skin::colour(Role::WelcomeDim), bg, 0);
	let ver = format!("v{} ", env!("CARGO_PKG_VERSION"));
	let vw = ver.chars().count();
	if r.w > vw {
		sc.text(r.x + r.w - vw, r.y, &ver, dim, vw);
	}
	let body = 8;
	if r.h < body + 1 || r.w < 20 {
		return none();
	}
	// From 90 columns the recents sit beside the tip; narrower, everything stacks in one centred column.
	let two = r.w >= TWO_COLS;
	// Header rows: ornaments, logo, tagline (and the centred tip when stacked).
	let head_h = if two { body - 1 } else { body + 1 };
	let budget = r.h.saturating_sub(head_h + 1 + 1);
	let (nfile, ndir) = if two {
		let nf = files.len().min(10).min(budget.saturating_sub(1));
		let rest = budget.saturating_sub(if nf > 0 { nf + 2 } else { 0 });
		(nf, if rest >= 3 { dirs.len().min(6).min(rest - 2) } else { 0 })
	} else {
		// Under the hint when there is room (blank row, title row, then one row each).
		let nf = if files.is_empty() { 0 } else { files.len().min(r.h.saturating_sub(body + 2 + 2)) };
		let used = body + if nf > 0 { nf + 2 } else { 0 };
		(nf, if dirs.is_empty() { 0 } else { dirs.len().min(r.h.saturating_sub(used + 2 + 2)) })
	};
	let left_h = if nfile > 0 { nfile + 1 } else { 0 } + if ndir > 0 { ndir + 1 + (nfile > 0) as usize } else { 0 };
	let cols_h = if two { left_h.max(3) } else { 0 };
	let body = if two {
		head_h + 1 + cols_h
	} else {
		let used = body + if nfile > 0 { nfile + 2 } else { 0 };
		used + if ndir > 0 { ndir + 2 } else { 0 }
	};
	let mut y = r.y + r.h.saturating_sub(body) / 2;
	let top = y;
	let center = |sc: &mut Screen, y: usize, s: &str, st: Style| {
		let n = s.chars().count();
		sc.text(r.x + r.w.saturating_sub(n) / 2, y, s, st, r.w);
	};
	let logo = Style::new(Some(crate::editview::accent()), bg, BOLD);
	// Barely-there rows of ornaments, edge to edge: `◇ · ◆ · ◈ · ○ · ...`.
	let faint = Style::new(skin::colour(Role::WelcomeFaint), bg, 0);
	let row: String = (0..r.w).map(|i| if i % 2 == 1 { ' ' } else if i % 4 == 0 { '·' } else { ORNAMENT[i / 4 % 4] }).collect();
	sc.text(r.x, y, &row, faint, r.w);
	y += 2;
	// A scaled title is centred by its drawn width; the spacer rows around it shrink so the rows below stay put.
	let k = h1.unwrap_or(1.0).max(1.0);
	let n = "Meridian".chars().count();
	let x = r.x + r.w.saturating_sub((n as f32 * k).ceil() as usize) / 2;
	sc.text(x, y, "Meridian", logo, r.w);
	if k > 1.0 {
		sc.set_row_scale_from(y, k, x);
		let spacer = (1.0 - (k - 1.0) / 2.0).max(0.0);
		sc.set_row_scale(y - 1, spacer);
		sc.set_row_scale(y + 1, spacer);
	}
	y += 2;
	sc.text(r.x, y, &row, faint, r.w);
	y += 2;
	center(sc, y, TAG, Style::new(skin::colour(Role::WelcomeTag), bg, 0));
	y += 2;
	// The blank row under the tagline holds the "Due today" line.
	let mut due_hit = None;
	if let Some(d) = due {
		let n = d.chars().count().min(r.w);
		let x = r.x + r.w.saturating_sub(n) / 2;
		sc.text(x, y - 1, d, Style::new(Some(crate::editview::accent()), bg, 0), n);
		due_hit = Some(Rect { x, y: y - 1, w: n, h: 1 });
	}
	let label = Style::new(Some(crate::editview::accent_light()), bg, BOLD);
	let on = Style::new(skin::colour(Role::StatusFg), Some(crate::editview::accent()), BOLD);
	let head = Style::new(Some(crate::editview::accent()), bg, BOLD);
	let mut recent_hits = Vec::new();
	let mut shown = 0;
	if two {
		let lw = files.iter().chain(dirs).map(|l| l.chars().count()).max().unwrap_or(0).clamp(30, 46);
		let hint_w = 38;
		let total = lw + 6 + hint_w;
		let x0 = r.x + r.w.saturating_sub(total) / 2;
		let mut ly = y;
		for (title, list) in [("Recent files  (\u{2193} to choose)", &files[..nfile]), ("Recent folders", &dirs[..ndir])] {
			if list.is_empty() {
				continue;
			}
			if ly > y {
				ly += 1;
			}
			sc.text(x0, ly, title, head, lw);
			for l in list {
				ly += 1;
				let st = if rsel == Some(shown) { on } else { label };
				// A long path keeps its tail, which is the part that tells files apart.
				let n = l.chars().count();
				let cut: String = if n > lw { std::iter::once('\u{2026}').chain(l.chars().skip(n - lw + 1)).collect() } else { l.clone() };
				sc.text(x0, ly, &cut, st, lw);
				recent_hits.push(Rect { x: x0, y: ly, w: cut.chars().count(), h: 1 });
				shown += 1;
			}
			ly += 1;
		}
		let rx = x0 + lw + 6;
		sc.text(rx, y, "Hint", head, hint_w);
		for (i, l) in wrap(tip, hint_w).iter().take(cols_h.saturating_sub(2)).enumerate() {
			sc.text(rx, y + 2 + i, l, dim, hint_w);
		}
		let _ = top;
		return Hits { recent: recent_hits, files: nfile, due: due_hit };
	}
	center(sc, y, tip, dim);
	for (title, list) in [("Recent files  (\u{2193} to choose)", &files[..nfile]), ("Recent folders", &dirs[..ndir])] {
		if list.is_empty() {
			continue;
		}
		y += 2;
		center(sc, y, title, dim);
		let width = list.iter().map(|l| l.chars().count()).max().unwrap_or(0).min(r.w.saturating_sub(4));
		let x = r.x + r.w.saturating_sub(width) / 2;
		for l in list {
			y += 1;
			let st = if rsel == Some(shown) { on } else { label };
			sc.text(x, y, l, st, width);
			recent_hits.push(Rect { x, y, w: width, h: 1 });
			shown += 1;
		}
	}
	Hits { recent: recent_hits, files: nfile, due: due_hit }
}

/// Greedy word wrap to `w` columns.
pub fn wrap(text: &str, w: usize) -> Vec<String> {
	let mut out = Vec::new();
	let mut cur = String::new();
	for word in text.split_whitespace() {
		let n = cur.chars().count();
		if n > 0 && n + 1 + word.chars().count() > w {
			out.push(std::mem::take(&mut cur));
		}
		if !cur.is_empty() {
			cur.push(' ');
		}
		cur.push_str(word);
	}
	if !cur.is_empty() {
		out.push(cur);
	}
	out
}

/// The single-column pages as styled lines.
fn lines(page: &Page, w: usize) -> Vec<Line> {
	let mut out: Vec<Line> = Vec::new();
	let blank = |out: &mut Vec<Line>| out.push(Vec::new());
	match page {
		Page::Text(ps) => {
			for p in ps.iter() {
				out.extend(wrap(p, w).into_iter().map(|l| vec![(l, Kind::Text)]));
				blank(&mut out);
			}
		}
		Page::Faq(qs) => {
			for (q, a) in qs.iter() {
				out.extend(wrap(q, w).into_iter().map(|l| vec![(l, Kind::Accent)]));
				out.extend(wrap(a, w.saturating_sub(2)).into_iter().map(|l| vec![(format!("  {l}"), Kind::Text)]));
				blank(&mut out);
			}
		}
		Page::Sections(secs) => {
			for (head, items) in secs.iter() {
				out.push(vec![(head.to_string(), Kind::Accent)]);
				let kw = items.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
				for (k, d) in items.iter() {
					out.push(vec![("  • ".into(), Kind::Dim), (format!("{k:kw$}  "), Kind::Label), (d.to_string(), Kind::Text)]);
				}
				blank(&mut out);
			}
		}
		Page::Table(_) => {}
	}
	out.pop_if(|l| l.is_empty());
	out
}

/// The `?` page: the title gains a `?`, a description follows, then tabs and the chosen tab's text.
/// `scroll` is the first visible row of long pages; it is clamped here. Returns the tab rectangles.
pub fn draw_info(sc: &mut Screen, r: Rect, bg: Option<u32>, tab: usize, scroll: &mut usize) -> Vec<Rect> {
	let dim = Style::new(skin::colour(Role::WelcomeDim), bg, 0);
	let faint = Style::new(skin::colour(Role::WelcomeFaint), bg, 0);
	let page = &PAGES[tab.min(PAGES.len() - 1)];
	let ncol = if matches!(page, Page::Table(_)) && r.w >= 100 { 2 } else { 1 };
	let tw = r.w.saturating_sub(6).clamp(10, if ncol == 2 { 108 } else { 72 });
	let blurb = wrap(BLURB, tw.min(72));
	let body = lines(page, tw);
	let cols: Vec<&[(&str, &str)]> = match page {
		Page::Table(t) => t.chunks(t.len().div_ceil(ncol)).collect(),
		_ => Vec::new(),
	};
	let total = if cols.is_empty() { body.len() } else { cols[0].len() };
	// Rows besides the body: ornament, gap, logo, gap, ornament, gap, blurb, gap, tabs, gap, gap, hint.
	let fixed = 1 + 1 + 1 + 1 + 1 + 1 + blurb.len() + 1 + 1 + 1 + 1 + 1;
	if r.w < 20 || r.h < fixed + 3 {
		return Vec::new();
	}
	let rows = total.clamp(3, r.h - fixed);
	*scroll = (*scroll).min(total.saturating_sub(rows));
	let top = r.y + (r.h - (fixed + rows)) / 2;
	let mut y = top;
	let center = |sc: &mut Screen, y: usize, s: &str, st: Style| {
		let n = s.chars().count();
		sc.text(r.x + r.w.saturating_sub(n) / 2, y, s, st, r.w);
	};
	let ornaments: String = (0..r.w).map(|i| if i % 2 == 1 { ' ' } else if i % 4 == 0 { '·' } else { ORNAMENT[i / 4 % 4] }).collect();
	sc.text(r.x, y, &ornaments, faint, r.w);
	y += 2;
	let st = Style::new(Some(crate::editview::accent()), bg, BOLD);
	center(sc, y, "MERIDIAN NOTES?", st);
	y += 2;
	sc.text(r.x, y, &ornaments, faint, r.w);
	y += 2;
	for l in &blurb {
		center(sc, y, l, Style::new(skin::colour(Role::WelcomeTag), bg, 0));
		y += 1;
	}
	y += 1;
	let tabs_total: usize = TABS.iter().map(|m| m.chars().count() + 2).sum::<usize>() + TABS.len() - 1;
	let mut x = r.x + r.w.saturating_sub(tabs_total) / 2;
	let label = Style::new(Some(crate::editview::accent_light()), bg, BOLD);
	let on = Style::new(skin::colour(Role::StatusFg), Some(crate::editview::accent()), BOLD);
	let mut hits = Vec::new();
	for (i, m) in TABS.iter().enumerate() {
		let x1 = x;
		let (b, l) = if i == tab { (on, on) } else { (dim, label) };
		x = sc.text(x, y, "[", b, r.w);
		x = sc.text(x, y, m, l, r.w);
		x = sc.text(x, y, "]", b, r.w);
		hits.push(Rect { x: x1, y, w: x - x1, h: 1 });
		x += 1;
	}
	y += 2;
	let left = r.x + r.w.saturating_sub(tw) / 2;
	let text = Style::new(skin::colour(Role::WelcomeText), bg, 0);
	let accent = Style::new(Some(crate::editview::accent()), bg, BOLD);
	for (i, line) in body.iter().skip(*scroll).take(rows).enumerate() {
		let mut x = left;
		for (t, k) in line {
			let st = match k {
				Kind::Text => text,
				Kind::Accent => accent,
				Kind::Label => label,
				Kind::Dim => dim,
			};
			x = sc.text(x, y + i, t, st, (left + tw).saturating_sub(x));
		}
	}
	// Each column: the left cells padded to one width so the right cells line up.
	let gap = 3;
	let cw = (tw.saturating_sub(gap * (cols.len().saturating_sub(1)))) / cols.len().max(1);
	for (ci, col) in cols.iter().enumerate() {
		let kw = col.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
		let x = left + ci * (cw + gap);
		for (i, (k, d)) in col.iter().skip(*scroll).take(rows).enumerate() {
			sc.text(x, y + i, k, accent, kw.min(cw));
			sc.text(x + kw + 2, y + i, d, text, cw.saturating_sub(kw + 2));
		}
	}
	// Scroll cues on the right edge of the text.
	let edge = left + tw;
	if *scroll > 0 && edge < r.x + r.w {
		sc.text(edge, y, "▲", dim, 1);
	}
	if *scroll + rows < total && edge < r.x + r.w {
		sc.text(edge, y + rows - 1, "▼", dim, 1);
	}
	let hint = if total > rows { "←/→ switch tab  ·  ↑/↓ scroll  ·  Esc back" } else { "←/→ switch tab  ·  Esc back" };
	center(sc, top + fixed + rows - 1, hint, dim);
	hits
}

#[cfg(test)]
mod tests {
	use super::*;

	fn text_of(sc: &Screen, y: usize) -> String {
		(0..sc.size().0).map(|x| sc.cell(x, y).0).collect()
	}

	#[test]
	fn title_is_plain_scaled_and_menu_is_gone() {
		let mut sc = Screen::new(100, 30);
		draw(&mut sc, Rect { x: 0, y: 0, w: 100, h: 30 }, None, Some(1.6), &[], &[], None, "tip", None);
		let rows: Vec<String> = (0..30).map(|y| text_of(&sc, y)).collect();
		let y = rows.iter().position(|r| r.contains("Meridian") && !r.contains("notes system")).expect("title row");
		assert!(!rows.iter().any(|r| r.contains("[Review]") || r.contains("Stats") || r.contains("MERIDIAN")));
		assert_eq!(sc.row_scale(y), 1.6);
		assert!(sc.row_scale(y - 1) < 1.0 && sc.row_scale(y + 1) < 1.0, "spacers give the height back");
		assert!(rows.iter().any(|r| r.contains('◇')), "ornaments use shapes the GUI font has");
	}
}
