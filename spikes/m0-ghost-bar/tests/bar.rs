// SPDX-License-Identifier: GPL-3.0-or-later
//! Headless keystroke tests: drive the real egui UI (`App::show`) with synthetic input and
//! assert the bar text, the ghost selection, the list and the status line.
//!
//! These answer M0 question 1 (ghost text) and the headless half of question 2.

use egui::text_edit::TextEditState;
use egui::{Event, ImeEvent, Key, Vec2};
use egui_kittest::Harness;
use m0_ghost_bar::{App, setup};

struct Ui<'a> {
    h: Harness<'a, App>,
}

impl Ui<'_> {
    fn new(big_n: usize) -> Self {
        let mut h = Harness::builder()
            .with_size(Vec2::new(960.0, 640.0))
            .build_ui_state(|ui, app: &mut App| app.show(ui), App::new(big_n));
        setup(&h.ctx);
        h.run_steps(3);
        Self { h }
    }

    /// Type one character at a time, with a frame in between, as a person would.
    fn type_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.h.event(Event::Text(ch.to_string()));
            self.h.run_steps(2);
        }
    }

    fn key(&mut self, key: Key) {
        self.h.key_press(key);
        self.h.run_steps(2);
    }

    fn app(&self) -> &App {
        self.h.state()
    }

    /// The bar's selection as (start, end) in chars.
    fn selection(&self) -> Option<(usize, usize)> {
        let st = TextEditState::load(&self.h.ctx, App::bar_id())?;
        let [a, b] = st.cursor.char_range()?.sorted_cursors();
        Some((a.index.0, b.index.0))
    }

    fn focused(&self) -> bool {
        self.h.ctx.memory(|m| m.has_focus(App::bar_id()))
    }
}

#[test]
fn starts_on_home_with_everything_selected() {
    let u = Ui::new(1_000);
    assert_eq!(u.app().text(), "~");
    assert_eq!(u.app().cwd(), "/home/user");
    assert!(u.focused());
    assert_eq!(u.selection(), Some((0, 1)), "first keystroke should replace the whole bar");
}

#[test]
fn ghost_text_is_appended_and_selected() {
    let mut u = Ui::new(1_000);
    // `Do`, not `D`: the seeded alias `dl` sorts before Documents and would win a bare `D`.
    u.type_str("Do");
    assert_eq!(u.app().typed(), "Do");
    assert_eq!(u.app().text(), "Documents/");
    assert_eq!(u.app().ghost_chars(), 8);
    assert_eq!(u.selection(), Some((2, 10)), "only the ghost is selected");
    assert_eq!(u.app().highlighted(), Some("Documents"), "the row Tab would go to is highlighted");
}

#[test]
fn typing_over_the_ghost_never_moves_typed_text() {
    let mut u = Ui::new(1_000);
    u.type_str("Dow");
    assert_eq!(u.app().typed(), "Dow");
    assert_eq!(u.app().text(), "Downloads/");
    assert_eq!(u.app().highlighted(), Some("Downloads"));
}

#[test]
fn typed_case_is_preserved_while_ghosting() {
    let mut u = Ui::new(1_000);
    u.type_str("doc");
    assert_eq!(u.app().typed(), "doc");
    assert_eq!(u.app().text(), "documents/");
}

#[test]
fn backspace_removes_the_ghost_and_does_not_reoffer_it() {
    // Concept-sketch bug: Backspace deleted the ghost, which was immediately offered again.
    let mut u = Ui::new(1_000);
    u.type_str("Doc");
    assert_eq!(u.app().text(), "Documents/");
    u.key(Key::Backspace);
    assert_eq!(u.app().text(), "Doc");
    assert_eq!(u.app().ghost_chars(), 0);
    // A second Backspace now deletes a typed character.
    u.key(Key::Backspace);
    assert_eq!(u.app().text(), "Do");
    assert_eq!(u.app().ghost_chars(), 0, "still no ghost after deleting");
    // Typing resumes completion.
    u.type_str("c");
    assert_eq!(u.app().text(), "Documents/");
}

