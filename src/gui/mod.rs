//! The graphical interface (egui / eframe).
//!
//! egui was chosen because it is a single, self-contained dependency that
//! renders the whole window itself, so the calm custom look is fully under our
//! control. See docs/ARCHITECTURE.md for iced, Slint and Tauri as alternatives.
//!
//! * `mod.rs` — application state and the frame layout
//! * `actions.rs` — what happens on clicks: tasks, vault, schedule, loaders
//! * `dialogs.rs` — confirmation and encryption dialogs
//! * `views/` — one module per screen

mod actions;
mod background;
mod dialogs;
mod manage;
mod tasks;
#[cfg(test)]
mod tests;
mod theme;
mod views;
mod widgets;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Stroke, Ui};

use crate::config::{Appearance, Config, ConfigNotice, Loaded, Schedule};
use crate::engine::backup::BackupReport;
use crate::engine::crypto::VaultKey;
use crate::engine::plan::{BackupPlan, RestorePlan};
use crate::engine::restore::RestoreReport;
use crate::engine::snapshots::SnapshotInfo;
use crate::engine::vault::VaultHeader;
use crate::error::EngineError;
use crate::i18n::Lang;
use crate::logging::LogBuffer;
use crate::paths::AppPaths;
use crate::platform::{self, apps};
use crate::state::State;
use background::Background;
use tasks::{Job, Task};
use theme::palette;
use widgets::{ButtonKind, NoticeKind};

/// How the window starts.
#[derive(Debug, Clone)]
pub struct StartOptions {
    /// Start in the notification area without showing the window.
    pub hidden: bool,
    /// Name that lets a second start find this instance.
    pub instance_key: String,
    /// Browse the backups at this path instead of the normal window.
    pub viewer: Option<PathBuf>,
}

pub fn run(
    paths: AppPaths,
    loaded: Loaded,
    log: LogBuffer,
    options: StartOptions,
) -> anyhow::Result<()> {
    let compatibility = loaded.config.advanced.compatibility_graphics
        || std::env::var("AETERNAVAULT_RENDERER").is_ok_and(|v| v.eq_ignore_ascii_case("glow"));

    if !compatibility {
        match launch(
            eframe::Renderer::Wgpu,
            paths.clone(),
            loaded.clone(),
            log.clone(),
            options.clone(),
        ) {
            Ok(()) => return Ok(()),
            // Some graphics drivers fail with Direct3D 12; OpenGL usually works there.
            Err(err) => tracing::warn!("Direct3D renderer failed, trying OpenGL: {err}"),
        }
    }
    launch(eframe::Renderer::Glow, paths, loaded, log, options).map_err(|e| anyhow::anyhow!("{e}"))
}

fn launch(
    renderer: eframe::Renderer,
    paths: AppPaths,
    loaded: Loaded,
    log: LogBuffer,
    options: StartOptions,
) -> Result<(), eframe::Error> {
    let lang = Lang::resolve(loaded.config.language);
    let (size, min_size) = window_sizes();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(lang.t().window_title)
        .with_inner_size(size)
        .with_min_inner_size(min_size)
        .with_visible(!options.hidden);
    match eframe::icon_data::from_png_bytes(include_bytes!(
        "../../assets/icon/aeterna-vault-256.png"
    )) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(err) => tracing::warn!("window icon could not be loaded: {err}"),
    }

    let mut native = eframe::NativeOptions {
        viewport,
        centered: true,
        renderer,
        ..Default::default()
    };
    native.wgpu_options.wgpu_setup = wgpu_setup();
    tracing::info!(?renderer, ?size, hidden = options.hidden, "opening window");

    eframe::run_native(
        "AeternaVault",
        native,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx, &loaded.config.fonts);
            theme::apply(&cc.egui_ctx, loaded.config.appearance);
            let mut app = AeternaApp::new(&cc.egui_ctx, paths, loaded, log);
            match &options.viewer {
                Some(path) => app.enter_viewer(&cc.egui_ctx, path),
                None => {
                    manage::clean_open_folder();
                    app.start_background(&cc.egui_ctx, &options);
                }
            }
            Ok(Box::new(app))
        }),
    )
}

