// SPDX-License-Identifier: GPL-3.0-or-later
//! Keystroke scripts for the bar parser: aliases, `:commands` and their quoting, name
//! validation, `:cd` completion, sorting, history, and the resolution order (brief §4–§8).

use std::sync::Arc;

use nocrumbs_core::sim::Session;
use nocrumbs_core::{Aliases, Bar, CompletionKind, Key, Mode, SortDir, SortKey, Tone};
use nocrumbs_testfs::MemFs;

const HOME: &str = "/home/user";

fn session_on(fs: MemFs, aliases: &[(&str, &str)]) -> Session {
    let mut bar = Bar::new(Arc::new(fs), HOME, HOME, '/');
    bar.set_aliases(Aliases::from_pairs(aliases.iter().copied()));
    Session::new(bar)
}

fn seeded() -> &'static [(&'static str, &'static str)] {
    &[("dl", "/home/user/Downloads"), ("proj", "/home/user/projects"), ("pics", "/home/user/Pictures")]
}

/// Windows-like (case-insensitive) with the usual three aliases.
fn session() -> Session {
    session_on(MemFs::sample(true, 100), seeded())
}

/// Select everything (as a user does after an error), type a line, press Enter.
fn run(s: &mut Session, line: &str) {
    s.select_all();
    s.type_str(line);
    s.key(Key::Enter);
}

fn names(s: &Session) -> Vec<String> {
    (0..s.bar().row_count()).map(|i| s.bar().row(i).name.clone()).collect()
}

fn status(s: &Session) -> (String, Tone) {
    let (m, t) = s.bar().status();
    (m.to_string(), t)
}

// ----- aliases: completion, preview, navigation -----

#[test]
fn alias_names_complete_as_the_first_segment() {
    let mut s = session();
    s.type_str("pi");
    // `pics` (alias) sorts before `Pictures` (folder).
    assert_eq!(s.text(), "pics/");
    let c = s.bar().analysis().completion.clone().unwrap();
    assert_eq!(c.kind, CompletionKind::Alias);
    assert_eq!(c.matches, 2, "the alias and the folder");
    assert_eq!(s.bar().highlighted_name(), None, "an alias has no row to highlight");
    s.key(Key::Tab);
    assert_eq!(s.text(), "pics/holiday-2026 (1).jpg", "Tab descends into the alias target");
    assert_eq!(s.bar().view_dir(), "/home/user/Pictures");
}

#[test]
fn alias_composes_with_subpaths_and_navigates() {
    let mut s = session();
    s.type_str("proj/nocr");
    assert_eq!(s.bar().analysis().alias.as_ref().map(|a| a.name.as_str()), Some("proj"));
    assert_eq!(s.text(), "proj/nocrumbs/");
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/projects/nocrumbs");
}

#[test]
fn typing_an_alias_previews_its_target_then_goes_there() {
    let mut s = session();
    s.type_str("dl");
    assert_eq!(s.bar().view_dir(), "/home/user/Downloads");
    assert_eq!(s.bar().cwd(), HOME, "previewing does not navigate");
    s.key(Key::Escape); // remove the ghost "/"
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Downloads");
}

#[test]
fn glob_works_through_an_alias_prefix() {
    let mut s = session();
    s.type_str("dl/*.pdf");
    assert_eq!(s.bar().mode(), Mode::Glob);
    assert_eq!(s.bar().view_dir(), "/home/user/Downloads");
    assert_eq!(names(&s), ["invoice-nov.pdf"]);
    assert_eq!(s.bar().analysis().alias.as_ref().map(|a| a.name.as_str()), Some("dl"));
}

#[test]
fn resolution_order_is_command_then_glob_then_alias_then_path() {
    let mut s = session();
    s.type_str(":cd dl");
    assert_eq!(s.bar().mode(), Mode::Command, "a leading colon is always a command");
    let mut s = session();
    s.type_str("proj*");
    assert_eq!(s.bar().mode(), Mode::Glob, "a glob in the last segment beats an alias");
    let mut s = session();
    s.type_str("proj");
    assert_eq!(s.bar().mode(), Mode::Path);
    assert!(s.bar().analysis().alias.is_some(), "an alias beats a literal path");
}

