// SPDX-License-Identifier: GPL-3.0-or-later
//! Keystroke-script tests for the whole bar (brief §16): scripted typing against the headless
//! `Bar` + `Session` and the in-memory filesystem. No UI, no GPU, milliseconds each.
//!
//! The same scenarios also run against the real egui widget in `spikes/m0-ghost-bar`, to prove
//! the front end forwards events faithfully; the logic itself is tested here.

use std::sync::Arc;

use nocrumbs_core::sim::Session;
use nocrumbs_core::{Bar, Key, Mode, Sel, Tone};
use nocrumbs_testfs::MemFs;

const HOME: &str = "/home/user";

fn session_at(big_n: usize, start: &str) -> Session {
    Session::new(Bar::new(Arc::new(MemFs::sample(true, big_n)), HOME, start, '/'))
}

fn session() -> Session {
    session_at(1_000, HOME)
}

fn sel(anchor: usize, caret: usize) -> Sel {
    Sel { anchor, caret }
}

// ----- launch -----

#[test]
fn starts_on_home_with_everything_selected() {
    let s = session();
    assert_eq!(s.text(), "~");
    assert_eq!(s.bar().cwd(), HOME);
    assert_eq!(s.selection(), sel(0, 1), "the first keystroke replaces the whole bar");
    assert_eq!(s.bar().mode(), Mode::Empty);
}

#[test]
fn falls_back_to_home_when_the_start_folder_is_missing() {
    let s = session_at(10, "/no/such");
    assert_eq!(s.bar().cwd(), HOME);
}

// ----- ghost text -----

#[test]
fn ghost_text_is_appended_and_selected() {
    let mut s = session();
    s.type_str("D");
    assert_eq!(s.bar().typed(), "D");
    assert_eq!(s.text(), "Documents/");
    assert_eq!(s.bar().ghost_chars(), 9);
    assert_eq!(s.selection(), sel(1, 10), "only the ghost is selected");
    assert_eq!(s.bar().highlighted_name(), Some("Documents"), "the row Tab would go to is highlighted");
}

#[test]
fn typing_over_the_ghost_never_moves_typed_text() {
    let mut s = session();
    s.type_str("Dow");
    assert_eq!(s.bar().typed(), "Dow");
    assert_eq!(s.text(), "Downloads/");
    assert_eq!(s.bar().highlighted_name(), Some("Downloads"));
}

#[test]
fn typed_case_is_preserved_while_ghosting() {
    let mut s = session();
    s.type_str("doc");
    assert_eq!(s.bar().typed(), "doc");
    assert_eq!(s.text(), "documents/");
}

#[test]
fn backspace_removes_the_ghost_and_does_not_reoffer_it() {
    // Concept-sketch bug: Backspace deleted the ghost, which was immediately offered again.
    let mut s = session();
    s.type_str("Doc");
    assert_eq!(s.text(), "Documents/");
    s.backspace();
    assert_eq!(s.text(), "Doc");
    assert_eq!(s.bar().ghost_chars(), 0);
    s.backspace();
    assert_eq!(s.text(), "Do");
    assert_eq!(s.bar().ghost_chars(), 0, "still no ghost after deleting");
    s.type_str("c");
    assert_eq!(s.text(), "Documents/", "typing resumes completion");
}

#[test]
fn delete_key_also_removes_the_ghost_without_reoffering() {
    let mut s = session();
    s.type_str("Doc");
    s.delete();
    assert_eq!(s.text(), "Doc");
    assert_eq!(s.bar().ghost_chars(), 0);
}

#[test]
fn moving_the_caret_commits_the_ghost_as_typed_text() {
    let mut s = session();
    s.type_str("Doc");
    s.right(); // collapses the selection to its end, like a browser URL bar
    assert_eq!(s.text(), "Documents/");
    assert_eq!(s.bar().ghost_chars(), 0, "now ordinary typed text");
    assert_eq!(s.bar().typed(), "Documents/");
}

#[test]
fn no_ghost_when_the_caret_is_not_at_the_end() {
    let mut s = session();
    s.type_str("Dx");
    s.left();
    s.type_str("o"); // "Dox" with the caret before the x: not at the end
    assert_eq!(s.bar().ghost_chars(), 0);
    assert_eq!(s.text(), "Dox");
}

#[test]
fn paste_offers_a_ghost_after_the_pasted_text() {
    let mut s = session();
    s.paste("~/Down");
    assert_eq!(s.bar().typed(), "~/Down");
    assert_eq!(s.text(), "~/Downloads/");
}

#[test]
fn composition_suppresses_completion() {
    let mut s = session();
    s.set_composing(true);
    s.type_str("D");
    assert_eq!(s.bar().ghost_chars(), 0, "no ghost text while an IME is composing");
    s.key(Key::Tab);
    assert_eq!(s.text(), "D", "Tab does nothing during composition");
    s.set_composing(false);
    s.type_str("o");
    assert_eq!(s.text(), "Documents/", "completion resumes afterwards");
}

