// SPDX-License-Identifier: GPL-3.0-or-later
//! The write side of the in-memory filesystem, tested directly (the core builds on it).

use nocrumbs_core::{Fs, FsError};
use nocrumbs_testfs::MemFs;

const H: &str = "/home/user";

fn fs() -> MemFs {
    MemFs::sample(true, 10)
}

fn names(fs: &MemFs, dir: &str) -> Vec<String> {
    fs.entries(dir).expect("dir").iter().map(|e| e.name.clone()).collect()
}

#[test]
fn create_dir_and_file_keep_the_listing_sorted() {
    let fs = fs();
    fs.create_dir(H, "Archive").unwrap();
    fs.create_file(H, "aaa.txt").unwrap();
    fs.create_file(H, "zzz.txt").unwrap();
    let n = names(&fs, H);
    assert_eq!(n[..3], [".config", "Archive", "Documents"], "folders first, alphabetical (hidden ones included)");
    assert!(n.iter().position(|x| x == "aaa.txt") < n.iter().position(|x| x == "notes.md"));
    assert_eq!(n.last().map(String::as_str), Some("zzz.txt"));
    assert_eq!(
        fs.canonical_dir("/home/user/archive").as_deref(),
        Some("/home/user/Archive"),
        "the new folder is a folder"
    );
    assert!(fs.entries("/home/user/Archive").unwrap().is_empty());
}

#[test]
fn creating_reports_collisions_and_missing_parents() {
    let fs = fs();
    assert_eq!(
        fs.create_dir(H, "documents"),
        Err(FsError::AlreadyExists("/home/user/documents".into())),
        "case-insensitive"
    );
    assert_eq!(fs.create_file(H, "NOTES.MD"), Err(FsError::AlreadyExists("/home/user/NOTES.MD".into())));
    assert!(matches!(fs.create_file("/nope", "a"), Err(FsError::NotFound(_))));
    let cs = MemFs::sample(false, 10);
    assert_eq!(cs.create_file(H, "NOTES.MD"), Ok(()), "case-sensitive allows it");
}

#[test]
fn rename_a_file_and_a_folder_with_its_contents() {
    let fs = fs();
    fs.rename(H, "notes.md", "journal.md").unwrap();
    assert!(!names(&fs, H).contains(&"notes.md".to_string()));
    assert!(names(&fs, H).contains(&"journal.md".to_string()));

    fs.rename(H, "Documents", "Papers").unwrap();
    assert_eq!(fs.canonical_dir("/home/user/documents"), None, "the old path is gone");
    assert_eq!(fs.canonical_dir("/home/user/papers/recipes").as_deref(), Some("/home/user/Papers/recipes"));
    assert!(names(&fs, "/home/user/Papers/recipes").contains(&"ramen.md".to_string()));
}

#[test]
fn rename_collisions_and_case_only_changes() {
    let fs = fs();
    assert_eq!(
        fs.rename(H, "notes.md", "SHOPPING-LIST.TXT"),
        Err(FsError::AlreadyExists("/home/user/SHOPPING-LIST.TXT".into()))
    );
    fs.rename(H, "notes.md", "NOTES.md").expect("a change of case is the same entry on a case-insensitive fs");
    assert!(names(&fs, H).contains(&"NOTES.md".to_string()));
    assert!(matches!(fs.rename(H, "ghost", "x"), Err(FsError::NotFound(_))));
}

#[test]
fn copy_a_folder_deeply_and_leave_the_original() {
    let fs = fs();
    fs.copy_entry("/home/user/Documents", "/tmp", "DocsCopy").unwrap();
    assert!(names(&fs, "/tmp/DocsCopy/recipes").contains(&"pancakes.md".to_string()));
    assert!(names(&fs, "/home/user/Documents/recipes").contains(&"pancakes.md".to_string()));
    // The copy is independent of the original.
    fs.create_file("/tmp/DocsCopy", "extra.txt").unwrap();
    assert!(!names(&fs, "/home/user/Documents").contains(&"extra.txt".to_string()));
}