/// Start and minimum size in points, limited to the screen's work area so the
/// window never opens larger than the screen (e.g. 1366×768 at 125 % scaling).
fn window_sizes() -> ([f32; 2], [f32; 2]) {
    const WANTED: [f32; 2] = [1000.0, 800.0];
    const MINIMUM: [f32; 2] = [760.0, 560.0];
    let Some((width, height)) = platform::work_area_points() else {
        return (WANTED, MINIMUM);
    };
    // Leave room for the title bar and a little air around the window.
    let (max_w, max_h) = (width * 0.94, height * 0.90);
    let size = [WANTED[0].min(max_w), WANTED[1].min(max_h)];
    let min = [MINIMUM[0].min(size[0]), MINIMUM[1].min(size[1])];
    (size, min)
}

/// Direct3D 12 on the integrated (power-saving) graphics chip. Hybrid laptops
/// otherwise wake the dedicated GPU, which can flicker at start; Vulkan layers
/// of overlay tools are a frequent source of glitches as well. The usual
/// `WGPU_BACKEND` / `WGPU_POWER_PREF` environment variables still override this.
fn wgpu_setup() -> eframe::egui_wgpu::WgpuSetup {
    use eframe::wgpu;
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    if cfg!(windows) && wgpu::Backends::from_env().is_none() {
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
    }
    if wgpu::PowerPreference::from_env().is_none() {
        setup.power_preference = wgpu::PowerPreference::LowPower;
    }
    eframe::egui_wgpu::WgpuSetup::CreateNew(setup)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Backup,
    Restore,
    Backups,
    Apps,
    Settings,
    Activity,
}

pub enum Screen {
    Main,
    Working,
    Preview(Box<views::preview::PreviewState>),
    Browse(Box<views::browse::BrowseState>),
    Done(Box<Done>),
}

// Lives boxed inside `Screen::Done`, so the size difference does not matter.
#[allow(clippy::large_enum_variant)]
pub enum Done {
    Backup(Result<BackupReport, String>),
    Restore(Result<RestoreReport, String>),
    Verify(Result<crate::engine::verify::VerifyReport, String>),
    Delete(Result<crate::engine::manage::DeleteReport, String>),
    Transfer(Result<crate::engine::manage::TransferReport, String>),
    Extract(Result<(crate::engine::manage::ExtractReport, PathBuf), String>),
}

/// Something waiting for the user's confirmation.
pub enum Pending {
    Backup(Box<BackupPlan>),
    Restore(Box<RestorePlan>),
    Delete(Vec<SnapshotInfo>),
    Transfer(Box<SnapshotInfo>, PathBuf),
    Extract {
        snapshot: Box<SnapshotInfo>,
        /// `None`: everything.
        files: Option<Vec<crate::engine::snapshots::StoredFile>>,
        target: PathBuf,
    },
}

/// Selection in the Backups view.
#[derive(Default)]
pub struct ManageUi {
    /// Qualified ids of the ticked backups.
    pub checked: HashSet<String>,
    pub extract_target: Option<PathBuf>,
}

pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

/// Folder path with (bytes, files), or `None` if the folder is missing.
type SizeLoad = (PathBuf, Option<(u64, u64)>);
type SnapshotLoad = (Result<Vec<SnapshotInfo>, EngineError>, Option<u64>);

#[derive(Default)]
pub struct RestoreUi {
    pub selected: Option<String>,
    pub to_folder: bool,
    pub folder: Option<PathBuf>,
    /// Parts of the selected backup that are left out (see `RestoreOptions::skip`).
    pub skip: HashSet<String>,
}

/// What the catalog knows about an application on this computer.
#[derive(Debug, Clone, Default)]
pub struct AppStatus {
    pub detected: bool,
    pub folders: usize,
    pub registry_keys: usize,
    pub bytes: Option<u64>,
}

#[derive(Default)]
pub struct AppsUi {
    pub status: HashMap<String, AppStatus>,
    pub status_job: Option<Job<(String, AppStatus)>>,
    pub installed: Option<Vec<apps::InstalledApp>>,
    pub installed_job: Option<Job<Vec<apps::InstalledApp>>>,
    pub search: String,
    pub show_not_found: bool,
}

#[derive(Default)]
pub struct SettingsUi {
    pub exclude_text: String,
    pub invalid_patterns: Vec<String>,
}

/// Entries of one folder in the "choose contents" tree.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Default)]
pub struct TreeUi {
    /// Sources (by path) whose tree is open.
    pub expanded_sources: HashSet<PathBuf>,
    /// Open folders: `(source path, relative folder)`.
    pub open_dirs: HashSet<(PathBuf, String)>,
    pub listings: HashMap<PathBuf, Vec<TreeEntry>>,
}

/// A step that was waiting for the vault to be unlocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterUnlock {
    Backup { confirm: bool },
    EnableEncryption,
    RefreshSnapshots,
    ChangePassphrase,
    ReplaceRecovery,
    Remember,
}