#[test]
fn no_such_path_means_no_ghost_and_no_highlight() {
    let mut s = session();
    s.type_str("zzz");
    assert_eq!(s.bar().ghost_chars(), 0);
    assert_eq!(s.bar().highlighted_name(), None);
}

#[test]
fn other_matches_are_reported() {
    let mut s = session();
    s.type_str("Do");
    assert_eq!(s.bar().status(), ("+1 more match", Tone::Info), "Documents and Downloads");
}

// ----- Tab -----

#[test]
fn tab_accepts_canonical_case_and_descends() {
    let mut s = session();
    s.type_str("doc");
    s.key(Key::Tab);
    assert_eq!(s.bar().typed(), "Documents/", "casing is canonical once accepted");
    // The next level is offered immediately (concept-sketch bug: it was not).
    assert_eq!(s.text(), "Documents/quarterly-report.pdf");
    assert_eq!(s.bar().ghost_chars(), "quarterly-report.pdf".len());
    assert_eq!(s.bar().view_dir(), "/home/user/Documents", "the list previews the folder being typed");
    assert_eq!(s.bar().highlighted_name(), Some("quarterly-report.pdf"));
    s.key(Key::Tab);
    assert_eq!(s.text(), "Documents/quarterly-report.pdf");
    assert_eq!(s.bar().ghost_chars(), 0, "a file ends the descent");
}

#[test]
fn tab_with_nothing_to_complete_changes_nothing() {
    let mut s = session();
    s.type_str("zzz");
    s.key(Key::Tab);
    assert_eq!(s.text(), "zzz");
    assert_eq!(s.selection(), sel(3, 3));
}

#[test]
fn tab_completes_commands_without_placeholders() {
    let mut s = session();
    s.type_str(":mk");
    assert_eq!(s.text(), ":mkdir ");
    assert_eq!(s.bar().typed(), ":mk");
    s.key(Key::Tab);
    assert_eq!(s.bar().typed(), ":mkdir ");
    assert_eq!(s.bar().ghost_chars(), 0);
    s.type_str("x");
    assert_eq!(s.text(), ":mkdir x");
}

// ----- Esc -----

#[test]
fn escape_is_staged() {
    let mut s = session();
    s.type_str("Doc");
    s.key(Key::Escape);
    assert_eq!(s.text(), "Doc", "first Esc removes only the ghost");
    assert!(!s.blurred());
    s.key(Key::Escape);
    assert_eq!(s.text(), "~", "second Esc reverts the bar to the current path");
    assert!(!s.blurred());
    s.key(Key::Escape);
    assert!(s.blurred(), "third Esc gives up focus");
}

#[test]
fn escape_clears_a_filter_and_restores_the_listing() {
    let mut s = session();
    let all = s.bar().row_count();
    s.type_str("*.md");
    assert_eq!(s.bar().row_count(), 1);
    assert_eq!(s.bar().mode(), Mode::Glob);
    s.key(Key::Escape);
    assert_eq!(s.text(), "~");
    assert_eq!(s.bar().row_count(), all);
}

// ----- Enter -----

#[test]
fn enter_navigates_to_the_completed_text() {
    let mut s = session();
    s.type_str("Doc");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Documents");
    assert_eq!(s.text(), "~/Documents");
    assert_eq!(s.selection(), sel(0, 11), "select all, ready for the next path");
    assert_eq!(s.bar().view_dir(), "/home/user/Documents");
    assert_eq!(s.bar().row_count(), 4);
    assert_eq!(s.bar().status(), ("Now in ~/Documents", Tone::Good));
}

#[test]
fn enter_on_a_missing_path_reports_and_stays() {
    let mut s = session();
    s.type_str("zzz");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), HOME);
    assert_eq!(s.bar().status(), ("No such path: zzz", Tone::Bad));
    assert_eq!(s.bar().error_count(), 1);
    assert_eq!(s.text(), "zzz", "the bar keeps what was typed so it can be corrected");
}

#[test]
fn enter_on_a_file_selects_it_in_its_folder() {
    let mut s = session();
    s.type_str("Documents/resume.docx");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Documents");
    assert_eq!(s.bar().selected_name(), Some("resume.docx"));
}

#[test]
fn enter_with_a_filter_keeps_the_filter() {
    let mut s = session();
    s.type_str("*.md");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), HOME);
    assert_eq!(s.text(), "*.md");
    assert_eq!(s.bar().status().1, Tone::Good);
}

#[test]
fn enter_on_a_command_says_so_without_crashing() {
    for cmd in [":rm", ":mkdir foo", ":bogus", ":", ":sort size desc"] {
        let mut s = session();
        s.type_str(cmd);
        s.key(Key::Enter);
        assert_eq!(s.bar().cwd(), HOME);
        assert_eq!(s.bar().mode(), Mode::Command, "{cmd}");
    }
}

#[test]
fn parent_and_home_navigation() {
    let mut s = session();
    s.type_str("Documents/recipes");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Documents/recipes");
    s.type_str("../..");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), HOME);
    s.type_str("/etc");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/etc");
    s.type_str("~");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), HOME);
}

