//! The graph view's data: which notes are nodes, which links are edges, after Obsidian's filters (search,
//! orphans, attachments, unresolved links, local depth) and colour groups. Pure; the panel in `app/graphui.rs`
//! feeds it the link index and the toolkit draws what comes out.

use crate::dialog_model::{GraphGroup, GraphNode, GraphSettings, NodeKind};
use crate::linkindex::{canon, wanted, Index};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const NOTE_EXTS: [&str; 3] = ["md", "markdown", "txt"];

/// What `build` made: the nodes and edges the toolkit draws, and where each node's note lives.
#[derive(Default)]
pub struct Built {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<(u32, u32)>,
    /// Per node: the file a click opens (none for an unresolved link).
    pub paths: Vec<Option<PathBuf>>,
    /// Notes in the project before the filters.
    pub total: usize,
}

struct Raw {
    key: String,
    label: String,
    kind: NodeKind,
    path: Option<PathBuf>,
    /// Project-relative path, lower case, for search and groups.
    rel: String,
}

pub fn node_id(key: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    h.finish()
}

fn rel_of(p: &Path, root: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn label_of(p: &Path) -> String {
    p.file_stem().or_else(|| p.file_name()).map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// Whether `query` (terms separated by spaces, all must match) selects the node. `path:x` looks in the path,
/// `file:x` in the file name, anything else in both.
pub fn matches(query: &str, label: &str, rel: &str) -> bool {
    let (label, rel) = (label.to_lowercase(), rel.to_lowercase());
    query.split_whitespace().all(|t| {
        let t = t.to_lowercase();
        if let Some(x) = t.strip_prefix("path:") {
            rel.contains(x)
        } else if let Some(x) = t.strip_prefix("file:") {
            label.contains(x)
        } else {
            label.contains(&t) || rel.contains(&t)
        }
    })
}

pub fn build(ix: &Index, files: &[PathBuf], root: &Path, current: Option<&Path>, local: bool, s: &GraphSettings, search: &str) -> Built {
    let exts: Vec<String> = NOTE_EXTS.iter().map(|e| e.to_string()).collect();
    let root = canon(root);
    let current = current.map(canon);
    let mut raws: Vec<Raw> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut add = |raws: &mut Vec<Raw>, key: String, mk: &dyn Fn() -> Raw| -> usize {
        *at.entry(key).or_insert_with(|| {
            raws.push(mk());
            raws.len() - 1
        })
    };
    let note = |p: &Path| {
        let p = canon(p);
        let kind = if wanted(&p, &exts) { NodeKind::Note } else { NodeKind::Attachment };
        let key = p.to_string_lossy().into_owned();
        let rel = rel_of(&p, &root);
        Raw { key, label: if kind == NodeKind::Note { label_of(&p) } else { p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()) }, kind, path: Some(p), rel }
    };
    let mut total = 0;
    for f in files.iter().filter(|f| wanted(f, &exts)) {
        let r = note(f);
        total += 1;
        add(&mut raws, r.key.clone(), &|| note(f));
    }
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (a, b) in ix.edges() {
        let i = add(&mut raws, canon(&a).to_string_lossy().into_owned(), &|| note(&a));
        let j = add(&mut raws, canon(&b).to_string_lossy().into_owned(), &|| note(&b));
        edges.push((i, j));
    }
    for (src, target) in ix.unresolved() {
        let key = format!("?{}", target.to_lowercase());
        let i = add(&mut raws, canon(&src).to_string_lossy().into_owned(), &|| note(&src));
        let j = add(&mut raws, key.clone(), &|| Raw { key: key.clone(), label: target.clone(), kind: NodeKind::Unresolved, path: None, rel: target.to_lowercase() });
        edges.push((i, j));
    }
    edges.sort_unstable();
    edges.dedup();

    let mut keep: Vec<bool> = raws.iter().map(|r| match r.kind {
        NodeKind::Note => true,
        NodeKind::Attachment => s.attachments,
        NodeKind::Unresolved => !s.existing_only,
    }).collect();
    edges.retain(|&(a, b)| keep[a] && keep[b]);
    let cur = current.as_ref().and_then(|c| at.get(&c.to_string_lossy().into_owned()).copied());
    // An orphan has no links at all, whatever the search or the depth leaves on screen.
    if !s.orphans {
        let mut deg = vec![0usize; raws.len()];
        for &(a, b) in &edges {
            deg[a] += 1;
            deg[b] += 1;
        }
        for (i, k) in keep.iter_mut().enumerate() {
            *k &= deg[i] > 0 || Some(i) == cur;
        }
        edges.retain(|&(a, b)| keep[a] && keep[b]);
    }

    if local {
        let mut seen = vec![false; raws.len()];
        if let Some(c) = cur {
            let (mut out, mut inc): (Vec<Vec<usize>>, Vec<Vec<usize>>) = (vec![Vec::new(); raws.len()], vec![Vec::new(); raws.len()]);
            for &(a, b) in &edges {
                out[a].push(b);
                inc[b].push(a);
            }
            let mut q = VecDeque::from([(c, 0u8)]);
            seen[c] = true;
            while let Some((n, d)) = q.pop_front() {
                if d >= s.depth.max(1) {
                    continue;
                }
                let next = (if s.outgoing { out[n].as_slice() } else { &[] }).iter().chain(if s.incoming { inc[n].as_slice() } else { &[] });
                for &m in next {
                    if !seen[m] {
                        seen[m] = true;
                        q.push_back((m, d + 1));
                    }
                }
            }
        }
        for (k, sn) in keep.iter_mut().zip(&seen) {
            *k &= *sn;
        }
        edges.retain(|&(a, b)| keep[a] && keep[b]);
    }
    if !search.trim().is_empty() {
        for (k, r) in keep.iter_mut().zip(&raws) {
            *k &= matches(search, &r.label, &r.rel);
        }
        edges.retain(|&(a, b)| keep[a] && keep[b]);
    }
    // Compact: kept nodes in key order, edges renumbered.
    let mut order: Vec<usize> = (0..raws.len()).filter(|&i| keep[i]).collect();
    order.sort_by(|&a, &b| raws[a].key.cmp(&raws[b].key));
    let mut new = vec![u32::MAX; raws.len()];
    for (n, &i) in order.iter().enumerate() {
        new[i] = n as u32;
    }
    let group_of = |r: &Raw| s.groups.iter().find(|g: &&GraphGroup| !g.query.trim().is_empty() && matches(&g.query, &r.label, &r.rel)).map(|g| g.colour);
    let mut out = Built { total, ..Built::default() };
    for &i in &order {
        let r = &raws[i];
        let born = r.path.as_ref().filter(|_| r.kind == NodeKind::Note).and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.created().or_else(|_| m.modified()).ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        out.nodes.push(GraphNode { id: node_id(&r.key), label: r.label.clone(), kind: r.kind, group: group_of(r), current: Some(i) == cur, born });
        out.paths.push(r.path.clone());
    }
    out.edges = edges.iter().map(|&(a, b)| (new[a], new[b])).collect();
    out
}

/// `graph.conf`: one `key=value` per line; `group=RRGGBB query` repeats.
pub fn to_conf(s: &GraphSettings) -> String {
    let b = |v: bool| if v { "on" } else { "off" };
    let mut o = format!(
        "existing_only={}\norphans={}\nattachments={}\ndepth={}\nincoming={}\noutgoing={}\narrows={}\ntext_fade={}\nnode_size={}\nlink_thickness={}\ncenter={}\nrepel={}\nlink_force={}\nlink_distance={}\n",
        b(s.existing_only), b(s.orphans), b(s.attachments), s.depth, b(s.incoming), b(s.outgoing), b(s.arrows), s.text_fade, s.node_size, s.link_thickness, s.center, s.repel, s.link_force, s.link_distance
    );
    for g in &s.groups {
        o.push_str(&format!("group={:06x} {}\n", g.colour, g.query));
    }
    o
}

/// Read what `to_conf` wrote. Unknown keys and unreadable values keep the default.
pub fn from_conf(text: &str) -> GraphSettings {
    let mut s = GraphSettings::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        let on = || matches!(v, "on" | "true" | "1");
        let f = |lo: f32, hi: f32, d: f32| v.parse::<f32>().ok().filter(|x| x.is_finite()).map_or(d, |x| x.clamp(lo, hi));
        match k {
            "existing_only" => s.existing_only = on(),
            "orphans" => s.orphans = on(),
            "attachments" => s.attachments = on(),
            "depth" => s.depth = v.parse::<u8>().map_or(1, |d| d.clamp(1, 5)),
            "incoming" => s.incoming = on(),
            "outgoing" => s.outgoing = on(),
            "arrows" => s.arrows = on(),
            "text_fade" => s.text_fade = f(-3.0, 3.0, s.text_fade),
            "node_size" => s.node_size = f(0.1, 5.0, s.node_size),
            "link_thickness" => s.link_thickness = f(0.1, 5.0, s.link_thickness),
            "center" => s.center = f(0.0, 1.0, s.center),
            "repel" => s.repel = f(0.0, 20.0, s.repel),
            "link_force" => s.link_force = f(0.0, 1.0, s.link_force),
            "link_distance" => s.link_distance = f(30.0, 500.0, s.link_distance),
            "group" => {
                if let Some((c, q)) = v.split_once(' ') {
                    if let Ok(colour) = u32::from_str_radix(c.trim_start_matches('#'), 16) {
                        s.groups.push(GraphGroup { query: q.trim().to_string(), colour: colour & 0xff_ffff });
                    }
                }
            }
            _ => {}
        }
    }
    s
}

