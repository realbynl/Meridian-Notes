//! Pipe the selection (or the whole text) through a shell command and replace it with the output.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use meridian_buffer::{Edit, EditKind};

use super::{App, PromptKind};

/// Run `sh -c cmd` with `input` on stdin; stdout on success, the first stderr line on failure.
/// Gives up after `limit`.
pub(super) fn run(cmd: &str, input: &str, limit: Duration) -> Result<String, String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{cmd}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let data = input.to_string();
    // Write on a thread so a command that fills its output pipe before reading cannot deadlock.
    std::thread::spawn(move || {
        let _ = stdin.write_all(data.as_bytes());
    });
    let start = Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            return Err(format!("{cmd}: timed out after {}s", limit.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().next().unwrap_or("").trim();
        Err(format!("{cmd}: {}", if first.is_empty() { format!("exit {}", out.status.code().unwrap_or(-1)) } else { first.to_string() }))
    }
}

impl App {
    pub(super) fn pipe_prompt(&mut self) {
        self.prompt_kind = PromptKind::Pipe;
        self.open_prompt = Some(String::new());
    }

    /// Replace each non-empty selection (else the whole text) with the command's output.
    pub(super) fn pipe_run(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        let mut spans: Vec<(usize, usize)> =
            self.buf.sels().iter().filter(|r| !r.is_empty()).map(|r| (r.start(), r.end())).collect();
        if spans.is_empty() {
            spans.push((0, self.buf.len_chars()));
        }
        let mut edits = Vec::new();
        for (a, b) in spans {
            let text = self.buf.rope().slice(a..b).to_string();
            match run(cmd, &text, Duration::from_secs(10)) {
                // A trailing newline the command adds to a selection that had none is dropped.
                Ok(mut out) => {
                    if !text.ends_with('\n') && out.ends_with('\n') {
                        out.pop();
                    }
                    edits.push(Edit { at: a, del: b - a, ins: out });
                }
                Err(e) => return self.sad(&e),
            }
        }
        self.buf.edit(EditKind::Other, edits);
        self.notify(&format!("piped through {cmd}"), false);
    }
}

impl App {
    /// Insert text art above the caret's line, as comments when the file's language has them.
    pub(super) fn banner_insert(&mut self, text: &str, boxed: bool) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let lines = if boxed { crate::banner::framed(text, '#') } else { crate::banner::art(text) };
        if lines.iter().all(String::is_empty) {
            return self.sad("banner: no drawable characters (A-Z 0-9 and a few symbols)");
        }
        let first: String = self.buf.rope().line(0).chars().take(80).collect();
        let style = crate::comment::style_for(self.buf.path(), &first);
        let block = crate::banner::comment(&lines, style);
        let at = self.buf.line_start(self.buf.line_of(self.buf.primary().head));
        self.buf.edit(EditKind::Other, vec![Edit { at, del: 0, ins: block }]);
        self.buf.break_group();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipes_and_reports_failures() {
        assert_eq!(run("sort", "b\na\n", Duration::from_secs(5)).unwrap(), "a\nb\n");
        assert!(run("cat >/dev/null; echo oops >&2; exit 3", "x", Duration::from_secs(5)).unwrap_err().contains("oops"));
        assert!(run("sleep 5", "", Duration::from_millis(100)).unwrap_err().contains("timed out"));
    }
}