// ----- commands, globs, preview -----

#[test]
fn command_arguments_can_be_typed_in_full() {
    // Concept-sketch bug: `:mkdir foo` turned into `:mkdir NAME`.
    for input in [":mkdir foo", ":alias p ~/x", ":cd /etc", ":rename new name"] {
        let mut s = session();
        s.type_str(input);
        assert_eq!(s.text(), input);
        assert_eq!(s.bar().ghost_chars(), 0, "{input}");
    }
}

#[test]
fn glob_filter_keeps_the_folder_in_view() {
    // Concept-sketch bug: the path disappeared while filtering.
    let mut s = session();
    s.type_str("~/Downloads/*.pdf");
    assert_eq!(s.bar().view_dir(), "/home/user/Downloads");
    assert_eq!(s.bar().view_dir_display(), "~/Downloads");
    assert_eq!(s.bar().row_count(), 1);
    assert_eq!(s.bar().entry_count(), 5);
    assert_eq!(s.bar().ghost_chars(), 0, "no ghost while filtering");
}

#[test]
fn typing_toward_a_folder_previews_it() {
    let mut s = session();
    s.type_str("Documents/rec");
    assert_eq!(s.bar().view_dir(), "/home/user/Documents");
    assert_eq!(s.bar().highlighted_name(), Some("recipes"));
    assert_eq!(s.bar().cwd(), HOME, "previewing does not navigate");
    s.key(Key::Escape);
    s.key(Key::Escape);
    assert_eq!(s.bar().view_dir(), HOME, "Esc reverts the preview");
}

#[test]
fn hidden_files_follow_the_setting_and_the_typed_dot() {
    let mut s = session();
    assert_eq!(s.bar().row_count(), 8);
    s.type_str(".conf");
    assert_eq!(s.text(), ".config/", "typing a dot offers dotfiles");
    s.key(Key::Escape);
    s.key(Key::Escape);
    s.bar_mut().set_show_hidden(true);
    assert_eq!(s.bar().row_count(), 10);
}

// ----- list selection -----

#[test]
fn arrow_keys_move_the_selection_and_wrap() {
    let mut s = session();
    let n = s.bar().row_count();
    s.key(Key::Down);
    assert_eq!(s.bar().selected_row(), Some(0));
    s.key(Key::Up);
    assert_eq!(s.bar().selected_row(), Some(n - 1), "from the first row, Up wraps to the last");
    s.key(Key::Down);
    assert_eq!(s.bar().selected_row(), Some(0), "and Down wraps back");
}

#[test]
fn selection_is_separate_from_the_completion_highlight() {
    let mut s = session();
    s.key(Key::Down);
    s.type_str("Dow");
    assert_eq!(s.bar().selected_row(), Some(0), "typing does not disturb the selection while the list is unchanged");
    assert_eq!(s.bar().highlighted_name(), Some("Downloads"));
    s.key(Key::Down);
    assert_eq!(s.bar().selected_name(), Some("Downloads"), "arrow keys move the selection...");
    s.key(Key::Down);
    assert_eq!(s.bar().selected_name(), Some("Music"));
    assert_eq!(s.bar().highlighted_name(), Some("Downloads"), "...but never the completion highlight");
}

#[test]
fn changing_the_listing_resets_the_selection() {
    let mut s = session();
    s.key(Key::Down);
    assert_eq!(s.bar().selected_row(), Some(0));
    s.type_str("Documents/"); // previews another folder
    assert_eq!(s.bar().selected_row(), None);
    s.key(Key::Escape);
    s.key(Key::Escape);
    s.key(Key::Down);
    s.type_str("*.md"); // filter changes the rows
    assert_eq!(s.bar().selected_row(), None);
}

#[test]
fn navigating_resets_the_selection() {
    let mut s = session();
    s.key(Key::Down);
    s.type_str("Doc");
    s.key(Key::Enter);
    assert_eq!(s.bar().selected_row(), None);
}

// ----- scale -----

#[test]
fn big_folder_completes_and_filters() {
    let mut s = session_at(100_000, HOME);
    s.type_str("/big/");
    assert_eq!(s.bar().view_dir(), "/big");
    assert_eq!(s.bar().row_count(), 100_000);
    s.type_str("rep");
    assert!(s.bar().highlighted_name().is_some_and(|n| n.starts_with("report-")));

    let mut s = session_at(100_000, HOME);
    s.type_str("/big/*.pdf");
    // i % 3 == 0 and not a folder (i % 20 == 0): 33,334 - 1,667.
    assert_eq!(s.bar().row_count(), 31_667);
}

#[test]
fn scroll_requests_follow_the_highlight() {
    let mut s = session_at(100_000, "/big");
    s.type_str("report-05");
    let r = s.bar_mut().take_scroll_request().expect("scroll to the highlighted row");
    assert_eq!(s.bar().row(r).name, s.bar().highlighted_name().unwrap());
}
