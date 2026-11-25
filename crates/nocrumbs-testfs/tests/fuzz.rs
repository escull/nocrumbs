// SPDX-License-Identifier: GPL-3.0-or-later
//! Property tests (brief §15.6, §16): random input never panics, and invariants hold.
//!
//! These are randomised property tests (proptest), not coverage-guided fuzzing. When one fails,
//! proptest shrinks the input to a small reproduction and records it under
//! `proptest-regressions/`; commit that file so the case is replayed forever.

use std::sync::Arc;

use nocrumbs_core::sim::Session;
use nocrumbs_core::{
    Aliases, Bar, BarContext, Frecency, Fs, Glob, Key, Mode, NameRules, SortKey, analyze, parse_command, resolve,
    unique_name, validate_name,
};
use nocrumbs_testfs::{MemFs, MemFsBuilder};
use proptest::prelude::*;
use proptest::sample::select;

const HOME: &str = "/home/user";

/// Pieces that exercise the parser: commands, paths, globs, aliases, quoting, odd characters.
const FRAGMENTS: &[&str] = &[
    ":",
    "sort",
    "size",
    "desc",
    "asc",
    "cd",
    "mkdir",
    "touch",
    "rename",
    "rm",
    "rm!",
    "copy",
    "cut",
    "paste",
    "undo",
    "redo",
    "alias",
    "unalias",
    "hidden",
    "back",
    "fwd",
    "help",
    "open",
    "log",
    " ",
    "  ",
    "\"",
    "\\\"",
    "\\",
    "/",
    "~",
    ".",
    "..",
    "../",
    "*",
    "?",
    "Doc",
    "doc",
    "D",
    "dl",
    "proj",
    "pics",
    "notes.md",
    "Downloads",
    "Documents/",
    "projects",
    "home",
    "user",
    "big",
    "etc",
    "CON",
    "a:b",
    "é",
    "日",
    "İ",
    "\u{0}",
    "\t",
    "(1)",
    "-",
];

fn bar_input() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        6 => select(FRAGMENTS).prop_map(str::to_string),
        1 => any::<char>().prop_map(String::from),
    ];
    prop::collection::vec(piece, 0..10).prop_map(|v| v.concat())
}

fn seeded_aliases() -> Aliases {
    Aliases::from_pairs([("dl", "/home/user/Downloads"), ("proj", "/home/user/projects"), ("gone", "/nowhere")])
}

/// Names that stress case folding: characters whose lower case has a different length, several
/// forms of sigma, combining marks, and ordinary punctuation.
fn tricky_name() -> impl Strategy<Value = String> {
    let c = select(vec![
        'a', 'A', 'b', 'i', 'I', 'İ', '\u{307}', 'ß', 'ẞ', 's', 'S', 'Σ', 'σ', 'ς', 'é', 'É', 'k', 'K', '\u{212A}',
        ' ', '.', '-', '(', ')', '日',
    ]);
    prop::collection::vec(c, 1..7)
        .prop_map(|v| v.into_iter().collect::<String>())
        .prop_filter("not a dot name", |s| s != "." && s != "..")
}

