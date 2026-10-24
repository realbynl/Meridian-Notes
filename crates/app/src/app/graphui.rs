//! The graph view (Ctrl+Shift+G): every note a node, every `[[link]]` an edge, drawn by the toolkit in the GUI.
//! The app owns the data, the filters and the options (saved in `graph.conf`); where the nodes sit, the pan and
//! zoom and the hover belong to the chrome, which runs the simulation a frame at a time.

use super::*;
use crate::dialog_model::{GraphAction, GraphModel, GraphNode, GraphSettings};
use crate::graph;
use std::sync::Arc;

/// What the cached nodes were built from; the options that only change the drawing are not part of it.
#[derive(PartialEq, Clone)]
struct Sig {
    generation: u64,
    files: usize,
    filters: (bool, bool, bool, u8, bool, bool, Vec<(String, u32)>),
    search: String,
    local: bool,
    current: Option<PathBuf>,
}

struct Cached {
    sig: Sig,
    nodes: Arc<Vec<GraphNode>>,
    edges: Arc<Vec<(u32, u32)>>,
    paths: Vec<Option<PathBuf>>,
    total: usize,
}

pub(super) struct GraphUi {
    pub s: GraphSettings,
    pub search: String,
    pub search_focus: bool,
    pub local: bool,
    pub panel: bool,
    pub fit_seq: u32,
    /// The index is not ready yet: the loop wakes to look again.
    pub waiting: bool,
    /// Started by `--graph`: the graph covers the whole window, chrome included, while it is open.
    pub fill: bool,
    /// The ribbon button's panel: beside the editor, following the open note, the keys stay with the editor.
    pub docked: bool,
    cached: Option<Cached>,
}

fn conf_path() -> Option<PathBuf> {
    crate::commands::config_dir().map(|d| d.join("graph.conf"))
}

impl App {
    pub(super) fn graph_open(&mut self, local: bool) {
        self.graph_open_as(local, false);
    }

    pub(super) fn graph_open_as(&mut self, local: bool, fill: bool) {
        let s = conf_path().and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(GraphSettings::default, |t| graph::from_conf(&t));
        self.ex.graph = Some(GraphUi { s, search: String::new(), search_focus: false, local, panel: false, fit_seq: 0, waiting: true, fill, docked: false, cached: None });
        self.graph_tick();
    }

    pub(super) fn graph_docked(&self) -> bool {
        self.ex.graph.as_ref().is_some_and(|g| g.docked)
    }

    /// The ribbon button: show the local graph of the open note in a panel, or close the graph if it is up.
    pub(super) fn graph_toggle_panel(&mut self) {
        if self.ex.graph.is_some() {
            self.ex.graph = None;
            return;
        }
        self.graph_open(true);
        if let Some(g) = self.ex.graph.as_mut() {
            g.docked = true;
        }
    }

    /// Ctrl+Shift+G: the whole-project graph, or close the graph if it is up.
    pub(super) fn graph_toggle_global(&mut self) {
        if self.ex.graph.is_some() {
            self.ex.graph = None;
        } else {
            self.graph_open(false);
        }
    }

    /// Start the file walk and the link index if they are missing, and rebuild the nodes when what they depend on moved.
    pub(super) fn graph_tick(&mut self) {
        let Some(g) = self.ex.graph.as_ref() else { return };
        let (settings, search, local) = (g.s.clone(), g.search.clone(), g.local);
        self.files_ready();
        self.links_start_index();
        let (Some(ix), Some(files)) = (self.links.ix.as_ref(), self.files.as_ref()) else {
            if let Some(g) = self.ex.graph.as_mut() {
                g.waiting = true;
            }
            return;
        };
        let current = self.buf.path().map(Path::to_path_buf);
        let sig = Sig {
            generation: ix.generation,
            files: files.len(),
            filters: (settings.existing_only, settings.orphans, settings.attachments, settings.depth, settings.incoming, settings.outgoing, settings.groups.iter().map(|g| (g.query.clone(), g.colour)).collect()),
            search,
            local,
            current: current.clone(),
        };
        let g = self.ex.graph.as_mut().unwrap();
        g.waiting = false;
        if g.cached.as_ref().is_some_and(|c| c.sig == sig) {
            return;
        }
        let b = graph::build(ix, files, &self.root, current.as_deref(), local, &settings, &sig.search);
        g.cached = Some(Cached { sig, nodes: Arc::new(b.nodes), edges: Arc::new(b.edges), paths: b.paths, total: b.total });
    }

    pub(super) fn graph_model(&self) -> Option<GraphModel> {
        let g = self.ex.graph.as_ref()?;
        let (nodes, edges, total) = g.cached.as_ref().map_or_else(Default::default, |c| (c.nodes.clone(), c.edges.clone(), c.total));
        Some(GraphModel { nodes, edges, settings: g.s.clone(), search: g.search.clone(), search_focus: g.search_focus, local: g.local, docked: g.docked, panel: g.panel, indexing: g.cached.is_none(), fit_seq: g.fit_seq, total })
    }

    pub(super) fn graph_key(&mut self, k: KeyEvent) {
        let Some(g) = self.ex.graph.as_mut() else { return };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let before = (g.search.clone(), g.local, g.s.clone());
        if g.search_focus {
            match k.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Tab | KeyCode::Down => g.search_focus = false,
                KeyCode::Backspace => {
                    g.search.pop();
                }
                KeyCode::Char('u') if ctrl => g.search.clear(),
                KeyCode::Char(c) if !ctrl && !k.modifiers.contains(KeyModifiers::ALT) => g.search.push(c),
                _ => {}
            }
        } else {
            match k.code {
                KeyCode::Esc => self.ex.graph = None,
                KeyCode::Char('f') if ctrl => {
                    g.panel = true;
                    g.search_focus = true;
                }
                KeyCode::Char('/') => {
                    g.panel = true;
                    g.search_focus = true;
                }
                KeyCode::Char('f') if !ctrl => g.fit_seq = g.fit_seq.wrapping_add(1),
                KeyCode::Char('s') if !ctrl => g.panel = !g.panel,
                KeyCode::Char('l') if !ctrl => g.local = !g.local,
                _ => {}
            }
        }
        if self.ex.graph.as_ref().is_some_and(|g| before != (g.search.clone(), g.local, g.s.clone())) {
            self.graph_tick();
        }
    }

    pub(super) fn graph_action(&mut self, a: GraphAction) {
        let Some(g) = self.ex.graph.as_mut() else { return };
        match a {
            GraphAction::Open(id) => {
                let hit = g.cached.as_ref().and_then(|c| c.nodes.iter().position(|n| n.id == id).map(|i| (c.nodes[i].label.clone(), c.paths[i].clone())));
                match hit {
                    Some((_, Some(p))) => {
                        self.ex.graph = None;
                        self.open_path(&p);
                    }
                    Some((label, None)) => self.notify(&format!("No note named {label} yet"), false),
                    None => {}
                }
            }
            GraphAction::Set(s) => {
                if let Some(p) = conf_path() {
                    let _ = std::fs::write(p, graph::to_conf(&s));
                }
                g.s = s;
            }
            GraphAction::FocusSearch => {
                g.panel = true;
                g.search_focus = true;
            }
            GraphAction::Search(q) => {
                g.search = q;
                g.search_focus = true;
            }
            GraphAction::ToggleLocal => g.local = !g.local,
            GraphAction::TogglePanel => {
                g.panel = !g.panel;
                g.search_focus &= g.panel;
            }
        }
        self.graph_tick();
    }
}