#[test]
fn copy_and_move_refuse_to_go_inside_themselves() {
    let fs = fs();
    assert_eq!(
        fs.copy_entry("/home/user/projects", "/home/user/projects/nocrumbs", "p"),
        Err(FsError::IntoItself("/home/user/projects".into()))
    );
    assert_eq!(
        fs.move_entry("/home/user/projects", "/home/user/projects", "p2"),
        Err(FsError::IntoItself("/home/user/projects".into())),
        "not even into its own folder"
    );
    assert!(fs.entries("/home/user/projects/nocrumbs").is_some(), "nothing was lost");
    assert!(names(&fs, H).contains(&"projects".to_string()));
}

#[test]
fn copy_collisions() {
    let fs = fs();
    assert_eq!(
        fs.copy_entry("/home/user/notes.md", H, "Shopping-List.txt"),
        Err(FsError::AlreadyExists("/home/user/Shopping-List.txt".into()))
    );
    assert!(matches!(fs.copy_entry("/home/user/ghost", "/tmp", "g"), Err(FsError::NotFound(_))));
    assert!(matches!(fs.copy_entry("/home/user/notes.md", "/nope", "g"), Err(FsError::NotFound(_))));
}

#[test]
fn move_a_folder_between_folders() {
    let fs = fs();
    fs.move_entry("/home/user/Documents", "/tmp", "Docs").unwrap();
    assert_eq!(fs.canonical_dir("/home/user/Documents"), None);
    assert!(names(&fs, "/tmp/Docs/recipes").contains(&"ramen.md".to_string()));
    assert!(!names(&fs, H).contains(&"Documents".to_string()));
    fs.move_entry("/tmp/Docs/resume.docx", H, "cv.docx").unwrap();
    assert!(names(&fs, H).contains(&"cv.docx".to_string()));
    assert!(!names(&fs, "/tmp/Docs").contains(&"resume.docx".to_string()));
}

#[test]
fn trash_and_restore_a_folder_with_everything_in_it() {
    let fs = fs();
    let token = fs.trash("/home/user/Documents").unwrap();
    assert_eq!(fs.trash_len(), 1);
    assert_eq!(fs.canonical_dir("/home/user/Documents"), None);
    assert!(!names(&fs, H).contains(&"Documents".to_string()));
    let path = fs.restore(token).unwrap();
    assert_eq!(path, "/home/user/Documents");
    assert_eq!(fs.trash_len(), 0);
    assert!(
        names(&fs, "/home/user/Documents/recipes").contains(&"pancakes.md".to_string()),
        "the whole subtree came back"
    );
    assert!(names(&fs, H).iter().position(|n| n == "Documents") < names(&fs, H).iter().position(|n| n == "Downloads"));
}

#[test]
fn restore_fails_cleanly_when_the_spot_is_taken_or_gone() {
    let fs = fs();
    let t = fs.trash("/home/user/notes.md").unwrap();
    fs.create_file(H, "notes.md").unwrap();
    assert_eq!(fs.restore(t), Err(FsError::AlreadyExists("/home/user/notes.md".into())));
    assert_eq!(fs.trash_len(), 1, "still in the trash");

    let t2 = fs.trash("/home/user/Documents/resume.docx").unwrap();
    fs.trash("/home/user/Documents").unwrap();
    assert!(matches!(fs.restore(t2), Err(FsError::NotFound(_))), "its folder is in the trash too");
}

#[test]
fn delete_forever_removes_without_a_trace() {
    let fs = fs();
    fs.delete_forever("/home/user/Documents").unwrap();
    assert_eq!(fs.canonical_dir("/home/user/Documents/recipes"), None);
    assert_eq!(fs.trash_len(), 0);
    assert!(matches!(fs.delete_forever("/home/user/Documents"), Err(FsError::NotFound(_))));
}

#[test]
fn a_read_only_fs_says_so() {
    #[derive(Debug)]
    struct ReadOnly;
    impl Fs for ReadOnly {
        fn canonical_dir(&self, _: &str) -> Option<String> {
            None
        }
        fn entries(&self, _: &str) -> Option<std::sync::Arc<[nocrumbs_core::Entry]>> {
            None
        }
        fn case_insensitive(&self) -> bool {
            false
        }
    }
    assert_eq!(ReadOnly.create_dir("/", "x"), Err(FsError::Unsupported("create folders")));
    assert_eq!(ReadOnly.trash("/x"), Err(FsError::Unsupported("use the trash")));
    assert_eq!(FsError::Unsupported("use the trash").to_string(), "This filesystem cannot use the trash");
}
