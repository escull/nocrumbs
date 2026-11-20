// SPDX-License-Identifier: GPL-3.0-or-later
//! The first keystroke-style cases. Each one is a regression for a bug found in the
//! original HTML concept sketch (see ROADMAP.md, progress log).

use nocrumbs_core::{Analysis, BarContext, Mode, analyze, display_path, resolve};
use nocrumbs_testfs::MemFs;

const HOME: &str = "/home/user";

fn typed(input: &str, ci: bool) -> Analysis {
    let fs = MemFs::sample(ci, 1_000);
    let c = BarContext { fs: &fs, cwd: HOME, home: HOME, sep: '/', show_hidden: false };
    analyze(input, &c)
}

#[test]
fn ghost_never_alters_typed_text() {
    let a = typed("Doc", true);
    let c = a.completion.expect("completion");
    assert_eq!(c.ghost, "uments/");
    assert_eq!(c.accepted, "Documents/");
    assert!(c.matches >= 1);
}

#[test]
fn tab_rewrites_to_canonical_case() {
    // Typed case is preserved while ghosting, canonical on accept (concept-sketch bug:
    // `doc` became `documents/`, which then failed to resolve).
    let a = typed("doc", true);
    let c = a.completion.expect("completion");
    assert_eq!(c.ghost, "uments/");
    assert_eq!(c.accepted, "Documents/");
}

#[test]
fn case_sensitive_fs_falls_back_to_folding() {
    let a = typed("doc", false);
    assert_eq!(a.completion.expect("completion").accepted, "Documents/");
    let a = typed("Doc", false);
    assert_eq!(a.completion.expect("completion").accepted, "Documents/");
}

#[test]
fn directories_offer_next_level_immediately() {
    // After Tab on `Doc` the bar holds `Documents/`; the next level is offered at once.
    let a = typed("Documents/", true);
    let c = a.completion.expect("next level");
    assert_eq!(c.ghost, "quarterly-report.pdf");
    assert_eq!(a.dir.as_deref(), Some("/home/user/Documents"));
}

#[test]
fn nested_partial_completes_in_that_folder() {
    let a = typed("Documents/rec", true);
    let c = a.completion.expect("completion");
    assert_eq!(c.accepted, "Documents/recipes/");
    assert_eq!(a.dir.as_deref(), Some("/home/user/Documents"));
}

#[test]
fn both_separators_work_and_are_preserved() {
    let a = typed(r"Documents\rec", true);
    assert_eq!(a.completion.expect("completion").accepted, r"Documents\recipes/".replace('/', "\\"));
}

#[test]
fn home_and_absolute_and_parent_paths() {
    assert_eq!(typed("~/Down", true).completion.unwrap().accepted, "~/Downloads/");
    assert_eq!(typed("/et", true).completion.unwrap().accepted, "/etc/");
    assert_eq!(typed("../u", true).dir.as_deref(), Some("/home"));
    assert_eq!(typed("../u", true).completion.unwrap().accepted, "../user/");
}

#[test]
fn no_such_path_means_no_completion() {
    let a = typed("zzz", true);
    assert!(a.completion.is_none());
    let a = typed("nope/also", true);
    assert!(a.completion.is_none());
    assert!(a.dir.is_none());
}

#[test]
fn dotfiles_complete_only_when_a_dot_is_typed() {
    assert!(typed(".conf", true).completion.is_some_and(|c| c.accepted == ".config/"));
    // `.` alone offers a hidden entry because the user asked for dotfiles.
    assert!(typed(".", true).completion.is_some());
    // A non-dot prefix never offers hidden entries.
    assert!(typed("zsh", true).completion.is_none());
}

#[test]
fn command_arguments_are_never_overwritten() {
    // Concept-sketch bug: `:mkdir foo` became `:mkdir NAME`.
    for input in [":mkdir foo", ":alias p ~/x", ":cd /etc", ":rename new name", ":touch a"] {
        let a = typed(input, true);
        assert_eq!(a.mode, Mode::Command);
        assert!(a.completion.is_none(), "{input} should not offer a completion");
    }
}

#[test]
fn commands_complete_the_word_and_keywords_only() {
    let c = typed(":mk", true).completion.unwrap();
    assert_eq!(c.ghost, "dir ");
    assert_eq!(c.accepted, ":mkdir ");
    let c = typed(":sort d", true).completion.unwrap();
    assert_eq!(c.accepted, ":sort date");
    assert_eq!(c.ghost, "ate");
    // Case of the typed prefix does not matter; the accepted text is canonical.
    assert_eq!(typed(":SOR", true).completion.unwrap().accepted, ":sort date");
}

#[test]
fn glob_selects_folder_by_path_prefix() {
    let a = typed("~/Downloads/*.pdf", true);
    assert_eq!(a.mode, Mode::Glob);
    assert_eq!(a.glob.as_deref(), Some("*.pdf"));
    assert_eq!(a.dir.as_deref(), Some("/home/user/Downloads"));
    assert!(a.completion.is_none());

    let a = typed("*.pdf", true);
    assert_eq!(a.dir.as_deref(), Some("/home/user"));
}

#[test]
fn resolves_existing_paths_and_reports_missing_ones() {
    let fs = MemFs::sample(true, 100);
    let c = BarContext { fs: &fs, cwd: HOME, home: HOME, sep: '/', show_hidden: false };
    let r = resolve("documents/RECIPES", &c).expect("case-insensitive");
    assert_eq!((r.path.as_str(), r.is_dir), ("/home/user/Documents/recipes", true));
    let r = resolve("Documents/resume.docx", &c).unwrap();
    assert!(!r.is_dir);
    assert_eq!(resolve("..", &c).unwrap().path, "/home");
    assert_eq!(resolve("~", &c).unwrap().path, HOME);
    assert_eq!(resolve("", &c).unwrap().path, HOME);
    assert!(resolve("no/such", &c).is_none());
    assert_eq!(display_path("/home/user/Documents", &c), "~/Documents");
    assert_eq!(display_path("/etc", &c), "/etc");
}

#[test]
fn big_folder_completion_is_correct() {
    let fs = MemFs::sample(true, 100_000);
    let c = BarContext { fs: &fs, cwd: HOME, home: HOME, sep: '/', show_hidden: false };
    let a = analyze("/big/report-0999", &c);
    let comp = a.completion.expect("completion");
    // Alphabetically first existing report-0999NN.pdf: i % 3 == 0, and i % 20 != 0 (those are folders).
    assert_eq!(comp.accepted, "/big/report-099903.pdf");
    assert!(comp.matches > 10);
}
