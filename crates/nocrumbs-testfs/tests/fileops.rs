// SPDX-License-Identifier: GPL-3.0-or-later
//! Keystroke scripts for file operations through the bar (brief §8, §9): create, rename, trash,
//! copy/cut/paste with collisions and the folder-into-itself guard, multi-select, and undo/redo.
//! Every script runs against the real `Bar` and the in-memory filesystem, headless.

use std::sync::Arc;

use nocrumbs_core::sim::Session;
use nocrumbs_core::{Bar, Entry, Fs, Key, Tone};
use nocrumbs_testfs::MemFs;

const HOME: &str = "/home/user";

fn setup_at(big_n: usize, ci: bool) -> (Session, Arc<MemFs>) {
    let fs = Arc::new(MemFs::sample(ci, big_n));
    let bar = Bar::new(fs.clone(), HOME, HOME, '/');
    (Session::new(bar), fs)
}

/// Windows-like (case-insensitive), 100 entries in /big.
fn setup() -> (Session, Arc<MemFs>) {
    setup_at(100, true)
}

/// Select everything (as after an error), type a line, press Enter.
fn run(s: &mut Session, line: &str) {
    s.select_all();
    s.type_str(line);
    s.key(Key::Enter);
}

fn status(s: &Session) -> (String, Tone) {
    let (m, t) = s.bar().status();
    (m.to_string(), t)
}

fn names(s: &Session) -> Vec<String> {
    (0..s.bar().row_count()).map(|i| s.bar().row(i).name.clone()).collect()
}

fn on_disk(fs: &MemFs, dir: &str) -> Vec<String> {
    fs.entries(dir).expect("dir").iter().map(|e: &Entry| e.name.clone()).collect()
}

fn has(fs: &MemFs, dir: &str, name: &str) -> bool {
    on_disk(fs, dir).iter().any(|n| n == name)
}

/// Select these names in the shown list: the first by click, the rest by Ctrl+click.
fn select(s: &mut Session, wanted: &[&str]) {
    for (i, name) in wanted.iter().enumerate() {
        let row =
            (0..s.bar().row_count()).find(|&r| s.bar().row(r).name == *name).unwrap_or_else(|| panic!("no row {name}"));
        s.bar_mut().click_row(row, i > 0, false);
    }
}

// ----- :mkdir / :touch -----

#[test]
fn mkdir_creates_selects_and_is_undoable() {
    let (mut s, fs) = setup();
    run(&mut s, ":mkdir Archive");
    assert_eq!(status(&s), ("Created folder Archive".into(), Tone::Good));
    assert!(has(&fs, HOME, "Archive"));
    assert_eq!(s.bar().selected_name(), Some("Archive"), "the new folder is selected");
    assert_eq!(s.text(), "~", "the bar is idle again");

    run(&mut s, ":undo");
    assert_eq!(status(&s), ("Undid: Created folder Archive".into(), Tone::Good));
    assert!(!has(&fs, HOME, "Archive"));
    assert!(!names(&s).contains(&"Archive".to_string()), "the list follows");

    run(&mut s, ":redo");
    assert!(has(&fs, HOME, "Archive"));
    assert!(names(&s).contains(&"Archive".to_string()));
}

#[test]
fn touch_creates_an_empty_file_and_reports_collisions() {
    let (mut s, fs) = setup();
    run(&mut s, ":touch todo.txt");
    assert_eq!(status(&s).0, "Created file todo.txt");
    assert!(has(&fs, HOME, "todo.txt"));
    run(&mut s, ":touch TODO.TXT");
    assert_eq!(status(&s), ("TODO.TXT already exists here".into(), Tone::Bad), "case-insensitive collision");
    run(&mut s, ":mkdir Documents");
    assert_eq!(status(&s).1, Tone::Bad);
    assert_eq!(s.bar().error_count(), 2);
}

