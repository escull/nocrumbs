// SPDX-License-Identifier: GPL-3.0-or-later
//! In-memory filesystem for tests and demos. Safe to explore: nothing real is touched.
//!
//! It implements the whole write side of [`Fs`] (create, rename, copy, move, trash, restore,
//! delete) so the core's file operations and undo can be tested end to end.

mod store;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

use nocrumbs_core::{Entry, Fs, FsError, NameRules, TrashToken, fold_cmp};

pub use store::MemStore;

/// Fixed timestamp for directories (2025-10-09).
const DIR_MTIME: i64 = 1_760_000_000;
/// Created files get increasing timestamps from here, so sort-by-date is deterministic.
const CLOCK_START: i64 = 1_800_000_000;

/// A finished in-memory filesystem. Build one with [`MemFsBuilder`] or [`MemFs::sample`];
/// mutate it through the [`Fs`] write methods.
#[derive(Debug)]
pub struct MemFs {
    case_insensitive: bool,
    state: RwLock<State>,
}

#[derive(Debug, Default)]
struct State {
    dirs: HashMap<String, Arc<[Entry]>>,
    /// Lookup key (lower-cased when case-insensitive) -> canonical path.
    index: HashMap<String, String>,
    trash: HashMap<u64, Trashed>,
    next_token: u64,
    clock: i64,
}

/// Something in the trash: the entry, where it was, and (for a folder) everything inside it,
/// with paths relative to the folder (`""` is the folder itself).
#[derive(Debug)]
struct Trashed {
    dir: String,
    entry: Entry,
    subtree: Vec<(String, Arc<[Entry]>)>,
}

impl MemFs {
    /// A small home directory plus a `/big` folder holding `big_n` generated entries.
    pub fn sample(case_insensitive: bool, big_n: usize) -> Self {
        let mut b = MemFsBuilder::new(case_insensitive);
        let t = |y: i64| 1_700_000_000 + y * 86_400;
        let h = "/home/user";

        b.file(&format!("{h}/Documents/tax-return-2025.pdf"), 284_120, t(10));
        b.file(&format!("{h}/Documents/resume.docx"), 46_080, t(200));
        b.file(&format!("{h}/Documents/quarterly-report.pdf"), 182_340, t(320));
        b.file(&format!("{h}/Documents/recipes/pancakes.md"), 1_240, t(40));
        b.file(&format!("{h}/Documents/recipes/ramen.md"), 980, t(250));
        b.file(&format!("{h}/Downloads/backup-2026-06.tar.gz"), 742_391_808, t(210));
        b.file(&format!("{h}/Downloads/wallpaper-purple.png"), 3_824_055, t(310));
        b.file(&format!("{h}/Downloads/invoice-nov.pdf"), 96_512, t(330));
        b.file(&format!("{h}/Downloads/installer-v3.exe"), 52_428_800, t(335));
        b.file(&format!("{h}/Downloads/lumo-2.0-notes.txt"), 2_048, t(212));
        b.file(&format!("{h}/Pictures/screenshots/shot-001.png"), 1_240_320, t(300));
        b.file(&format!("{h}/Pictures/screenshots/shot-002.png"), 1_188_760, t(307));
        b.file(&format!("{h}/Pictures/holiday-2026 (1).jpg"), 3_821_055, t(260));
        b.file(&format!("{h}/Pictures/holiday-2026 (2).jpg"), 4_188_211, t(260));
        b.dir(&format!("{h}/Music"));
        b.file(&format!("{h}/Videos/demo-cut.mp4"), 2_147_483_648, t(270));
        b.file(&format!("{h}/projects/nocrumbs/index.html"), 21_400, t(330));
        b.file(&format!("{h}/projects/nocrumbs/main.rs"), 8_620, t(330));
        b.file(&format!("{h}/projects/dotfiles/.zshrc"), 2_214, t(320));
        b.file(&format!("{h}/projects/README.md"), 512, t(325));
        b.file(&format!("{h}/.config/starship.toml"), 892, t(310));
        b.file(&format!("{h}/.config/lumo/memory.json"), 342, t(333));
        b.file(&format!("{h}/notes.md"), 1_420, t(334));
        b.file(&format!("{h}/shopping-list.txt"), 320, t(335));
        b.file(&format!("{h}/.zsh_history"), 9_812, t(336));
        b.file("/etc/hosts", 221, t(30));
        b.file("/etc/nginx.conf", 1_024, t(110));
        b.file("/var/log/system.log", 248_112, t(336));
        b.dir("/tmp");
        b.big_dir("/big", big_n);
        b.finish()
    }

