// SPDX-License-Identifier: GPL-3.0-or-later
//! The settings file: round trips, recovery, version safety, atomic saves, and where it lives.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nocrumbs_config::{ConfigEnv, FileStore, PORTABLE_FILE, locate};
use nocrumbs_core::{Aliases, Frecency, PersistedState, StateStore};

/// A scratch folder that deletes itself.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "nocrumbs-config-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sample_state() -> PersistedState {
    let mut frecency = Frecency::new();
    frecency.record("/home/user/Downloads", 1_000);
    frecency.record("/home/user/Downloads", 2_000);
    frecency.record("/home/user/Documents/recipes", 1_500);
    PersistedState {
        aliases: Aliases::from_pairs([("dl", "/home/user/Downloads"), ("proj", "/home/user/projects")]),
        frecency,
    }
}

#[test]
fn a_missing_file_loads_as_empty_without_creating_anything() {
    let s = Scratch::new();
    let store = FileStore::new(s.path().join("config.toml"));
    let loaded = store.load().unwrap();
    assert_eq!(loaded.state, PersistedState::default());
    assert_eq!(loaded.notice, None);
    assert!(!store.path().exists(), "loading never writes");
}

#[test]
fn save_then_load_round_trips_aliases_and_frecency() {
    let s = Scratch::new();
    let store = FileStore::new(s.path().join("config.toml"));
    store.save(&sample_state()).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.state, sample_state());
    assert_eq!(loaded.notice, None);
}

#[test]
fn the_file_is_readable_toml_with_a_version() {
    let s = Scratch::new();
    let store = FileStore::new(s.path().join("config.toml"));
    store.save(&sample_state()).unwrap();
    let text = fs::read_to_string(store.path()).unwrap();
    assert!(text.starts_with("version = 1"), "{text}");
    assert!(text.contains("[aliases]") && text.contains("dl = \"/home/user/Downloads\""), "{text}");
    assert!(text.contains("[[frecency]]") && text.contains("count = 2"), "{text}");
}

#[test]
fn saving_creates_the_folder_and_leaves_no_temp_file() {
    let s = Scratch::new();
    let store = FileStore::new(s.path().join("deep").join("er").join("config.toml"));
    store.save(&sample_state()).unwrap();
    assert!(store.path().is_file());
    let names: Vec<String> = fs::read_dir(store.path().parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["config.toml"], "no config.toml.tmp left behind");
}

#[test]
fn saving_replaces_the_previous_contents() {
    let s = Scratch::new();
    let store = FileStore::new(s.path().join("config.toml"));
    store.save(&sample_state()).unwrap();
    store.save(&PersistedState::default()).unwrap();
    assert_eq!(store.load().unwrap().state, PersistedState::default());
}

#[test]
fn a_damaged_file_is_set_aside_not_overwritten() {
    let s = Scratch::new();
    let path = s.path().join("config.toml");
    fs::write(&path, "this is [not valid toml = =").unwrap();
    let store = FileStore::new(&path);
    let loaded = store.load().unwrap();
    assert_eq!(loaded.state, PersistedState::default(), "starts fresh");
    let notice = loaded.notice.expect("the user is told");
    assert!(notice.contains("damaged") && notice.contains("config.toml.bad"), "{notice}");
    assert!(!path.exists(), "the damaged file moved out of the way");
    assert_eq!(
        fs::read_to_string(s.path().join("config.toml.bad")).unwrap(),
        "this is [not valid toml = =",
        "its contents are kept"
    );
    // And the next save works normally.
    store.save(&sample_state()).unwrap();
    assert_eq!(store.load().unwrap().state, sample_state());
}

#[test]
fn a_file_of_the_wrong_shape_counts_as_damaged() {
    let s = Scratch::new();
    let path = s.path().join("config.toml");
    fs::write(&path, "version = 1\naliases = 5\n").unwrap();
    let loaded = FileStore::new(&path).load().unwrap();
    assert!(loaded.notice.is_some());
    assert!(s.path().join("config.toml.bad").exists());
}

