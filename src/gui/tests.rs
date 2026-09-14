//! Interface tests without a visible window (egui_kittest + AccessKit):
//! buttons are found by their accessible label and clicked like a user would.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::{AeternaApp, Done, Screen, VaultDialog, View};
use crate::config::{Config, LanguageSetting, Loaded, Source};
use crate::logging::LogBuffer;
use crate::paths::AppPaths;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn harness(tmp: &Path, configure: impl FnOnce(&mut Config)) -> Harness<'static, AeternaApp> {
    let docs = tmp.join("Docs");
    write(&docs.join("letter.txt"), "Dear archive,");
    write(&docs.join("photos/summer.jpg"), "not a jpeg");

    let mut config = Config {
        language: LanguageSetting::En,
        destination: tmp.join("Vault"),
        sources: vec![Source::new("Docs", &docs, true)],
        ..Config::default()
    };
    config.advanced.save_program_list = false;
    configure(&mut config);

    let paths = AppPaths {
        config_file: tmp.join("home").join("config.toml"),
        log_dir: tmp.join("home").join("logs"),
        portable: false,
    };
    let loaded = Loaded {
        config,
        notice: None,
    };

    let mut harness = Harness::builder()
        .with_size([1000.0, 900.0])
        .with_max_steps(20)
        .build_eframe(move |cc| {
            super::theme::install_fonts(&cc.egui_ctx, &crate::config::Fonts::default());
            super::theme::apply(&cc.egui_ctx, crate::config::Appearance::Dark);
            AeternaApp::new(&cc.egui_ctx, paths, loaded, LogBuffer::default())
        });
    harness.step();
    harness.step();
    harness
}