    /// How many items are in the trash.
    pub fn trash_len(&self) -> usize {
        self.state.read().expect("lock").trash.len()
    }
}

impl Fs for MemFs {
    fn canonical_dir(&self, dir: &str) -> Option<String> {
        self.state.read().expect("lock").index.get(&self.key(dir)).cloned()
    }

    fn entries(&self, dir: &str) -> Option<Arc<[Entry]>> {
        self.state.read().expect("lock").dirs.get(dir).cloned()
    }

    fn case_insensitive(&self) -> bool {
        self.case_insensitive
    }

    /// A case-insensitive MemFs plays Windows; a case-sensitive one plays Linux.
    fn name_rules(&self) -> NameRules {
        if self.case_insensitive { NameRules::Windows } else { NameRules::Posix }
    }

    fn create_dir(&self, parent: &str, name: &str) -> Result<(), FsError> {
        let mut st = self.state.write().expect("lock");
        let parent = self.dir_of(&st, parent)?;
        self.ensure_free(&st, &parent, name)?;
        let entry = st.new_entry(name, true);
        st.insert_entry(&parent, entry);
        st.register_dir(&join(&parent, name), self.case_insensitive);
        Ok(())
    }

    fn create_file(&self, parent: &str, name: &str) -> Result<(), FsError> {
        let mut st = self.state.write().expect("lock");
        let parent = self.dir_of(&st, parent)?;
        self.ensure_free(&st, &parent, name)?;
        let entry = st.new_entry(name, false);
        st.insert_entry(&parent, entry);
        Ok(())
    }

    fn rename(&self, dir: &str, from: &str, to: &str) -> Result<(), FsError> {
        let mut st = self.state.write().expect("lock");
        let dir = self.dir_of(&st, dir)?;
        let from_entry = self.find(&st, &dir, from).ok_or_else(|| FsError::NotFound(join(&dir, from)))?;
        let from = from_entry.name.clone();
        // Another entry with the target name is a collision; the same entry (a change of case
        // on a case-insensitive filesystem) is fine.
        if let Some(other) = self.find(&st, &dir, to)
            && other.name != from
        {
            return Err(FsError::AlreadyExists(join(&dir, to)));
        }
        let mut entry = st.take_entry(&dir, &from);
        let is_dir = entry.is_dir;
        entry.name = to.to_string();
        st.insert_entry(&dir, entry);
        if is_dir {
            st.rename_subtree(&join(&dir, &from), &join(&dir, to), self.case_insensitive);
        }
        Ok(())
    }

    fn copy_entry(&self, from: &str, to_dir: &str, name: &str) -> Result<(), FsError> {
        let mut st = self.state.write().expect("lock");
        let (src_dir, entry) = self.locate(&st, from)?;
        let to_dir = self.dir_of(&st, to_dir)?;
        self.ensure_free(&st, &to_dir, name)?;
        let src_path = join(&src_dir, &entry.name);
        if entry.is_dir && self.is_inside(&to_dir, &src_path) {
            return Err(FsError::IntoItself(src_path));
        }
        let mut copy = entry.clone();
        copy.name = name.to_string();
        let subtree = if entry.is_dir { st.subtree(&src_path) } else { Vec::new() };
        st.insert_entry(&to_dir, copy);
        st.graft(&join(&to_dir, name), subtree, self.case_insensitive);
        Ok(())
    }