pub enum VaultDialog {
    Create {
        passphrase: String,
        repeat: String,
        remember: bool,
        options: crate::engine::vault::VaultOptions,
        error: Option<String>,
    },
    /// Enter a recovery key to see whether it still unlocks the backups.
    TestRecovery {
        secret: String,
        result: Option<Result<(), String>>,
    },
    ShowRecovery {
        key: String,
        confirmed: bool,
        /// egui time of the last click on "Copy", for a short confirmation.
        copied_at: Option<f64>,
    },
    Unlock {
        secret: String,
        remember: bool,
        error: Option<String>,
        then: AfterUnlock,
    },
    Change {
        passphrase: String,
        repeat: String,
        error: Option<String>,
    },
}

#[derive(Default)]
pub struct VaultUi {
    /// The vault at the current destination, if one exists.
    pub header: Option<VaultHeader>,
    pub key: Option<VaultKey>,
    pub remembered: bool,
    pub dialog: Option<VaultDialog>,
}

/// A schedule being created or changed in the dialog.
pub struct ScheduleEditor {
    pub schedule: Schedule,
    pub is_new: bool,
}

#[derive(Default)]
pub struct ScheduleUi {
    pub editor: Option<ScheduleEditor>,
}

pub struct AeternaApp {
    pub paths: AppPaths,
    pub config: Config,
    pub state: State,
    pub catalog: apps::Catalog,
    pub lang: Lang,
    pub view: View,
    pub screen: Screen,
    pub task: Option<Task>,
    pub pending: Option<Pending>,
    pub notices: Vec<Notice>,
    pub log: LogBuffer,
    pub computer: String,

    pub sizes: HashMap<PathBuf, Option<(u64, u64)>>,
    sizes_job: Option<Job<SizeLoad>>,
    pub snapshots: Option<Result<Vec<SnapshotInfo>, EngineError>>,
    snapshots_job: Option<Job<SnapshotLoad>>,
    pub destination_free: Option<u64>,
    pub running: HashSet<String>,
    running_checked: Option<Instant>,

    pub restore: RestoreUi,
    pub apps: AppsUi,
    pub settings: SettingsUi,
    pub tree: TreeUi,
    pub vault: VaultUi,
    pub schedule: ScheduleUi,
    pub manage: ManageUi,
    manage_job: Option<Job<Option<Result<crate::engine::manage::DeleteReport, EngineError>>>>,
    pub background: Option<Background>,
    /// Opened by double-click on encrypted backups: only browse and restore
    /// the backups at this folder; the configuration is not changed.
    pub viewer: Option<PathBuf>,
    config_dirty: bool,
    applied_title: Option<Lang>,
}

impl AeternaApp {
    fn new(ctx: &egui::Context, paths: AppPaths, loaded: Loaded, log: LogBuffer) -> Self {
        let lang = Lang::resolve(loaded.config.language);
        let config_dir = paths
            .config_file
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        let state = State::load(&paths.config_file);
        let mut app = Self {
            settings: SettingsUi {
                exclude_text: loaded.config.exclude.join("\n"),
                invalid_patterns: Vec::new(),
            },
            catalog: apps::Catalog::load(&config_dir),
            state,
            paths,
            config: loaded.config,
            lang,
            view: View::Backup,
            screen: Screen::Main,
            task: None,
            pending: None,
            notices: Vec::new(),
            log,
            computer: platform::computer_name(),
            sizes: HashMap::new(),
            sizes_job: None,
            snapshots: None,
            snapshots_job: None,
            destination_free: None,
            running: HashSet::new(),
            running_checked: None,
            restore: RestoreUi::default(),
            apps: AppsUi::default(),
            tree: TreeUi::default(),
            vault: VaultUi::default(),
            schedule: ScheduleUi::default(),
            manage: ManageUi::default(),
            manage_job: None,
            background: None,
            viewer: None,
            config_dirty: false,
            applied_title: Some(lang),
        };

        ctx.set_zoom_factor(app.config.zoom_factor());
        if let Some(notice) = loaded.notice {
            app.config_notice(notice);
        }
        if let Some(run) = app.state.unseen_automatic().cloned() {
            let (ok, text) = app.lang.automatic_result(&run);
            app.notify(
                if ok {
                    NoticeKind::Success
                } else {
                    NoticeKind::Warning
                },
                text,
            );
            app.state = State::update(&app.paths.config_file, |state| {
                state.acknowledged_at = Some(chrono::Utc::now());
            });
        }
        app.refresh_vault();
        app.refresh_sizes(ctx);
        app.refresh_snapshots(ctx);
        app.refresh_app_status(ctx);
        app
    }