#[test]
fn create_ignores_the_selection_and_the_highlight() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":mkdir fresh");
    assert!(has(&fs, HOME, "fresh"), "created in the current folder");
    assert!(has(&fs, HOME, "notes.md"), "and nothing else touched");
}

// ----- :rename -----

#[test]
fn rename_the_selected_entry_and_undo_it() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":rename journal.md");
    assert_eq!(status(&s), ("Renamed notes.md to journal.md".into(), Tone::Good));
    assert!(has(&fs, HOME, "journal.md") && !has(&fs, HOME, "notes.md"));
    assert_eq!(s.bar().selected_name(), Some("journal.md"));
    run(&mut s, ":undo");
    assert!(has(&fs, HOME, "notes.md") && !has(&fs, HOME, "journal.md"));
    run(&mut s, ":redo");
    assert!(has(&fs, HOME, "journal.md"));
}

#[test]
fn rename_needs_exactly_one_selected_entry() {
    let (mut s, _) = setup();
    run(&mut s, ":rename x");
    assert_eq!(status(&s), ("Select an entry first".into(), Tone::Bad));
    select(&mut s, &["notes.md", "shopping-list.txt"]);
    run(&mut s, ":rename x");
    assert_eq!(status(&s), ("Select exactly one entry to rename".into(), Tone::Bad));
}

#[test]
fn rename_collisions_and_case_only_changes() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":rename SHOPPING-LIST.TXT");
    assert_eq!(status(&s), ("SHOPPING-LIST.TXT already exists here".into(), Tone::Bad));
    run(&mut s, ":rename NOTES.md");
    assert_eq!(status(&s).1, Tone::Good, "a change of case is the same entry on Windows");
    assert!(has(&fs, HOME, "NOTES.md"));
    select(&mut s, &["NOTES.md"]);
    run(&mut s, ":rename NOTES.md");
    assert_eq!(status(&s), ("NOTES.md already has that name".into(), Tone::Info));
}

#[test]
fn renaming_a_folder_keeps_its_contents() {
    let (mut s, fs) = setup();
    select(&mut s, &["Documents"]);
    run(&mut s, ":rename Papers");
    assert!(has(&fs, "/home/user/Papers/recipes", "ramen.md"));
    run(&mut s, "Papers");
    assert_eq!(s.bar().cwd(), "/home/user/Papers");
}

// ----- :rm and the trash -----

#[test]
fn rm_moves_the_selection_to_the_trash_and_undo_restores_it() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md", "shopping-list.txt"]);
    run(&mut s, ":rm");
    assert_eq!(status(&s), ("Moved 2 items to the trash. :undo brings them back".into(), Tone::Good));
    assert_eq!(fs.trash_len(), 2);
    assert!(!has(&fs, HOME, "notes.md") && !has(&fs, HOME, "shopping-list.txt"));
    assert_eq!(s.bar().selection_len(), 0);
    run(&mut s, ":undo");
    assert_eq!(status(&s).0, "Undid: Moved 2 items to the trash");
    assert!(has(&fs, HOME, "notes.md") && has(&fs, HOME, "shopping-list.txt"));
    assert_eq!(fs.trash_len(), 0);
}

#[test]
fn rm_a_folder_and_undo_brings_everything_back() {
    let (mut s, fs) = setup();
    select(&mut s, &["Documents"]);
    run(&mut s, ":rm");
    assert_eq!(status(&s).0, "Moved Documents to the trash. :undo brings it back");
    assert_eq!(fs.canonical_dir("/home/user/Documents/recipes"), None);
    run(&mut s, ":undo");
    assert!(has(&fs, "/home/user/Documents/recipes", "pancakes.md"));
}

#[test]
fn only_the_selection_is_ever_deleted_never_the_highlight() {
    let (mut s, fs) = setup();
    s.type_str("Doc");
    assert_eq!(s.bar().highlighted_name(), Some("Documents"), "a completion highlight, not a selection");
    s.key(Key::Delete);
    assert_eq!(status(&s), ("Select an entry first".into(), Tone::Bad));
    assert!(has(&fs, HOME, "Documents"), "nothing was deleted");
    assert_eq!(fs.trash_len(), 0);
}

