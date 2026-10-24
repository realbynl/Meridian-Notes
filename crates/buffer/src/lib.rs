//! Rope-backed text buffer with multi-range selections and linear undo.
//!
//! All offsets are `char` indices into the rope. Selections are a sorted,
//! non-overlapping list of ranges. Edits are applied in reverse offset order so
//! earlier offsets stay valid while a batch is applied.

pub mod crdt;

use ropey::Rope;
use std::collections::VecDeque;
use std::fs;
use std::borrow::Cow;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};
use unicode_width::UnicodeWidthChar;

/// Undo depth cap (transactions). Bounds memory on long sessions.
pub const MAX_UNDO: usize = 1000;
/// Undo history cap in bytes of stored text. The newest transaction is always kept.
pub const MAX_UNDO_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
}

impl Range {
    pub fn caret(p: usize) -> Self {
        Range { anchor: p, head: p }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// Replace `del` chars at `at` with `ins`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub at: usize,
    pub del: usize,
    pub ins: String,
}

/// Used to decide whether consecutive edits merge into one undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Other,
}

#[derive(Clone, Debug)]
struct Change {
    at: usize,
    removed: String,
    rlen: usize,
    inserted: String,
    ilen: usize,
}

#[derive(Clone, Debug)]
struct Txn {
    /// Chronological: undo walks this backwards, redo forwards.
    changes: Vec<Change>,
    sel_before: Vec<Range>,
    sel_after: Vec<Range>,
    /// What the step was ("replace all"), for the undo toast; inferred from the changes when unset.
    label: Option<String>,
}

impl Txn {
    fn describe(&self) -> String {
        if let Some(l) = &self.label {
            return l.clone();
        }
        let plural = |n: usize, w: &str| if n == 1 { format!("{n} {w}") } else { format!("{n} {w}s") };
        match self.changes.as_slice() {
            [c] if c.rlen == 0 => format!("typing [{}]", plural(c.ilen, "char")),
            [c] if c.ilen == 0 => format!("delete [{}]", plural(c.rlen, "char")),
            [c] => format!("replace [{} for {}]", plural(c.rlen, "char"), plural(c.ilen, "char")),
            cs => format!("edit [{}]", plural(cs.len(), "change")),
        }
    }

    fn bytes(&self) -> usize {
        self.changes
            .iter()
            .map(|c| c.removed.len() + c.inserted.len())
            .sum()
    }
}

/// How the file's bytes map to text. Text in the rope is always Unicode; this is what save writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// Decoded losslessly from non-UTF-8 bytes; saved back as Latin-1 until an edit adds a char
    /// above U+00FF, at which point it is saved as UTF-8.
    Latin1,
}

impl Encoding {
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 BOM",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Latin1 => "Latin-1",
        }
    }
}

/// Line ending written on save. The rope itself only ever holds `\n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
    Cr,
}

impl LineEnding {
    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::Crlf => "CRLF",
            LineEnding::Cr => "CR",
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::Crlf => "\r\n",
            LineEnding::Cr => "\r",
        }
    }
}

pub struct Buffer {
    rope: Rope,
    sels: Vec<Range>,
    undo: VecDeque<Txn>,
    redo: Vec<Txn>,
    undo_bytes: usize,
    path: Option<PathBuf>,
    /// Number of applied transactions; `save_point` is the depth at last save.
    depth: usize,
    save_point: Option<usize>,
    group_open: bool,
    last_kind: EditKind,
    pub encoding: Encoding,
    pub eol: LineEnding,
    /// The file mixed line endings; they were unified to `eol` on load, and save writes `eol` only.
    pub mixed_eol: bool,
    /// Modified time and length of the file as last read or written here (external change detection).
    stamp: Option<(std::time::SystemTime, u64)>,
    /// Bumped on every content change (edit, undo, redo); lets callers cache derived data.
    version: u64,
}