#[test]
fn an_alias_shadows_a_real_folder_and_says_so() {
    let mut s = session_on(MemFs::sample(true, 10), &[("Documents", "/home/user/Downloads")]);
    s.type_str("Documents");
    assert_eq!(s.bar().analysis().alias.as_ref().map(|a| a.shadows), Some(true));
    assert_eq!(status(&s).0, "Alias Documents shadows the folder of the same name here");
    s.key(Key::Escape);
    s.key(Key::Enter);
    assert_eq!(s.bar().cwd(), "/home/user/Downloads", "the alias wins");
}

#[test]
fn a_dangling_alias_gets_its_own_message() {
    let mut s = session_on(MemFs::sample(true, 10), &[("gone", "/nowhere/at/all")]);
    run(&mut s, "gone");
    assert_eq!(s.bar().cwd(), HOME);
    let (msg, tone) = status(&s);
    assert_eq!(tone, Tone::Bad);
    assert!(msg.contains("gone") && msg.contains("no longer exists") && msg.contains(":unalias gone"), "{msg}");
    let mut s = session_on(MemFs::sample(true, 10), &[("gone", "/nowhere/at/all")]);
    run(&mut s, "gone/sub");
    assert!(status(&s).0.contains("no longer exists"));
}

// ----- :alias / :unalias -----

#[test]
fn alias_command_creates_and_the_alias_works() {
    let mut s = session();
    run(&mut s, ":alias work ~/Documents");
    assert_eq!(status(&s), ("Alias work created: ~/Documents. Type work to jump there".into(), Tone::Good));
    assert_eq!(s.text(), "~", "a successful command returns to the idle bar");
    run(&mut s, "work");
    assert_eq!(s.bar().cwd(), "/home/user/Documents");
}

#[test]
fn alias_command_updates_an_existing_alias() {
    let mut s = session();
    run(&mut s, ":alias dl /etc");
    assert!(status(&s).0.starts_with("Alias dl updated: /etc"));
    run(&mut s, "dl");
    assert_eq!(s.bar().cwd(), "/etc");
}

#[test]
fn alias_command_accepts_paths_with_spaces_and_quotes() {
    let mut s = session();
    run(&mut s, ":alias \"my pics\" ~/Pictures");
    assert!(status(&s).0.contains("cannot contain spaces"), "{}", status(&s).0);
    run(&mut s, ":alias p2 \"~/Pictures\"");
    assert_eq!(status(&s).1, Tone::Good);
}

#[test]
fn alias_target_must_be_an_existing_folder() {
    let mut s = session();
    run(&mut s, ":alias x ~/notes.md");
    assert_eq!(status(&s), ("Alias target is not a folder: ~/notes.md".into(), Tone::Bad));
    let mut s = session();
    run(&mut s, ":alias x ~/nope");
    assert_eq!(status(&s).1, Tone::Bad);
    assert!(s.bar().aliases().get("x", true).is_none());
}

#[test]
fn alias_names_are_validated() {
    for (line, expect) in [
        (":alias a/b ~", "cannot contain / or \\"),
        (":alias bad* ~", "cannot contain *"),
        (":alias v1.0 ~", "cannot contain ."),
        (":alias ~x ~", "cannot start with : or ~"),
    ] {
        let mut s = session();
        run(&mut s, line);
        assert!(status(&s).0.contains(expect), "{line}: {}", status(&s).0);
        assert_eq!(status(&s).1, Tone::Bad);
    }
}

#[test]
fn unalias_removes_and_reports_missing() {
    let mut s = session();
    run(&mut s, ":unalias dl");
    assert_eq!(status(&s), ("Removed alias dl".into(), Tone::Good));
    run(&mut s, "dl");
    assert_eq!(status(&s).1, Tone::Bad, "no alias, no folder called dl");
    run(&mut s, ":unalias nope");
    assert_eq!(status(&s), ("No such alias: nope".into(), Tone::Bad));
}