    fn config_notice(&mut self, notice: ConfigNotice) {
        let (kind, text) = match notice {
            ConfigNotice::Created => (NoticeKind::Info, self.lang.t().config_created.to_string()),
            ConfigNotice::Invalid { kept_copy, message } => (
                NoticeKind::Warning,
                self.lang
                    .config_invalid(&message, &kept_copy.display().to_string()),
            ),
            ConfigNotice::NotSaved { message } => {
                (NoticeKind::Warning, self.lang.config_not_saved(&message))
            }
        };
        self.notify(kind, text);
    }

    pub fn notify(&mut self, kind: NoticeKind, text: impl Into<String>) {
        let text = text.into();
        self.notices.retain(|n| n.text != text);
        self.notices.push(Notice { kind, text });
        if self.notices.len() > 3 {
            self.notices.remove(0);
        }
    }

    pub fn mark_dirty(&mut self) {
        self.config_dirty = true;
    }

    /// A backup or restore runs, from the window or automatically.
    pub fn is_busy(&self) -> bool {
        self.task.is_some()
            || self
                .background
                .as_ref()
                .is_some_and(|b| b.service.is_running())
    }

    fn save_if_dirty(&mut self) {
        if self.config_dirty {
            self.save_now();
        }
    }

    pub fn save_now(&mut self) {
        self.config_dirty = false;
        if self.viewer.is_some() {
            // Browsing someone's backups never changes this computer's settings.
            return;
        }
        if let Err(err) = self.config.save(&self.paths.config_file) {
            tracing::error!("configuration could not be saved: {err}");
            self.notify(
                NoticeKind::Error,
                self.lang.config_not_saved(&err.to_string()),
            );
        }
        if let Some(background) = &self.background {
            background.service.set_config(&self.config);
        }
    }

    // --- layout ------------------------------------------------------------------

    fn header(&mut self, ui: &mut Ui) {
        let p = *palette(ui);
        let t = self.lang.t();
        let busy = self.is_busy() || !matches!(self.screen, Screen::Main | Screen::Done(_));

        egui::Panel::top("header")
            .show_separator_line(false)
            .frame(Frame::new().fill(p.background).inner_margin(Margin {
                left: 32,
                right: 28,
                top: 18,
                bottom: 0,
            }))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::logo(ui, 48.0);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        widgets::title(ui, "AeternaVault");
                        widgets::slogan(ui, t.slogan);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let dark = ui.visuals().dark_mode;
                        let theme_button = theme_toggle(ui, dark).on_hover_text(if dark {
                            t.switch_to_light
                        } else {
                            t.switch_to_dark
                        });
                        if theme_button.clicked() {
                            self.config.appearance = if dark {
                                Appearance::Light
                            } else {
                                Appearance::Dark
                            };
                            theme::apply(ui.ctx(), self.config.appearance);
                            self.mark_dirty();
                        }
                        if widgets::button(ui, ButtonKind::Quiet, t.switch_language, true).clicked()
                        {
                            self.config.language = match self.lang {
                                Lang::En => crate::config::LanguageSetting::De,
                                Lang::De => crate::config::LanguageSetting::En,
                            };
                            self.lang = Lang::resolve(self.config.language);
                            self.mark_dirty();
                        }
                    });
                });

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 26.0;
                    for (view, label) in [
                        (View::Backup, t.nav_backup),
                        (View::Restore, t.nav_restore),
                        (View::Backups, t.nav_backups),
                        (View::Apps, t.nav_apps),
                        (View::Settings, t.nav_settings),
                        (View::Activity, t.nav_activity),
                    ] {
                        if self.viewer.is_some() && !matches!(view, View::Restore | View::Backups) {
                            continue;
                        }
                        let selected = self.view == view && matches!(self.screen, Screen::Main);
                        if widgets::nav_tab(ui, label, selected, !busy).clicked() {
                            self.view = view;
                            self.screen = Screen::Main;
                            if matches!(view, View::Restore | View::Backups) {
                                self.refresh_snapshots(ui.ctx());
                            }
                        }
                    }
                });
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left() - 32.0, ui.cursor().top()),
                        egui::pos2(rect.right() + 28.0, ui.cursor().top()),
                    ],
                    Stroke::new(1.0, p.border),
                );
            });
    }

    fn footer(&self, ui: &mut Ui) {
        let p = *palette(ui);
        egui::Panel::bottom("footer")
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(p.background)
                    .inner_margin(Margin::symmetric(32, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::secondary_text(
                        ui,
                        format!("AeternaVault {}", env!("CARGO_PKG_VERSION")),
                    );
                    if self.paths.portable {
                        widgets::secondary_text(ui, "·");
                        widgets::secondary_text(ui, self.lang.t().portable_mode);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        widgets::secondary_text(ui, &self.computer);
                    });
                });
            });
    }

    fn handle_dropped_folders(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if dropped.is_empty() || self.is_busy() {
            return;
        }
        for path in dropped.into_iter().filter(|p| p.is_dir()) {
            self.add_folder(ctx, path);
        }
        self.view = View::Backup;
    }
}