#[test]
fn tab_accepts_canonical_case_and_descends() {
    let mut u = Ui::new(1_000);
    u.type_str("doc");
    u.key(Key::Tab);
    assert!(u.focused(), "Tab must not move focus out of the bar");
    assert_eq!(u.app().typed(), "Documents/", "casing is canonical once accepted");
    // The next level is offered immediately (concept-sketch bug: it was not).
    assert_eq!(u.app().text(), "Documents/quarterly-report.pdf");
    assert_eq!(u.app().ghost_chars(), "quarterly-report.pdf".len());
    assert_eq!(u.app().view_dir(), "/home/user/Documents", "the list previews the folder being typed");
    assert_eq!(u.app().highlighted(), Some("quarterly-report.pdf"));
    // Tab again accepts the file.
    u.key(Key::Tab);
    assert_eq!(u.app().text(), "Documents/quarterly-report.pdf");
    assert_eq!(u.app().ghost_chars(), 0);
}

#[test]
fn escape_is_staged() {
    let mut u = Ui::new(1_000);
    u.type_str("Doc");
    u.key(Key::Escape);
    assert_eq!(u.app().text(), "Doc", "first Esc removes only the ghost");
    assert!(u.focused());
    u.key(Key::Escape);
    assert_eq!(u.app().text(), "~", "second Esc reverts the bar to the current path");
    assert!(u.focused());
    u.key(Key::Escape);
    assert!(!u.focused(), "third Esc gives up focus");
}

#[test]
fn enter_navigates_to_the_completed_text() {
    let mut u = Ui::new(1_000);
    u.type_str("Doc");
    u.key(Key::Enter);
    assert_eq!(u.app().cwd(), "/home/user/Documents");
    assert_eq!(u.app().text(), "~/Documents");
    assert!(u.focused(), "the bar keeps focus after Enter");
    assert_eq!(u.selection(), Some((0, 11)), "and selects all, ready for the next path");
}

#[test]
fn enter_on_a_missing_path_reports_and_stays() {
    let mut u = Ui::new(1_000);
    u.type_str("zzz");
    assert_eq!(u.app().ghost_chars(), 0, "no ghost, no highlight");
    assert_eq!(u.app().highlighted(), None);
    u.key(Key::Enter);
    assert_eq!(u.app().cwd(), "/home/user");
    assert!(u.app().status_text().starts_with("No such path"), "{}", u.app().status_text());
}

#[test]
fn command_arguments_can_be_typed_in_full() {
    // Concept-sketch bug: `:mkdir foo` turned into `:mkdir NAME`.
    let mut u = Ui::new(1_000);
    u.type_str(":mkdir foo");
    assert_eq!(u.app().text(), ":mkdir foo");
    assert_eq!(u.app().ghost_chars(), 0);

    let mut u = Ui::new(1_000);
    u.type_str(":mk");
    assert_eq!(u.app().text(), ":mkdir ", "the command word completes, with no placeholder");
    assert_eq!(u.app().typed(), ":mk");
    u.key(Key::Tab);
    assert_eq!(u.app().typed(), ":mkdir ");
    assert_eq!(u.app().ghost_chars(), 0);
    u.type_str("x");
    assert_eq!(u.app().text(), ":mkdir x");
}

#[test]
fn glob_filter_shows_the_folder_path_in_the_status_line() {
    // Concept-sketch bug: the path disappeared from view while filtering.
    let mut u = Ui::new(1_000);
    u.type_str("~/Downloads/*.pdf");
    assert_eq!(u.app().view_dir(), "/home/user/Downloads");
    assert_eq!(u.app().shown_rows(), 1);
    assert_eq!(u.app().ghost_chars(), 0, "no ghost while filtering");
}