/// Group colours handed out by "New group", cycling.
pub const GROUP_COLOURS: [u32; 6] = [0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2];

pub fn path_set(files: &[PathBuf]) -> HashSet<PathBuf> {
    files.iter().map(|f| canon(f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(tag: &str, files: &[(&str, &str)]) -> (PathBuf, Vec<PathBuf>, Index) {
        let root = std::env::temp_dir().join(format!("meridian-graph-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut out = Vec::new();
        for (f, body) in files {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, body).unwrap();
            out.push(p);
        }
        let ix = Index::build(&out, &root, &NOTE_EXTS.map(String::from));
        (root, out, ix)
    }

    fn names(b: &Built) -> Vec<String> {
        let mut v: Vec<String> = b.nodes.iter().map(|n| n.label.clone()).collect();
        v.sort();
        v
    }

    fn sample(tag: &str) -> (PathBuf, Vec<PathBuf>, Index) {
        project(tag, &[("a.md", "[[b]] [[ghost]] {{pic.png}}"), ("b.md", "[[c]]"), ("c.md", ""), ("lonely.md", ""), ("sub/d.md", "[[a]]"), ("pic.png", "")])
    }

    #[test]
    fn nodes_are_notes_edges_are_links_and_unresolved_targets_get_nodes() {
        let (root, files, ix) = sample("basic");
        let b = build(&ix, &files, &root, None, false, &GraphSettings::default(), "");
        assert_eq!(names(&b), ["a", "b", "c", "d", "ghost", "lonely"], "no attachment by default");
        assert_eq!(b.edges.len(), 4, "a-b, a-ghost, b-c, d-a");
        assert_eq!(b.total, 5);
        let ghost = b.nodes.iter().position(|n| n.label == "ghost").unwrap();
        assert_eq!((b.nodes[ghost].kind, b.paths[ghost].is_none()), (NodeKind::Unresolved, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_filters_work_like_obsidians() {
        let (root, files, ix) = sample("filters");
        let g = |f: &dyn Fn(&mut GraphSettings), q: &str| {
            let mut s = GraphSettings::default();
            f(&mut s);
            names(&build(&ix, &files, &root, None, false, &s, q))
        };
        assert_eq!(g(&|s| s.attachments = true, ""), ["a", "b", "c", "d", "ghost", "lonely", "pic.png"]);
        assert_eq!(g(&|s| s.existing_only = true, ""), ["a", "b", "c", "d", "lonely"]);
        assert_eq!(g(&|s| s.orphans = false, ""), ["a", "b", "c", "d", "ghost"]);
        assert_eq!(g(&|_| {}, "path:sub"), ["d"]);
        assert_eq!(g(&|_| {}, "file:B"), ["b"]);
        assert_eq!(g(&|_| {}, "lone"), ["lonely"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_local_graph_walks_links_to_the_depth_in_the_chosen_directions() {
        let (root, files, ix) = sample("local");
        let a = root.join("a.md");
        let g = |f: &dyn Fn(&mut GraphSettings)| {
            let mut s = GraphSettings::default();
            f(&mut s);
            names(&build(&ix, &files, &root, Some(&a), true, &s, ""))
        };
        assert_eq!(g(&|_| {}), ["a", "b", "d", "ghost"]);
        assert_eq!(g(&|s| s.depth = 2), ["a", "b", "c", "d", "ghost"]);
        assert_eq!(g(&|s| s.incoming = false), ["a", "b", "ghost"]);
        assert_eq!(g(&|s| s.outgoing = false), ["a", "d"]);
        assert!(names(&build(&ix, &files, &root, None, true, &GraphSettings::default(), "")).is_empty(), "no open note, nothing local");
        let b = build(&ix, &files, &root, Some(&a), true, &GraphSettings::default(), "");
        assert_eq!(b.nodes.iter().filter(|n| n.current).count(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_open_note_survives_the_orphan_filter_and_groups_colour_nodes() {
        let (root, files, ix) = sample("orph");
        let l = root.join("lonely.md");
        let mut s = GraphSettings { orphans: false, ..GraphSettings::default() };
        s.groups = vec![GraphGroup { query: "path:sub".into(), colour: 0x112233 }, GraphGroup { query: "".into(), colour: 0xffffff }];
        let b = build(&ix, &files, &root, Some(&l), false, &s, "");
        assert!(b.nodes.iter().any(|n| n.label == "lonely" && n.current));
        assert_eq!(b.nodes.iter().find(|n| n.label == "d").unwrap().group, Some(0x112233));
        assert_eq!(b.nodes.iter().find(|n| n.label == "a").unwrap().group, None, "an empty query matches nothing");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_search_does_not_turn_a_matching_note_into_an_orphan() {
        let (root, files, ix) = sample("orph-search");
        let s = GraphSettings { orphans: false, ..GraphSettings::default() };
        let b = build(&ix, &files, &root, None, false, &s, "path:sub");
        assert_eq!(names(&b), ["d"], "d links to a, so it is not an orphan even though a is filtered out");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ids_are_stable_and_edges_index_the_compacted_nodes() {
        let (root, files, ix) = sample("ids");
        let one = build(&ix, &files, &root, None, false, &GraphSettings::default(), "");
        let two = build(&ix, &files, &root, None, false, &GraphSettings::default(), "");
        assert_eq!(one.nodes, two.nodes);
        let filtered = build(&ix, &files, &root, None, false, &GraphSettings::default(), "file:b");
        assert!(filtered.edges.is_empty() && filtered.nodes.len() == 1);
        assert_eq!(filtered.nodes[0].id, one.nodes.iter().find(|n| n.label == "b").unwrap().id, "the id does not depend on the filter");
        for (a, b) in &one.edges {
            assert!((*a as usize) < one.nodes.len() && (*b as usize) < one.nodes.len());
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn settings_round_trip_through_the_conf_text_and_bad_values_fall_back() {
        let mut s = GraphSettings { arrows: true, depth: 3, repel: 4.5, existing_only: true, ..GraphSettings::default() };
        s.groups = vec![GraphGroup { query: "path:Journal 2026".into(), colour: 0xabcdef }];
        assert_eq!(from_conf(&to_conf(&s)), s);
        let bad = from_conf("depth=99\nrepel=NaN\nlink_distance=9999\nnonsense\ngroup=zz q\n");
        assert_eq!((bad.depth, bad.repel, bad.link_distance, bad.groups.len()), (5, 10.0, 500.0, 0));
    }
}
