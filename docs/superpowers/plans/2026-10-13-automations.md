# Automations and RSS (n8n-style) (outline plan)

> **For agentic workers:** This is an outline. It depends on decisions not yet made (Lua, D11; an HTTP route; a model plugin, D2). Expand each task with `superpowers:writing-plans` only after the "Blocked on" items below are answered. Do not start before the plugin protocol additions in Task 1 are reviewed.

**Goal:** A user can define automations: *a trigger* (schedule, new RSS item, later a webhook or file change) runs *steps* (fetch, filter, summarise, write a note, tag, append to the daily note). Example: every morning, fetch five feeds, keep items matching a filter, summarise each with the user's model, and write one note per item under `Feeds/` with the source link. Roadmap D16.

**Architecture:** A plugin, not core. A runner process (`meridian-automate`, an out-of-process plugin on the existing protocol) owns the schedule, runs steps, and writes notes through the editor's plugin API or directly into the vault with path checks. Step logic is data first (a small declarative file); Lua (D11) is the escape hatch for custom steps. Summarising calls the user-supplied model plugin (D2); with no model, the step falls back to a title plus first N characters.

## Decisions (2026-10-13) and what is still open
1. **HTTP route: `curl` (decided 2026-10-13).** `curl` ships with Windows 10+, macOS and nearly every Linux, needs no dependency and matches the project's shell-out style. Hide it behind a `Fetcher` trait so an HTTP crate (`ureq`, in the runner binary only, which does not count against the editor's 15 MB budget) can replace it later without touching the steps. 
2. **Lua: approved** (crate `mlua`). Luau is the chosen variant (user took the recommendation, 2026-10-13) (sandbox and interrupt hook built in); Lua 5.4 is the alternative. The declarative format ships first; Lua steps arrive in Task 9.
3. **Background runs: wanted, behind a setting.** `automations.background = off | on` in `settings.conf` (default `off`, additive key). When turned on, Meridian installs one OS timer entry that runs `meridian-automate tick` every 5 minutes; `tick` runs whatever is due. One entry keeps all scheduling logic in our code instead of per-OS cron syntax. Per OS: Linux `systemd --user` timer (cron fallback), macOS a `launchd` agent in `~/Library/LaunchAgents`, Windows Task Scheduler via `schtasks`. Turning the setting off removes the entry. Commands: `meridian-automate install-timer`, `uninstall-timer`, `timer-status`. A lock file per automation stops the app and the background runner running the same one at once.
4. **RSS parsing: `quick-xml` in the runner only (approved 2026-10-13).** A hand-written XML parser is where bugs live (CDATA, entities, namespaces, encodings); `quick-xml` is small and dependency-light. The alternative is a minimal std parser. 

Still to ask: the model-plugin protocol (D2 plugin does not exist yet, so summarise uses the excerpt fallback until it does).

## Global Constraints
- Out-of-process; a runner crash, hang or flood never affects the editor (toast only). Protocol additions are additive (CLAUDE.md compat rule).
- Network and file writes need explicit permission grants shown to the user per automation (never silent), listing domains and target folders. Default deny. Refuse writes outside the vault (`path_is_inside` style) and refuse `..`.
- Secrets (API keys, tokens) are never stored in the automation file or the vault; read from the environment or a user-owned file outside the vault with 0600 permissions.
- Background runs have no UI: errors go to the run log and an optional desktop notification; the same permissions apply; a missed run runs once on the next tick, never a backlog storm; the timer entry is never installed without the setting on and is removed when it is turned off.
- Be a good citizen: honour `ETag`/`Last-Modified`, cap response size (default 2 MB), per-host rate limit, timeouts, a user-agent that names Meridian, and respect `robots.txt` for scraping steps (not RSS fetches the user asked for).
- Idempotent by design: each item is keyed (feed URL + item GUID or link hash) in a state file so a rerun never duplicates notes; the state file is rebuildable from the notes' front matter.
- A visible run log (what ran, what it fetched, what it wrote, errors), kept under the config dir with rotation. "Dry run" prints what would be written without writing.
- Untrusted content: feed text is data, never instructions. Summarising passes it to the model plugin as quoted data; steps never execute text from a feed; written notes escape or strip anything that could break the note's own syntax (`{{embeds}}`, `[[links]]` created from feed text must not resolve to vault files).
- Budgets: no effect on startup or typing; idle CPU 0% when no automation is due.
- `cargo test --workspace` and `cargo build --release` pass; build with `CARGO_TARGET_DIR` in the scratchpad.