#[test]
fn alias_alone_lists_them() {
    let mut s = session();
    run(&mut s, ":alias");
    assert_eq!(status(&s).0, "Aliases: dl = ~/Downloads | pics = ~/Pictures | proj = ~/projects");
    let mut s = session_on(MemFs::sample(true, 10), &[]);
    run(&mut s, ":alias");
    assert!(status(&s).0.starts_with("No aliases yet"));
}

// ----- :cd and path-argument completion -----

#[test]
fn cd_completes_its_path_argument() {
    let mut s = session();
    s.type_str(":cd Doc");
    assert_eq!(s.bar().mode(), Mode::Command);
    assert_eq!(s.text(), ":cd Documents/");
    assert_eq!(s.bar().highlighted_name(), Some("Documents"));
    s.key(Key::Tab);
    assert_eq!(s.bar().typed(), ":cd Documents/");
    assert_eq!(s.text(), ":cd Documents/quarterly-report.pdf", "descends like a path");
    assert_eq!(s.bar().view_dir(), "/home/user/Documents");
}

#[test]
fn cd_navigates_with_unquoted_spaces_and_aliases() {
    let mut s = session();
    run(&mut s, ":cd Documents/recipes");
    assert_eq!(s.bar().cwd(), "/home/user/Documents/recipes");
    run(&mut s, ":cd dl");
    assert_eq!(s.bar().cwd(), "/home/user/Downloads");
    run(&mut s, ":cd /home/user/Pictures");
    run(&mut s, ":cd screenshots");
    assert_eq!(s.bar().cwd(), "/home/user/Pictures/screenshots");
    run(&mut s, ":cd nowhere");
    assert_eq!(status(&s), ("No such path: nowhere".into(), Tone::Bad));
    assert_eq!(s.bar().cwd(), "/home/user/Pictures/screenshots");
}

#[test]
fn cd_completion_offers_aliases_and_alias_arguments_complete_paths() {
    let mut s = session();
    s.type_str(":cd pr");
    assert_eq!(s.text(), ":cd proj/");
    let mut s = session();
    s.type_str(":alias w ~/Doc");
    assert_eq!(s.text(), ":alias w ~/Documents/");
}

#[test]
fn no_path_completion_for_quoted_or_glob_arguments() {
    for line in [":cd \"Doc", ":cd *.x", ":alias w \"~/Doc", ":mkdir Doc"] {
        let mut s = session();
        s.type_str(line);
        assert_eq!(s.bar().ghost_chars(), 0, "{line}");
        assert_eq!(s.text(), line);
    }
}

// ----- sorting -----

#[test]
fn sort_by_size_descending_keeps_folders_first() {
    let mut s = session();
    run(&mut s, ":cd Downloads");
    run(&mut s, ":sort size desc");
    assert_eq!(status(&s).0, "Sorted by size, descending");
    assert_eq!(
        names(&s),
        ["backup-2026-06.tar.gz", "installer-v3.exe", "wallpaper-purple.png", "invoice-nov.pdf", "lumo-2.0-notes.txt"]
    );
    run(&mut s, ":cd ~/Documents");
    assert_eq!(names(&s)[0], "recipes", "folders stay first even when descending");
    assert_eq!(s.bar().sort(), (SortKey::Size, SortDir::Desc), "the sort sticks across folders");
}

#[test]
fn sort_commands_and_direction_rules() {
    let mut s = session();
    run(&mut s, ":cd Downloads");
    run(&mut s, ":sort date");
    assert_eq!(s.bar().sort(), (SortKey::Date, SortDir::Asc));
    assert_eq!(names(&s)[0], "backup-2026-06.tar.gz", "oldest first");
    run(&mut s, ":desc");
    assert_eq!(s.bar().sort(), (SortKey::Date, SortDir::Desc));
    assert_eq!(names(&s)[0], "installer-v3.exe", "newest first");
    run(&mut s, ":sort date");
    assert_eq!(s.bar().sort().1, SortDir::Desc, "re-sorting by the same key keeps the direction");
    run(&mut s, ":sort name");
    assert_eq!(s.bar().sort(), (SortKey::Name, SortDir::Asc), "a new key starts ascending");
    run(&mut s, ":asc");
    assert_eq!(names(&s)[0], "backup-2026-06.tar.gz");
}