fn fold(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn the_command_parser_never_panics(input in bar_input()) {
        let _ = parse_command(&input);
        let _ = parse_command(&format!(":{input}"));
    }

    #[test]
    fn completion_never_panics_and_what_it_offers_exists(input in bar_input(), case_insensitive in any::<bool>()) {
        let fs = MemFs::sample(case_insensitive, 20);
        let aliases = seeded_aliases();
        let frecency = Frecency::new();
        let ctx = BarContext {
            fs: &fs, aliases: &aliases, frecency: &frecency, now: 0,
            cwd: HOME, home: HOME, sep: '/', show_hidden: false,
        };
        let a = analyze(&input, &ctx);
        let _ = resolve(&input, &ctx);
        if let (Mode::Path, Some(c)) = (a.mode, a.completion) {
            // The ghost only ever appends: typed + ghost is the accepted text, up to case.
            prop_assert_eq!(fold(&format!("{input}{}", c.ghost)), fold(&c.accepted));
            // What Tab would produce must resolve (a dangling alias is the one exception).
            if c.name != "gone" && !input.starts_with("gone") {
                prop_assert!(resolve(&c.accepted, &ctx).is_some(), "accepted {:?} does not resolve", c.accepted);
            }
        }
    }

    #[test]
    fn completion_of_tricky_unicode_names_never_mangles_what_was_typed(
        names in prop::collection::btree_set(tricky_name(), 1..6),
        pick in any::<prop::sample::Index>(),
        cut in any::<prop::sample::Index>(),
        recase in 0u8..3,
    ) {
        let mut b = MemFsBuilder::new(true);
        for n in &names {
            b.file(&format!("/d/{n}"), 1, 0);
        }
        let fs = b.finish();
        let name = pick.get(&names.iter().collect::<Vec<_>>()).to_string();
        let chars: Vec<char> = name.chars().collect();
        let keep = cut.index(chars.len()) + 1;
        let mut typed: String = chars[..keep].iter().collect();
        // As typed, SHOUTED, or lower-cased: lower-casing can change the number of characters.
        typed = match recase {
            1 => typed.to_uppercase(),
            2 => typed.to_lowercase(),
            _ => typed,
        };
        let aliases = Aliases::new();
        let frecency = Frecency::new();
        let ctx = BarContext {
            fs: &fs, aliases: &aliases, frecency: &frecency, now: 0,
            cwd: "/d", home: "/d", sep: '/', show_hidden: true,
        };
        if let Some(c) = analyze(&typed, &ctx).completion {
            prop_assert_eq!(
                fold(&format!("{typed}{}", c.ghost)), fold(&c.accepted),
                "typed {:?} + ghost {:?} is not {:?}", typed, c.ghost, c.accepted
            );
            prop_assert!(resolve(&c.accepted, &ctx).is_some());
        }
    }

    #[test]
    fn globs_never_panic_and_plain_names_match_themselves(pattern in bar_input(), name in tricky_name()) {
        let _ = Glob::new(&pattern).is_match(&name);
        prop_assert!(Glob::new("*").is_match(&name));
        if !Glob::is_glob(&name) {
            prop_assert!(Glob::new(&name).is_match(&name), "{:?} does not match itself", name);
        }
    }

    #[test]
    fn name_validation_never_panics(name in bar_input()) {
        for rules in [NameRules::Posix, NameRules::Windows] {
            if validate_name(&name, rules).is_ok() {
                prop_assert!(!name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..");
            }
        }
    }

    #[test]
    fn unique_names_are_free(
        name in tricky_name(),
        taken in prop::collection::vec(tricky_name(), 0..6),
        is_dir in any::<bool>(),
    ) {
        let mut taken = taken;
        taken.push(name.clone());
        let got = unique_name(&name, is_dir, &|n| taken.iter().any(|t| t == n));
        prop_assert!(!taken.contains(&got), "{:?} is taken", got);
    }
}

// ----- random sessions against the whole bar -----

#[derive(Debug, Clone)]
enum Op {
    Type(String),
    Paste(String),
    Backspace,
    Delete,
    Key(Key),
    Left,
    Right,
    Home,
    End,
    SelectAll,
    Click(usize, bool, bool),
    Sort(SortKey),
    ShowHidden(bool),
    Compose(bool),
}

fn any_key() -> impl Strategy<Value = Key> {
    select(vec![
        Key::Tab,
        Key::Escape,
        Key::Enter,
        Key::Up,
        Key::Down,
        Key::Back,
        Key::Forward,
        Key::ExtendUp,
        Key::ExtendDown,
        Key::SelectAll,
        Key::Delete,
        Key::Copy,
        Key::Cut,
        Key::Paste,
        Key::Undo,
        Key::Redo,
        Key::ParentFolder,
        Key::Open,
        Key::ListEscape,
        Key::DeletePermanent,
        Key::ListUp,
        Key::ListDown,
    ])
}

fn any_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => select(FRAGMENTS).prop_map(|s| Op::Type(s.to_string())),
        1 => bar_input().prop_map(Op::Paste),
        1 => Just(Op::Backspace),
        1 => Just(Op::Delete),
        8 => any_key().prop_map(Op::Key),
        1 => Just(Op::Left),
        1 => Just(Op::Right),
        1 => Just(Op::Home),
        1 => Just(Op::End),
        2 => Just(Op::SelectAll),
        3 => (0usize..12, any::<bool>(), any::<bool>()).prop_map(|(r, c, s)| Op::Click(r, c, s)),
        1 => select(vec![SortKey::Name, SortKey::Size, SortKey::Date]).prop_map(Op::Sort),
        1 => any::<bool>().prop_map(Op::ShowHidden),
        1 => any::<bool>().prop_map(Op::Compose),
    ]
}