/// Appearance switch drawn as a half-filled circle (a common, wordless symbol).
fn theme_toggle(ui: &mut Ui, dark: bool) -> egui::Response {
    let p = *palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 30.0), egui::Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), p.raised);
    }
    let color = if hovered { p.text } else { p.text_secondary };
    let c = rect.center();
    let r = 7.5;
    ui.painter().circle_stroke(c, r, Stroke::new(1.3, color));
    let half: Vec<egui::Pos2> = (0..=24)
        .map(|i| {
            let a = std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 24.0;
            let a = if dark { a + std::f32::consts::PI } else { a };
            c + egui::vec2(a.cos(), a.sin()) * r
        })
        .collect();
    ui.painter()
        .add(egui::Shape::convex_polygon(half, color, Stroke::NONE));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl eframe::App for AeternaApp {
    /// Runs before every frame, and also while the window is hidden.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs(ctx);
        self.poll_manage_job(ctx);
        self.background_tick(ctx);
        self.save_if_dirty();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dropped_folders(&ctx);
        self.refresh_running_processes(&ctx);

        if self.viewer.is_none() && self.applied_title != Some(self.lang) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.lang.t().window_title.to_string(),
            ));
            self.applied_title = Some(self.lang);
        }

        self.header(ui);
        self.footer(ui);

        let p = *palette(ui);
        if matches!(self.screen, Screen::Main) && matches!(self.view, View::Backup | View::Restore)
        {
            egui::Panel::bottom("actions")
                .show_separator_line(false)
                .frame(
                    Frame::new()
                        .fill(p.panel)
                        .inner_margin(Margin::symmetric(32, 14)),
                )
                .show(ui, |ui| {
                    widgets::centered_column(ui, 860.0, |ui| match self.view {
                        View::Restore => views::restore::action_bar(self, ui),
                        _ => views::backup::action_bar(self, ui),
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.background).inner_margin(Margin {
                left: 32,
                right: 32,
                top: 20,
                bottom: 12,
            }))
            .show(ui, |ui| {
                // Notices sit above every screen.
                let mut dismissed = None;
                if !self.notices.is_empty() {
                    widgets::centered_column(ui, 860.0, |ui| {
                        for (i, n) in self.notices.iter().enumerate() {
                            if widgets::notice(ui, &n.kind, &n.text) {
                                dismissed = Some(i);
                            }
                        }
                        ui.add_space(6.0);
                    });
                }
                if let Some(i) = dismissed {
                    self.notices.remove(i);
                }

                match &self.screen {
                    Screen::Preview(_) => views::preview::show(self, ui),
                    Screen::Browse(_) => views::browse::show(self, ui),
                    Screen::Working => views::working::show_working(self, ui),
                    Screen::Done(_) => views::working::show_done(self, ui),
                    Screen::Main => {
                        egui::ScrollArea::vertical()
                            .id_salt(("main", self.view as u8))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                widgets::centered_column(ui, 860.0, |ui| match self.view {
                                    View::Backup => views::backup::show(self, ui),
                                    View::Restore => views::restore::show(self, ui),
                                    View::Backups => views::backups::show(self, ui),
                                    View::Apps => views::apps::show(self, ui),
                                    View::Settings => views::settings::show(self, ui),
                                    View::Activity => views::activity::show(self, ui),
                                });
                            });
                    }
                }
            });

        dialogs::confirmation(self, ui);
        dialogs::vault_dialog(self, ui);
        dialogs::schedule_dialog(self, ui);
        self.save_if_dirty();
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        let p = if visuals.dark_mode {
            theme::DARK
        } else {
            theme::LIGHT
        };
        p.background.to_normalized_gamma_f32()
    }
}