    fn move_entry(&self, from: &str, to_dir: &str, name: &str) -> Result<(), FsError> {
        let mut st = self.state.write().expect("lock");
        let (src_dir, entry) = self.locate(&st, from)?;
        let to_dir = self.dir_of(&st, to_dir)?;
        let src_path = join(&src_dir, &entry.name);
        if entry.is_dir && self.is_inside(&to_dir, &src_path) {
            return Err(FsError::IntoItself(src_path));
        }
        if let Some(other) = self.find(&st, &to_dir, name)
            && !(src_dir == to_dir && other.name == entry.name)
        {
            return Err(FsError::AlreadyExists(join(&to_dir, name)));
        }
        let mut moved = st.take_entry(&src_dir, &entry.name);
        let subtree = if moved.is_dir { st.remove_subtree(&src_path, self.case_insensitive) } else { Vec::new() };
        moved.name = name.to_string();
        st.insert_entry(&to_dir, moved);
        st.graft(&join(&to_dir, name), subtree, self.case_insensitive);
        Ok(())
    }

    fn trash(&self, path: &str) -> Result<TrashToken, FsError> {
        let mut st = self.state.write().expect("lock");
        let (dir, entry) = self.locate(&st, path)?;
        let full = join(&dir, &entry.name);
        let taken = st.take_entry(&dir, &entry.name);
        let subtree = if taken.is_dir { st.remove_subtree(&full, self.case_insensitive) } else { Vec::new() };
        st.next_token += 1;
        let token = st.next_token;
        st.trash.insert(token, Trashed { dir, entry: taken, subtree });
        Ok(TrashToken(token))
    }

    fn restore(&self, token: TrashToken) -> Result<String, FsError> {
        let mut st = self.state.write().expect("lock");
        let Some(t) = st.trash.get(&token.0) else {
            return Err(FsError::NotFound(format!("trash item {}", token.0)));
        };
        let (want_dir, name) = (t.dir.clone(), t.entry.name.clone());
        let dir = self.dir_of(&st, &want_dir)?;
        self.ensure_free(&st, &dir, &name)?;
        let t = st.trash.remove(&token.0).expect("checked above");
        let path = join(&dir, &t.entry.name);
        st.insert_entry(&dir, t.entry);
        st.graft(&path, t.subtree, self.case_insensitive);
        Ok(path)
    }

    fn delete_forever(&self, path: &str) -> Result<(), FsError> {
        let mut st = self.state.write().expect("lock");
        let (dir, entry) = self.locate(&st, path)?;
        let full = join(&dir, &entry.name);
        let taken = st.take_entry(&dir, &entry.name);
        if taken.is_dir {
            st.remove_subtree(&full, self.case_insensitive);
        }
        Ok(())
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir == "/" { format!("/{name}") } else { format!("{dir}/{name}") }
}

impl MemFs {
    fn key(&self, path: &str) -> String {
        if self.case_insensitive { path.to_lowercase() } else { path.to_string() }
    }

    fn dir_of(&self, st: &State, dir: &str) -> Result<String, FsError> {
        st.index.get(&self.key(dir)).cloned().ok_or_else(|| FsError::NotFound(dir.to_string()))
    }

    fn same_name(&self, a: &str, b: &str) -> bool {
        if self.case_insensitive { fold_cmp(a, b).is_eq() } else { a == b }
    }

