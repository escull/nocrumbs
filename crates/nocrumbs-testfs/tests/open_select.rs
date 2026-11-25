// SPDX-License-Identifier: GPL-3.0-or-later
//! Opening the selected row (brief §9.3), the keys the list has when the bar does not have focus,
//! what the status line says about a selection, and which rows are "other matches" (brief §5).

use std::sync::Arc;

use nocrumbs_core::sim::Session;
use nocrumbs_core::{Aliases, Bar, Fs, Key, SelectionInfo, Tone};
use nocrumbs_testfs::MemFs;

const HOME: &str = "/home/user";

fn setup() -> (Session, Arc<MemFs>) {
    let fs = Arc::new(MemFs::sample(true, 100));
    (Session::new(Bar::new(fs.clone(), HOME, HOME, '/')), fs)
}

fn run(s: &mut Session, line: &str) {
    s.select_all();
    s.type_str(line);
    s.key(Key::Enter);
}

fn status(s: &Session) -> (String, Tone) {
    let (m, t) = s.bar().status();
    (m.to_string(), t)
}

fn row_of(s: &Session, name: &str) -> usize {
    (0..s.bar().row_count()).find(|&r| s.bar().row(r).name == name).unwrap_or_else(|| panic!("no row {name}"))
}

fn click(s: &mut Session, name: &str) {
    let r = row_of(s, name);
    s.bar_mut().click_row(r, false, false);
}

fn opened(s: &mut Session) -> Option<String> {
    s.bar_mut().take_open_request()
}

fn other_matches(s: &Session) -> Vec<String> {
    (0..s.bar().row_count()).filter(|&r| s.bar().is_other_match(r)).map(|r| s.bar().row(r).name.clone()).collect()
}

// ----- Enter opens what the arrows chose -----

#[test]
fn enter_enters_the_folder_the_arrows_selected() {
    let (mut s, _) = setup();
    s.key(Key::Down);
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Documents");
    assert_eq!(s.text(), "~/Documents", "the bar is idle in the new folder");
    assert_eq!(opened(&mut s), None, "a folder is entered, not handed to the platform");
}

#[test]
fn enter_on_a_selected_file_asks_the_platform_to_open_it() {
    let (mut s, _) = setup();
    click(&mut s, "notes.md");
    s.key(Key::Enter);
    assert_eq!(opened(&mut s).as_deref(), Some("/home/user/notes.md"));
    assert_eq!(opened(&mut s), None, "the request is delivered once");
    assert_eq!(status(&s), ("Opening notes.md".into(), Tone::Good));
    assert_eq!(s.bar().cwd(), HOME);
}

#[test]
fn a_filter_result_opens_from_the_keyboard_and_the_filter_stays() {
    // The keyboard dead end of the concept sketch: no way to open a filtered result.
    let (mut s, _) = setup();
    s.type_str("~/Downloads/*.pdf");
    s.key(Key::Down);
    s.key(Key::Enter);
    assert_eq!(opened(&mut s).as_deref(), Some("/home/user/Downloads/invoice-nov.pdf"));
    assert_eq!(s.text(), "~/Downloads/*.pdf", "the filter stays, to carry on from");
    assert_eq!(s.bar().row_count(), 1);
}

#[test]
fn a_filtered_folder_is_entered() {
    let (mut s, _) = setup();
    s.type_str("Dow*");
    s.key(Key::Down);
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Downloads");
    assert_eq!(s.text(), "~/Downloads");
}

#[test]
fn the_selection_beats_a_pending_ghost() {
    let (mut s, _) = setup();
    s.type_str("Doc"); // ghost: Documents/
    s.key(Key::Down);
    s.key(Key::Down); // Downloads
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Downloads", "the arrow selection wins over the completion");
}

#[test]
fn editing_after_the_arrows_makes_enter_act_on_the_text() {
    let (mut s, _) = setup();
    s.key(Key::Down); // Documents
    s.type_str("Downloads");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Downloads");
}

