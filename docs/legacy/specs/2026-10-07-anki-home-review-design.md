# Anki import/export and home daily review — design

Sub-project 4 of the Study section (ROADMAP.md "Study"). Builds on `2026-10-06-study-cards-design.md` (clozes in notes, `cards/index`, schedules) and `2026-10-07-study-stats-design.md`.

## Intent

Move cards between the legacy editor and Anki, and show what is due today on the home screen so a review is one keypress away. Nothing to learn: every feature starts from Ctrl+K or the home screen, no syntax is typed.

Notes stay the source of truth; cards stay `{{cN::text}}` clozes.

Out of scope: importing Anki schedules or review history (imported cards start new, because card ids hash path and text), media files, deck hierarchy beyond one note per deck, the new `.anki21b` (Zstd/protobuf) format, typed or multiple-choice quiz.

## 1. `ankiio.rs` (pure)

No UI, no direct file reads except where noted. Unit-tested.

### Field mapping
- Cloze note: `{{c1::x}}` kept; Anki hints `{{c1::x::hint}}` drop the hint.
- Basic note (front, back): becomes `Front {{c1::Back}}`. Braces `{{`/`}}` in a field make the card skipped and counted in the report (same rule as `cards::new_card`).
- Basic (and reversed) notes: only the forward card is made.
- HTML in fields is reduced to text: `<br>`/`<div>`/`<p>` become newlines, other tags dropped, entities decoded (`&amp; &lt; &gt; &quot; &nbsp; &#N;`).

### Import output
One note per deck: `<config>/anki/<deck>.md` (deck name sanitised to a file name; `::` in a deck name becomes ` - `). Layout: `# <deck>`, then one paragraph per Anki note, separated by blank lines. Re-importing a deck replaces that file's imported paragraphs: the whole file is rewritten, so edits made to an imported note are lost; the confirmation prompt says so when the file exists. The note is added to `cards/index` (`cards::index_add`) so Review picks it up at once.

### TSV
- Import: Anki "Notes in plain text". Lines starting `#` are headers (`#separator`, `#html`, `#columns` honoured only for `#separator:tab`; anything else is refused with a message). Column 1 front, column 2 back; a single column is a cloze text. Result goes to `<config>/anki/<file stem>.md`.
- Export: one row per paragraph block of a note that holds clozes, `text<TAB>` (cloze text kept as `{{cN::..}}`, newlines as `<br>`, tabs as spaces), with `#separator:tab`, `#html:true` and `#notetype:Cloze` headers.

### `.apkg`
External tools, no new crates (same pattern as the ImageMagick fallback): `unzip`/`zip` and `sqlite3` run through `std::process::Command`, in a fresh temp directory. A missing tool gives a SAD-style error naming it.
- Import reads `collection.anki21` if present, else `collection.anki2`. If the zip only holds `collection.anki21b`, the error says to export from Anki with "Support older Anki versions" ticked. Decks from `col.decks` (JSON), notes from `notes` (fields split on `\x1f`), card-to-deck from `cards.did`; note type is cloze when `col.models` flags it (`"type": 1`), else basic.
- Export builds a minimal collection (`col`, `notes`, `cards`, empty `revlog`/`graves`) with one Cloze model and one deck, one Anki note per paragraph block, one Anki card per distinct cloze number, then zips it with `media` = `{}`.

## 2. Commands (Ctrl+K, context-sensitive)

- "Export cards (TSV)" and "Export cards (Anki .apkg)": exports the current note when it holds clozes, else every note in `cards/index`. Prompts for the output path (default `<note stem>.tsv` / `.apkg` beside the note; `~/` for all notes). Overwrite asks first.
- "Import cards…": path prompt; the extension picks TSV or `.apkg`. Reports "N cards in M decks, K skipped" and opens the first imported note.
- Entries appear in the study mode palette and in the Ctrl+K prose menu only where it makes sense (export when the note has clozes; import always).

## 3. Home screen daily review

- A "Due today" panel beside "On this day" in `welcome.rs`: `N due · M new`, where M respects the daily new-card limit and exam mode (it is `cards::build_deck` run over the indexed notes, counted by schedule status).
- Computed when the home screen opens and after each review session, not every frame.
- Zero due shows "Nothing due".
- Clicking the line starts a review (the `[Review]` menu entry stays as the keyboard route).

## Code

`ankiio.rs` (pure), `app/ankiui.rs` (prompts, file and process I/O, report), `welcome.rs` (panel), `commands.rs` (entries). Docs: FEATURES.md "Milestone 42", ROADMAP.md row and Study list, ARCHITECTURE.md.

## Testing

- Unit: HTML reduction, field mapping (cloze, basic, hint, braces), TSV round trip and header handling, deck-name sanitising, import-file layout.
- `.apkg`: tests build a fixture with `sqlite3` + `zip` (skipped with a message when the tools are absent), import it, export, and re-import the export.
- Welcome: panel text for zero/some due.
- tmux with a throwaway `HOME`: home panel, import a TSV and an `.apkg`, review an imported card, export.
