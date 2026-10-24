//! File templates for Ctrl+N. A template is a plain file named `<Name>.<ext>` in
//! `~/.config/meridian/templates/` or `<project>/.meridian/templates/`; creating a new `.<ext>` file
//! inserts its text. The body is expanded like a snippet (`$1`, `${1:Title}`, `$0`, `$TM_FILENAME`).

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub name: String,
    pub ext: String,
    pub body: String,
    /// The file it was read from.
    pub path: PathBuf,
}

/// File types offered by Ctrl+N even when no template exists for them.
pub const TYPES: &[(&str, &str)] = &[
    ("Markdown", "md"),
    ("Plain text", "txt"),
    ("Shell script", "sh"),
    ("Python", "py"),
    ("Rust", "rs"),
    ("JavaScript", "js"),
    ("TypeScript", "ts"),
    ("HTML", "html"),
    ("C", "c"),
    ("Go", "go"),
];

/// Every `<Name>.<ext>` file in `dir`, sorted by file name.
pub fn load_dir(dir: &Path) -> Vec<Template> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    files.sort();
    files
        .iter()
        .filter_map(|p| {
            let name = p.file_stem()?.to_str()?.to_string();
            let ext = p.extension()?.to_str()?.to_string();
            let body = std::fs::read_to_string(p).ok()?;
            (!name.starts_with('.')).then(|| Template { name, ext, body: body.trim_end_matches(['\n', '\r']).to_string(), path: p.clone() })
        })
        .collect()
}

/// User templates, then project templates; a project template replaces a user one of the same name and type.
pub fn load(user: Option<&Path>, project: &Path) -> Vec<Template> {
    let mut all = user.map(load_dir).unwrap_or_default();
    for t in load_dir(project) {
        all.retain(|u| !(u.name == t.name && u.ext == t.ext));
        all.push(t);
    }
    all
}

pub fn for_ext<'a>(all: &'a [Template], ext: &str) -> Vec<&'a Template> {
    all.iter().filter(|t| t.ext == ext).collect()
}

/// The file types to list: the built-in ones, then any other extension that has a template.
pub fn types(all: &[Template]) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = TYPES.iter().map(|&(l, e)| (l.to_string(), e.to_string())).collect();
    let mut extra: Vec<&str> = all.iter().map(|t| t.ext.as_str()).filter(|e| !TYPES.iter().any(|t| t.1 == *e)).collect();
    extra.sort();
    extra.dedup();
    v.extend(extra.into_iter().map(|e| (format!(".{e}"), e.to_string())));
    v
}

/// `untitled.<ext>` in `dir`, or `untitled-2.<ext>`, ... when that exists.
pub fn unique(dir: &Path, ext: &str) -> PathBuf {
    let first = dir.join(format!("untitled.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("untitled-{n}.{ext}"))).find(|p| !p.exists()).expect("unbounded range")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, ext: &str) -> Template {
        Template { name: name.into(), ext: ext.into(), body: String::new(), path: PathBuf::new() }
    }

    #[test]
    fn types_add_extensions_with_templates() {
        let v = types(&[t("a", "md"), t("b", "toml"), t("c", "toml")]);
        assert_eq!(v.len(), TYPES.len() + 1);
        assert_eq!(v.last().unwrap(), &(".toml".to_string(), "toml".to_string()));
    }

    #[test]
    fn for_ext_filters() {
        let all = [t("a", "md"), t("b", "sh"), t("c", "md")];
        let names: Vec<_> = for_ext(&all, "md").iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["a", "c"]);
    }

    #[test]
    fn dir_loading_and_override() {
        let base = std::env::temp_dir().join(format!("sub-tmpl-{}", std::process::id()));
        let (u, p) = (base.join("u"), base.join("p"));
        std::fs::create_dir_all(&u).unwrap();
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(u.join("Note.md"), "# user\n").unwrap();
        std::fs::write(u.join("Post.md"), "post").unwrap();
        std::fs::write(p.join("Note.md"), "# project\n").unwrap();
        std::fs::write(p.join("noext"), "x").unwrap();
        let all = load(Some(&u), &p);
        assert_eq!(all.len(), 2);
        assert_eq!(all.iter().find(|t| t.name == "Note").unwrap().body, "# project");
        assert_eq!(unique(&u, "md"), u.join("untitled.md"));
        std::fs::write(u.join("untitled.md"), "").unwrap();
        assert_eq!(unique(&u, "md"), u.join("untitled-2.md"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