#[test]
fn the_delete_key_trashes_the_selection() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    s.key(Key::Delete);
    assert_eq!(fs.trash_len(), 1);
    assert!(!has(&fs, HOME, "notes.md"));
    s.key(Key::Undo);
    assert!(has(&fs, HOME, "notes.md"));
    s.key(Key::Redo);
    assert!(!has(&fs, HOME, "notes.md"));
}

#[test]
fn permanent_delete_asks_first() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":rm!");
    let (msg, tone) = status(&s);
    assert_eq!(tone, Tone::Warn);
    assert_eq!(msg, "Delete notes.md permanently? This cannot be undone. Enter confirms, Esc cancels");
    assert!(has(&fs, HOME, "notes.md"), "nothing happens until confirmed");

    s.key(Key::Escape);
    assert_eq!(status(&s), ("Cancelled. Nothing was deleted".into(), Tone::Info));
    assert!(has(&fs, HOME, "notes.md"));
}

#[test]
fn permanent_delete_runs_on_confirmation_and_cannot_be_undone() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md", "shopping-list.txt"]);
    run(&mut s, ":rm!");
    assert!(status(&s).0.starts_with("Delete 2 items permanently?"));
    s.key(Key::Enter);
    assert_eq!(status(&s), ("Deleted 2 items permanently".into(), Tone::Good));
    assert!(!has(&fs, HOME, "notes.md") && !has(&fs, HOME, "shopping-list.txt"));
    assert_eq!(fs.trash_len(), 0, "not in the trash either");
    run(&mut s, ":undo");
    assert_eq!(status(&s).0, "Nothing to undo");
}

#[test]
fn typing_is_not_a_yes_to_a_pending_delete() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":rm!");
    s.type_str("x"); // edits the bar, which withdraws the question
    s.key(Key::Enter);
    assert!(has(&fs, HOME, "notes.md"));
}

// ----- copy / cut / paste -----

#[test]
fn copy_then_paste_into_another_folder_leaves_the_original() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":copy");
    assert_eq!(s.bar().clipboard_summary().as_deref(), Some("Copied notes.md"));
    run(&mut s, ":cd Documents");
    run(&mut s, ":paste");
    assert_eq!(status(&s), ("Pasted notes.md".into(), Tone::Good));
    assert!(has(&fs, "/home/user/Documents", "notes.md") && has(&fs, HOME, "notes.md"));
    assert_eq!(s.bar().selected_name(), Some("notes.md"), "the pasted item is selected");
    assert!(s.bar().clipboard().is_some(), "a copy can be pasted again");
}

#[test]
fn pasting_into_a_folder_with_the_same_name_renames_the_copy() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":copy");
    run(&mut s, ":paste"); // same folder
    assert_eq!(status(&s), ("Pasted notes (2).md (1 renamed to avoid a collision)".into(), Tone::Good));
    run(&mut s, ":paste");
    assert!(has(&fs, HOME, "notes (2).md") && has(&fs, HOME, "notes (3).md"));
    select(&mut s, &["Documents"]);
    run(&mut s, ":copy");
    run(&mut s, ":paste");
    assert!(has(&fs, HOME, "Documents (2)"), "folders are numbered after the whole name");
}

#[test]
fn cut_then_paste_moves_and_the_clipboard_is_used_up() {
    let (mut s, fs) = setup();
    run(&mut s, ":cd Documents");
    select(&mut s, &["resume.docx"]);
    run(&mut s, ":cut");
    assert_eq!(s.bar().clipboard_summary().as_deref(), Some("Cut resume.docx"));
    assert!(has(&fs, "/home/user/Documents", "resume.docx"), "nothing moves until paste");
    run(&mut s, ":cd ~");
    run(&mut s, ":paste");
    assert_eq!(status(&s), ("Moved resume.docx".into(), Tone::Good));
    assert!(has(&fs, HOME, "resume.docx") && !has(&fs, "/home/user/Documents", "resume.docx"));
    assert!(s.bar().clipboard().is_none());

    run(&mut s, ":undo");
    assert!(has(&fs, "/home/user/Documents", "resume.docx") && !has(&fs, HOME, "resume.docx"));
    run(&mut s, ":redo");
    assert!(has(&fs, HOME, "resume.docx"));
}