/// Runs frames until `condition` holds (background work happens on threads).
fn wait_until(
    harness: &mut Harness<'static, AeternaApp>,
    what: &str,
    condition: impl Fn(&AeternaApp) -> bool,
) {
    for _ in 0..1500 {
        harness.step();
        if condition(harness.state()) {
            harness.step();
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn click(harness: &mut Harness<'static, AeternaApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.step();
    harness.step();
}

#[test]
fn folder_tree_backup_and_restore_through_the_interface() {
    let tmp = tempfile::tempdir().unwrap();
    let mut harness = harness(tmp.path(), |_| {});

    // Open the contents tree and leave out the photos folder.
    click(&mut harness, "Choose what to keep from this folder: Docs");
    wait_until(&mut harness, "tree entries", |app| {
        !app.tree.listings.is_empty()
    });
    click(&mut harness, "photos");
    assert_eq!(
        harness.state().config.sources[0].exclude_paths,
        vec!["photos".to_string()]
    );

    // Back up: plan, confirm, run.
    click(&mut harness, "Back up now");
    wait_until(&mut harness, "confirmation", |app| app.pending.is_some());
    click(&mut harness, "Start");
    wait_until(&mut harness, "backup result", |app| {
        matches!(app.screen, Screen::Done(_))
    });
    match &harness.state().screen {
        Screen::Done(done) => match done.as_ref() {
            Done::Backup(Ok(report)) => {
                assert_eq!(report.header.stats.files, 1, "photos were deselected");
                assert!(
                    report
                        .snapshot_dir
                        .join("Docs")
                        .join("letter.txt")
                        .is_file()
                );
            }
            _ => panic!("backup did not succeed"),
        },
        _ => unreachable!(),
    }
    click(&mut harness, "Back to overview");

    // Restore into a separate folder.
    let target = tmp.path().join("Restored");
    {
        let app = harness.state_mut();
        app.view = View::Restore;
        app.restore.to_folder = true;
        app.restore.folder = Some(target.clone());
    }
    let ctx = harness.ctx.clone();
    harness.state_mut().refresh_snapshots(&ctx);
    wait_until(&mut harness, "backup list", |app| {
        app.restore.selected.is_some()
    });
    // "Restore" is both a navigation tab and the action button; the button comes last.
    harness
        .get_all_by_label("Restore")
        .last()
        .expect("restore button")
        .click();
    harness.step();
    wait_until(&mut harness, "restore confirmation or result", |app| {
        app.pending.is_some() || matches!(app.screen, Screen::Done(_))
    });
    if harness.state().pending.is_some() {
        click(&mut harness, "Start");
    }
    wait_until(&mut harness, "restore result", |app| {
        matches!(app.screen, Screen::Done(_))
    });
    assert_eq!(
        std::fs::read_to_string(target.join("Docs").join("letter.txt")).unwrap(),
        "Dear archive,"
    );
    assert!(!target.join("Docs").join("photos").exists());

    // Settings were saved.
    assert!(tmp.path().join("home").join("config.toml").is_file());
}

#[test]
fn set_up_encryption_and_back_up_encrypted() {
    let tmp = tempfile::tempdir().unwrap();
    let mut harness = harness(tmp.path(), |_| {});

    harness
        .get_by_role_and_label(eframe::egui::accesskit::Role::CheckBox, "Encrypt backups")
        .click();
    harness.step();
    harness.step();
    assert!(matches!(
        harness.state().vault.dialog,
        Some(VaultDialog::Create { .. })
    ));

    // Too short first: the dialog explains and stays open.
    if let Some(VaultDialog::Create {
        passphrase, repeat, ..
    }) = &mut harness.state_mut().vault.dialog
    {
        *passphrase = "short".into();
        *repeat = "short".into();
    }
    click(&mut harness, "Set up");
    assert!(matches!(
        harness.state().vault.dialog,
        Some(VaultDialog::Create { error: Some(_), .. })
    ));

    if let Some(VaultDialog::Create {
        passphrase, repeat, ..
    }) = &mut harness.state_mut().vault.dialog
    {
        *passphrase = "quiet archive of many summers".into();
        *repeat = "quiet archive of many summers".into();
    }
    click(&mut harness, "Set up");
    wait_until(&mut harness, "recovery key", |app| {
        matches!(app.vault.dialog, Some(VaultDialog::ShowRecovery { .. }))
    });
    click(&mut harness, "I have kept the recovery key in a safe place");
    click(&mut harness, "Done");
    assert!(harness.state().vault.dialog.is_none());
    assert!(harness.state().config.encryption.enabled);
    assert!(crate::engine::vault::exists(&tmp.path().join("Vault")));

    click(&mut harness, "Back up now");
    wait_until(&mut harness, "confirmation", |app| app.pending.is_some());
    click(&mut harness, "Start");
    wait_until(&mut harness, "backup result", |app| {
        matches!(app.screen, Screen::Done(_))
    });
    match &harness.state().screen {
        Screen::Done(done) => match done.as_ref() {
            Done::Backup(Ok(report)) => {
                assert!(report.header.encrypted);
                assert_eq!(report.header.stats.files, 2);
            }
            Done::Backup(Err(message)) => panic!("backup failed: {message}"),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }

    // No readable file names in the vault.
    let names: Vec<PathBuf> = walkdir::WalkDir::new(tmp.path().join("Vault"))
        .into_iter()
        .flatten()
        .map(|e| e.into_path())
        .collect();
    assert!(
        names
            .iter()
            .all(|p| !p.display().to_string().contains("letter"))
    );
    click(&mut harness, "Back to overview");

    // Forget and lock: the backup is listed, but locked.
    let ctx = harness.ctx.clone();
    {
        let app = harness.state_mut();
        app.set_remembered(false);
        app.lock_vault(&ctx);
        app.view = View::Restore;
        app.restore.to_folder = true;
        app.restore.folder = Some(tmp.path().join("Restored"));
    }
    wait_until(&mut harness, "locked backup", |app| {
        app.selected_snapshot().is_some_and(|s| s.is_locked())
    });

    click(&mut harness, "Unlock…");
    assert!(matches!(
        harness.state().vault.dialog,
        Some(VaultDialog::Unlock { .. })
    ));
    if let Some(VaultDialog::Unlock {
        secret, remember, ..
    }) = &mut harness.state_mut().vault.dialog
    {
        *secret = "wrong words".into();
        *remember = false;
    }
    click(&mut harness, "Unlock");
    assert!(matches!(
        harness.state().vault.dialog,
        Some(VaultDialog::Unlock { error: Some(_), .. })
    ));
    if let Some(VaultDialog::Unlock { secret, .. }) = &mut harness.state_mut().vault.dialog {
        *secret = "quiet archive of many summers".into();
    }
    click(&mut harness, "Unlock");
    wait_until(&mut harness, "unlocked backup", |app| {
        app.vault.dialog.is_none() && app.selected_snapshot().is_some_and(|s| s.header.is_some())
    });

    harness
        .get_all_by_label("Restore")
        .last()
        .expect("restore button")
        .click();
    harness.step();
    wait_until(&mut harness, "restore confirmation or result", |app| {
        app.pending.is_some() || matches!(app.screen, Screen::Done(_))
    });
    if harness.state().pending.is_some() {
        click(&mut harness, "Start");
    }
    wait_until(&mut harness, "restore result", |app| {
        matches!(app.screen, Screen::Done(_))
    });
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("Restored").join("Docs").join("letter.txt"))
            .unwrap(),
        "Dear archive,"
    );
}

/// Renders the README screenshots from a prepared demo configuration.
///
/// ```text
/// $env:AETERNAVAULT_HOME = '<folder with a demo config.toml>'
/// $env:AETERNAVAULT_PROFILE_ROOT = '<fake user profile>'
/// $env:AETERNAVAULT_SCREENSHOTS = 'docs\screenshots'
/// cargo test render_readme_screenshots -- --ignored
/// ```
#[test]
#[ignore = "needs demo data; run manually to refresh docs/screenshots"]
fn render_readme_screenshots() {
    let Some(out) = std::env::var_os("AETERNAVAULT_SCREENSHOTS").map(PathBuf::from) else {
        panic!("set AETERNAVAULT_SCREENSHOTS, AETERNAVAULT_HOME and AETERNAVAULT_PROFILE_ROOT");
    };
    std::fs::create_dir_all(&out).unwrap();
    let paths = AppPaths::resolve();

    let build = |height: f32, configure: &dyn Fn(&mut Config)| {
        let mut loaded = crate::config::load_or_create(&paths);
        configure(&mut loaded.config);
        let appearance = loaded.config.appearance;
        let paths = paths.clone();
        let theme = match appearance {
            crate::config::Appearance::Light => eframe::egui::Theme::Light,
            _ => eframe::egui::Theme::Dark,
        };
        let mut harness = Harness::builder()
            .with_size([1000.0, height])
            .with_max_steps(20)
            .with_theme(theme)
            .wgpu()
            .build_eframe(move |cc| {
                super::theme::install_fonts(&cc.egui_ctx, &crate::config::Fonts::default());
                super::theme::apply(&cc.egui_ctx, appearance);
                let mut app = AeternaApp::new(&cc.egui_ctx, paths, loaded, LogBuffer::default());
                // Never touch the real Task Scheduler while taking pictures.
                app.schedule.applied = Some(app.config.schedule.clone());
                app
            });
        harness.step();
        harness
    };
    let settle = |harness: &mut Harness<'static, AeternaApp>| {
        wait_until(harness, "background loaders", |app| {
            app.snapshots.is_some()
                && !app.apps.status.is_empty()
                && app.sizes.len() >= app.config.sources.len()
        });
        for _ in 0..40 {
            harness.step();
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let save = |harness: &mut Harness<'static, AeternaApp>, name: &str| {
        // Move the pointer out of the way so no hover state is photographed.
        harness.event(eframe::egui::Event::PointerGone);
        for _ in 0..10 {
            harness.step();
        }
        let image = harness.render().expect("rendering works");
        image.save(out.join(format!("{name}.png"))).unwrap();
    };

    // 1. Backup view, full height, with the Desktop contents tree open.
    let mut h = build(1800.0, &|config| {
        config.language = LanguageSetting::En;
        config.appearance = crate::config::Appearance::Dark;
        config.schedule.enabled = true;
    });
    settle(&mut h);
    click(&mut h, "Choose what to keep from this folder: Desktop");
    settle(&mut h);
    save(&mut h, "backup-view");

    // 2. Preview of the next backup.
    let mut h = build(800.0, &|config| config.language = LanguageSetting::En);
    settle(&mut h);
    click(&mut h, "Preview");
    wait_until(&mut h, "preview", |app| {
        matches!(app.screen, Screen::Preview(_))
    });
    settle(&mut h);
    save(&mut h, "preview");

    // 3. Applications.
    let mut h = build(1000.0, &|config| config.language = LanguageSetting::En);
    h.state_mut().view = View::Apps;
    settle(&mut h);
    save(&mut h, "applications");

    // 4. Restore with the choice of parts.
    let mut h = build(1000.0, &|config| config.language = LanguageSetting::En);
    h.state_mut().view = View::Restore;
    settle(&mut h);
    save(&mut h, "restore");

    // 5. Encryption set-up dialog.
    let mut h = build(800.0, &|config| config.language = LanguageSetting::En);
    settle(&mut h);
    h.state_mut().vault.dialog = Some(VaultDialog::Create {
        passphrase: "quiet archive of many summers".into(),
        repeat: "quiet archive of many summers".into(),
        remember: true,
        error: None,
    });
    settle(&mut h);
    save(&mut h, "encryption");

    // 6. German, light appearance.
    let mut h = build(800.0, &|config| {
        config.language = LanguageSetting::De;
        config.appearance = crate::config::Appearance::Light;
    });
    settle(&mut h);
    save(&mut h, "overview-de-light");
}