    fn find<'a>(&self, st: &'a State, dir: &str, name: &str) -> Option<&'a Entry> {
        st.dirs.get(dir)?.iter().find(|e| self.same_name(&e.name, name))
    }

    fn ensure_free(&self, st: &State, dir: &str, name: &str) -> Result<(), FsError> {
        match self.find(st, dir, name) {
            Some(_) => Err(FsError::AlreadyExists(join(dir, name))),
            None => Ok(()),
        }
    }

    /// Split a full path into (canonical parent folder, its entry).
    fn locate(&self, st: &State, path: &str) -> Result<(String, Entry), FsError> {
        let (parent, name) = path.rsplit_once('/').ok_or_else(|| FsError::NotFound(path.to_string()))?;
        let parent = if parent.is_empty() { "/" } else { parent };
        let parent = self.dir_of(st, parent).map_err(|_| FsError::NotFound(path.to_string()))?;
        let entry = self.find(st, &parent, name).cloned().ok_or_else(|| FsError::NotFound(path.to_string()))?;
        Ok((parent, entry))
    }

    /// Is `dir` the folder `ancestor` or somewhere below it?
    fn is_inside(&self, dir: &str, ancestor: &str) -> bool {
        let (d, a) = (self.key(dir), self.key(ancestor));
        d == a || d.starts_with(&format!("{}/", a.trim_end_matches('/')))
    }
}

impl State {
    fn new_entry(&mut self, name: &str, is_dir: bool) -> Entry {
        self.clock = self.clock.max(CLOCK_START) + 1;
        Entry { name: name.to_string(), is_dir, hidden: name.starts_with('.'), size: 0, mtime: self.clock }
    }

    /// Insert keeping the folders-first, by-name order.
    fn insert_entry(&mut self, dir: &str, entry: Entry) {
        let old = self.dirs.get(dir).cloned().unwrap_or_else(|| Arc::from(Vec::new()));
        let mut v: Vec<Entry> = old.to_vec();
        let pos = v
            .iter()
            .position(|e| {
                entry
                    .is_dir
                    .cmp(&e.is_dir)
                    .reverse()
                    .then_with(|| fold_cmp(&entry.name, &e.name))
                    .then_with(|| entry.name.cmp(&e.name))
                    .is_lt()
            })
            .unwrap_or(v.len());
        v.insert(pos, entry);
        self.dirs.insert(dir.to_string(), Arc::from(v));
    }

    fn take_entry(&mut self, dir: &str, name: &str) -> Entry {
        let old = self.dirs.get(dir).cloned().expect("dir exists");
        let mut v: Vec<Entry> = old.to_vec();
        let pos = v.iter().position(|e| e.name == name).expect("entry exists");
        let e = v.remove(pos);
        self.dirs.insert(dir.to_string(), Arc::from(v));
        e
    }

    fn register_dir(&mut self, path: &str, ci: bool) {
        self.dirs.entry(path.to_string()).or_insert_with(|| Arc::from(Vec::new()));
        self.index.insert(if ci { path.to_lowercase() } else { path.to_string() }, path.to_string());
    }

