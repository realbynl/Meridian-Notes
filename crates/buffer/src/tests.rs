use super::*;
use proptest::prelude::*;

fn text(b: &Buffer) -> String {
    b.rope().to_string()
}

#[test]
fn insert_and_undo_coalesces_typing() {
    let mut b = Buffer::default();
    for c in "hello".chars() {
        b.insert(&c.to_string());
    }
    assert_eq!(text(&b), "hello");
    assert!(b.undo());
    assert_eq!(text(&b), "");
    assert!(b.redo());
    assert_eq!(text(&b), "hello");
    assert_eq!(b.primary(), Range::caret(5));
}

#[test]
fn cursor_move_breaks_undo_group() {
    let mut b = Buffer::default();
    b.insert("a");
    b.break_group();
    b.insert("b");
    b.undo();
    assert_eq!(text(&b), "a");
}

#[test]
fn multi_cursor_insert_keeps_ranges_valid() {
    let mut b = Buffer::from_str("aaa\nbbb\nccc");
    b.set_sels(vec![Range::caret(0), Range::caret(4), Range::caret(8)]);
    b.insert("X");
    assert_eq!(text(&b), "Xaaa\nXbbb\nXccc");
    let carets: Vec<usize> = b.sels().iter().map(|r| r.head).collect();
    assert_eq!(carets, vec![1, 6, 11]);
    b.undo();
    assert_eq!(text(&b), "aaa\nbbb\nccc");
    assert_eq!(b.sels().len(), 3);
}

#[test]
fn multi_cursor_replace_selections_and_backspace() {
    let mut b = Buffer::from_str("one two three");
    b.set_sels(vec![
        Range { anchor: 0, head: 3 },
        Range { anchor: 4, head: 7 },
        Range {
            anchor: 8,
            head: 13,
        },
    ]);
    b.insert("#");
    assert_eq!(text(&b), "# # #");
    b.backspace();
    assert_eq!(text(&b), "  ");
    b.undo();
    assert_eq!(text(&b), "# # #");
}

#[test]
fn touching_carets_backspace_does_not_overdelete() {
    let mut b = Buffer::from_str("abcd");
    b.set_sels(vec![Range::caret(2), Range::caret(3)]);
    b.backspace();
    assert_eq!(text(&b), "ad");
}

#[test]
fn backspace_and_delete_coalesce() {
    let mut b = Buffer::from_str("abcdef");
    b.set_sels(vec![Range::caret(3)]);
    b.backspace();
    b.backspace();
    assert_eq!(text(&b), "adef");
    b.undo();
    assert_eq!(text(&b), "abcdef");
    b.delete_forward();
    b.delete_forward();
    assert_eq!(text(&b), "abcf");
    b.undo();
    assert_eq!(text(&b), "abcdef");
}

#[test]
fn graphemes_are_atomic() {
    // e + combining acute, then a flag (two regional indicators).
    let mut b = Buffer::from_str("e\u{301}\u{1F1E9}\u{1F1EA}x");
    assert_eq!(b.next_grapheme(0), 2);
    assert_eq!(b.next_grapheme(2), 4);
    assert_eq!(b.prev_grapheme(4), 2);
    assert_eq!(b.prev_grapheme(2), 0);
    b.set_sels(vec![Range::caret(4)]);
    b.backspace();
    assert_eq!(text(&b), "e\u{301}x");
}

#[test]
fn line_geometry_and_columns() {
    let b = Buffer::from_str("ab\n\tx\r\nlast");
    assert_eq!(b.line_count(), 3);
    assert_eq!(b.line_end(0), 2);
    assert_eq!(b.line_end(1), 5); // excludes \r\n
    assert_eq!(b.display_col(4, 4), 4); // after the tab
    assert_eq!(b.pos_at_col(1, 2, 4), 3); // inside the tab: stay before it
    assert_eq!(b.pos_at_col(1, 99, 4), 5);
}

#[test]
fn dirty_tracking_follows_undo() {
    let mut b = Buffer::from_str("x");
    assert!(!b.is_dirty());
    b.insert("y");
    assert!(b.is_dirty());
    b.undo();
    assert!(!b.is_dirty());
    b.redo();
    assert!(b.is_dirty());
}