#[test]
fn a_newer_version_is_refused_and_left_untouched() {
    let s = Scratch::new();
    let path = s.path().join("config.toml");
    let text = "version = 99\nfuture_thing = [1, 2, 3]\n[aliases]\nx = \"/y\"\n";
    fs::write(&path, text).unwrap();
    let err = FileStore::new(&path).load().unwrap_err();
    assert!(err.0.contains("newer NoCrumbs") && err.0.contains("99"), "{err}");
    assert_eq!(fs::read_to_string(&path).unwrap(), text, "not modified");
    assert!(!s.path().join("config.toml.bad").exists(), "and not set aside either");
}

#[test]
fn unknown_keys_are_ignored_and_invalid_alias_names_skipped() {
    let s = Scratch::new();
    let path = s.path().join("config.toml");
    fs::write(&path, "version = 1\nmystery = true\n[aliases]\nok = \"/a\"\n\"bad name\" = \"/b\"\n\"x/y\" = \"/c\"\n")
        .unwrap();
    let loaded = FileStore::new(&path).load().unwrap();
    assert_eq!(loaded.notice, None);
    let names: Vec<&str> = loaded.state.aliases.iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["ok"]);
}

#[test]
fn a_file_without_a_version_is_read_as_version_one() {
    let s = Scratch::new();
    let path = s.path().join("config.toml");
    fs::write(&path, "[aliases]\ndl = \"/d\"\n").unwrap();
    assert_eq!(FileStore::new(&path).load().unwrap().state.aliases.len(), 1);
}

#[test]
fn frecency_beyond_the_cap_is_trimmed_on_load() {
    let s = Scratch::new();
    let path = s.path().join("config.toml");
    let mut text = String::from("version = 1\n");
    for i in 0..Frecency::MAX_ENTRIES + 50 {
        text.push_str(&format!("[[frecency]]\npath = \"/p{i}\"\ncount = 1\nlast = {i}\n"));
    }
    fs::write(&path, text).unwrap();
    assert_eq!(FileStore::new(&path).load().unwrap().state.frecency.len(), Frecency::MAX_ENTRIES);
}

#[test]
fn unusual_alias_targets_survive_a_round_trip() {
    let s = Scratch::new();
    let store = FileStore::new(s.path().join("config.toml"));
    let mut state = PersistedState::default();
    state.aliases.set("odd", "/home/user/My \"Quoted\" Folder/日本語").unwrap();
    store.save(&state).unwrap();
    assert_eq!(store.load().unwrap().state, state);
}

// ----- where the file lives -----

#[test]
fn portable_mode_wins_when_the_file_sits_next_to_the_exe() {
    let exe = Scratch::new();
    let cfg = Scratch::new();
    fs::write(exe.path().join(PORTABLE_FILE), "").unwrap();
    let env = ConfigEnv { exe_dir: Some(exe.path().to_path_buf()), config_dir: Some(cfg.path().to_path_buf()) };
    let loc = locate(&env).unwrap();
    assert!(loc.portable);
    assert_eq!(loc.path, exe.path().join(PORTABLE_FILE));
}

#[test]
fn otherwise_the_users_config_folder_is_used() {
    let exe = Scratch::new();
    let cfg = Scratch::new();
    let env = ConfigEnv { exe_dir: Some(exe.path().to_path_buf()), config_dir: Some(cfg.path().to_path_buf()) };
    let loc = locate(&env).unwrap();
    assert!(!loc.portable);
    let app = if cfg!(windows) { "NoCrumbs" } else { "nocrumbs" };
    assert_eq!(loc.path, cfg.path().join(app).join("config.toml"));
    assert!(!loc.path.exists(), "locating does not create anything");
}

#[test]
fn nothing_to_locate_with_no_environment() {
    assert_eq!(locate(&ConfigEnv::default()), None);
    // A portable file is only a switch when it is next to the exe; with no config folder either,
    // an exe folder without it offers nowhere to store.
    let exe = Scratch::new();
    assert_eq!(locate(&ConfigEnv { exe_dir: Some(exe.path().to_path_buf()), config_dir: None }), None);
}