    /// The folder at `path` and every folder below it, as (path relative to `path`, entries).
    fn subtree(&self, path: &str) -> Vec<(String, Arc<[Entry]>)> {
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let mut out: Vec<(String, Arc<[Entry]>)> = self
            .dirs
            .iter()
            .filter_map(|(p, e)| {
                if p == path {
                    Some((String::new(), e.clone()))
                } else {
                    p.strip_prefix(&prefix).map(|rel| (rel.to_string(), e.clone()))
                }
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Remove the folder at `path` and everything below it from the maps, returning it.
    fn remove_subtree(&mut self, path: &str, ci: bool) -> Vec<(String, Arc<[Entry]>)> {
        let sub = self.subtree(path);
        for (rel, _) in &sub {
            let p = if rel.is_empty() { path.to_string() } else { format!("{path}/{rel}") };
            self.dirs.remove(&p);
            self.index.remove(&if ci { p.to_lowercase() } else { p });
        }
        sub
    }

    /// Put a subtree (as produced by `subtree`/`remove_subtree`) at `path`.
    fn graft(&mut self, path: &str, sub: Vec<(String, Arc<[Entry]>)>, ci: bool) {
        for (rel, entries) in sub {
            let p = if rel.is_empty() { path.to_string() } else { format!("{path}/{rel}") };
            self.dirs.insert(p.clone(), entries);
            self.index.insert(if ci { p.to_lowercase() } else { p.clone() }, p);
        }
    }

    fn rename_subtree(&mut self, from: &str, to: &str, ci: bool) {
        let sub = self.remove_subtree(from, ci);
        self.graft(to, sub, ci);
    }
}

/// Builds a [`MemFs`]. Parent directories are created on demand.
#[derive(Debug)]
pub struct MemFsBuilder {
    case_insensitive: bool,
    dirs: BTreeMap<String, Vec<Entry>>,
}

impl MemFsBuilder {
    pub fn new(case_insensitive: bool) -> Self {
        let mut dirs = BTreeMap::new();
        dirs.insert("/".to_string(), Vec::new());
        Self { case_insensitive, dirs }
    }

    /// Create a directory (and its ancestors) if missing.
    pub fn dir(&mut self, path: &str) -> &mut Self {
        let mut cur = String::from("/");
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            let child = join(&cur, seg);
            if !self.dirs.contains_key(&child) {
                self.dirs.insert(child.clone(), Vec::new());
                self.dirs.get_mut(&cur).expect("parent exists").push(Entry {
                    name: seg.to_string(),
                    is_dir: true,
                    hidden: seg.starts_with('.'),
                    size: 0,
                    mtime: DIR_MTIME,
                });
            }
            cur = child;
        }
        self
    }

    /// Create a file, and its parent directories if missing.
    pub fn file(&mut self, path: &str, size: u64, mtime: i64) -> &mut Self {
        let (parent, name) = path.rsplit_once('/').expect("absolute path");
        let parent = if parent.is_empty() { "/" } else { parent };
        self.dir(parent);
        self.dirs.get_mut(parent).expect("created above").push(Entry {
            name: name.to_string(),
            is_dir: false,
            hidden: name.starts_with('.'),
            size,
            mtime,
        });
        self
    }

    /// A folder of `n` deterministic pseudo-random entries: mostly `report-N.pdf`,
    /// `img_N.png` and `notes-N.txt`, with one folder in twenty.
    pub fn big_dir(&mut self, path: &str, n: usize) -> &mut Self {
        self.dir(path);
        let mut x: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            x >> 24
        };
        let entries = self.dirs.get_mut(path).expect("created above");
        entries.reserve(n);
        for i in 0..n {
            let r = next();
            let (name, is_dir) = if i % 20 == 0 {
                (format!("archive-{i:06}"), true)
            } else {
                match i % 3 {
                    0 => (format!("report-{i:06}.pdf"), false),
                    1 => (format!("img_{i:06}.png"), false),
                    _ => (format!("notes-{i:06}.txt"), false),
                }
            };
            entries.push(Entry {
                name,
                is_dir,
                hidden: false,
                size: if is_dir { 0 } else { r % 50_000_000 },
                mtime: 1_600_000_000 + (r % 150_000_000) as i64,
            });
        }
        // Folders in the big directory must exist as directories too.
        let folders: Vec<String> =
            self.dirs[path].iter().filter(|e| e.is_dir).map(|e| format!("{path}/{}", e.name)).collect();
        for f in folders {
            self.dirs.entry(f).or_default();
        }
        self
    }

    pub fn finish(self) -> MemFs {
        let ci = self.case_insensitive;
        let mut dirs = HashMap::with_capacity(self.dirs.len());
        let mut index = HashMap::with_capacity(self.dirs.len());
        for (path, mut entries) in self.dirs {
            entries.sort_by(|a, b| {
                b.is_dir.cmp(&a.is_dir).then_with(|| fold_cmp(&a.name, &b.name)).then_with(|| a.name.cmp(&b.name))
            });
            index.insert(if ci { path.to_lowercase() } else { path.clone() }, path.clone());
            dirs.insert(path, Arc::from(entries));
        }
        MemFs { case_insensitive: ci, state: RwLock::new(State { dirs, index, ..State::default() }) }
    }
}