#[test]
fn composition_suppresses_completion() {
    let mut u = Ui::new(1_000);
    u.h.event(Event::Ime(ImeEvent::Preedit { text: "d".into(), active_range_chars: None }));
    u.h.run_steps(2);
    u.type_str("D");
    assert_eq!(u.app().ghost_chars(), 0, "no ghost text while an IME is composing");
    u.h.event(Event::Ime(ImeEvent::Commit("D".into())));
    u.h.run_steps(2);
}

#[test]
fn arrow_keys_move_the_selection_and_wrap() {
    let mut u = Ui::new(1_000);
    let n = u.app().shown_rows();
    assert!(n > 3);
    u.key(Key::ArrowDown);
    let first = u.app().selected_name().map(str::to_string);
    assert!(first.is_some());
    u.key(Key::ArrowUp);
    // From the first row, Up wraps to the last.
    let last = u.app().selected_name().map(str::to_string);
    assert_ne!(first, last);
    u.key(Key::ArrowDown);
    assert_eq!(u.app().selected_name().map(str::to_string), first, "wraps back to the first row");
}

#[test]
fn big_folder_100k_completes_and_filters() {
    let mut u = Ui::new(100_000);
    u.type_str("/big/");
    assert_eq!(u.app().view_dir(), "/big");
    assert_eq!(u.app().shown_rows(), 100_000);
    u.type_str("rep");
    assert!(u.app().highlighted().is_some_and(|n| n.starts_with("report-")));

    let mut u = Ui::new(100_000);
    u.type_str("/big/*.pdf");
    // i % 3 == 0 and not a folder (i % 20 == 0): 33,334 - 1,667.
    assert_eq!(u.app().shown_rows(), 31_667);
}

/// Run with: cargo test -p m0-ghost-bar --release --test bar perf -- --ignored --nocapture
#[test]
#[ignore = "timing probe, not a pass/fail test"]
fn perf_100k_keystrokes() {
    use std::time::Instant;
    let mut u = Ui::new(100_000);
    let frame = |u: &mut Ui, label: &str, f: &dyn Fn(&mut Ui)| {
        let t = Instant::now();
        f(u);
        eprintln!("{label:<34} {:>8.2} ms (2 frames, headless layout, no GPU)", t.elapsed().as_secs_f64() * 1000.0);
    };
    frame(&mut u, "type '/' (idle -> path)", &|u| u.type_str("/"));
    frame(&mut u, "type 'b' (ghost in root)", &|u| u.type_str("b"));
    frame(&mut u, "type 'ig/' (enter 100k folder)", &|u| u.type_str("ig/"));
    frame(&mut u, "type 'r' (scan 100k for prefix)", &|u| u.type_str("r"));
    frame(&mut u, "type 'e' (narrower prefix)", &|u| u.type_str("e"));
    frame(&mut u, "backspace", &|u| u.key(Key::Backspace));
    frame(&mut u, "Tab accept", &|u| u.key(Key::Tab));
    let mut u = Ui::new(100_000);
    frame(&mut u, "type '/big/' then '*' (filter all)", &|u| u.type_str("/big/*"));
    frame(&mut u, "type '.' (filter 100k by glob)", &|u| u.type_str("."));
    frame(&mut u, "type 'pdf' (filter 100k by glob)", &|u| u.type_str("pdf"));
    eprintln!("rows now: {}", u.app().shown_rows());
}

#[test]
fn aliases_and_cd_arguments_complete_in_the_real_widget() {
    let mut u = Ui::new(1_000);
    u.type_str("pi");
    assert_eq!(u.app().text(), "pics/", "an alias is offered as ghost text");
    assert_eq!(u.app().ghost_chars(), 3);
    u.key(Key::Escape);
    u.key(Key::Escape);
    u.type_str(":cd Doc");
    assert_eq!(u.app().text(), ":cd Documents/");
    assert_eq!(u.app().highlighted(), Some("Documents"));
}