#[test]
fn save_and_reopen_roundtrip() {
    let dir = std::env::temp_dir().join(format!("sub-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join("a.txt");
    let mut b = Buffer::from_str("héllo\nworld");
    b.save_as(&p).unwrap();
    assert!(!b.is_dirty());
    let c = Buffer::open(&p).unwrap();
    assert_eq!(text(&c), "héllo\nworld");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn undo_cap_is_enforced() {
    let mut b = Buffer::default();
    for _ in 0..MAX_UNDO + 50 {
        b.insert("ab\n"); // multi-char: never coalesces
    }
    let mut n = 0;
    while b.undo() {
        n += 1;
    }
    assert_eq!(n, MAX_UNDO);
}

#[derive(Debug, Clone)]
enum Op {
    Ins(usize, String),
    Del(usize, usize),
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    let op = prop_oneof![
        (0usize..1000, "[a-z\u{e9}\n ]{0,6}").prop_map(|(p, s)| Op::Ins(p, s)),
        (0usize..1000, 0usize..8).prop_map(|(p, n)| Op::Del(p, n)),
    ];
    prop::collection::vec(op, 0..40)
}

proptest! {
    // Rope edits match a Vec<char> model; undo-all restores the original;
    // redo-all restores the final text.
    #[test]
    fn edits_match_model_and_undo_redo_roundtrip(init in "[a-z\n]{0,30}", ops in ops()) {
        let mut b = Buffer::from_str(&init);
        let mut model: Vec<char> = init.chars().collect();
        for op in &ops {
            match op {
                Op::Ins(p, s) => {
                    let at = p % (model.len() + 1);
                    b.edit(EditKind::Other, vec![Edit { at, del: 0, ins: s.clone() }]);
                    model.splice(at..at, s.chars());
                }
                Op::Del(p, n) => {
                    let at = p % (model.len() + 1);
                    let del = (*n).min(model.len() - at);
                    b.edit(EditKind::Other, vec![Edit { at, del, ins: String::new() }]);
                    model.drain(at..at + del);
                }
            }
            prop_assert_eq!(text(&b), model.iter().collect::<String>());
        }
        let final_text = text(&b);
        while b.undo() {}
        prop_assert_eq!(text(&b), init);
        while b.redo() {}
        prop_assert_eq!(text(&b), final_text);
    }

    // Many carets inserting at once equals inserting right-to-left by hand.
    #[test]
    fn multi_cursor_insert_matches_reverse_application(
        init in "[a-z\n]{0,40}",
        mut carets in prop::collection::vec(0usize..50, 1..6),
    ) {
        let len = init.chars().count();
        for c in carets.iter_mut() { *c %= len + 1; }
        carets.sort();
        carets.dedup();
        let mut b = Buffer::from_str(&init);
        b.set_sels(carets.iter().map(|&c| Range::caret(c)).collect());
        b.insert("XY");
        let mut model: Vec<char> = init.chars().collect();
        for &c in carets.iter().rev() { model.splice(c..c, "XY".chars()); }
        prop_assert_eq!(text(&b), model.iter().collect::<String>());
        b.undo();
        prop_assert_eq!(text(&b), init);
    }
}

#[test]
fn undo_byte_cap_keeps_newest() {
    let mut b = Buffer::default();
    let big = "x".repeat(MAX_UNDO_BYTES / 2 + 1);
    for _ in 0..3 {
        b.insert(&big);
    }
    let mut n = 0;
    while b.undo() {
        n += 1;
    }
    assert_eq!(n, 1);
}

#[test]
fn non_utf8_roundtrips_as_latin1() {
    let dir = std::env::temp_dir().join(format!("sub-latin1-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join("f.txt");
    let bytes = b"caf\xe9 \xff\x80\n".to_vec();
    fs::write(&p, &bytes).unwrap();
    let mut b = Buffer::open(&p).unwrap();
    assert_eq!(b.encoding, Encoding::Latin1);
    b.save().unwrap();
    assert_eq!(fs::read(&p).unwrap(), bytes);
    b.insert("€"); // not representable: falls back to UTF-8
    b.save().unwrap();
    assert_eq!(b.encoding, Encoding::Utf8);
    assert!(String::from_utf8(fs::read(&p).unwrap())
        .unwrap()
        .contains('€'));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn word_helpers() {
    let b = Buffer::from_str("foo  bar_baz, qux");
    assert_eq!(b.word_at(1), Some((0, 3)));
    assert_eq!(b.word_at(3), Some((0, 3))); // touching the end
    assert_eq!(b.word_at(4), None); // between spaces
    assert_eq!(b.word_at(6), Some((5, 12))); // `_` is a word char
    assert_eq!(b.next_word(0), 3);
    assert_eq!(b.next_word(3), 12);
    assert_eq!(b.next_word(15), 17);
    assert_eq!(b.next_word(17), 17);
    assert_eq!(b.prev_word(17), 14);
    assert_eq!(b.prev_word(14), 5);
    assert_eq!(b.prev_word(2), 0);
}

#[test]
fn word_moves_stop_at_punctuation_runs() {
    let b = Buffer::from_str("foo.bar  ::baz_1");
    let mut p = 0;
    let mut stops = vec![];
    for _ in 0..5 {
        p = b.move_word_right(p);
        stops.push(p);
    }
    assert_eq!(stops, [3, 4, 7, 11, 16]);
    let mut p = 16;
    let mut stops = vec![];
    for _ in 0..5 {
        p = b.move_word_left(p);
        stops.push(p);
    }
    assert_eq!(stops, [11, 9, 4, 3, 0]);
}

#[test]
fn save_keeps_symlink() {
    let dir = std::env::temp_dir().join(format!("sub-link-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let real = dir.join("real.txt");
    let link = dir.join("link.txt");
    fs::write(&real, "old\n").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let mut b = Buffer::open(&link).unwrap();
    b.insert("new ");
    b.save().unwrap();
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&real).unwrap(), "new old\n");
    assert!(!b.disk_changed());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn detects_and_reloads_external_change() {
    let dir = std::env::temp_dir().join(format!("sub-ext-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let f = dir.join("a.txt");
    fs::write(&f, "one\n").unwrap();
    let mut b = Buffer::open(&f).unwrap();
    assert!(!b.disk_changed());
    fs::write(&f, "two two\n").unwrap(); // length differs, so detected even within one mtime tick
    assert!(b.disk_changed());
    b.keep_disk();
    assert!(!b.disk_changed());
    fs::write(&f, "three three three\n").unwrap();
    assert!(b.disk_changed());
    b.reload().unwrap();
    assert_eq!(text(&b), "three three three\n");
    assert!(!b.is_dirty() && !b.disk_changed());
    assert!(b.undo());
    assert_eq!(text(&b), "one\n");
    assert!(b.is_dirty());
    fs::remove_dir_all(&dir).unwrap();
}

fn tmp(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("meridian-enc-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join("f.txt");
    fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn crlf_is_kept_on_save() {
    let p = tmp("crlf", b"a\r\nb\r\nc");
    let mut b = Buffer::open(&p).unwrap();
    assert_eq!(b.eol, LineEnding::Crlf);
    assert!(!b.mixed_eol);
    assert_eq!(b.rope().to_string(), "a\nb\nc");
    b.insert("x");
    b.save().unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"xa\r\nb\r\nc");
}

#[test]
fn mixed_endings_unify_to_the_dominant_one() {
    let p = tmp("mixed", b"a\r\nb\r\nc\nd\re");
    let mut b = Buffer::open(&p).unwrap();
    assert_eq!((b.eol, b.mixed_eol), (LineEnding::Crlf, true));
    assert_eq!(b.rope().to_string(), "a\nb\nc\nd\ne");
    b.save().unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"a\r\nb\r\nc\r\nd\r\ne");
}

#[test]
fn convert_line_endings_marks_dirty() {
    let p = tmp("conv", b"a\nb\n");
    let mut b = Buffer::open(&p).unwrap();
    assert!(!b.is_dirty());
    b.set_eol(LineEnding::Crlf);
    assert!(b.is_dirty());
    b.save().unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"a\r\nb\r\n");
}

#[test]
fn utf16_roundtrips() {
    for (enc, bom, le) in [(Encoding::Utf16Le, [0xffu8, 0xfe], true), (Encoding::Utf16Be, [0xfe, 0xff], false)] {
        let mut bytes = bom.to_vec();
        for u in "h\u{e9}llo \u{1f600}\r\nx".encode_utf16() {
            bytes.extend(if le { u.to_le_bytes() } else { u.to_be_bytes() });
        }
        let p = tmp("u16", &bytes);
        let mut b = Buffer::open(&p).unwrap();
        assert_eq!(b.encoding, enc);
        assert_eq!(b.rope().to_string(), "h\u{e9}llo \u{1f600}\nx");
        b.save().unwrap();
        assert_eq!(fs::read(&p).unwrap(), bytes);
        assert!(!Buffer::looks_binary(&p).unwrap());
    }
}

#[test]
fn utf8_bom_is_kept_not_shown() {
    let p = tmp("bom", b"\xef\xbb\xbfhi\n");
    let mut b = Buffer::open(&p).unwrap();
    assert_eq!(b.encoding, Encoding::Utf8Bom);
    assert_eq!(b.rope().to_string(), "hi\n");
    b.save().unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"\xef\xbb\xbfhi\n");
    b.set_encoding(Encoding::Utf8);
    b.save().unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"hi\n");
}

#[test]
fn binary_is_detected_and_dumped() {
    let p = tmp("bin", b"\x7fELF\x00\x01hello world!!\x00\xff");
    assert!(Buffer::looks_binary(&p).unwrap());
    let (v, size) = Buffer::hex_view(&p, 8).unwrap();
    assert_eq!(size, 21);
    assert_eq!(v.rope().to_string(), "00000000  7f 45 4c 46 00 01 68 65                           |.ELF..he|\n");
    assert!(!v.is_dirty());
    let t = tmp("txt", b"plain text\n");
    assert!(!Buffer::looks_binary(&t).unwrap());
}

#[test]
fn encoded_matches_what_save_writes() {
    let p = tmp("enc", b"a\r\nb");
    let b = Buffer::open(&p).unwrap();
    assert_eq!(b.encoded(), b"a\r\nb");
}
