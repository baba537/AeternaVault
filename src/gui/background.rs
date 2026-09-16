//! Running in the background: the notification area icon, hiding instead of
//! closing, and the connection to the automatic backup service.
//!
//! All of this is only set up for a real window (not in interface tests).

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, ViewportCommand};

use super::AeternaApp;
use super::tasks::Job;
use super::widgets::NoticeKind;
use crate::automatic::service::{Event, Service};
use crate::automatic::timing;
use crate::platform::tray::{Tray, TrayEvent, TrayLabels};
use crate::platform::{self, autostart, file_association, scheduler};
use crate::state::{AutomaticOutcome, State};

pub enum AppEvent {
    Tray(TrayEvent),
    Service(Event),
}

pub struct Background {
    pub service: Service,
    tray: Option<Tray>,
    events: Receiver<AppEvent>,
    pub hidden: bool,
    quitting: bool,
    tray_visible: bool,
    pushed_key: Option<String>,
    pub autostart: bool,
    legacy_job: Option<Job<std::io::Result<bool>>>,
}

impl AeternaApp {
    /// Starts the service, the tray icon and the clean-up of the 0.2 task.
    pub(super) fn start_background(&mut self, ctx: &egui::Context, options: &super::StartOptions) {
        let (tx, events) = mpsc::channel::<AppEvent>();

        let tray_tx = tx.clone();
        let tray_ctx = ctx.clone();
        let tray = Tray::start(
            &options.instance_key,
            self.tray_labels(),
            Arc::new(move |event| {
                let _ = tray_tx.send(AppEvent::Tray(event));
                tray_ctx.request_repaint();
            }),
        );

        let service_ctx = ctx.clone();
        let service = Service::start(
            self.paths.clone(),
            self.config.clone(),
            Box::new(move |event| {
                let _ = tx.send(AppEvent::Service(event));
                service_ctx.request_repaint();
            }),
        );

        if let Err(err) = autostart::ensure_registered() {
            tracing::warn!("startup entry could not be registered: {err}");
        }
        if self.config.advanced.explorer_menu
            && platform::system_changes_allowed()
            && !platform::context_menu::is_registered()
            && let Err(err) =
                platform::context_menu::set_registered(true, self.lang.t().explorer_menu_label)
        {
            tracing::warn!("Explorer menu entry could not be set up: {err}");
        }
        if self.config.advanced.open_by_double_click
            && platform::system_changes_allowed()
            && file_association::supported()
            && !file_association::is_registered()
            && let Err(err) = file_association::set_registered(true)
        {
            tracing::warn!("double-click on encrypted backups could not be set up: {err}");
        }
        let legacy_job = Some(Job::spawn(ctx, |_, send| {
            send(scheduler::remove_legacy_task())
        }));

        self.background = Some(Background {
            service,
            tray,
            events,
            hidden: options.hidden,
            quitting: false,
            tray_visible: false,
            pushed_key: None,
            autostart: autostart::is_enabled(),
            legacy_job,
        });
        self.update_tray();
        self.take_pending_folders(ctx);
    }

    fn tray_labels(&self) -> TrayLabels {
        let t = self.lang.t();
        let running = self.background.as_ref().and_then(|b| b.service.running());
        let tooltip = match running {
            Some(running) => self
                .lang
                .tray_running(&running.label, running.progress.fraction()),
            None => {
                let now = chrono::Local::now();
                let next = self
                    .config
                    .schedules
                    .iter()
                    .filter(|s| s.enabled)
                    .filter_map(|s| timing::next_occurrence(s, now))
                    .min();
                self.lang.tray_idle(next)
            }
        };
        TrayLabels {
            tooltip,
            open: t.tray_open.to_string(),
            backup_now: t.back_up_now.to_string(),
            quit: t.tray_quit.to_string(),
        }
    }

    /// Whether closing the window should keep AeternaVault running.
    ///
    /// Up to 0.3 this also required a switched-on automatic backup, so the
    /// option seemed to do nothing without one.
    pub fn keeps_running_when_closed(&self) -> bool {
        self.background.is_some() && self.config.background.keep_running
    }

    fn update_tray(&mut self) {
        let visible =
            self.keeps_running_when_closed() || self.background.as_ref().is_some_and(|b| b.hidden);
        let labels = self.tray_labels();
        if let Some(background) = &mut self.background
            && let Some(tray) = &background.tray
        {
            tray.set_labels(labels);
            if background.tray_visible != visible {
                tray.set_visible(visible);
                background.tray_visible = visible;
            }
        }
    }

    pub fn show_window(&mut self, ctx: &egui::Context) {
        if let Some(background) = &mut self.background {
            if background.hidden {
                tracing::info!("window shown again");
            }
            background.hidden = false;
        }
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.state = State::load(&self.paths.config_file);
        self.refresh_snapshots(ctx);
        self.take_pending_folders(ctx);
        self.update_tray();
    }