#[test]
fn commands_run_and_alt_arrows_walk_the_history() {
    let mut u = Ui::new(1_000);
    u.type_str("proj/nocr");
    u.key(Key::Enter);
    assert_eq!(u.app().cwd(), "/home/user/projects/nocrumbs");
    u.h.key_press_modifiers(egui::Modifiers::ALT, Key::ArrowLeft);
    u.h.run_steps(2);
    assert_eq!(u.app().cwd(), "/home/user", "Alt+Left goes back");
    u.h.key_press_modifiers(egui::Modifiers::ALT, Key::ArrowRight);
    u.h.run_steps(2);
    assert_eq!(u.app().cwd(), "/home/user/projects/nocrumbs", "Alt+Right goes forward");
    u.type_str(":sort size desc");
    u.key(Key::Enter);
    assert_eq!(u.app().status_text(), "Sorted by size, descending");
    assert_eq!(u.app().text(), "~/projects/nocrumbs", "a finished command returns to the idle bar");
}

#[test]
fn list_keys_work_once_the_bar_has_given_up_focus() {
    let mut u = Ui::new(1_000);
    u.key(Key::Escape); // idle bar: the first Esc blurs it
    assert!(!u.focused());
    let before = u.app().shown_rows();

    u.key(Key::ArrowDown);
    u.h.key_press_modifiers(egui::Modifiers::SHIFT, Key::ArrowDown);
    u.h.run_steps(2);
    assert_eq!(u.app().bar().selection_len(), 2, "Shift+Down extends the selection");

    u.key(Key::Delete);
    assert_eq!(u.app().shown_rows(), before - 2, "Del moves the selection to the trash");
    u.h.key_press_modifiers(egui::Modifiers::COMMAND, Key::Z);
    u.h.run_steps(2);
    assert_eq!(u.app().shown_rows(), before, "Ctrl+Z restores it");

    u.h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    u.h.run_steps(2);
    assert_eq!(u.app().bar().selection_len(), before, "Ctrl+A selects everything shown");
    u.h.event(Event::Copy);
    u.h.run_steps(2);
    assert_eq!(u.app().bar().clipboard_summary().as_deref(), Some("Copied 8 items"));

    u.key(Key::Backspace);
    assert_eq!(u.app().cwd(), "/home", "Backspace goes up one folder");
    u.h.event(Event::Text("/".into()));
    u.h.run_steps(2);
    assert!(u.focused(), "/ gives the bar focus back");
}

#[test]
fn while_the_bar_has_focus_ctrl_keys_edit_text_not_files() {
    let mut u = Ui::new(1_000);
    let before = u.app().shown_rows();
    u.type_str("x");
    u.key(Key::Delete); // deletes text, never a file
    u.h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    u.h.run_steps(2);
    assert_eq!(u.app().bar().selection_len(), 0);
    assert_eq!(u.app().shown_rows(), before);
}

#[test]
fn aliases_and_history_survive_a_restart_through_a_real_settings_file() {
    use std::sync::Arc;

    let dir = std::env::temp_dir().join(format!("nocrumbs-spike-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("config.toml");
    let launch = || {
        let mut app = App::with_start(1_000, "/home/user");
        app.attach_store(Arc::new(nocrumbs_config::FileStore::new(&file)));
        let mut h =
            Harness::builder().with_size(Vec2::new(960.0, 640.0)).build_ui_state(|ui, app: &mut App| app.show(ui), app);
        setup(&h.ctx);
        h.run_steps(3);
        Ui { h }
    };

    let mut first = launch();
    assert_eq!(first.app().bar().aliases().len(), 3, "a first run starts with the seed aliases");
    first.type_str(":alias work ~/Documents");
    first.key(Key::Enter);
    first.type_str("Downloads");
    first.key(Key::Enter);
    drop(first);
    assert!(file.is_file(), "the settings file was written");

    let mut second = launch();
    assert_eq!(second.app().bar().aliases().len(), 4, "the new alias is still there");
    second.type_str("wo");
    assert_eq!(second.app().text(), "work/", "and it completes");
    second.key(Key::Escape);
    second.key(Key::Escape);
    second.type_str("Do");
    assert_eq!(second.app().text(), "Downloads/", "the folder used last time is offered first");
    let _ = std::fs::remove_dir_all(&dir);
}
