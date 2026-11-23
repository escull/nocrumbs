// SPDX-License-Identifier: GPL-3.0-or-later
//! Frecency ranking of completions (brief §5.1) and persistence of aliases and frecency across
//! sessions (brief §6), as keystroke scripts.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use nocrumbs_core::sim::Session;
use nocrumbs_core::{Aliases, Bar, Frecency, Key, PersistedState, Tone};
use nocrumbs_testfs::{MemFs, MemStore};

const HOME: &str = "/home/user";
const WEEK: i64 = 7 * 86_400;

struct World {
    s: Session,
    clock: Arc<AtomicI64>,
}

fn bar_with(store: Option<Arc<MemStore>>, clock: Arc<AtomicI64>) -> Bar {
    let mut bar = Bar::new(Arc::new(MemFs::sample(true, 100)), HOME, HOME, '/');
    let c = clock.clone();
    bar.set_clock(Arc::new(move || c.load(Ordering::SeqCst)));
    if let Some(store) = store {
        bar.attach_store(store);
    }
    bar
}

fn world(store: Option<Arc<MemStore>>) -> World {
    let clock = Arc::new(AtomicI64::new(1_000_000));
    World { s: Session::new(bar_with(store, clock.clone())), clock }
}

/// A new session (a restart) on the same store.
fn restart(store: &Arc<MemStore>) -> World {
    world(Some(store.clone()))
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

/// Type `partial` into the idle bar and return what the bar then shows.
fn offer(s: &mut Session, partial: &str) -> String {
    s.select_all();
    s.type_str(partial);
    let shown = s.text().to_string();
    s.key(Key::Escape);
    s.key(Key::Escape);
    shown
}

// ----- frecency ranking -----

#[test]
fn without_history_completion_is_alphabetical() {
    let mut w = world(None);
    assert_eq!(offer(&mut w.s, "Do"), "Documents/");
}

#[test]
fn the_folder_you_actually_use_comes_first() {
    let mut w = world(None);
    run(&mut w.s, "Downloads");
    run(&mut w.s, ":cd ~");
    assert_eq!(offer(&mut w.s, "Do"), "Downloads/", "used once beats never used");
    assert_eq!(offer(&mut w.s, "D"), "Downloads/");
    assert_eq!(offer(&mut w.s, "Doc"), "Documents/", "a longer prefix still narrows it");
}

#[test]
fn frecency_is_per_folder_and_applies_to_the_next_level_too() {
    let mut w = world(None);
    assert_eq!(offer(&mut w.s, "Documents/"), "Documents/quarterly-report.pdf", "alphabetical to begin with");
    run(&mut w.s, "Documents/resume.docx"); // selects the file: that counts as using it
    run(&mut w.s, ":cd ~");
    assert_eq!(offer(&mut w.s, "Documents/"), "Documents/resume.docx");
}

#[test]
fn many_old_visits_can_outweigh_one_recent_visit_and_the_reverse() {
    let mut w = world(None);
    for _ in 0..20 {
        run(&mut w.s, "Documents");
        run(&mut w.s, ":cd ~");
    }
    w.clock.store(1_000_000 + 3 * WEEK, Ordering::SeqCst); // those visits are now old
    run(&mut w.s, "Downloads");
    run(&mut w.s, ":cd ~");
    assert_eq!(offer(&mut w.s, "Do"), "Documents/", "20 old visits (score 20) beat 1 fresh one (score 16)");

    let mut w = world(None);
    for _ in 0..3 {
        run(&mut w.s, "Documents");
        run(&mut w.s, ":cd ~");
    }
    w.clock.store(1_000_000 + 3 * WEEK, Ordering::SeqCst);
    run(&mut w.s, "Downloads");
    run(&mut w.s, ":cd ~");
    assert_eq!(offer(&mut w.s, "Do"), "Downloads/", "3 old visits (score 3) lose to 1 fresh one");
}

#[test]
fn renaming_a_folder_carries_its_history() {
    let mut w = world(None);
    run(&mut w.s, "Downloads");
    run(&mut w.s, ":cd ~");
    let row = (0..w.s.bar().row_count()).find(|&r| w.s.bar().row(r).name == "Downloads").unwrap();
    w.s.bar_mut().click_row(row, false, false);
    run(&mut w.s, ":rename Fetched");
    assert!(w.s.bar().frecency().score("/home/user/Fetched", 1_000_000) > 0);
    assert_eq!(w.s.bar().frecency().score("/home/user/Downloads", 1_000_000), 0);
    assert_eq!(offer(&mut w.s, "F"), "Fetched/");
}

#[test]
fn history_never_offers_something_that_is_gone() {
    let mut w = world(None);
    run(&mut w.s, "Downloads");
    run(&mut w.s, ":cd ~");
    let row = (0..w.s.bar().row_count()).find(|&r| w.s.bar().row(r).name == "Downloads").unwrap();
    w.s.bar_mut().click_row(row, false, false);
    run(&mut w.s, ":rm");
    assert_eq!(offer(&mut w.s, "Do"), "Documents/", "candidates come from what exists");
}

// ----- persistence -----

#[test]
fn aliases_survive_a_restart() {
    let store = Arc::new(MemStore::new());
    let mut w = world(Some(store.clone()));
    run(&mut w.s, ":alias work ~/Documents");
    assert_eq!(status(&w.s).1, Tone::Good);
    assert_eq!(store.saved().unwrap().aliases.get("work", true).unwrap().1, "/home/user/Documents");

    let mut w2 = restart(&store);
    assert_eq!(w2.s.bar().aliases().len(), 1);
    assert_eq!(offer(&mut w2.s, "wo"), "work/", "and it completes");
    run(&mut w2.s, "work");
    assert_eq!(w2.s.bar().cwd(), "/home/user/Documents");
}

#[test]
fn unalias_is_persisted_too() {
    let store = Arc::new(MemStore::with_state(PersistedState {
        aliases: Aliases::from_pairs([("dl", "/home/user/Downloads"), ("p", "/home/user/projects")]),
        frecency: Frecency::new(),
    }));
    let mut w = world(Some(store.clone()));
    assert_eq!(w.s.bar().aliases().len(), 2, "loaded at start");
    run(&mut w.s, ":unalias dl");
    let names: Vec<String> = store.saved().unwrap().aliases.iter().map(|(n, _)| n.to_string()).collect();
    assert_eq!(names, ["p"]);
}

#[test]
fn navigation_history_survives_a_restart_and_keeps_ranking() {
    let store = Arc::new(MemStore::new());
    let mut w = world(Some(store.clone()));
    run(&mut w.s, "Downloads");
    assert_eq!(store.saved().unwrap().frecency.score("/home/user/Downloads", 1_000_000), 16);

    let mut w2 = restart(&store);
    assert_eq!(offer(&mut w2.s, "Do"), "Downloads/", "the next session already knows");
}

#[test]
fn renaming_persists_the_carried_history() {
    let store = Arc::new(MemStore::new());
    let mut w = world(Some(store.clone()));
    run(&mut w.s, "Downloads");
    run(&mut w.s, ":cd ~");
    let row = (0..w.s.bar().row_count()).find(|&r| w.s.bar().row(r).name == "Downloads").unwrap();
    w.s.bar_mut().click_row(row, false, false);
    run(&mut w.s, ":rename Fetched");
    let saved = store.saved().unwrap();
    assert!(saved.frecency.score("/home/user/Fetched", 1_000_000) > 0);
    assert_eq!(saved.frecency.score("/home/user/Downloads", 1_000_000), 0);
}

#[test]
fn a_failing_disk_is_reported_once_and_the_bar_keeps_working() {
    let store = Arc::new(MemStore::new());
    let mut w = world(Some(store.clone()));
    store.fail_saves(true);
    run(&mut w.s, ":alias a ~");
    assert_eq!(status(&w.s), ("Could not save settings: the disk is read-only".into(), Tone::Warn));
    run(&mut w.s, ":alias b ~");
    assert_eq!(status(&w.s).1, Tone::Good, "no repeat nagging");
    assert_eq!(w.s.bar().aliases().len(), 2, "still works in memory");
    assert_eq!(store.save_count(), 0);

    store.fail_saves(false);
    run(&mut w.s, ":alias c ~");
    assert_eq!(status(&w.s).1, Tone::Good);
    assert_eq!(store.saved().unwrap().aliases.len(), 3, "everything is saved once the disk recovers");

    store.fail_saves(true);
    run(&mut w.s, ":alias d ~");
    assert_eq!(status(&w.s).1, Tone::Warn, "a new failure is reported again");
}

#[test]
fn a_recovered_damaged_file_is_announced() {
    let store = Arc::new(MemStore::new());
    store.set_notice("The settings file was damaged. A copy was kept as config.toml.bad; starting fresh.");
    let w = world(Some(store));
    assert_eq!(
        status(&w.s),
        ("The settings file was damaged. A copy was kept as config.toml.bad; starting fresh.".into(), Tone::Warn)
    );
}

#[test]
fn a_store_that_cannot_load_is_not_written_to() {
    let store = Arc::new(MemStore::new());
    store.fail_load("config.toml was written by a newer NoCrumbs (settings version 99); leaving it alone");
    let mut w = world(Some(store.clone()));
    let (msg, tone) = status(&w.s);
    assert_eq!(tone, Tone::Warn);
    assert!(
        msg.starts_with("Settings were not loaded and will not be saved: ") && msg.contains("newer NoCrumbs"),
        "{msg}"
    );
    run(&mut w.s, ":alias a ~");
    assert_eq!(status(&w.s).1, Tone::Good, "the bar works normally");
    assert_eq!(store.saved(), None, "nothing was written over the newer file");
}

#[test]
fn state_reports_exactly_what_persists() {
    let mut w = world(None);
    run(&mut w.s, ":alias a ~/Documents");
    run(&mut w.s, "Downloads");
    let st = w.s.bar().state();
    assert_eq!(st.aliases.len(), 1);
    assert_eq!(st.frecency.len(), 1, "creating an alias is not a visit; going to Downloads is");
}