#[test]
fn tab_after_the_arrows_also_makes_enter_act_on_the_text() {
    let (mut s, _) = setup();
    s.type_str("no"); // ghost: notes.md
    s.key(Key::Down); // select Documents
    s.key(Key::Tab); // accept notes.md; the list does not change
    s.key(Key::Enter);
    assert_eq!(opened(&mut s), None, "Enter acted on the text, not on the Documents selection");
    assert_eq!(s.bar().selected_name(), Some("notes.md"));
    assert_eq!(s.bar().cwd(), HOME);
}

#[test]
fn a_selection_made_by_typing_a_path_is_not_opened_by_the_next_enter() {
    let (mut s, _) = setup();
    run(&mut s, "notes.md"); // selects it
    assert_eq!(s.bar().selected_name(), Some("notes.md"));
    s.key(Key::Enter); // Enter on the idle bar acts on its text
    assert_eq!(opened(&mut s), None);
    assert_eq!(s.bar().cwd(), HOME);
}

#[test]
fn with_several_rows_selected_enter_opens_the_cursor_row() {
    let (mut s, _) = setup();
    s.key(Key::Down);
    s.key(Key::ExtendDown); // cursor on Downloads
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Downloads");
}

#[test]
fn enter_in_the_list_always_opens_the_selection() {
    let (mut s, _) = setup();
    s.key(Key::Open);
    assert_eq!(status(&s), ("Select an entry first".into(), Tone::Bad));
    run(&mut s, "notes.md"); // a selection the arrows did not make
    s.key(Key::Open);
    assert_eq!(opened(&mut s).as_deref(), Some("/home/user/notes.md"));
}

#[test]
fn the_open_command_opens_the_selection() {
    let (mut s, _) = setup();
    run(&mut s, ":open");
    assert_eq!(status(&s), ("Select an entry first".into(), Tone::Bad));
    click(&mut s, "notes.md");
    run(&mut s, ":open");
    assert_eq!(opened(&mut s).as_deref(), Some("/home/user/notes.md"));
    assert_eq!(s.text(), "~", "a finished command returns to the idle bar");
    click(&mut s, "Music");
    run(&mut s, ":open");
    assert_eq!(s.bar().cwd(), "/home/user/Music");
}

#[test]
fn opening_something_deleted_underneath_says_so() {
    let (mut s, fs) = setup();
    click(&mut s, "notes.md");
    fs.delete_forever("/home/user/notes.md").unwrap();
    s.key(Key::Enter);
    assert_eq!(status(&s), ("notes.md no longer exists".into(), Tone::Bad));
    assert_eq!(opened(&mut s), None);
    assert!((0..s.bar().row_count()).all(|r| s.bar().row(r).name != "notes.md"), "the list caught up");
}

#[test]
fn opening_counts_as_use_for_ranking() {
    let (mut s, _) = setup();
    click(&mut s, "shopping-list.txt");
    s.key(Key::Enter);
    assert!(s.bar().frecency().score("/home/user/shopping-list.txt", 0) > 0);
}

// ----- permanent delete and Esc from the list -----

#[test]
fn shift_del_asks_first_and_enter_in_the_list_confirms() {
    let (mut s, fs) = setup();
    click(&mut s, "notes.md");
    s.key(Key::DeletePermanent);
    assert_eq!(status(&s).1, Tone::Warn);
    assert!(fs.entries(HOME).unwrap().iter().any(|e| e.name == "notes.md"), "nothing yet");
    s.key(Key::Open);
    assert_eq!(status(&s), ("Deleted 1 item permanently".into(), Tone::Good));
    assert!(fs.entries(HOME).unwrap().iter().all(|e| e.name != "notes.md"));
    assert_eq!(fs.trash_len(), 0, "not in the trash");
}

#[test]
fn shift_del_with_nothing_selected_asks_nothing() {
    let (mut s, _) = setup();
    s.key(Key::DeletePermanent);
    assert_eq!(status(&s), ("Select an entry first".into(), Tone::Bad));
}

#[test]
fn esc_in_the_list_cancels_the_question_then_clears_the_selection() {
    let (mut s, fs) = setup();
    click(&mut s, "notes.md");
    s.key(Key::DeletePermanent);
    s.key(Key::ListEscape);
    assert_eq!(status(&s), ("Cancelled. Nothing was deleted".into(), Tone::Info));
    assert_eq!(s.bar().selection_len(), 1, "the first Esc only withdraws the question");
    s.key(Key::ListEscape);
    assert_eq!(s.bar().selection_len(), 0);
    s.key(Key::Open);
    assert!(fs.entries(HOME).unwrap().iter().any(|e| e.name == "notes.md"));
}

