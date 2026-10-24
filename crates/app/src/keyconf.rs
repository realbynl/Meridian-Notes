//! Editing `keys.conf` text for the rebinding screen: set, unbind and reset one command's key
//! while keeping comments and every other line as they were.

/// The key a line binds and its target (`none` unbinds), when it is a `key = target` line.
fn parts(line: &str) -> Option<(String, String)> {
    let code = line.split('#').next().unwrap_or("").trim();
    let (k, c) = code.split_once('=')?;
    let spec = crate::commands::normalize_spec(k)?;
    Some((spec, c.trim().to_string()))
}

fn rebuild(lines: Vec<String>) -> String {
    let mut s = lines.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

/// Bind `spec` to `cmd` (and nothing else to `spec`; the command's previous keys go).
pub fn set(conf: &str, cmd: &str, spec: &str) -> String {
    let mut lines: Vec<String> = conf
        .lines()
        .filter(|l| !parts(l).is_some_and(|(k, c)| c == cmd || k == spec))
        .map(String::from)
        .collect();
    lines.push(format!("{spec} = {cmd}"));
    rebuild(lines)
}

/// Remove the command's keys: its own lines go, and its default key (if any) is turned off.
pub fn unbind(conf: &str, cmd: &str, default: Option<&str>) -> String {
    let mut lines: Vec<String> = conf.lines().filter(|l| !parts(l).is_some_and(|(_, c)| c == cmd)).map(String::from).collect();
    if let Some(d) = default.and_then(crate::commands::normalize_spec) {
        lines.retain(|l| !parts(l).is_some_and(|(k, c)| k == d && c.eq_ignore_ascii_case("none")));
        lines.push(format!("{d} = none"));
    }
    rebuild(lines)
}

/// Back to the built-in key: drop the command's lines and any `none` aimed at its default key.
pub fn reset(conf: &str, cmd: &str, default: Option<&str>) -> String {
    let d = default.and_then(crate::commands::normalize_spec);
    let lines: Vec<String> = conf
        .lines()
        .filter(|l| {
            !parts(l).is_some_and(|(k, c)| c == cmd || (c.eq_ignore_ascii_case("none") && d.as_deref() == Some(k.as_str())))
        })
        .map(String::from)
        .collect();
    rebuild(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Keymap;

    #[test]
    fn set_replaces_the_command_and_the_key_and_keeps_comments() {
        let conf = "# mine\nctrl+j = save\nctrl+q = none # no quit\nalt+x = find\n";
        let out = set(conf, "save", "alt+x");
        assert_eq!(out, "# mine\nctrl+q = none # no quit\nalt+x = save\n");
        assert_eq!(set("", "save", "ctrl+shift+s"), "ctrl+shift+s = save\n");
    }

    #[test]
    fn unbind_and_reset() {
        let out = unbind("ctrl+j = save\n", "save", Some("ctrl+s"));
        assert_eq!(out, "ctrl+s = none\n");
        assert_eq!(unbind(&out, "save", Some("ctrl+s")), "ctrl+s = none\n", "no duplicate none lines");
        assert_eq!(reset(&out, "save", Some("ctrl+s")), "");
        assert_eq!(unbind("", "settings", None), "");
    }

    #[test]
    fn edits_take_effect_in_the_keymap() {
        let key = |km: &Keymap, name: &str| km.key_for(km.find(name).unwrap()).map(String::from);
        let (km, _) = Keymap::build(&set("", "save", "ctrl+j"));
        assert_eq!(key(&km, "save").as_deref(), Some("ctrl+j"));
        assert_eq!(key(&km, "word-swap"), None);
        let (km, errs) = Keymap::build(&unbind("", "save", Some("ctrl+s")));
        assert!(errs.is_empty());
        assert_eq!(key(&km, "save"), None);
        assert!(km.resolve(crate::commands::spec_key("ctrl+s").unwrap()).is_none());
        let (km, _) = Keymap::build(&reset(&unbind("", "save", Some("ctrl+s")), "save", Some("ctrl+s")));
        assert_eq!(key(&km, "save").as_deref(), Some("ctrl+s"));
        // Taking another command's key moves it.
        let (km, _) = Keymap::build(&set("", "find", "ctrl+s"));
        assert_eq!(key(&km, "find").as_deref(), Some("ctrl+s"));
        assert_eq!(key(&km, "save"), None);
    }
}