fn disk_stamp(p: &Path) -> Option<(std::time::SystemTime, u64)> {
    let m = fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
}

fn decode_utf16(bytes: &[u8], enc: Encoding) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units = bytes.chunks_exact(2).map(|p| match enc {
        Encoding::Utf16Be => u16::from_be_bytes([p[0], p[1]]),
        _ => u16::from_le_bytes([p[0], p[1]]),
    });
    char::decode_utf16(units).collect::<Result<String, _>>().ok()
}

/// `offset  hex bytes  |ascii|`, 16 bytes a line.
pub fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4);
    for (i, row) in bytes.chunks(16).enumerate() {
        out.push_str(&format!("{:08x} ", i * 16));
        for j in 0..16 {
            match row.get(j) {
                Some(b) => out.push_str(&format!(" {b:02x}")),
                None => out.push_str("   "),
            }
            if j == 7 {
                out.push(' ');
            }
        }
        out.push_str("  |");
        out.extend(row.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }));
        out.push_str("|\n");
    }
    out
}

fn is_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

impl Default for Buffer {
    fn default() -> Self {
        Self::from_rope(Rope::new())
    }
}

impl Buffer {
    fn from_rope(rope: Rope) -> Self {
        Buffer {
            rope,
            sels: vec![Range::caret(0)],
            undo: VecDeque::new(),
            redo: Vec::new(),
            undo_bytes: 0,
            path: None,
            depth: 0,
            save_point: Some(0),
            group_open: false,
            last_kind: EditKind::Other,
            encoding: Encoding::Utf8,
            eol: LineEnding::Lf,
            mixed_eol: false,
            stamp: None,
            version: 0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        Self::from_rope(Rope::from_str(s))
    }

    /// Stream a file into the rope. A BOM picks UTF-8 BOM or UTF-16; invalid UTF-8 falls back to a
    /// lossless Latin-1 decode. CRLF and CR are turned into `\n` in the rope and remembered in `eol`.
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = fs::File::open(path)?;
        let mut head = [0u8; 3];
        let mut n = 0;
        while n < head.len() {
            match file.read(&mut head[n..])? {
                0 => break,
                k => n += k,
            }
        }
        let utf16 = match head[..n] {
            [0xff, 0xfe, ..] => Some(Encoding::Utf16Le),
            [0xfe, 0xff, ..] => Some(Encoding::Utf16Be),
            _ => None,
        };
        let bom8 = head[..n] == [0xef, 0xbb, 0xbf];
        let mut b = if let Some(enc) = utf16 {
            let mut bytes = Vec::new();
            file.seek(SeekFrom::Start(2))?;
            file.read_to_end(&mut bytes)?;
            let text = decode_utf16(&bytes, enc)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-16"))?;
            let mut b = Self::from_rope(Rope::from_str(&text));
            b.encoding = enc;
            b
        } else {
            file.seek(SeekFrom::Start(if bom8 { 3 } else { 0 }))?;
            match Rope::from_reader(BufReader::with_capacity(1 << 16, file)) {
                Ok(rope) => {
                    let mut b = Self::from_rope(rope);
                    if bom8 {
                        b.encoding = Encoding::Utf8Bom;
                    }
                    b
                }
                Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                    let mut bytes = Vec::new();
                    fs::File::open(path)?.read_to_end(&mut bytes)?;
                    let text: String = bytes.iter().map(|&c| c as char).collect();
                    let mut b = Self::from_rope(Rope::from_str(&text));
                    b.encoding = Encoding::Latin1;
                    b
                }
                Err(e) => return Err(e),
            }
        };
        b.unify_eol();
        b.path = Some(path.to_path_buf());
        b.stamp = disk_stamp(path);
        Ok(b)
    }

    /// Detect the dominant line ending (ties prefer LF, then CRLF) and rewrite the rope to `\n`.
    fn unify_eol(&mut self) {
        let (mut lf, mut crlf, mut cr, mut prev) = (0usize, 0usize, 0usize, 0u8);
        for chunk in self.rope.chunks() {
            for &c in chunk.as_bytes() {
                if c == b'\n' {
                    if prev == b'\r' {
                        crlf += 1
                    } else {
                        lf += 1
                    }
                } else if prev == b'\r' {
                    cr += 1
                }
                prev = c;
            }
        }
        if prev == b'\r' {
            cr += 1;
        }
        if crlf + cr == 0 {
            return;
        }
        let mut best = (lf, LineEnding::Lf);
        for k in [(crlf, LineEnding::Crlf), (cr, LineEnding::Cr)] {
            if k.0 > best.0 {
                best = k;
            }
        }
        self.eol = best.1;
        self.mixed_eol = [lf, crlf, cr].iter().filter(|&&n| n > 0).count() > 1;
        let text = self.rope.to_string().replace("\r\n", "\n").replace('\r', "\n");
        self.rope = Rope::from_str(&text);
    }

    /// Does the file start with NUL bytes' worth of binary data (and is not UTF-16 with a BOM)?
    pub fn looks_binary(path: &Path) -> io::Result<bool> {
        let mut head = Vec::new();
        fs::File::open(path)?.take(8192).read_to_end(&mut head)?;
        if head.starts_with(&[0xff, 0xfe]) || head.starts_with(&[0xfe, 0xff]) {
            return Ok(false);
        }
        Ok(head.contains(&0))
    }

    /// A read-only hex dump of the first `max` bytes of a file, as text (see `hex_dump`).
    pub fn hex_view(path: &Path, max: u64) -> io::Result<(Self, u64)> {
        let size = fs::metadata(path)?.len();
        let mut bytes = Vec::new();
        fs::File::open(path)?.take(max).read_to_end(&mut bytes)?;
        let mut b = Self::from_str(&hex_dump(&bytes));
        b.mark_clean();
        Ok((b, size))
    }

    /// Write via a temp file in the same directory, then rename.
    pub fn save(&mut self) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no file name"))?;
        self.save_as(&path)
    }

    /// The encoding save would use: Latin-1 text with a char above U+00FF can only go out as UTF-8.
    fn effective_encoding(&self) -> Encoding {
        if self.encoding == Encoding::Latin1 && !self.rope.chars().all(|c| (c as u32) < 256) {
            Encoding::Utf8
        } else {
            self.encoding
        }
    }

    fn write_encoded<W: Write>(&self, w: &mut W, enc: Encoding) -> io::Result<()> {
        match enc {
            Encoding::Utf8Bom => w.write_all(&[0xef, 0xbb, 0xbf])?,
            Encoding::Utf16Le => w.write_all(&[0xff, 0xfe])?,
            Encoding::Utf16Be => w.write_all(&[0xfe, 0xff])?,
            _ => {}
        }
        let eol = self.eol.as_str();
        for chunk in self.rope.chunks() {
            let s: Cow<str> = if self.eol == LineEnding::Lf { chunk.into() } else { chunk.replace('\n', eol).into() };
            match enc {
                Encoding::Utf8 | Encoding::Utf8Bom => w.write_all(s.as_bytes())?,
                Encoding::Latin1 => w.write_all(&s.chars().map(|c| c as u8).collect::<Vec<_>>())?,
                Encoding::Utf16Le => s.encode_utf16().try_for_each(|u| w.write_all(&u.to_le_bytes()))?,
                Encoding::Utf16Be => s.encode_utf16().try_for_each(|u| w.write_all(&u.to_be_bytes()))?,
            }
        }
        Ok(())
    }

    /// The exact bytes a save would write.
    pub fn encoded(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let _ = self.write_encoded(&mut out, self.effective_encoding());
        out
    }

    /// Change the line ending save writes. The buffer counts as modified until saved.
    pub fn set_eol(&mut self, eol: LineEnding) {
        self.eol = eol;
        self.mixed_eol = false;
        self.save_point = None;
    }

    /// Change the encoding save writes. The buffer counts as modified until saved.
    pub fn set_encoding(&mut self, enc: Encoding) {
        self.encoding = enc;
        self.save_point = None;
    }

    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        // Write through a symlink: renaming over the link would replace it with a regular file.
        let target = if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            fs::canonicalize(path)?
        } else {
            path.to_path_buf()
        };
        let target = target.as_path();
        let mut tmp = target.as_os_str().to_owned();
        tmp.push(".sub-tmp");
        let tmp = PathBuf::from(tmp);
        let enc = self.effective_encoding();
        {
            let f = fs::File::create(&tmp)?;
            let mut w = BufWriter::with_capacity(1 << 16, f);
            self.write_encoded(&mut w, enc)?;
            w.flush()?;
            w.get_ref().sync_all()?;
        }
        if let Ok(meta) = fs::metadata(target) {
            let _ = fs::set_permissions(&tmp, meta.permissions());
        }
        if let Err(e) = fs::rename(&tmp, target) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        self.encoding = enc;
        self.mixed_eol = false;
        self.path = Some(path.to_path_buf());
        self.stamp = disk_stamp(target);
        self.save_point = Some(self.depth);
        self.group_open = false;
        Ok(())
    }

    /// The file on disk was modified (or removed) since this buffer last read or wrote it.
    pub fn disk_changed(&self) -> bool {
        match (&self.path, self.stamp) {
            (Some(p), Some(s)) => disk_stamp(p) != Some(s),
            _ => false,
        }
    }

    /// Accept the disk's current state as the baseline without touching the text (keep my version).
    pub fn keep_disk(&mut self) {
        self.stamp = self.path.as_deref().and_then(disk_stamp);
    }

    /// Replace the text with the file's current content as one undoable edit, and mark it saved.
    pub fn reload(&mut self) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no file name"))?;
        let fresh = Self::open(&path)?;
        let (sels, n) = (self.sels.clone(), self.len_chars());
        self.edit(EditKind::Other, vec![Edit { at: 0, del: n, ins: fresh.rope.to_string() }]);
        self.set_sels(sels);
        self.encoding = fresh.encoding;
        self.eol = fresh.eol;
        self.mixed_eol = fresh.mixed_eol;
        self.stamp = fresh.stamp;
        self.save_point = Some(self.depth);
        self.group_open = false;
        Ok(())
    }

    // ---- accessors -------------------------------------------------------

    pub fn rope(&self) -> &Rope {
        &self.rope
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    /// Name an unsaved buffer (first save writes to this path).
    pub fn set_path(&mut self, p: &Path) {
        self.path = Some(p.to_path_buf());
        self.save_point = None; // a new file is dirty until written
    }
    /// Treat the current text as saved (generated buffers such as build output are never "unsaved").
    pub fn mark_clean(&mut self) {
        self.save_point = Some(self.depth);
    }
    pub fn is_dirty(&self) -> bool {
        self.save_point != Some(self.depth)
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }
    pub fn sels(&self) -> &[Range] {
        &self.sels
    }
    pub fn primary(&self) -> Range {
        self.sels[self.sels.len() - 1]
    }
    pub fn set_sels(&mut self, sels: Vec<Range>) {
        self.sels = normalize(sels, self.len_chars());
        self.group_open = false;
    }

    // ---- line geometry ---------------------------------------------------

    pub fn line_of(&self, pos: usize) -> usize {
        self.rope.char_to_line(pos.min(self.len_chars()))
    }
    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_char(line.min(self.rope.len_lines() - 1))
    }
    /// End of line content, excluding the line break.
    pub fn line_end(&self, line: usize) -> usize {
        let line = line.min(self.rope.len_lines() - 1);
        let start = self.rope.line_to_char(line);
        let l = self.rope.line(line);
        let mut n = l.len_chars();
        while n > 0 && is_break(l.char(n - 1)) {
            n -= 1;
        }
        start + n
    }
    /// Display column of `pos` within its line.
    pub fn display_col(&self, pos: usize, tab: usize) -> usize {
        let line = self.line_of(pos);
        let start = self.line_start(line);
        let mut col = 0;
        for c in self.rope.slice(start..pos).chars() {
            col += cell_width(c, col, tab);
        }
        col
    }
    /// Char offset on `line` closest to display column `col` (not past it).
    pub fn pos_at_col(&self, line: usize, col: usize, tab: usize) -> usize {
        let start = self.line_start(line);
        let end = self.line_end(line);
        let mut cur = 0;
        let mut pos = start;
        for c in self.rope.slice(start..end).chars() {
            let w = cell_width(c, cur, tab);
            if cur + w > col {
                break;
            }
            cur += w;
            pos += 1;
        }
        pos
    }

    pub fn next_grapheme(&self, pos: usize) -> usize {
        let r = &self.rope;
        let len = r.len_chars();
        if pos >= len {
            return len;
        }
        let byte = r.char_to_byte(pos);
        let mut gc = GraphemeCursor::new(byte, r.len_bytes(), true);
        let (mut chunk, mut cb, _, _) = r.chunk_at_byte(byte);
        loop {
            match gc.next_boundary(chunk, cb) {
                Ok(None) => return len,
                Ok(Some(b)) => return r.byte_to_char(b),
                Err(GraphemeIncomplete::NextChunk) => {
                    let (c, s, _, _) = r.chunk_at_byte(cb + chunk.len());
                    chunk = c;
                    cb = s;
                }
                Err(GraphemeIncomplete::PreContext(n)) => {
                    let (c, s, _, _) = r.chunk_at_byte(n - 1);
                    gc.provide_context(c, s);
                }
                Err(_) => return pos + 1,
            }
        }
    }

    pub fn prev_grapheme(&self, pos: usize) -> usize {
        let r = &self.rope;
        if pos == 0 {
            return 0;
        }
        let byte = r.char_to_byte(pos.min(r.len_chars()));
        let mut gc = GraphemeCursor::new(byte, r.len_bytes(), true);
        let (mut chunk, mut cb, _, _) = r.chunk_at_byte(byte.saturating_sub(1));
        loop {
            match gc.prev_boundary(chunk, cb) {
                Ok(None) => return 0,
                Ok(Some(b)) => return r.byte_to_char(b),
                Err(GraphemeIncomplete::PrevChunk) => {
                    let (c, s, _, _) = r.chunk_at_byte(cb - 1);
                    chunk = c;
                    cb = s;
                }
                Err(GraphemeIncomplete::PreContext(n)) => {
                    let (c, s, _, _) = r.chunk_at_byte(n - 1);
                    gc.provide_context(c, s);
                }
                Err(_) => return pos - 1,
            }
        }
    }

    // ---- selection movement ---------------------------------------------

    /// Replace every selection with `f(rope, range)`; result is normalized.
    fn is_word(&self, i: usize) -> bool {
        let c = self.rope.char(i);
        c.is_alphanumeric() || c == '_'
    }

    /// 0 whitespace, 1 word, 2 punctuation: Sublime's word-movement classes.
    fn class(&self, i: usize) -> u8 {
        let c = self.rope.char(i);
        if c.is_whitespace() {
            0
        } else if self.is_word(i) {
            1
        } else {
            2
        }
    }

    /// Ctrl+Right as Sublime does it: skip whitespace, then run to the end of a word or of a
    /// punctuation run.
    pub fn move_word_right(&self, mut pos: usize) -> usize {
        let n = self.len_chars();
        while pos < n && self.class(pos) == 0 {
            pos += 1;
        }
        if pos < n {
            let c = self.class(pos);
            while pos < n && self.class(pos) == c {
                pos += 1;
            }
        }
        pos
    }

    /// Ctrl+Left, mirror of `move_word_right`.
    pub fn move_word_left(&self, mut pos: usize) -> usize {
        while pos > 0 && self.class(pos - 1) == 0 {
            pos -= 1;
        }
        if pos > 0 {
            let c = self.class(pos - 1);
            while pos > 0 && self.class(pos - 1) == c {
                pos -= 1;
            }
        }
        pos
    }

    /// The next word at or after `pos` as (start, end), skipping anything that is not a letter or digit.
    pub fn word_after(&self, mut pos: usize) -> Option<(usize, usize)> {
        let n = self.len_chars();
        while pos < n && !self.is_word(pos) {
            pos += 1;
        }
        if pos >= n {
            return None;
        }
        let mut end = pos;
        while end < n && self.is_word(end) {
            end += 1;
        }
        Some((pos, end))
    }

    /// The nearest word ending at or before `pos` as (start, end).
    pub fn word_before(&self, mut pos: usize) -> Option<(usize, usize)> {
        while pos > 0 && !self.is_word(pos - 1) {
            pos -= 1;
        }
        if pos == 0 {
            return None;
        }
        let mut start = pos;
        while start > 0 && self.is_word(start - 1) {
            start -= 1;
        }
        Some((start, pos))
    }

    /// The word containing or touching `pos`.
    pub fn word_at(&self, pos: usize) -> Option<(usize, usize)> {
        let n = self.len_chars();
        if pos < n && self.is_word(pos) {
            let (s, _) = self.word_before(pos + 1)?;
            let (_, e) = self.word_after(pos)?;
            Some((s, e))
        } else {
            self.word_before(pos).filter(|&(_, e)| e == pos)
        }
    }

    /// Start of the previous word (or the document start).
    pub fn prev_word(&self, pos: usize) -> usize {
        self.word_before(pos).map_or(0, |w| w.0)
    }

    /// End of the next word (or the document end).
    pub fn next_word(&self, pos: usize) -> usize {
        self.word_after(pos).map_or(self.len_chars(), |w| w.1)
    }

    pub fn move_sels(&mut self, f: impl Fn(&Buffer, Range) -> Range) {
        let new: Vec<Range> = self.sels.iter().map(|r| f(self, *r)).collect();
        self.set_sels(new);
    }

    // ---- editing ---------------------------------------------------------

    /// Replace each selection with `text`.
    pub fn insert(&mut self, text: &str) {
        let edits: Vec<Edit> = self
            .sels
            .iter()
            .map(|r| Edit {
                at: r.start(),
                del: r.end() - r.start(),
                ins: text.to_string(),
            })
            .collect();
        let kind = if text.chars().count() == 1 && !text.contains('\n') {
            EditKind::Insert
        } else {
            EditKind::Other
        };
        self.edit(kind, edits);
    }

    pub fn backspace(&mut self) {
        let edits = self.sels.iter().map(|r| {
            if r.is_empty() {
                let s = self.prev_grapheme(r.head);
                Edit {
                    at: s,
                    del: r.head - s,
                    ins: String::new(),
                }
            } else {
                Edit {
                    at: r.start(),
                    del: r.end() - r.start(),
                    ins: String::new(),
                }
            }
        });
        let edits: Vec<Edit> = edits.collect();
        self.edit(EditKind::Delete, edits);
    }

    /// Ctrl+Backspace: delete back to the previous word boundary (a selection is deleted as is).
    pub fn backspace_word(&mut self) {
        let edits: Vec<Edit> = self
            .sels
            .iter()
            .map(|r| {
                let (at, end) = if r.is_empty() { (self.move_word_left(r.head), r.head) } else { (r.start(), r.end()) };
                Edit { at, del: end - at, ins: String::new() }
            })
            .collect();
        self.edit(EditKind::Other, edits);
    }

    /// Ctrl+Delete: delete forward to the next word boundary.
    pub fn delete_word_forward(&mut self) {
        let edits: Vec<Edit> = self
            .sels
            .iter()
            .map(|r| {
                let (at, end) = if r.is_empty() { (r.head, self.move_word_right(r.head)) } else { (r.start(), r.end()) };
                Edit { at, del: end - at, ins: String::new() }
            })
            .collect();
        self.edit(EditKind::Other, edits);
    }

    pub fn delete_forward(&mut self) {
        let edits = self.sels.iter().map(|r| {
            if r.is_empty() {
                let e = self.next_grapheme(r.head);
                Edit {
                    at: r.head,
                    del: e - r.head,
                    ins: String::new(),
                }
            } else {
                Edit {
                    at: r.start(),
                    del: r.end() - r.start(),
                    ins: String::new(),
                }
            }
        });
        let edits: Vec<Edit> = edits.collect();
        self.edit(EditKind::Delete, edits);
    }

    /// Apply `edits` (ascending by offset) as one undo step. Each selection
    /// becomes a caret after its edit's inserted text.
    pub fn edit(&mut self, kind: EditKind, mut edits: Vec<Edit>) {
        edits.retain(|e| e.del > 0 || !e.ins.is_empty());
        if edits.is_empty() {
            return;
        }
        edits.sort_by_key(|e| e.at);
        // Clip overlaps (e.g. two backspacing carets that touch).
        let mut prev_end = 0;
        for e in edits.iter_mut() {
            if e.at < prev_end {
                let cut = prev_end - e.at;
                e.at = prev_end;
                e.del = e.del.saturating_sub(cut);
            }
            prev_end = e.at + e.del;
        }
        self.version += 1;
        let sel_before = self.sels.clone();
        let mut changes = Vec::with_capacity(edits.len());
        for e in edits.iter().rev() {
            let removed = if e.del > 0 {
                let s = self.rope.slice(e.at..e.at + e.del).to_string();
                self.rope.remove(e.at..e.at + e.del);
                s
            } else {
                String::new()
            };
            let ilen = e.ins.chars().count();
            if ilen > 0 {
                self.rope.insert(e.at, &e.ins);
            }
            changes.push(Change {
                at: e.at,
                rlen: e.del,
                removed,
                ilen,
                inserted: e.ins.clone(),
            });
        }
        let mut delta: isize = 0;
        let sels: Vec<Range> = changes
            .iter()
            .rev()
            .map(|c| {
                let p = (c.at as isize + delta) as usize + c.ilen;
                delta += c.ilen as isize - c.rlen as isize;
                Range::caret(p)
            })
            .collect();
        self.sels = normalize(sels, self.rope.len_chars());
        self.record(kind, changes, sel_before);
    }

    fn record(&mut self, kind: EditKind, changes: Vec<Change>, sel_before: Vec<Range>) {
        if !self.redo.is_empty() {
            self.redo.clear();
            if self.save_point.map_or(false, |s| s > self.depth) {
                self.save_point = None;
            }
        }
        if self.group_open && kind == self.last_kind && kind != EditKind::Other {
            if let Some(last) = self.undo.back_mut() {
                let added: usize = changes
                    .iter()
                    .map(|c| c.removed.len() + c.inserted.len())
                    .sum();
                if merge(last, &changes) {
                    self.undo_bytes += added;
                    last.sel_after = self.sels.clone();
                    return;
                }
            }
        }
        let txn = Txn {
            changes,
            sel_before,
            sel_after: self.sels.clone(),
            label: None,
        };
        self.undo_bytes += txn.bytes();
        self.undo.push_back(txn);
        while self.undo.len() > MAX_UNDO
            || (self.undo.len() > 1 && self.undo_bytes > MAX_UNDO_BYTES)
        {
            if let Some(old) = self.undo.pop_front() {
                self.undo_bytes -= old.bytes();
            }
        }
        self.depth += 1;
        self.group_open = true;
        self.last_kind = kind;
    }

    /// Start a new undo step on the next edit (call on cursor moves).
    pub fn break_group(&mut self) {
        self.group_open = false;
    }

    /// Name the most recent undo step (shown by the undo toast).
    pub fn label_last(&mut self, label: &str) {
        if let Some(t) = self.undo.back_mut() {
            t.label = Some(label.to_string());
        }
    }

    /// Plain-words description of the step `undo` would revert.
    pub fn undo_label(&self) -> Option<String> {
        self.undo.back().map(Txn::describe)
    }

    /// Plain-words description of the step `redo` would reapply.
    pub fn redo_label(&self) -> Option<String> {
        self.redo.last().map(Txn::describe)
    }

    pub fn undo(&mut self) -> bool {
        let Some(txn) = self.undo.pop_back() else {
            return false;
        };
        self.undo_bytes -= txn.bytes();
        for c in txn.changes.iter().rev() {
            if c.ilen > 0 {
                self.rope.remove(c.at..c.at + c.ilen);
            }
            if c.rlen > 0 {
                self.rope.insert(c.at, &c.removed);
            }
        }
        self.version += 1;
        self.sels = txn.sel_before.clone();
        self.redo.push(txn);
        self.depth -= 1;
        self.group_open = false;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(txn) = self.redo.pop() else {
            return false;
        };
        for c in &txn.changes {
            if c.rlen > 0 {
                self.rope.remove(c.at..c.at + c.rlen);
            }
            if c.ilen > 0 {
                self.rope.insert(c.at, &c.inserted);
            }
        }
        self.version += 1;
        self.sels = txn.sel_after.clone();
        self.undo_bytes += txn.bytes();
        self.undo.push_back(txn);
        self.depth += 1;
        self.group_open = false;
        true
    }
}