// ----- the status line's selection summary -----

#[test]
fn selection_info_counts_files_folders_and_file_bytes() {
    let (mut s, _) = setup();
    assert_eq!(s.bar().selection_info(), None);
    click(&mut s, "notes.md"); // 1,420 bytes
    assert_eq!(s.bar().selection_info(), Some(SelectionInfo { count: 1, files: 1, folders: 0, file_bytes: 1_420 }));
    let (a, b) = (row_of(&s, "shopping-list.txt"), row_of(&s, "Music"));
    s.bar_mut().click_row(a, true, false); // +320 bytes
    s.bar_mut().click_row(b, true, false); // a folder: counted, not sized
    assert_eq!(s.bar().selection_info(), Some(SelectionInfo { count: 3, files: 2, folders: 1, file_bytes: 1_740 }));
}

// ----- other matches -----

#[test]
fn the_other_candidates_are_marked_and_the_winner_is_not() {
    let (mut s, _) = setup();
    s.type_str("Do");
    assert_eq!(s.bar().highlighted_name(), Some("Documents"));
    assert_eq!(other_matches(&s), ["Downloads"]);
}

#[test]
fn the_marks_survive_removing_the_ghost() {
    let (mut s, _) = setup();
    s.type_str("Do");
    s.backspace();
    assert_eq!(s.bar().ghost_chars(), 0);
    assert_eq!(other_matches(&s), ["Downloads"]);
}

#[test]
fn nothing_is_marked_when_nothing_is_ambiguous_or_nothing_is_typed() {
    let (mut s, _) = setup();
    assert!(other_matches(&s).is_empty(), "idle");
    s.type_str("Doc");
    assert!(other_matches(&s).is_empty(), "one match");
    s.select_all();
    s.type_str("Documents/");
    assert!(other_matches(&s).is_empty(), "descending: every entry would be marked, which is noise");
    s.select_all();
    s.type_str(":so");
    assert!(other_matches(&s).is_empty(), "commands have no rows");
}

#[test]
fn a_folder_is_marked_when_an_alias_wins() {
    let mut bar = Bar::new(Arc::new(MemFs::sample(true, 10)), HOME, HOME, '/');
    bar.set_aliases(Aliases::from_pairs([("pics", "/home/user/Pictures")]));
    let mut s = Session::new(bar);
    s.type_str("pi");
    assert_eq!(s.bar().highlighted_name(), None, "the alias wins and has no row");
    assert_eq!(other_matches(&s), ["Pictures"]);
}

#[test]
fn esc_in_the_bar_deselects_before_it_gives_up_focus() {
    let (mut s, _) = setup();
    s.key(Key::Down);
    s.key(Key::Escape);
    assert_eq!(s.bar().selection_len(), 0);
    assert!(!s.blurred(), "the selection goes first");
    s.key(Key::Escape);
    assert!(s.blurred(), "then the focus");
}

// ----- arrows wrap from the bar and stop at the ends in the list -----

#[test]
fn arrows_in_the_list_stop_at_the_ends() {
    let (mut s, _) = setup();
    let last = s.bar().row_count() - 1;
    s.key(Key::ListUp);
    assert_eq!(s.bar().selected_row(), Some(last), "with nothing selected, Up starts at the bottom");
    s.key(Key::ListDown);
    assert_eq!(s.bar().selected_row(), Some(last), "and Down stays there");
    for _ in 0..=last {
        s.key(Key::ListUp);
    }
    assert_eq!(s.bar().selected_row(), Some(0), "Up stops at the top");
}

#[test]
fn shift_arrows_never_wrap_around() {
    let (mut s, _) = setup();
    s.key(Key::Down); // first row
    s.key(Key::ExtendUp);
    assert_eq!(s.bar().selected_rows(), [0], "not the whole list");
    s.key(Key::Up); // from the bar, a plain arrow still wraps
    assert_eq!(s.bar().selected_row(), Some(s.bar().row_count() - 1));
    s.key(Key::ExtendDown);
    assert_eq!(s.bar().selection_len(), 1);
}
