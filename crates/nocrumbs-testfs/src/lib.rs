// SPDX-License-Identifier: GPL-3.0-or-later
//! In-memory filesystem for tests and demos. Safe to explore: nothing real is touched.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use nocrumbs_core::{Entry, Fs, fold_cmp};

/// Fixed timestamp for directories (2025-10-09).
const DIR_MTIME: i64 = 1_760_000_000;

/// A finished, read-only in-memory filesystem. Build one with [`MemFsBuilder`] or
/// [`MemFs::sample`].
#[derive(Debug)]
pub struct MemFs {
    case_insensitive: bool,
    dirs: HashMap<String, Arc<[Entry]>>,
    /// Lookup key (lower-cased when case-insensitive) -> canonical path.
    index: HashMap<String, String>,
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
}

impl Fs for MemFs {
    fn canonical_dir(&self, dir: &str) -> Option<String> {
        self.index.get(&self.key(dir)).cloned()
    }

    fn entries(&self, dir: &str) -> Option<Arc<[Entry]>> {
        self.dirs.get(dir).cloned()
    }

    fn case_insensitive(&self) -> bool {
        self.case_insensitive
    }
}

impl MemFs {
    fn key(&self, path: &str) -> String {
        if self.case_insensitive { path.to_lowercase() } else { path.to_string() }
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
            let child = if cur == "/" { format!("/{seg}") } else { format!("{cur}/{seg}") };
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
        MemFs { case_insensitive: ci, dirs, index }
    }
}