#[test]
fn name_sort_descending_and_ties() {
    let mut s = session();
    run(&mut s, ":cd Documents");
    run(&mut s, ":sort name desc");
    assert_eq!(names(&s), ["recipes", "tax-return-2025.pdf", "resume.docx", "quarterly-report.pdf"]);
}

#[test]
fn clicking_a_column_header_sorts_and_flips() {
    let mut s = session();
    run(&mut s, ":cd Downloads");
    s.bar_mut().sort_by_column(SortKey::Size);
    assert_eq!(s.bar().sort(), (SortKey::Size, SortDir::Asc));
    s.bar_mut().sort_by_column(SortKey::Size);
    assert_eq!(s.bar().sort(), (SortKey::Size, SortDir::Desc));
    s.bar_mut().sort_by_column(SortKey::Date);
    assert_eq!(s.bar().sort(), (SortKey::Date, SortDir::Asc));
}

#[test]
fn sorting_survives_filters_and_hidden_toggles() {
    let mut s = session();
    run(&mut s, ":cd Documents");
    run(&mut s, ":sort size desc");
    s.type_str("*.pdf");
    assert_eq!(names(&s), ["tax-return-2025.pdf", "quarterly-report.pdf"]);
    s.key(Key::Escape);
    s.key(Key::Escape);
    run(&mut s, ":cd ~");
    run(&mut s, ":hidden");
    assert_eq!(status(&s).0, "Hidden files shown");
    assert_eq!(s.bar().row_count(), 10);
    assert_eq!(s.bar().sort(), (SortKey::Size, SortDir::Desc));
    run(&mut s, ":hidden");
    assert_eq!(status(&s).0, "Hidden files hidden");
    assert_eq!(s.bar().row_count(), 8);
}

#[test]
fn sorting_100k_entries_works() {
    let mut s = session_on(MemFs::sample(true, 100_000), &[]);
    run(&mut s, ":cd /big");
    run(&mut s, ":sort size desc");
    let n = s.bar().row_count();
    assert_eq!(n, 100_000);
    let sizes: Vec<u64> = (0..n).map(|i| s.bar().row(i)).filter(|e| !e.is_dir).map(|e| e.size).take(1000).collect();
    assert!(sizes.windows(2).all(|w| w[0] >= w[1]), "descending within files");
    assert!(s.bar().row(0).is_dir, "folders first");
}

// ----- history -----

#[test]
fn back_and_forward_walk_the_history() {
    let mut s = session();
    run(&mut s, "Documents");
    run(&mut s, "~/Downloads");
    run(&mut s, ":back");
    assert_eq!(s.bar().cwd(), "/home/user/Documents");
    run(&mut s, ":back");
    assert_eq!(s.bar().cwd(), HOME);
    run(&mut s, ":back");
    assert_eq!(status(&s), ("Nothing to go back to".into(), Tone::Info));
    run(&mut s, ":fwd");
    assert_eq!(s.bar().cwd(), "/home/user/Documents");
    s.key(Key::Forward);
    assert_eq!(s.bar().cwd(), "/home/user/Downloads");
    s.key(Key::Forward);
    assert_eq!(status(&s).0, "Nothing to go forward to");
    s.key(Key::Back);
    s.key(Key::Back);
    assert_eq!(s.bar().cwd(), HOME);
}

#[test]
fn navigating_after_going_back_drops_the_forward_history() {
    let mut s = session();
    run(&mut s, "Documents");
    run(&mut s, ":back");
    run(&mut s, "Pictures");
    s.key(Key::Forward);
    assert_eq!(s.bar().cwd(), "/home/user/Pictures", "Documents is no longer ahead");
    assert_eq!(status(&s).0, "Nothing to go forward to");
}

#[test]
fn revisiting_the_same_folder_does_not_grow_the_history() {
    let mut s = session();
    run(&mut s, "Documents");
    run(&mut s, "."); // already here
    run(&mut s, ":back");
    assert_eq!(s.bar().cwd(), HOME, "one step back, not two");
}