#[test]
fn a_folder_can_never_be_pasted_into_itself() {
    // Concept-sketch bug: cutting `projects` and pasting inside it deleted it.
    for (op, inside) in
        [(":cut", "projects"), (":copy", "projects"), (":cut", "projects/nocrumbs"), (":copy", "projects/dotfiles")]
    {
        let (mut s, fs) = setup();
        select(&mut s, &["projects"]);
        run(&mut s, op);
        run(&mut s, &format!(":cd {inside}"));
        run(&mut s, ":paste");
        assert_eq!(status(&s), ("Cannot paste projects inside itself".into(), Tone::Bad), "{op} into {inside}");
        assert!(has(&fs, HOME, "projects"), "{op} into {inside}: the folder is still there");
        assert!(has(&fs, "/home/user/projects/nocrumbs", "main.rs"), "with everything in it");
        assert_eq!(on_disk(&fs, "/home/user/projects").iter().filter(|n| *n == "projects").count(), 0);
    }
}

#[test]
fn cutting_into_the_folder_it_is_already_in_does_nothing() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":cut");
    run(&mut s, ":paste");
    assert_eq!(status(&s), ("Already in this folder".into(), Tone::Info));
    assert!(has(&fs, HOME, "notes.md"));
    assert!(!has(&fs, HOME, "notes (2).md"));
}

#[test]
fn paste_with_an_empty_clipboard_is_an_error() {
    let (mut s, _) = setup();
    run(&mut s, ":paste");
    assert_eq!(status(&s), ("Clipboard is empty. :copy or :cut something first".into(), Tone::Bad));
}

#[test]
fn copying_nothing_is_an_error() {
    let (mut s, _) = setup();
    run(&mut s, ":copy");
    assert_eq!(status(&s), ("Select an entry first".into(), Tone::Bad));
    assert!(s.bar().clipboard().is_none());
}

#[test]
fn a_deleted_source_is_reported_not_resurrected() {
    let (mut s, _) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":copy");
    select(&mut s, &["notes.md"]);
    run(&mut s, ":rm");
    run(&mut s, ":cd Documents");
    run(&mut s, ":paste");
    assert_eq!(status(&s), ("/home/user/notes.md no longer exists".into(), Tone::Bad));
}

#[test]
fn renaming_a_clipboard_item_updates_the_clipboard() {
    // The clipboard must follow a rename, or a later paste silently duplicates or loses it.
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    run(&mut s, ":cut");
    select(&mut s, &["notes.md"]);
    run(&mut s, ":rename journal.md");
    assert_eq!(s.bar().clipboard().map(|c| c.items[0].name.as_str()), Some("journal.md"));
    run(&mut s, ":cd Documents");
    run(&mut s, ":paste");
    assert!(has(&fs, "/home/user/Documents", "journal.md"));
    assert!(!has(&fs, HOME, "journal.md") && !has(&fs, HOME, "notes.md"), "moved, not duplicated");
}

#[test]
fn pasting_several_items_selects_them_all() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md", "shopping-list.txt", "Music"]);
    run(&mut s, ":copy");
    assert_eq!(s.bar().clipboard_summary().as_deref(), Some("Copied 3 items"));
    run(&mut s, ":cd /tmp");
    run(&mut s, ":paste");
    assert_eq!(status(&s).0, "Pasted 3 items");
    assert_eq!(on_disk(&fs, "/tmp"), ["Music", "notes.md", "shopping-list.txt"]);
    assert_eq!(s.bar().selection_len(), 3);
    run(&mut s, ":undo");
    assert!(on_disk(&fs, "/tmp").is_empty(), "one undo removes the whole paste");
}