    /// Folders handed over by "Back up with AeternaVault" in Explorer.
    pub(super) fn take_pending_folders(&mut self, ctx: &egui::Context) {
        let folders = platform::context_menu::pending::take(&self.paths.config_file);
        if folders.is_empty() {
            return;
        }
        for folder in folders.into_iter().filter(|f| f.is_dir()) {
            let known = self.config.has_source_path(&folder);
            let name = folder
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| folder.display().to_string());
            self.add_folder(ctx, folder);
            self.notify(NoticeKind::Success, self.lang.folder_added(&name, known));
        }
        if !self.is_busy() && self.pending.is_none() {
            self.view = super::View::Backup;
            self.screen = super::Screen::Main;
        }
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        if let Some(background) = &mut self.background {
            background.hidden = true;
        }
        self.update_tray();
        if !self.config.background.close_hint_shown {
            let t = self.lang.t();
            if let Some(tray) = self.background.as_ref().and_then(|b| b.tray.as_ref()) {
                tray.balloon(t.close_hint_title, t.close_hint_text, false);
            }
            self.config.background.close_hint_shown = true;
            self.mark_dirty();
        }
    }

    pub fn quit(&mut self, ctx: &egui::Context) {
        if let Some(background) = &mut self.background {
            background.quitting = true;
            background.service.shut_down();
        }
        self.save_now();
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    pub fn set_autostart(&mut self, enabled: bool) {
        match autostart::set_enabled(enabled) {
            Ok(()) => {
                if let Some(background) = &mut self.background {
                    background.autostart = autostart::is_enabled();
                }
                self.record(crate::history::Event::StartWithWindows { on: enabled });
            }
            Err(err) => {
                tracing::warn!("autostart could not be changed: {err}");
                self.notify(
                    NoticeKind::Error,
                    self.lang.autostart_error(&err.to_string()),
                );
            }
        }
    }

    /// Called before every frame and, while the window is hidden, whenever
    /// something asks for a repaint.
    pub(super) fn background_tick(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        let Some(background) = &mut self.background else {
            return;
        };

        background
            .service
            .set_window_busy(self.task.is_some() || self.pending.is_some());
        let fingerprint = self.vault.key.as_ref().map(|k| k.fingerprint());
        if background.pushed_key != fingerprint {
            background.service.set_key(self.vault.key.clone());
            background.pushed_key = fingerprint;
        }

        let mut events = Vec::new();
        while let Ok(event) = background.events.try_recv() {
            events.push(event);
        }
        let legacy = match &background.legacy_job {
            Some(job) => {
                let (mut values, finished) = job.drain();
                if finished {
                    background.legacy_job = None;
                }
                values.pop()
            }
            None => None,
        };
        let quitting = background.quitting;

        if let Some(Ok(true)) = legacy {
            // The 0.2 task used to run without the window; starting with Windows
            // keeps automatic backups going after a restart.
            if self.config.any_schedule_enabled() {
                self.set_autostart(true);
            }
            self.notify(NoticeKind::Info, self.lang.t().legacy_task_removed);
        } else if let Some(Err(err)) = legacy {
            tracing::warn!("the scheduled task of AeternaVault 0.2 could not be removed: {err}");
        }

        for event in events {
            match event {
                AppEvent::Tray(TrayEvent::Open | TrayEvent::Activate) => self.show_window(ctx),
                AppEvent::Tray(TrayEvent::BackupNow) => {
                    if self.background.as_ref().is_some_and(|b| b.hidden) || self.is_busy() {
                        if let Some(background) = &self.background {
                            background.service.run_everything_now();
                        }
                    } else {
                        self.show_window(ctx);
                        self.plan_backup(ctx, true);
                    }
                }
                AppEvent::Tray(TrayEvent::Quit) => self.quit(ctx),
                AppEvent::Service(Event::Started | Event::Progress) => {}
                AppEvent::Service(Event::Finished(run)) => self.automatic_finished(ctx, &run),
            }
        }

        if close_requested && !quitting {
            tracing::info!(
                keep_running = self.keeps_running_when_closed(),
                "window close requested"
            );
            if self.keeps_running_when_closed() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.hide_window(ctx);
            } else if let Some(background) = &mut self.background {
                background.quitting = true;
                background.service.shut_down();
                self.save_now();
            }
        }
        self.update_tray();
    }

    fn automatic_finished(&mut self, ctx: &egui::Context, run: &crate::state::AutomaticRun) {
        self.state = State::load(&self.paths.config_file);
        // The service wrote to the history.
        self.history = crate::history::load(&self.paths.config_file);
        let hidden = self.background.as_ref().is_some_and(|b| b.hidden);
        let (ok, text) = self.lang.automatic_result(run);
        if hidden {
            let worth_telling = !ok && run.outcome != AutomaticOutcome::DestinationUnavailable;
            if worth_telling
                && let Some(tray) = self.background.as_ref().and_then(|b| b.tray.as_ref())
            {
                tray.balloon("AeternaVault", &text, true);
            }
            return;
        }
        self.notify(
            if ok {
                NoticeKind::Success
            } else {
                NoticeKind::Warning
            },
            text,
        );
        self.state = State::update(&self.paths.config_file, |state| {
            state.acknowledged_at = Some(chrono::Utc::now());
        });
        self.refresh_snapshots(ctx);
    }
}
