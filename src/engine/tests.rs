//! End-to-end tests of the engine on temporary folders.

use std::fs;
use std::path::{Path, PathBuf};

use super::plan::{self, ItemKind, Plan, RestoreOptions, RestoreTarget};
use super::{CancelToken, Progress, backup, restore, snapshots};
use crate::config::{BackupMode, Config, ConflictPolicy, Source};
use crate::platform::vss::LiveFiles;

const COMPUTER: &str = "TESTPC";

fn quiet() -> impl FnMut(&Progress) {
    |_| {}
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn setup() -> (tempfile::TempDir, PathBuf, Config) {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("Documents");
    write(&source.join("letter.txt"), "Dear archive,");
    write(&source.join("photos/summer.jpg"), "not really a jpeg");
    write(&source.join("cache/skip.tmp"), "temporary");

    let config = Config {
        destination: tmp.path().join("Vault"),
        sources: vec![Source::new("Documents", &source, true)],
        ..Config::default()
    };
    (tmp, source, config)
}

fn list_all_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect()
}

#[test]
fn preview_does_not_write_anything() {
    let (tmp, _source, config) = setup();
    let before = list_all_files(tmp.path());
    let plan = plan::plan_backup(&config, COMPUTER, &CancelToken::default(), &mut quiet()).unwrap();
    assert_eq!(
        plan.summary.count(ItemKind::New),
        2,
        "*.tmp is excluded by default"
    );
    assert!(
        !config.destination.exists(),
        "preview must not create the destination"
    );
    assert_eq!(before, list_all_files(tmp.path()));
}

#[test]
fn full_then_incremental_then_restore() {
    let (tmp, source, mut config) = setup();
    let cancel = CancelToken::default();

    // First backup: everything is new.
    config.mode = BackupMode::Full;
    let plan1 = plan::plan_backup(&config, COMPUTER, &cancel, &mut quiet()).unwrap();
    let report1 = backup::run_backup(&plan1, &LiveFiles, &cancel, &mut quiet()).unwrap();
    assert_eq!(report1.header.stats.copied_files, 2);
    assert!(
        report1
            .snapshot_dir
            .join("data/Documents/letter.txt")
            .is_file()
    );

    // Change one file, add one, remove one.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write(&source.join("letter.txt"), "Dear archive, a second page.");
    write(&source.join("new.txt"), "fresh");
    fs::remove_file(source.join("photos/summer.jpg")).unwrap();

    config.mode = BackupMode::Incremental;
    let plan2 = plan::plan_backup(&config, COMPUTER, &cancel, &mut quiet()).unwrap();
    assert_eq!(plan2.summary.count(ItemKind::New), 1);
    assert_eq!(plan2.summary.count(ItemKind::Changed), 1);
    assert_eq!(plan2.summary.count(ItemKind::Removed), 1);

    std::thread::sleep(std::time::Duration::from_millis(1100)); // distinct snapshot id
    let report2 = backup::run_backup(&plan2, &LiveFiles, &cancel, &mut quiet()).unwrap();
    assert_eq!(report2.header.stats.copied_files, 2);

    // Third backup without changes: nothing is copied.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let plan3 = plan::plan_backup(&config, COMPUTER, &cancel, &mut quiet()).unwrap();
    assert_eq!(plan3.summary.count(ItemKind::Unchanged), 2);
    let report3 = backup::run_backup(&plan3, &LiveFiles, &cancel, &mut quiet()).unwrap();
    assert_eq!(report3.header.stats.copied_files, 0);
    assert_eq!(
        report3.header.stats.linked_files + report3.header.stats.referenced_files,
        2
    );

    let list = snapshots::list(&config.destination).unwrap();
    assert_eq!(list.len(), 3);

    // Restore the latest backup into a separate folder.
    let latest = snapshots::find(&config.destination, "latest", COMPUTER).unwrap();
    let target = tmp.path().join("Restored");
    let options = RestoreOptions {
        target: RestoreTarget::Folder(target.clone()),
        conflict: ConflictPolicy::ReplaceChanged,
        verify: true,
    };
    let restore_plan = plan::plan_restore(&latest, options, &cancel, &mut quiet()).unwrap();
    assert_eq!(restore_plan.summary.count(ItemKind::New), 2);
    assert!(!target.exists(), "restore preview must not write");

    let report = restore::run_restore(&restore_plan, &cancel, &mut quiet()).unwrap();
    assert_eq!(report.restored_files, 2);
    assert_eq!(
        fs::read_to_string(target.join("Documents/letter.txt")).unwrap(),
        "Dear archive, a second page."
    );

    // A second restore finds everything identical.
    let again = plan::plan_restore(
        &latest,
        RestoreOptions {
            target: RestoreTarget::Folder(target.clone()),
            conflict: ConflictPolicy::ReplaceChanged,
            verify: true,
        },
        &cancel,
        &mut quiet(),
    )
    .unwrap();
    assert_eq!(again.summary.count(ItemKind::Unchanged), 2);
    assert!(again.item_path(&again.items[0]).starts_with(&target));
}

#[test]
fn damaged_backup_is_detected_on_restore() {
    let (tmp, _source, config) = setup();
    let cancel = CancelToken::default();
    let plan = plan::plan_backup(&config, COMPUTER, &cancel, &mut quiet()).unwrap();
    let report = backup::run_backup(&plan, &LiveFiles, &cancel, &mut quiet()).unwrap();

    // Silently corrupt a stored file.
    fs::write(
        report.snapshot_dir.join("data/Documents/letter.txt"),
        "tampered",
    )
    .unwrap();

    let latest = snapshots::find(&config.destination, "latest", COMPUTER).unwrap();
    let target = tmp.path().join("Restored");
    let restore_plan = plan::plan_restore(
        &latest,
        RestoreOptions {
            target: RestoreTarget::Folder(target.clone()),
            conflict: ConflictPolicy::ReplaceChanged,
            verify: true,
        },
        &cancel,
        &mut quiet(),
    )
    .unwrap();
    let result = restore::run_restore(&restore_plan, &cancel, &mut quiet()).unwrap();
    assert_eq!(result.failed, 1);
    assert!(!target.join("Documents/letter.txt").exists());
}

#[test]
fn destination_inside_source_is_skipped() {
    let (_tmp, source, mut config) = setup();
    config.destination = source.join("Backups");
    let cancel = CancelToken::default();
    let plan = plan::plan_backup(&config, COMPUTER, &cancel, &mut quiet()).unwrap();
    backup::run_backup(&plan, &LiveFiles, &cancel, &mut quiet()).unwrap();

    // The second run must not back up the first backup.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let plan2 = plan::plan_backup(&config, COMPUTER, &cancel, &mut quiet()).unwrap();
    assert!(
        plan2
            .items
            .iter()
            .all(|i| !i.rel.to_lowercase().starts_with("backups/"))
    );
}