#[test]
fn clipboard_keys_work_from_the_list() {
    let (mut s, fs) = setup();
    select(&mut s, &["notes.md"]);
    s.key(Key::Copy);
    assert_eq!(s.bar().selected_name(), Some("notes.md"), "a key leaves the selection alone");
    run(&mut s, ":cd Documents");
    s.key(Key::Paste);
    assert!(has(&fs, "/home/user/Documents", "notes.md"));
    select(&mut s, &["notes.md"]);
    s.key(Key::Cut);
    assert_eq!(s.bar().clipboard_summary().as_deref(), Some("Cut notes.md"));
}

// ----- multi-select -----

#[test]
fn click_modifiers_toggle_and_extend() {
    let (mut s, _) = setup();
    s.bar_mut().click_row(1, false, false);
    s.bar_mut().click_row(3, true, false);
    assert_eq!(s.bar().selected_rows(), [1, 3], "Ctrl+click adds");
    s.bar_mut().click_row(1, true, false);
    assert_eq!(s.bar().selected_rows(), [3], "Ctrl+click toggles off");
    s.bar_mut().click_row(5, false, false);
    s.bar_mut().click_row(2, false, true);
    assert_eq!(s.bar().selected_rows(), [2, 3, 4, 5], "Shift+click selects the range from the anchor");
    s.bar_mut().click_row(0, false, false);
    assert_eq!(s.bar().selected_rows(), [0]);
}

#[test]
fn shift_arrows_extend_and_select_all_takes_what_is_shown() {
    let (mut s, _) = setup();
    s.key(Key::Down);
    s.key(Key::ExtendDown);
    s.key(Key::ExtendDown);
    assert_eq!(s.bar().selected_rows(), [0, 1, 2]);
    s.key(Key::ExtendUp);
    assert_eq!(s.bar().selected_rows(), [0, 1]);
    s.key(Key::Down);
    assert_eq!(s.bar().selected_rows(), [2], "a plain arrow collapses to one row");

    let mut s = setup().0;
    s.type_str("*.md");
    s.key(Key::SelectAll);
    assert_eq!(s.bar().selected_names(), ["notes.md"], "Ctrl+A takes filter results");
    assert_eq!(status(&s).0, "1 selected");
}

#[test]
fn the_selection_clears_when_the_list_changes() {
    let (mut s, _) = setup();
    select(&mut s, &["notes.md", "Music"]);
    run(&mut s, ":cd Documents");
    assert_eq!(s.bar().selection_len(), 0);
}

// ----- undo / redo -----

#[test]
fn undo_and_redo_walk_a_sequence_of_operations() {
    let (mut s, fs) = setup();
    run(&mut s, ":mkdir a");
    run(&mut s, ":mkdir b");
    select(&mut s, &["a"]);
    run(&mut s, ":rename c");
    assert!(has(&fs, HOME, "c") && has(&fs, HOME, "b") && !has(&fs, HOME, "a"));
    run(&mut s, ":undo");
    assert!(has(&fs, HOME, "a") && !has(&fs, HOME, "c"));
    run(&mut s, ":undo");
    assert!(!has(&fs, HOME, "b"));
    run(&mut s, ":undo");
    assert!(!has(&fs, HOME, "a"));
    run(&mut s, ":undo");
    assert_eq!(status(&s), ("Nothing to undo".into(), Tone::Info));
    run(&mut s, ":redo");
    run(&mut s, ":redo");
    run(&mut s, ":redo");
    assert!(has(&fs, HOME, "c") && has(&fs, HOME, "b"));
    run(&mut s, ":redo");
    assert_eq!(status(&s).0, "Nothing to redo");
}

#[test]
fn a_new_action_ends_the_redo_history() {
    let (mut s, _) = setup();
    run(&mut s, ":mkdir a");
    run(&mut s, ":undo");
    assert!(s.bar().can_redo());
    run(&mut s, ":mkdir b");
    assert!(!s.bar().can_redo());
}

