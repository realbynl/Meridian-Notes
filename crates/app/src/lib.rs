pub mod align;
pub mod app;
pub mod autocap;
pub mod banner;
pub mod bookmarks;
pub mod ankiio;
pub mod cards;
pub mod quiz;
pub mod feynman;
pub mod quote;
pub mod readqueue;
pub mod recall;
pub mod exam;
pub mod studystats;
pub mod browser;
pub mod nativepick;
pub mod hypr;
pub mod ink;
pub mod build;
pub mod chrome;
pub mod side_model;
pub mod chrome_model;
pub mod dialog_model;
pub mod notebooks;
pub mod cli;
pub mod clip;
pub mod comment;
pub mod mdfmt;
pub mod mdstyle;
pub mod daylog;
pub mod commands;
pub mod editview;
pub mod filetree;
pub mod findbar;
pub mod focus;
pub mod fontlist;
pub mod fold;
pub mod fuzzy;
pub mod gitdiff;
pub mod goto;
pub mod gutter_marks;
#[allow(dead_code)] // data layer only until the scrubber (stage I) uses it
pub mod history;
pub mod host;
pub mod indent;
pub mod lint;
pub mod layouts;
pub mod graph;
pub mod linkindex;
pub mod endless;
pub mod embed;
pub mod links;
pub mod lists;
pub mod markers;
pub mod menu;
pub mod ribbon;
pub mod meta;
pub mod margins;
pub mod minimap;
pub mod movelines;
pub mod openables;
pub mod multi;
pub mod natural;
pub mod pane;
pub mod palette;
pub mod skin;
pub mod skin_ui;
pub mod skin_fill;
pub mod skin_image;
pub mod skin_tint;
pub mod notebook_colors;
pub mod plugin;
pub mod projects;
pub mod radial;
pub mod reader;
pub mod recents;
pub mod regexbuild;
pub mod remote;
pub mod resedit;
pub mod search;
pub mod sidetree;
pub mod settings;
pub mod snippets;
pub mod speedread;
pub mod stats;
pub mod splits;
pub mod swap;
pub mod symbols;
pub mod templates;
pub mod table;
pub mod datatree;
pub mod views;
pub mod datephrase;
pub mod emmet;
pub mod emmet_css;
pub mod tagscan;
pub mod units;
pub mod colors;
pub mod usage;
pub mod keyconf;
pub mod vim;
pub mod wordswap;
pub mod welcome;
pub mod timers;
pub mod when;
pub mod wordfreq;
pub mod wrap;

use std::path::PathBuf;

#[derive(Default, Debug)]
pub struct Args {
    pub files: Vec<PathBuf>,
    /// Directories on the command line: added as folders.
    pub dirs: Vec<PathBuf>,
    /// `host:/path` arguments: remote folders opened over ssh.
    pub remotes: Vec<String>,
    pub line: Option<usize>,
    /// Per file in `files`: 1-based (line, col) from a `file:line[:col]` argument.
    pub at: Vec<Option<(usize, usize)>>,
    /// `key=value` lines from `--set`, layered over the user and project settings.
    pub set: String,
    pub read_only: bool,
    pub fullscreen: bool,
    /// Open the graph view fullscreen at start (`--graph`).
    pub graph: bool,
    pub icons: Option<String>,
    pub config: Option<PathBuf>,
}