// ----- names, quoting and errors through the bar -----

#[test]
fn file_commands_validate_names_by_platform() {
    let mut s = session();
    for (line, expect) in [
        (":mkdir con", "reserved device name"),
        (":touch COM1.txt", "reserved device name"),
        (":mkdir a:b", "cannot contain :"),
        (":rename name.", "cannot end with a space or a dot"),
        (":mkdir a/b", "cannot contain / or \\"),
        (":mkdir ..", "is not a valid name"),
    ] {
        run(&mut s, line);
        assert_eq!(status(&s).1, Tone::Bad, "{line}");
        assert!(status(&s).0.contains(expect), "{line}: {}", status(&s).0);
    }
    run(&mut s, ":mkdir my folder");
    assert_eq!(status(&s), ("File operations are not implemented yet".into(), Tone::Info), "a valid name passes");
    run(&mut s, "\u{0}");
    run(&mut s, ":mkdir \"quoted name\"");
    assert_eq!(status(&s).1, Tone::Info);
}

#[test]
fn unix_rules_allow_what_windows_forbids() {
    let mut s = session_on(MemFs::sample(false, 10), &[]);
    run(&mut s, ":mkdir a:b");
    assert_eq!(status(&s).1, Tone::Info, "legal on Linux");
    run(&mut s, ":mkdir con");
    assert_eq!(status(&s).1, Tone::Info);
    run(&mut s, ":mkdir a/b");
    assert_eq!(status(&s).1, Tone::Bad, "never legal");
}

#[test]
fn parser_errors_are_friendly_and_keep_the_text() {
    for (line, msg) in [
        (":sort color", "Usage: :sort name|size|date [asc|desc]"),
        (":bogus", "Unknown command :bogus. Try :help"),
        (":mkdir \"oops", "Unterminated quote: add the closing \""),
        (":cd", "Usage: :cd PATH"),
        (":asc now", ":asc takes no arguments"),
    ] {
        let mut s = session();
        s.type_str(line);
        let typed = s.bar().typed().to_string();
        let before = s.bar().error_count();
        s.key(Key::Enter);
        assert_eq!(status(&s), (msg.to_string(), Tone::Bad), "{line}");
        assert_eq!(s.bar().error_count(), before + 1, "{line}");
        assert!(s.text().starts_with(&typed), "the bar keeps what was typed so it can be fixed: {line}");
    }
}

#[test]
fn a_bare_colon_with_the_ghost_removed_is_an_error() {
    let mut s = session();
    s.type_str(":");
    assert_eq!(s.text(), ":alias ", "the first command is offered as ghost text");
    s.key(Key::Escape);
    s.key(Key::Enter);
    assert_eq!(status(&s), ("Type a command after the colon. Try :help".into(), Tone::Bad));
}

#[test]
fn a_ghosted_command_runs_as_completed() {
    let mut s = session();
    s.type_str(":sor");
    assert_eq!(s.text(), ":sort date");
    s.key(Key::Enter);
    assert_eq!(s.bar().sort(), (SortKey::Date, SortDir::Asc));
    assert_eq!(status(&s).0, "Sorted by date, ascending");
}

#[test]
fn help_is_one_line_and_mentions_everything() {
    let mut s = session();
    run(&mut s, ":help");
    let (msg, tone) = status(&s);
    assert_eq!(tone, Tone::Good);
    for word in
        [":alias", ":unalias", ":cd", ":back", ":sort", ":hidden", ":mkdir", ":rename", ":rm", ":paste", ":undo"]
    {
        assert!(msg.contains(word), "help mentions {word}");
    }
    assert!(!msg.contains('\n'));
}

#[test]
fn successful_commands_return_to_the_idle_bar() {
    let mut s = session();
    run(&mut s, ":hidden");
    assert_eq!(s.text(), "~");
    assert_eq!(s.bar().mode(), Mode::Empty);
    assert_eq!(s.selection().end(), 1, "and select it, ready for the next input");
}