/// Fold a single-change typing/deleting step into the previous one if adjacent.
fn merge(last: &mut Txn, new: &[Change]) -> bool {
    if last.changes.len() != 1 || new.len() != 1 {
        return false;
    }
    let (l, n) = (&mut last.changes[0], &new[0]);
    if n.rlen == 0 && l.rlen == 0 && n.at == l.at + l.ilen {
        // typing forward
        l.inserted.push_str(&n.inserted);
        l.ilen += n.ilen;
        true
    } else if n.ilen == 0 && l.ilen == 0 && n.at + n.rlen == l.at {
        // backspacing
        l.at = n.at;
        l.removed.insert_str(0, &n.removed);
        l.rlen += n.rlen;
        true
    } else if n.ilen == 0 && l.ilen == 0 && n.at == l.at {
        // forward delete
        l.removed.push_str(&n.removed);
        l.rlen += n.rlen;
        true
    } else {
        false
    }
}

fn normalize(mut sels: Vec<Range>, len: usize) -> Vec<Range> {
    for r in sels.iter_mut() {
        r.anchor = r.anchor.min(len);
        r.head = r.head.min(len);
    }
    sels.sort_by_key(|r| (r.start(), r.end()));
    let mut out: Vec<Range> = Vec::with_capacity(sels.len());
    for r in sels {
        match out.last_mut() {
            Some(p)
                if r.start() < p.end()
                    || (r.start() == p.end() && (r.is_empty() || p.is_empty())) =>
            {
                let (s, e) = (p.start(), p.end().max(r.end()));
                *p = if r.head >= r.anchor {
                    Range { anchor: s, head: e }
                } else {
                    Range { anchor: e, head: s }
                };
            }
            _ => out.push(r),
        }
    }
    if out.is_empty() {
        out.push(Range::caret(0));
    }
    out
}

/// Cells occupied by `c` when it starts at display column `col`.
pub fn cell_width(c: char, col: usize, tab: usize) -> usize {
    if c == '\t' {
        tab - col % tab
    } else {
        c.width().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
