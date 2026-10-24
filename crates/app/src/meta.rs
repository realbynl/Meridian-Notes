//! Per-document state that is not text: detected indent style, and what a results buffer
//! (Find in Files, build output) maps its lines to.

use crate::indent::{self, Indent};
use std::path::PathBuf;
use meridian_buffer::Buffer;

/// A result line and the file position it jumps to (all 0-based).
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub row: usize,
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
    /// A file-name header row: Enter opens the file, next/previous result skips it.
    pub header: bool,
    /// The header's shown file name, or for a line row the file line as the search read it.
    pub name: String,
    pub orig: String,
}

#[derive(Default, Clone)]
pub struct Meta {
    pub indent: Option<Indent>,
    /// Display name for buffers that are not files (results buffers).
    pub title: Option<String>,
    pub readonly: bool,
    /// Big file: lint, minimap, highlighting and plugin view events are off.
    pub huge: bool,
    pub hits: Vec<Hit>,
    /// Index into `hits` of the last result visited with next/previous result.
    pub cursor: usize,
    /// A Find Results buffer whose rows can be edited and written back to the files.
    pub editable: bool,
    /// Header lines of folded blocks, and the buffer version and line count they were last synced to.
    pub folds: Vec<usize>,
    pub fold_sync: (u64, usize),
    /// Targets of embeds shown collapsed (keyed by text so edits above do not matter).
    pub embed_collapsed: Vec<String>,
    /// The Coding-mode view this file is in (plain text, CSV/TSV table, JSON/YAML/TOML checks).
    pub sub: crate::views::Sub,
    /// Word count before this session's first edit (or at the last save): the writing log counts from it.
    pub wc_base: Option<usize>,
    /// Line bookmarks of this buffer.
    pub bookmarks: crate::bookmarks::Bookmarks,
}

impl Meta {
    pub fn of(buf: &Buffer) -> Self {
        Meta {
            indent: indent::detect(buf),
            ..Meta::default()
        }
    }

    pub fn results(title: &str, hits: Vec<Hit>) -> Self {
        Meta {
            title: Some(title.to_string()),
            readonly: true,
            hits,
            ..Meta::default()
        }
    }

    /// The hit on result row `row`, if that row is one.
    pub fn hit_at(&self, row: usize) -> Option<&Hit> {
        self.hits.iter().find(|h| h.row == row)
    }
}