#[test]
fn the_undo_stack_is_at_least_twenty_deep() {
    let (mut s, fs) = setup();
    for i in 0..25 {
        run(&mut s, &format!(":touch f{i:02}"));
    }
    for _ in 0..25 {
        run(&mut s, ":undo");
        assert_eq!(status(&s).1, Tone::Good);
    }
    assert!(!has(&fs, HOME, "f00") && !has(&fs, HOME, "f24"));
}

#[test]
fn undo_reports_when_the_world_has_changed_underneath_it() {
    let (mut s, fs) = setup();
    run(&mut s, ":mkdir gone");
    fs.delete_forever("/home/user/gone").unwrap(); // something else deleted it
    run(&mut s, ":undo");
    assert_eq!(status(&s), ("Undo failed: /home/user/gone no longer exists".into(), Tone::Bad));
    assert!(s.bar().can_undo(), "the failed step stays undoable");
}

#[test]
fn log_lists_recent_operations() {
    let (mut s, _) = setup();
    run(&mut s, ":log");
    assert_eq!(status(&s).0, "No operations yet");
    run(&mut s, ":mkdir a");
    run(&mut s, ":touch b");
    run(&mut s, ":undo");
    run(&mut s, ":log");
    assert_eq!(status(&s).0, "Recent: Created folder a | Created file b | Undid: Created file b");
    assert_eq!(s.bar().operation_log().len(), 3);
}

// ----- keys, platforms, scale -----

#[test]
fn backspace_in_the_list_goes_up_one_folder() {
    let (mut s, _) = setup();
    run(&mut s, "Documents/recipes");
    s.key(Key::ParentFolder);
    assert_eq!(s.bar().cwd(), "/home/user/Documents");
    // `:cd /` offers `big/` as ghost text; Esc drops it so Enter goes to the root itself.
    s.select_all();
    s.type_str(":cd /");
    s.key(Key::Escape);
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/");
    s.key(Key::ParentFolder);
    assert_eq!(status(&s), ("Already at the top".into(), Tone::Info));
}

#[test]
fn a_read_only_filesystem_says_what_it_cannot_do() {
    #[derive(Debug)]
    struct ReadOnly(MemFs);
    impl Fs for ReadOnly {
        fn canonical_dir(&self, d: &str) -> Option<String> {
            self.0.canonical_dir(d)
        }
        fn entries(&self, d: &str) -> Option<Arc<[Entry]>> {
            self.0.entries(d)
        }
        fn case_insensitive(&self) -> bool {
            true
        }
    }
    let bar = Bar::new(Arc::new(ReadOnly(MemFs::sample(true, 10))), HOME, HOME, '/');
    let mut s = Session::new(bar);
    run(&mut s, ":mkdir x");
    assert_eq!(status(&s), ("This filesystem cannot create folders".into(), Tone::Bad));
    select(&mut s, &["notes.md"]);
    run(&mut s, ":rm");
    assert_eq!(status(&s), ("This filesystem cannot use the trash".into(), Tone::Bad));
}

#[test]
fn case_sensitive_filesystems_allow_names_that_differ_only_by_case() {
    let (mut s, fs) = setup_at(10, false);
    run(&mut s, ":touch NOTES.MD");
    assert_eq!(status(&s).1, Tone::Good);
    assert!(has(&fs, HOME, "NOTES.MD") && has(&fs, HOME, "notes.md"));
}

#[test]
fn file_operations_in_a_100k_folder() {
    let (mut s, fs) = setup_at(100_000, true);
    run(&mut s, ":cd /big");
    run(&mut s, ":touch zzz-new.txt");
    assert_eq!(s.bar().row_count(), 100_001);
    assert_eq!(s.bar().selected_name(), Some("zzz-new.txt"));
    run(&mut s, ":undo");
    assert_eq!(s.bar().row_count(), 100_000);
    assert!(!has(&fs, "/big", "zzz-new.txt"));
}