fn apply(s: &mut Session, op: &Op) {
    match op {
        Op::Type(t) => s.type_str(t),
        Op::Paste(t) => s.paste(t),
        Op::Backspace => s.backspace(),
        Op::Delete => s.delete(),
        Op::Key(k) => s.key(*k),
        Op::Left => s.left(),
        Op::Right => s.right(),
        Op::Home => s.home(),
        Op::End => s.end(),
        Op::SelectAll => s.select_all(),
        Op::Click(r, c, sh) => s.bar_mut().click_row(*r, *c, *sh),
        Op::Sort(k) => s.bar_mut().sort_by_column(*k),
        Op::ShowHidden(on) => s.bar_mut().set_show_hidden(*on),
        Op::Compose(on) => s.set_composing(*on),
    }
}

/// What must be true after any keystroke.
fn check(s: &Session, fs: &MemFs) -> Result<(), TestCaseError> {
    let b = s.bar();
    let len = b.text().chars().count();
    prop_assert!(b.ghost_chars() <= len, "ghost longer than the text");
    prop_assert!(b.text().starts_with(b.typed()), "typed is not a prefix of the text");
    let sel = s.selection();
    prop_assert!(sel.start() <= len && sel.end() <= len, "text selection outside the text");
    if b.ghost_chars() > 0 {
        prop_assert_eq!((sel.start(), sel.end()), (len - b.ghost_chars(), len), "a ghost must be the selected tail");
    }
    let rows = b.row_count();
    prop_assert!(b.selected_rows().iter().all(|&r| r < rows), "a selected row does not exist");
    prop_assert!(b.selected_row().is_none_or(|r| r < rows), "the cursor row does not exist");
    prop_assert!(b.highlighted_row().is_none_or(|r| r < rows), "the highlighted row does not exist");
    prop_assert_eq!(b.selection_info().map_or(0, |i| i.count), b.selection_len());
    let (cwd, view) = (fs.canonical_dir(b.cwd()), fs.canonical_dir(b.view_dir()));
    prop_assert_eq!(cwd.as_deref(), Some(b.cwd()), "the current folder does not exist");
    prop_assert_eq!(view.as_deref(), Some(b.view_dir()), "the shown folder does not exist");
    for i in 0..rows.min(20) {
        let _ = (b.row(i), b.is_other_match(i), b.is_selected(i));
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn random_sessions_never_panic_and_keep_their_invariants(
        ops in prop::collection::vec(any_op(), 1..60),
        case_insensitive in any::<bool>(),
        start in select(vec!["/home/user", "/home/user/Documents", "/home", "/"]),
    ) {
        let fs = Arc::new(MemFs::sample(case_insensitive, 20));
        let mut bar = Bar::new(fs.clone(), HOME, start, '/');
        bar.set_aliases(seeded_aliases());
        let mut s = Session::new(bar);
        check(&s, &fs)?;
        for op in &ops {
            apply(&mut s, op);
            let _ = s.bar_mut().take_open_request();
            check(&s, &fs)?;
        }
    }
}
