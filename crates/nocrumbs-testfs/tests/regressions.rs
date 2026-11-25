// SPDX-License-Identifier: GPL-3.0-or-later
//! Bugs found by the property tests (`fuzz.rs`), pinned as readable keystroke scripts.

use std::sync::Arc;

use nocrumbs_core::sim::Session;
use nocrumbs_core::{Bar, Fs, Key, Tone};
use nocrumbs_testfs::{MemFs, MemFsBuilder};

const HOME: &str = "/home/user";

fn setup() -> (Session, Arc<MemFs>) {
    let fs = Arc::new(MemFs::sample(true, 20));
    (Session::new(Bar::new(fs.clone(), HOME, HOME, '/')), fs)
}

fn run(s: &mut Session, line: &str) {
    s.select_all();
    s.type_str(line);
    s.key(Key::Enter);
}

fn exists(s: &Session, fs: &MemFs) {
    assert_eq!(fs.canonical_dir(s.bar().cwd()).as_deref(), Some(s.bar().cwd()), "the current folder must exist");
}

#[test]
fn going_back_never_lands_in_a_deleted_folder() {
    // Shrunk by proptest: up a folder, select all, delete, go back.
    let (mut s, fs) = setup();
    s.key(Key::ParentFolder);
    s.key(Key::SelectAll);
    s.key(Key::Delete);
    s.key(Key::Back);
    assert_eq!(s.bar().cwd(), "/home", "the deleted folder is skipped");
    assert_eq!(s.bar().status(), ("Nothing to go back to", Tone::Info));
    exists(&s, &fs);

    // The history entry is kept: once the delete is undone, Back reaches it again.
    s.key(Key::Undo);
    s.key(Key::Back);
    assert_eq!(s.bar().cwd(), HOME);
    exists(&s, &fs);
}

#[test]
fn history_steps_over_a_stale_entry_in_the_middle() {
    let (mut s, fs) = setup();
    run(&mut s, "Music");
    run(&mut s, "/etc");
    fs.delete_forever("/home/user/Music").unwrap();
    s.key(Key::Back);
    assert_eq!(s.bar().cwd(), HOME, "straight past the missing folder");
    s.key(Key::Forward);
    assert_eq!(s.bar().cwd(), "/etc");
}

#[test]
fn undoing_the_folder_you_are_in_moves_you_out_of_it() {
    let (mut s, fs) = setup();
    run(&mut s, ":mkdir scratch");
    run(&mut s, "scratch");
    assert_eq!(s.bar().cwd(), "/home/user/scratch");
    run(&mut s, ":undo");
    assert_eq!(s.bar().cwd(), HOME);
    assert_eq!(s.text(), "~", "and the bar says where you are");
    exists(&s, &fs);
}

#[test]
fn a_folder_removed_underneath_you_is_left_for_the_nearest_survivor() {
    let (mut s, fs) = setup();
    run(&mut s, "Documents/recipes");
    fs.delete_forever("/home/user/Documents").unwrap(); // something else removed it
    s.bar_mut().refresh();
    assert_eq!(s.bar().cwd(), HOME);
    assert_eq!(s.text(), "~");
    assert_eq!(s.bar().status(), ("That folder is gone. Now in ~", Tone::Warn));
    exists(&s, &fs);
}

#[test]
fn completion_keeps_every_character_when_lower_casing_changes_the_length() {
    // Found by proptest: `İ` lower-cases to two characters, so cutting the ghost by the number
    // of typed characters silently dropped the next letter of the name.
    let mut b = MemFsBuilder::new(true);
    b.file("/d/\u{130}stanbul.txt", 1, 0);
    let fs = Arc::new(b.finish());
    let mut s = Session::new(Bar::new(fs, "/d", "/d", '/'));
    s.type_str("i\u{307}s");
    assert_eq!(s.text(), "i\u{307}stanbul.txt", "the ghost is the whole rest of the name");
    s.key(Key::Tab);
    assert_eq!(s.text(), "\u{130}stanbul.txt", "and accepting restores the real name");
}

#[test]
fn half_of_an_expanded_character_does_not_match() {
    let mut b = MemFsBuilder::new(true);
    b.file("/d/\u{130}stanbul.txt", 1, 0);
    let mut s = Session::new(Bar::new(Arc::new(b.finish()), "/d", "/d", '/'));
    s.type_str("i");
    assert_eq!(s.bar().ghost_chars(), 0, "no ghost rather than a wrong one");
}