## Automation file (draft)
`<config>/meridian/automations/<name>.conf` (key = value, plus step blocks), or `.lua` once D11 lands.
```
name = Morning feeds
every = daily 07:30            # or: every 30m, weekly mon 09:00, on-start
permit.net = hnrss.org, example.com
permit.write = Feeds/
source.1 = rss https://hnrss.org/frontpage
filter = title matches "rust|notes"
limit = 10
step.1 = summarise max=80words
step.2 = note path="Feeds/{date}/{slug}.md" template=feed-item
step.3 = append-daily line="- [[{note}]]"
```
Template variables: `{title} {link} {date} {slug} {summary} {feed}`. A `feed-item` note gets front matter (`source`, `guid`, `fetched`) so state is rebuildable.

## Tasks (outline)
1. **Protocol and runner skeleton.** Plugin messages to register an automation, request a run, report status; `meridian-automate` binary under `crates/plugins/src/bin/`; `run <name>`, `list`, `dry-run <name>` subcommands usable from a shell. Tests with a fake clock and a fake fetcher.
2. **Parser for the automation file.** Table tests; unknown keys warn; bad schedule errors with line number.
3. **Scheduler.** Next-run computation (daily, weekly, interval, on-start), missed-run policy (run once on start if missed, never a backlog storm), time zones (local time), DST edge cases, clock changes. Pure function with exhaustive tests.
4. **Fetcher.** Per the HTTP decision; conditional GET, size and time caps, redirect limit, scheme allow-list (`https`, plus `http` if the user allows), refusal of private/loopback addresses unless allowed, so a feed cannot be used to probe the local network.
5. **Feed parser.** RSS 2.0 and Atom; CDATA, entities, relative links, missing GUIDs, huge items, bad encodings; fixture feeds including malformed ones; fuzz test (no panic, bounded time).
6. **Steps.** `filter`, `limit`, `note`, `append-daily`, `tag`, `summarise` (model plugin or excerpt fallback), template expansion with escaping per the untrusted-content rule. Idempotency state file.
7. **Permissions and run log.** Permission prompt on first run and on any change to `permit.*`; log viewer command in the palette (`automation-log`).
7b. **Background timer.** `install-timer`/`uninstall-timer`/`timer-status` for systemd user timer, launchd agent and `schtasks`; generated unit text is unit-tested as strings on every OS; real installs are tested manually per OS (Windows on the user's Windows machine, one command per session). Lock-file test: two runners, one automation, one run.
8. **Editor integration.** Palette commands: `automation-run`, `automation-dry-run`, `automation-log`, `automation-new` (template). Toasts on failure. Settings keys additive.
9. **Lua steps (after D11).** `step.N = lua file.lua` with a sandbox: no `os`/`io`, a `net.get` and `vault.write` that go through the same permission checks, instruction limit, memory limit.
10. **Visual editor (later).** Nodes as Endless containers connected by lines, stored in the layout format. Not planned until the Endless view exists.
11. **Docs and review.** `docs/automations.md` with the n8n-concept mapping, security notes, examples. Review pass focused on injection and path escape.

## Review Focus
- Prompt/markup injection from feed content into the note and into the model prompt.
- Path traversal in `note path=` templates (slug from feed titles: strip separators, `..`, control characters, reserved Windows names, length cap).
- Duplicate notes after a crash mid-run (write note first, then state; or two-phase with a temp file).
- A feed that returns 10,000 items, a 2 GB body, an endless redirect, slow-loris.
- Time: DST jumps, laptop sleep, the app starting at 07:31 for a 07:30 schedule.
- Privacy: the runner makes network requests the user did not trigger interactively; list them in the log; no telemetry; no requests to hosts outside `permit.net`.
