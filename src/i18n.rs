//! User-interface texts in English and German.
//!
//! A plain struct with one field per text is used instead of a runtime
//! translation framework: the compiler guarantees that every language defines
//! every text, and there are no files to ship. For many more languages,
//! `fluent` or `rust-i18n` would be the natural next step.
//!
//! Tone: factual, friendly, calm. No exclamation marks, no jargon.

use chrono::{DateTime, Datelike, Local};

use crate::config::LanguageSetting;
use crate::engine::manifest::SnapshotStatus;
use crate::engine::plan::{ItemKind, Note};
use crate::error::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    pub fn resolve(setting: LanguageSetting) -> Self {
        match setting {
            LanguageSetting::En => Lang::En,
            LanguageSetting::De => Lang::De,
            LanguageSetting::Auto => {
                let locale = sys_locale::get_locale().unwrap_or_default().to_lowercase();
                if locale.starts_with("de") {
                    Lang::De
                } else {
                    Lang::En
                }
            }
        }
    }

    pub fn t(self) -> &'static Tr {
        match self {
            Lang::En => &EN,
            Lang::De => &DE,
        }
    }

    // --- numbers, sizes, time ------------------------------------------------

    pub fn count(self, n: u64) -> String {
        let digits = n.to_string();
        let sep = match self {
            Lang::En => ',',
            Lang::De => '.',
        };
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(sep);
            }
            out.push(c);
        }
        out
    }

    pub fn bytes(self, n: u64) -> String {
        const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
        if n < 1024 {
            return format!("{n} B");
        }
        let mut value = n as f64;
        let mut unit = 0;
        while value >= 1024.0 && unit < UNITS.len() - 1 {
            value /= 1024.0;
            unit += 1;
        }
        let text = if value >= 100.0 {
            format!("{value:.0}")
        } else {
            format!("{value:.1}")
        };
        let text = match self {
            Lang::En => text,
            Lang::De => text.replace('.', ","),
        };
        format!("{text} {}", UNITS[unit])
    }

    pub fn duration(self, d: std::time::Duration) -> String {
        let secs = d.as_secs();
        match (secs / 3600, secs / 60 % 60, secs % 60) {
            (0, 0, s) => format!("{s} s"),
            (0, m, s) => format!("{m} min {s} s"),
            (h, m, _) => format!("{h} h {m} min"),
        }
    }

    pub fn datetime(self, time: DateTime<Local>) -> String {
        match self {
            Lang::En => time.format("%-d %b %Y, %H:%M").to_string(),
            Lang::De => time.format("%d.%m.%Y, %H:%M").to_string(),
        }
    }

    /// "2 days ago, 14:32" / "vor 2 Tagen, 14:32".
    pub fn relative_time(self, time: DateTime<Local>) -> String {
        let now = Local::now();
        let minutes = (now - time).num_minutes();
        let clock = time.format("%H:%M");
        let days = (now.date_naive() - time.date_naive()).num_days();
        match self {
            Lang::En => match () {
                _ if minutes < 1 => "just now".to_string(),
                _ if minutes < 60 => plural_en(minutes, "minute") + " ago",
                _ if days == 0 => format!("today, {clock}"),
                _ if days == 1 => format!("yesterday, {clock}"),
                _ if days < 14 => format!("{days} days ago, {clock}"),
                _ => self.datetime(time),
            },
            Lang::De => match () {
                _ if minutes < 1 => "gerade eben".to_string(),
                _ if minutes == 1 => "vor 1 Minute".to_string(),
                _ if minutes < 60 => format!("vor {minutes} Minuten"),
                _ if days == 0 => format!("heute, {clock}"),
                _ if days == 1 => format!("gestern, {clock}"),
                _ if days < 14 => format!("vor {days} Tagen, {clock}"),
                _ => self.datetime(time),
            },
        }
    }

    pub fn weekday_date(self, time: DateTime<Local>) -> String {
        let weekday = time.weekday().num_days_from_monday() as usize;
        let name = match self {
            Lang::En => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][weekday],
            Lang::De => ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"][weekday],
        };
        format!("{name}, {}", self.datetime(time))
    }

    // --- composed sentences --------------------------------------------------

    pub fn files(self, n: u64) -> String {
        match self {
            Lang::En if n == 1 => "1 file".to_string(),
            Lang::En => format!("{} files", self.count(n)),
            Lang::De if n == 1 => "1 Datei".to_string(),
            Lang::De => format!("{} Dateien", self.count(n)),
        }
    }

    pub fn files_of(self, done: u64, total: u64) -> String {
        match self {
            Lang::En => format!("{} of {} files", self.count(done), self.count(total)),
            Lang::De => format!("{} von {} Dateien", self.count(done), self.count(total)),
        }
    }

    pub fn bytes_of(self, done: u64, total: u64) -> String {
        match self {
            Lang::En => format!("{} of {}", self.bytes(done), self.bytes(total)),
            Lang::De => format!("{} von {}", self.bytes(done), self.bytes(total)),
        }
    }

    pub fn files_found(self, n: u64, bytes: u64) -> String {
        match self {
            Lang::En => format!("{} found so far ({})", self.files(n), self.bytes(bytes)),
            Lang::De => format!("{} bisher gefunden ({})", self.files(n), self.bytes(bytes)),
        }
    }

    pub fn free_space(self, bytes: u64) -> String {
        match self {
            Lang::En => format!("{} free", self.bytes(bytes)),
            Lang::De => format!("{} frei", self.bytes(bytes)),
        }
    }

    pub fn confirm_backup(self, files: u64, bytes: u64, destination: &str) -> String {
        match self {
            Lang::En => format!(
                "{} ({}) will be copied to {destination}. Unchanged files are taken over from the previous backup.",
                self.files(files),
                self.bytes(bytes)
            ),
            Lang::De => format!(
                "{} ({}) werden nach {destination} kopiert. Unveränderte Dateien werden aus der vorherigen Sicherung übernommen.",
                self.files(files),
                self.bytes(bytes)
            ),
        }
    }

    pub fn confirm_restore(self, create: u64, replace: u64) -> String {
        match self {
            Lang::En => format!(
                "{} will be created and {} replaced. Nothing is deleted.",
                self.files(create),
                self.files(replace)
            ),
            Lang::De => format!(
                "{} werden neu angelegt und {} ersetzt. Es wird nichts gelöscht.",
                self.files(create),
                self.files(replace)
            ),
        }
    }

    pub fn backup_result(
        self,
        copied: u64,
        reused: u64,
        bytes: u64,
        took: std::time::Duration,
    ) -> String {
        match self {
            Lang::En => format!(
                "{} copied, {} taken over unchanged. {} in total, took {}.",
                self.files(copied),
                self.files(reused),
                self.bytes(bytes),
                self.duration(took)
            ),
            Lang::De => format!(
                "{} kopiert, {} unverändert übernommen. Insgesamt {}, Dauer {}.",
                self.files(copied),
                self.files(reused),
                self.bytes(bytes),
                self.duration(took)
            ),
        }
    }

    pub fn restore_result(
        self,
        restored: u64,
        bytes: u64,
        skipped: u64,
        took: std::time::Duration,
    ) -> String {
        match self {
            Lang::En => format!(
                "{} restored ({}), {} left as they were. Took {}.",
                self.files(restored),
                self.bytes(bytes),
                self.files(skipped),
                self.duration(took)
            ),
            Lang::De => format!(
                "{} wiederhergestellt ({}), {} unverändert gelassen. Dauer {}.",
                self.files(restored),
                self.bytes(bytes),
                self.files(skipped),
                self.duration(took)
            ),
        }
    }

    pub fn failed_files(self, n: u64) -> String {
        match self {
            Lang::En => format!(
                "{} could not be read or written. Details are listed below.",
                self.files(n)
            ),
            Lang::De => format!(
                "{} konnten nicht gelesen oder geschrieben werden. Die Details stehen unten.",
                self.files(n)
            ),
        }
    }

    pub fn exported(self, path: &str) -> String {
        match self {
            Lang::En => format!("Saved to {path}"),
            Lang::De => format!("Gespeichert unter {path}"),
        }
    }

    pub fn apps_count(self, n: usize) -> String {
        match self {
            Lang::En => format!("{} applications", self.count(n as u64)),
            Lang::De => format!("{} Anwendungen", self.count(n as u64)),
        }
    }

    pub fn invalid_patterns(self, patterns: &[String]) -> String {
        match self {
            Lang::En => format!(
                "These patterns are not valid and are ignored: {}",
                patterns.join(", ")
            ),
            Lang::De => format!(
                "Diese Muster sind ungültig und werden ignoriert: {}",
                patterns.join(", ")
            ),
        }
    }

    pub fn config_invalid(self, message: &str, kept_copy: &str) -> String {
        match self {
            Lang::En => format!(
                "The configuration file could not be read ({message}). Default settings are in use; a copy of your file was kept at {kept_copy}."
            ),
            Lang::De => format!(
                "Die Konfigurationsdatei konnte nicht gelesen werden ({message}). Es gelten die Standardeinstellungen; eine Kopie deiner Datei liegt unter {kept_copy}."
            ),
        }
    }

    pub fn config_not_saved(self, message: &str) -> String {
        match self {
            Lang::En => format!("The settings could not be saved: {message}"),
            Lang::De => format!("Die Einstellungen konnten nicht gespeichert werden: {message}"),
        }
    }

    pub fn computer_label(self, computer: &str) -> String {
        match self {
            Lang::En => format!("from {computer}"),
            Lang::De => format!("von {computer}"),
        }
    }

    pub fn list_names(self, names: &[String]) -> String {
        let and = match self {
            Lang::En => "and",
            Lang::De => "und",
        };
        match names {
            [] => String::new(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} {and} {last}", rest.join(", ")),
        }
    }

    pub fn apps_summary(self, count: usize, bytes: Option<u64>) -> String {
        let apps = match (self, count) {
            (Lang::En, 1) => "1 application".to_string(),
            (Lang::En, n) => format!("{n} applications"),
            (Lang::De, 1) => "1 Anwendung".to_string(),
            (Lang::De, n) => format!("{n} Anwendungen"),
        };
        match bytes {
            Some(bytes) => format!("{apps} · {}", self.bytes(bytes)),
            None => apps,
        }
    }

    pub fn more_items(self, n: usize) -> String {
        match self {
            Lang::En => format!("+{n} more"),
            Lang::De => format!("+{n} weitere"),
        }
    }

    pub fn running_warning(self, names: &[String]) -> String {
        let list = self.list_names(names);
        match (self, names.len()) {
            (Lang::En, 1) => {
                format!("{list} is open. Please close it for a complete, consistent copy.")
            }
            (Lang::En, _) => {
                format!("{list} are open. Please close them for a complete, consistent copy.")
            }
            (Lang::De, 1) => format!(
                "{list} ist geöffnet. Für eine vollständige, stimmige Kopie bitte schließen."
            ),
            (Lang::De, _) => format!(
                "{list} sind geöffnet. Für eine vollständige, stimmige Kopie bitte schließen."
            ),
        }
    }

    pub fn close_before_restore(self, names: &[String]) -> String {
        let list = self.list_names(names);
        match self {
            Lang::En => format!(
                "Please close {list} before restoring, otherwise the settings may be overwritten again."
            ),
            Lang::De => format!(
                "Bitte {list} vor dem Wiederherstellen schließen, sonst werden die Einstellungen womöglich wieder überschrieben."
            ),
        }
    }

    pub fn app_contents(self, folders: usize, registry_keys: usize) -> String {
        let mut parts = Vec::new();
        match (self, folders) {
            (_, 0) => {}
            (Lang::En, 1) => parts.push("1 folder".to_string()),
            (Lang::En, n) => parts.push(format!("{n} folders")),
            (Lang::De, 1) => parts.push("1 Ordner".to_string()),
            (Lang::De, n) => parts.push(format!("{n} Ordner")),
        }
        match (self, registry_keys) {
            (_, 0) => {}
            (Lang::En, 1) => parts.push("1 registry key".to_string()),
            (Lang::En, n) => parts.push(format!("{n} registry keys")),
            (Lang::De, 1) => parts.push("1 Registry-Schlüssel".to_string()),
            (Lang::De, n) => parts.push(format!("{n} Registry-Schlüssel")),
        }
        parts.join(" · ")
    }

    pub fn every_hours(self, hours: u8) -> String {
        match self {
            Lang::En => format!("every {hours} hours"),
            Lang::De => format!("alle {hours} Stunden"),
        }
    }

    pub fn next_backup(self, when: DateTime<Local>) -> String {
        match self {
            Lang::En => format!("Next backup: {}", self.upcoming_time(when)),
            Lang::De => format!("Nächste Sicherung: {}", self.upcoming_time(when)),
        }
    }

    fn upcoming_time(self, when: DateTime<Local>) -> String {
        let days = (when.date_naive() - Local::now().date_naive()).num_days();
        let clock = when.format("%H:%M");
        match (self, days) {
            (Lang::En, 0) => format!("today, {clock}"),
            (Lang::En, 1) => format!("tomorrow, {clock}"),
            (Lang::De, 0) => format!("heute, {clock}"),
            (Lang::De, 1) => format!("morgen, {clock}"),
            _ => self.weekday_date(when),
        }
    }

    pub fn automatic_result(self, run: &crate::state::AutomaticRun) -> (bool, String) {
        use crate::state::AutomaticOutcome as O;
        let when = self.relative_time(run.at.with_timezone(&Local));
        let ok = matches!(run.outcome, O::Complete | O::CompleteWithNotes);
        let text = match (self, run.outcome) {
            (Lang::En, O::Complete) => format!(
                "The automatic backup ({when}) completed: {}, {}.",
                self.files(run.files),
                self.bytes(run.bytes)
            ),
            (Lang::De, O::Complete) => format!(
                "Die automatische Sicherung ({when}) ist abgeschlossen: {}, {}.",
                self.files(run.files),
                self.bytes(run.bytes)
            ),
            (Lang::En, O::CompleteWithNotes) => format!(
                "The automatic backup ({when}) completed with notes. Details are in the activity log."
            ),
            (Lang::De, O::CompleteWithNotes) => format!(
                "Die automatische Sicherung ({when}) ist mit Hinweisen abgeschlossen. Details stehen im Protokoll."
            ),
            (Lang::En, O::DestinationUnavailable) => format!(
                "The automatic backup ({when}) was skipped because the destination was not connected."
            ),
            (Lang::De, O::DestinationUnavailable) => format!(
                "Die automatische Sicherung ({when}) wurde übersprungen, weil das Ziel nicht angeschlossen war."
            ),
            (Lang::En, O::NeedsPassphrase) => format!(
                "The automatic backup ({when}) could not run: the passphrase is not remembered on this computer."
            ),
            (Lang::De, O::NeedsPassphrase) => format!(
                "Die automatische Sicherung ({when}) konnte nicht laufen: Die Passphrase ist auf diesem Computer nicht gemerkt."
            ),
            (Lang::En, O::AlreadyRunning) => format!(
                "The automatic backup ({when}) was skipped because another backup was running."
            ),
            (Lang::De, O::AlreadyRunning) => format!(
                "Die automatische Sicherung ({when}) wurde übersprungen, weil schon eine andere Sicherung lief."
            ),
            (Lang::En, O::Failed) => format!(
                "The automatic backup ({when}) did not complete: {}",
                run.message
            ),
            (Lang::De, O::Failed) => format!(
                "Die automatische Sicherung ({when}) wurde nicht abgeschlossen: {}",
                run.message
            ),
        };
        (ok, text)
    }

    pub fn registry_applied(self, n: u64) -> String {
        match (self, n) {
            (Lang::En, 1) => "1 registry key was applied.".to_string(),
            (Lang::En, n) => format!("{n} registry keys were applied."),
            (Lang::De, 1) => "1 Registry-Schlüssel wurde übernommen.".to_string(),
            (Lang::De, n) => format!("{n} Registry-Schlüssel wurden übernommen."),
        }
    }

    pub fn autostart_error(self, message: &str) -> String {
        match self {
            Lang::En => format!("Starting with Windows could not be changed: {message}"),
            Lang::De => format!("Der Start mit Windows konnte nicht geändert werden: {message}"),
        }
    }

    pub fn every_hours_capital(self, hours: u8) -> String {
        match self {
            Lang::En => format!("Every {hours} hours"),
            Lang::De => format!("Alle {hours} Stunden"),
        }
    }

    pub fn running_automatic(self, label: &str) -> String {
        match self {
            Lang::En => format!("Automatic backup running: {label}"),
            Lang::De => format!("Automatische Sicherung läuft: {label}"),
        }
    }

    pub fn schedule_last_run(self, run: &crate::state::AutomaticRun) -> String {
        let when = self.relative_time(run.at.with_timezone(&Local));
        let status = self.outcome_word(run.outcome);
        match self {
            Lang::En => format!("last: {when}, {status}"),
            Lang::De => format!("zuletzt: {when}, {status}"),
        }
    }

    fn outcome_word(self, outcome: crate::state::AutomaticOutcome) -> &'static str {
        use crate::state::AutomaticOutcome as O;
        match (self, outcome) {
            (Lang::En, O::Complete) => "completed",
            (Lang::De, O::Complete) => "abgeschlossen",
            (Lang::En, O::CompleteWithNotes) => "completed with notes",
            (Lang::De, O::CompleteWithNotes) => "mit Hinweisen abgeschlossen",
            (Lang::En, O::DestinationUnavailable) => "skipped, destination not connected",
            (Lang::De, O::DestinationUnavailable) => "übersprungen, Ziel nicht angeschlossen",
            (Lang::En, O::NeedsPassphrase) => "passphrase needed",
            (Lang::De, O::NeedsPassphrase) => "Passphrase benötigt",
            (Lang::En, O::AlreadyRunning) => "skipped",
            (Lang::De, O::AlreadyRunning) => "übersprungen",
            (Lang::En, O::Failed) => "not completed",
            (Lang::De, O::Failed) => "nicht abgeschlossen",
        }
    }

    pub fn tray_running(self, label: &str, fraction: Option<f32>) -> String {
        let percent = fraction
            .map(|f| format!(" – {} %", (f * 100.0).round() as u32))
            .unwrap_or_default();
        match self {
            Lang::En => format!("AeternaVault – backing up{percent}\n{label}"),
            Lang::De => format!("AeternaVault – Sicherung läuft{percent}\n{label}"),
        }
    }

    pub fn tray_idle(self, next: Option<DateTime<Local>>) -> String {
        match (self, next) {
            (Lang::En, Some(next)) => {
                format!("AeternaVault\nNext backup: {}", self.upcoming_time(next))
            }
            (Lang::De, Some(next)) => {
                format!(
                    "AeternaVault\nNächste Sicherung: {}",
                    self.upcoming_time(next)
                )
            }
            (_, None) => "AeternaVault".to_string(),
        }
    }
    /// Content of the text file offered in the recovery key dialog.
    pub fn recovery_file_text(self, key: &str, vault_id: &str, destination: &str) -> String {
        let date = Local::now().format("%Y-%m-%d");
        match self {
            Lang::En => format!(
                "AeternaVault — recovery key\n\
                 ===========================\n\n\
                 Recovery key:  {key}\n\n\
                 Vault:         {vault_id}\n\
                 Destination:   {destination}\n\
                 Created:       {date}\n\n\
                 This key unlocks the encrypted backups if the passphrase is forgotten.\n\
                 Anyone who has it can read the backups. Keep this file (or a printout)\n\
                 in a safe place, separate from the backups.\n\n\
                 Upper/lower case, spaces and dashes do not matter when typing it.\n"
            ),
            Lang::De => format!(
                "AeternaVault — Wiederherstellungsschlüssel\n\
                 ==========================================\n\n\
                 Wiederherstellungsschlüssel:  {key}\n\n\
                 Tresor:                       {vault_id}\n\
                 Ziel:                         {destination}\n\
                 Erstellt:                     {date}\n\n\
                 Dieser Schlüssel entsperrt die verschlüsselten Sicherungen, falls die\n\
                 Passphrase vergessen wurde. Wer ihn hat, kann die Sicherungen lesen.\n\
                 Bewahre diese Datei (oder einen Ausdruck) sicher und getrennt von den\n\
                 Sicherungen auf.\n\n\
                 Groß-/Kleinschreibung, Leerzeichen und Bindestriche spielen beim Eintippen\n\
                 keine Rolle.\n"
            ),
        }
    }

    pub fn kind_label(self, kind: ItemKind, restore: bool) -> &'static str {
        let t = self.t();
        match (kind, restore) {
            (ItemKind::New, false) => t.kind_new,
            (ItemKind::Changed, false) => t.kind_changed,
            (ItemKind::Unchanged, false) => t.kind_unchanged,
            (ItemKind::Removed, _) => t.kind_removed,
            (ItemKind::Skipped, _) => t.kind_skipped,
            (ItemKind::New, true) => t.kind_create,
            (ItemKind::Changed, true) => t.kind_replace,
            (ItemKind::Unchanged, true) => t.kind_identical,
        }
    }

    pub fn note(self, note: &Note) -> String {
        let t = self.t();
        match note {
            Note::Link => t.note_link.to_string(),
            Note::OnlineOnly => t.note_online_only.to_string(),
            Note::InsideDestination => t.note_inside_destination.to_string(),
            Note::SourceMissing => t.note_source_missing.to_string(),
            Note::NoAccess(detail) => format!("{} ({detail})", t.note_no_access),
            Note::KeepExisting => t.note_keep_existing.to_string(),
            Note::TargetNewer => t.note_target_newer.to_string(),
            Note::TargetIsFolder => t.note_target_is_folder.to_string(),
            Note::BackupFileMissing => t.note_backup_file_missing.to_string(),
            Note::UnsafePath => t.note_unsafe_path.to_string(),
        }
    }

    pub fn status(self, status: Option<SnapshotStatus>) -> &'static str {
        let t = self.t();
        match status {
            Some(SnapshotStatus::Complete) => t.status_complete,
            Some(SnapshotStatus::CompleteWithWarnings) => t.status_warnings,
            Some(SnapshotStatus::Cancelled) => t.status_cancelled,
            Some(SnapshotStatus::Failed) => t.status_failed,
            None => t.status_unfinished,
        }
    }

    /// Calm, localized explanation of an engine error.
    pub fn error_message(self, error: &EngineError) -> String {
        use EngineError as E;
        match (self, error) {
            (Lang::En, E::NoSources) => "Please select at least one folder to back up.".into(),
            (Lang::De, E::NoSources) => {
                "Bitte wähle mindestens einen Ordner zum Sichern aus.".into()
            }
            (Lang::En, E::NoDestination) => "Please choose where the backup should be kept.".into(),
            (Lang::De, E::NoDestination) => {
                "Bitte wähle aus, wo die Sicherung aufbewahrt werden soll.".into()
            }
            (Lang::En, E::DestinationUnavailable(p)) => {
                format!(
                    "The destination {} is not reachable at the moment. Is the drive connected?",
                    p.display()
                )
            }
            (Lang::De, E::DestinationUnavailable(p)) => {
                format!(
                    "Das Ziel {} ist zurzeit nicht erreichbar. Ist das Laufwerk angeschlossen?",
                    p.display()
                )
            }
            (Lang::En, E::SourceInsideDestination { source_path, .. }) => format!(
                "The folder {} lies inside the backup destination. Please choose a different destination.",
                source_path.display()
            ),
            (Lang::De, E::SourceInsideDestination { source_path, .. }) => format!(
                "Der Ordner {} liegt innerhalb des Sicherungsziels. Bitte wähle ein anderes Ziel.",
                source_path.display()
            ),
            (Lang::En, E::NotEnoughSpace { needed, available }) => format!(
                "There is not enough free space at the destination: {} needed, {} available.",
                self.bytes(*needed),
                self.bytes(*available)
            ),
            (Lang::De, E::NotEnoughSpace { needed, available }) => format!(
                "Am Ziel ist nicht genug Platz frei: {} benötigt, {} verfügbar.",
                self.bytes(*needed),
                self.bytes(*available)
            ),
            (Lang::En, E::SnapshotNotFound(_)) => "This backup could not be found anymore.".into(),
            (Lang::De, E::SnapshotNotFound(_)) => {
                "Diese Sicherung wurde nicht mehr gefunden.".into()
            }
            (Lang::En, E::Manifest { path, .. }) => format!(
                "The backup index {} could not be read. The backup may be damaged.",
                path.display()
            ),
            (Lang::De, E::Manifest { path, .. }) => format!(
                "Der Sicherungsindex {} konnte nicht gelesen werden. Die Sicherung ist möglicherweise beschädigt.",
                path.display()
            ),
            (Lang::En, E::Cancelled) => "The operation was cancelled.".into(),
            (Lang::De, E::Cancelled) => "Der Vorgang wurde abgebrochen.".into(),
            (Lang::En, E::Locked) => "The encrypted backups are locked. Please enter the passphrase.".into(),
            (Lang::De, E::Locked) => {
                "Die verschlüsselten Sicherungen sind gesperrt. Bitte die Passphrase eingeben.".into()
            }
            (Lang::En, E::EncryptionNotSetUp) => {
                "Encryption is turned on, but no encrypted vault exists at this destination yet. Please set it up in the settings.".into()
            }
            (Lang::De, E::EncryptionNotSetUp) => {
                "Die Verschlüsselung ist eingeschaltet, am Ziel gibt es aber noch keinen verschlüsselten Tresor. Bitte in den Einstellungen einrichten.".into()
            }
            (Lang::En, E::AlreadyRunning) => {
                "Another backup is running for this destination at the moment. Please try again when it has finished.".into()
            }
            (Lang::De, E::AlreadyRunning) => {
                "Für dieses Ziel läuft gerade schon eine Sicherung. Bitte versuche es erneut, wenn sie fertig ist.".into()
            }
            (Lang::En, E::Crypto(crate::engine::crypto::CryptoError::WrongKey)) => {
                "The passphrase or recovery key is not correct.".into()
            }
            (Lang::De, E::Crypto(crate::engine::crypto::CryptoError::WrongKey)) => {
                "Die Passphrase oder der Wiederherstellungsschlüssel ist nicht korrekt.".into()
            }
            (Lang::En, E::Crypto(err)) => format!("The encrypted data could not be processed: {err}."),
            (Lang::De, E::Crypto(err)) => {
                format!("Die verschlüsselten Daten konnten nicht verarbeitet werden: {err}.")
            }
            (Lang::En, E::Io { context, source }) => {
                format!("A file operation did not succeed: {context} ({source}).")
            }
            (Lang::De, E::Io { context, source }) => {
                format!("Ein Dateivorgang ist nicht gelungen: {context} ({source}).")
            }
        }
    }
}

fn plural_en(n: i64, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

pub struct Tr {
    pub window_title: &'static str,
    pub slogan: &'static str,

    pub nav_backup: &'static str,
    pub nav_restore: &'static str,
    pub nav_apps: &'static str,
    pub nav_settings: &'static str,
    pub nav_activity: &'static str,
    pub switch_to_dark: &'static str,
    pub switch_to_light: &'static str,
    pub switch_language: &'static str,

    pub sources_title: &'static str,
    pub sources_empty: &'static str,
    pub add_folder: &'static str,
    pub remove_source: &'static str,
    pub source_missing: &'static str,
    pub calculating: &'static str,
    pub drop_hint: &'static str,
    pub destination_title: &'static str,
    pub choose: &'static str,
    pub destination_not_set: &'static str,
    pub destination_unreachable: &'static str,
    pub mode_title: &'static str,
    pub mode_incremental: &'static str,
    pub mode_incremental_hint: &'static str,
    pub mode_full: &'static str,
    pub mode_full_hint: &'static str,
    pub last_backup_title: &'static str,
    pub last_backup_none: &'static str,
    pub preview: &'static str,
    pub back_up_now: &'static str,
    pub restore_ellipsis: &'static str,

    pub status_complete: &'static str,
    pub status_warnings: &'static str,
    pub status_cancelled: &'static str,
    pub status_failed: &'static str,
    pub status_unfinished: &'static str,

    pub restore_choose_title: &'static str,
    pub restore_selected: &'static str,
    pub restore_none: &'static str,
    pub refresh: &'static str,
    pub restore_target_title: &'static str,
    pub restore_original: &'static str,
    pub restore_folder: &'static str,
    pub advanced: &'static str,
    pub conflict_title: &'static str,
    pub conflict_replace: &'static str,
    pub conflict_keep: &'static str,
    pub conflict_newer: &'static str,
    pub verify_checksums: &'static str,
    pub restore: &'static str,
    pub choose_folder_first: &'static str,
    pub loading: &'static str,

    pub preview_backup_title: &'static str,
    pub preview_restore_title: &'static str,
    pub preview_read_only: &'static str,
    pub full_mode_note: &'static str,
    pub removed_note: &'static str,
    pub filter_all: &'static str,
    pub filter_hint: &'static str,
    pub col_status: &'static str,
    pub col_source: &'static str,
    pub col_path: &'static str,
    pub col_size: &'static str,
    pub export_text: &'static str,
    pub export_csv: &'static str,
    pub back: &'static str,
    pub start_backup: &'static str,
    pub start_restore: &'static str,
    pub nothing_for_filter: &'static str,

    pub kind_new: &'static str,
    pub kind_changed: &'static str,
    pub kind_unchanged: &'static str,
    pub kind_removed: &'static str,
    pub kind_skipped: &'static str,
    pub kind_create: &'static str,
    pub kind_replace: &'static str,
    pub kind_identical: &'static str,

    pub note_link: &'static str,
    pub note_online_only: &'static str,
    pub note_inside_destination: &'static str,
    pub note_source_missing: &'static str,
    pub note_no_access: &'static str,
    pub note_keep_existing: &'static str,
    pub note_target_newer: &'static str,
    pub note_target_is_folder: &'static str,
    pub note_backup_file_missing: &'static str,
    pub note_unsafe_path: &'static str,

    pub confirm_backup_title: &'static str,
    pub confirm_restore_title: &'static str,
    pub confirm_overwrite: &'static str,
    pub start: &'static str,
    pub show_details: &'static str,
    pub cancel: &'static str,

    pub working_scan_backup: &'static str,
    pub working_scan_restore: &'static str,
    pub working_backup: &'static str,
    pub working_restore: &'static str,
    pub working_finishing: &'static str,
    pub stopping: &'static str,

    pub done_backup: &'static str,
    pub done_backup_notes: &'static str,
    pub done_backup_cancelled: &'static str,
    pub done_restore: &'static str,
    pub done_restore_notes: &'static str,
    pub done_restore_cancelled: &'static str,
    pub done_failed: &'static str,
    pub notes_title: &'static str,
    pub to_overview: &'static str,
    pub open_backup_folder: &'static str,

    pub apps_installed_title: &'static str,
    pub apps_installed_hint: &'static str,
    pub search: &'static str,
    pub open_install_folder: &'static str,

    pub settings_general: &'static str,
    pub language: &'static str,
    pub language_auto: &'static str,
    pub appearance: &'static str,
    pub appearance_system: &'static str,
    pub appearance_dark: &'static str,
    pub appearance_light: &'static str,
    pub exclusions_title: &'static str,
    pub exclusions_hint: &'static str,
    pub apply: &'static str,
    pub reset_defaults: &'static str,
    pub adv_skip_online: &'static str,
    pub adv_hardlinks: &'static str,
    pub adv_confirm: &'static str,
    pub config_title: &'static str,
    pub config_hint: &'static str,
    pub open_in_editor: &'static str,
    pub reload: &'static str,
    pub open_log_folder: &'static str,
    pub portable_mode: &'static str,
    pub config_reloaded: &'static str,
    pub config_created: &'static str,

    pub activity_empty: &'static str,
    pub unexpected_problem: &'static str,
    pub csv_columns: [&'static str; 5],

    // --- 0.2: application settings -------------------------------------------
    pub apps_card_title: &'static str,
    pub apps_card_empty: &'static str,
    pub choose_apps: &'static str,
    pub apps_hint: &'static str,
    pub select_all_found: &'static str,
    pub show_not_found: &'static str,
    pub not_found: &'static str,
    pub app_open: &'static str,
    pub no_matches: &'static str,
    pub categories: [&'static str; 10],

    // --- 0.2: folder tree -----------------------------------------------------
    pub select_all: &'static str,
    pub select_none: &'static str,
    pub partial_selection: &'static str,
    pub choose_contents: &'static str,
    pub folder_empty: &'static str,

    // --- 0.2: encryption -------------------------------------------------------
    pub encrypt_backups: &'static str,
    pub encrypt_hint_on: &'static str,
    pub encrypt_hint_off: &'static str,
    pub encryption_disabled_note: &'static str,
    pub enc_create_title: &'static str,
    pub enc_create_hint: &'static str,
    pub passphrase: &'static str,
    pub passphrase_repeat: &'static str,
    pub strength: [&'static str; 5],
    pub passphrases_differ: &'static str,
    pub passphrase_too_weak: &'static str,
    pub remember_on_computer: &'static str,
    pub enc_warning: &'static str,
    pub set_up: &'static str,
    pub recovery_title: &'static str,
    pub recovery_hint: &'static str,
    pub copy: &'static str,
    pub recovery_confirm: &'static str,
    pub done: &'static str,
    pub unlock_title: &'static str,
    pub unlock_hint: &'static str,
    pub unlock: &'static str,
    pub wrong_passphrase: &'static str,
    pub change_title: &'static str,
    pub change_hint: &'static str,
    pub new_passphrase: &'static str,
    pub save: &'static str,
    pub passphrase_changed: &'static str,
    pub enc_settings_title: &'static str,
    pub enc_status_on: &'static str,
    pub enc_status_off: &'static str,
    pub change_passphrase: &'static str,
    pub lock_now: &'static str,
    pub encrypted_label: &'static str,
    pub locked_backup: &'static str,

    // --- 0.2: automatic backups ------------------------------------------------
    pub schedule_title: &'static str,
    pub schedule_hint: &'static str,
    pub freq_daily: &'static str,
    pub freq_weekly: &'static str,
    pub freq_hourly: &'static str,
    pub freq_at_start: &'static str,
    pub at_time: &'static str,
    pub on_day: &'static str,
    pub weekdays: [&'static str; 7],
    pub catch_up: &'static str,
    pub only_ac: &'static str,
    pub at_start_hint: &'static str,
    pub schedule_needs_key: &'static str,
    pub remember_now: &'static str,

    // --- 0.2: restore ------------------------------------------------------------
    pub restore_what_title: &'static str,
    pub restore_folders: &'static str,
    pub restore_registry_note: &'static str,
    pub open_program_list: &'static str,
    pub adv_program_list: &'static str,
    pub selected_backup_locked: &'static str,

    // --- 0.3: dialogs, destination, display --------------------------------------
    pub show_secret: &'static str,
    pub hide_secret: &'static str,
    pub copied: &'static str,
    pub save_as_file: &'static str,
    pub recovery_file_name: &'static str,
    pub recovery_file_saved: &'static str,
    pub destination_app_folder: &'static str,
    pub interface_size: &'static str,
    pub adv_compatibility_graphics: &'static str,
    pub adv_compatibility_hint: &'static str,

    // --- 0.3: automatic backups run by AeternaVault --------------------------------
    pub weekdays_every: [&'static str; 7],
    pub scope_everything: &'static str,
    pub scope_all_folders: &'static str,
    pub scope_nothing: &'static str,
    pub schedules_empty: &'static str,
    pub add_schedule: &'static str,
    pub edit: &'static str,
    pub run_now: &'static str,
    pub remove_schedule: &'static str,
    pub stop: &'static str,
    pub start_with_windows: &'static str,
    pub keep_running: &'static str,
    pub schedule_only_while_running: &'static str,
    pub schedule_new_title: &'static str,
    pub schedule_edit_title: &'static str,
    pub schedule_when: &'static str,
    pub schedule_what: &'static str,
    pub schedule_name: &'static str,
    pub scope_radio_everything: &'static str,
    pub scope_radio_only: &'static str,
    pub starting_at: &'static str,
    pub tray_open: &'static str,
    pub tray_quit: &'static str,
    pub close_hint_title: &'static str,
    pub close_hint_text: &'static str,
    pub legacy_task_removed: &'static str,
}

pub static EN: Tr = Tr {
    window_title: "AeternaVault — Your data, kept for eternity",
    slogan: "Your data, kept for eternity.",

    nav_backup: "Backup",
    nav_restore: "Restore",
    nav_apps: "Applications",
    nav_settings: "Settings",
    nav_activity: "Activity",
    switch_to_dark: "Switch to dark appearance",
    switch_to_light: "Switch to light appearance",
    switch_language: "Deutsch",

    sources_title: "What is kept safe",
    sources_empty: "No folders selected yet. Add the folders you would like to keep safe.",
    add_folder: "Add folder…",
    remove_source: "Remove from the list (nothing is deleted)",
    source_missing: "folder not found",
    calculating: "calculating…",
    drop_hint: "Folders can also be dropped onto this window.",
    destination_title: "Where it is kept",
    choose: "Choose…",
    destination_not_set: "No destination chosen yet",
    destination_unreachable: "Not reachable at the moment. Is the drive connected?",
    mode_title: "Kind of backup",
    mode_incremental: "Incremental",
    mode_incremental_hint: "Only new and changed files are copied. Every backup is still complete.",
    mode_full: "Full",
    mode_full_hint: "Every file is copied again. Takes longer and needs more space.",
    last_backup_title: "Last backup",
    last_backup_none: "No backup yet.",
    preview: "Preview",
    back_up_now: "Back up now",
    restore_ellipsis: "Restore…",

    status_complete: "completed",
    status_warnings: "completed with notes",
    status_cancelled: "cancelled",
    status_failed: "not completed",
    status_unfinished: "unfinished",

    restore_choose_title: "Choose a backup",
    restore_selected: "Selected backup",
    restore_none: "No backups were found at the destination.",
    refresh: "Refresh",
    restore_target_title: "Restore to",
    restore_original: "The original locations",
    restore_folder: "Another folder",
    advanced: "Advanced",
    conflict_title: "If a file already exists",
    conflict_replace: "Replace it if it differs",
    conflict_keep: "Keep it",
    conflict_newer: "Keep it if it is newer",
    verify_checksums: "Verify checksums while restoring",
    restore: "Restore",
    choose_folder_first: "Please choose a folder first.",
    loading: "Loading…",

    preview_backup_title: "Preview of the backup",
    preview_restore_title: "Preview of the restore",
    preview_read_only: "This is a preview. Nothing has been changed.",
    full_mode_note: "Full backup: all files will be copied again.",
    removed_note: "Files that are no longer present stay in earlier backups. Nothing is deleted.",
    filter_all: "All",
    filter_hint: "Filter by path…",
    col_status: "Status",
    col_source: "Source",
    col_path: "Path",
    col_size: "Size",
    export_text: "Export as text…",
    export_csv: "Export as CSV…",
    back: "Back",
    start_backup: "Start backup",
    start_restore: "Start restore",
    nothing_for_filter: "Nothing to show for this filter.",

    kind_new: "New",
    kind_changed: "Changed",
    kind_unchanged: "Unchanged",
    kind_removed: "No longer present",
    kind_skipped: "Skipped",
    kind_create: "Create",
    kind_replace: "Replace",
    kind_identical: "Already identical",

    note_link: "link or junction, not followed",
    note_online_only: "online-only file, not stored on this computer",
    note_inside_destination: "this is the backup destination",
    note_source_missing: "folder not found",
    note_no_access: "no access",
    note_keep_existing: "the existing file is kept",
    note_target_newer: "the existing file is newer",
    note_target_is_folder: "a folder with this name exists",
    note_backup_file_missing: "missing in the backup",
    note_unsafe_path: "invalid path in the backup index",

    confirm_backup_title: "Start the backup?",
    confirm_restore_title: "Start the restore?",
    confirm_overwrite: "Existing files will be overwritten with the version from the backup.",
    start: "Start",
    show_details: "Show details",
    cancel: "Cancel",

    working_scan_backup: "Looking through your files",
    working_scan_restore: "Comparing with the backup",
    working_backup: "Backing up",
    working_restore: "Restoring",
    working_finishing: "Finishing",
    stopping: "Stopping…",

    done_backup: "The backup is complete.",
    done_backup_notes: "The backup is complete, with a few notes.",
    done_backup_cancelled: "The backup was cancelled. Files copied so far are kept.",
    done_restore: "The restore is complete.",
    done_restore_notes: "The restore is complete, with a few notes.",
    done_restore_cancelled: "The restore was cancelled.",
    done_failed: "The operation could not be completed.",
    notes_title: "Notes",
    to_overview: "Back to overview",
    open_backup_folder: "Open backup folder",

    apps_installed_title: "Installed applications",
    apps_installed_hint: "A list of these programs is saved with every backup, as a checklist for reinstalling on a new computer.",
    search: "Search…",
    open_install_folder: "Open install folder",

    settings_general: "General",
    language: "Language",
    language_auto: "Automatic",
    appearance: "Appearance",
    appearance_system: "Like Windows",
    appearance_dark: "Dark",
    appearance_light: "Light",
    exclusions_title: "Exclusions",
    exclusions_hint: "One pattern per line, for example *.tmp or node_modules. Applies to file and folder names in all sources.",
    apply: "Apply",
    reset_defaults: "Restore defaults",
    adv_skip_online: "Skip online-only cloud files (for example OneDrive)",
    adv_hardlinks: "Link unchanged files instead of storing them again",
    adv_confirm: "Ask before a backup or restore starts",
    config_title: "Configuration file",
    config_hint: "All settings are stored in this file and can also be edited by hand.",
    open_in_editor: "Open in editor",
    reload: "Reload",
    open_log_folder: "Open log folder",
    portable_mode: "Portable mode",
    config_reloaded: "The configuration was reloaded.",
    config_created: "Welcome. A configuration with sensible defaults was created. Please check the folders and the destination.",

    activity_empty: "Nothing has happened yet.",
    unexpected_problem: "An unexpected problem occurred. Details are in the activity log.",
    csv_columns: ["status", "source", "path", "size_bytes", "note"],

    apps_card_title: "Application settings",
    apps_card_empty: "No applications chosen yet.",
    choose_apps: "Choose applications…",
    apps_hint: "Choose which settings are kept. On a new computer they are put back in the right place, even under a different user name.",
    select_all_found: "Select all found",
    show_not_found: "Also show applications that are not installed",
    not_found: "not found on this computer",
    app_open: "open",
    no_matches: "Nothing matches the search.",
    categories: [
        "Browsers",
        "E-mail",
        "Communication",
        "Office and notes",
        "Development",
        "Photo, video and music",
        "Games",
        "Passwords and keys",
        "Utilities",
        "Windows settings",
    ],

    select_all: "Select all",
    select_none: "Select none",
    partial_selection: "Only the checked items are kept.",
    choose_contents: "Choose what to keep from this folder",
    folder_empty: "This folder is empty.",

    encrypt_backups: "Encrypt backups",
    encrypt_hint_on: "Protected with your passphrase. File names and contents cannot be read without it.",
    encrypt_hint_off: "Recommended for cloud folders and sensitive data.",
    encryption_disabled_note: "New backups are no longer encrypted. Existing encrypted backups stay protected.",
    enc_create_title: "Set up encryption",
    enc_create_hint: "Choose a passphrase. A few unrelated words are easy to remember and hard to guess.",
    passphrase: "Passphrase",
    passphrase_repeat: "Repeat passphrase",
    strength: ["Too short", "Weak", "Fair", "Good", "Strong"],
    passphrases_differ: "The two passphrases are different.",
    passphrase_too_weak: "Please use at least 10 characters.",
    remember_on_computer: "Remember on this computer (needed for automatic backups)",
    enc_warning: "Without the passphrase or the recovery key, nobody can restore these backups — not even you.",
    set_up: "Set up",
    recovery_title: "Your recovery key",
    recovery_hint: "If you ever forget the passphrase, this key unlocks your backups. Write it down or print it and keep it in a safe place. It is shown only now.",
    copy: "Copy",
    recovery_confirm: "I have kept the recovery key in a safe place",
    done: "Done",
    unlock_title: "Unlock encrypted backups",
    unlock_hint: "Enter the passphrase or the recovery key.",
    unlock: "Unlock…",
    wrong_passphrase: "That did not work. Please check the passphrase or recovery key.",
    change_title: "Change passphrase",
    change_hint: "Existing backups stay readable, and the recovery key stays valid.",
    new_passphrase: "New passphrase",
    save: "Save",
    passphrase_changed: "The passphrase was changed.",
    enc_settings_title: "Encryption",
    enc_status_on: "Backups to this destination are encrypted.",
    enc_status_off: "Backups are not encrypted. Encryption can be turned on in the Backup view.",
    change_passphrase: "Change passphrase…",
    lock_now: "Lock now",
    encrypted_label: "encrypted",
    locked_backup: "Encrypted — unlock to see the details",

    schedule_title: "Automatic backups",
    schedule_hint: "AeternaVault runs these backups itself, quietly and with low priority, while it is open or waiting in the notification area. Missed backups are made up.",
    freq_daily: "Every day",
    freq_weekly: "Every week",
    freq_hourly: "Every few hours",
    freq_at_start: "When AeternaVault starts",
    at_time: "at",
    on_day: "on",
    weekdays: [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ],
    catch_up: "Make up missed backups",
    only_ac: "Only when plugged in (laptops)",
    at_start_hint: "A few minutes after AeternaVault starts — with “Start with Windows” that is shortly after signing in.",
    schedule_needs_key: "Encrypted automatic backups need the key: remember it on this computer, or unlock the backups while AeternaVault runs.",
    remember_now: "Remember…",

    restore_what_title: "What to restore",
    restore_folders: "Folders",
    restore_registry_note: "Registry settings are applied to your current Windows account.",
    open_program_list: "Open list of installed programs",
    adv_program_list: "Save a list of installed programs with every backup",
    selected_backup_locked: "Locked",

    show_secret: "Show passphrase",
    hide_secret: "Hide passphrase",
    copied: "Copied to the clipboard",
    save_as_file: "Save as text file…",
    recovery_file_name: "AeternaVault recovery key.txt",
    recovery_file_saved: "The recovery key was saved. Keep the file away from the backups — for example printed, or on a USB stick in a drawer.",
    destination_app_folder: "Create a folder named “AeternaVault” inside the chosen folder",
    interface_size: "Interface size",
    adv_compatibility_graphics: "Compatibility graphics (OpenGL)",
    adv_compatibility_hint: "Try this if the window looks distorted or flickers. Takes effect at the next start.",

    weekdays_every: [
        "Every Monday",
        "Every Tuesday",
        "Every Wednesday",
        "Every Thursday",
        "Every Friday",
        "Every Saturday",
        "Every Sunday",
    ],
    scope_everything: "everything that is ticked",
    scope_all_folders: "all ticked folders",
    scope_nothing: "Nothing is selected for this backup.",
    schedules_empty: "No automatic backups yet.",
    add_schedule: "Add…",
    edit: "Edit…",
    run_now: "Run now",
    remove_schedule: "Remove this automatic backup",
    stop: "Stop",
    start_with_windows: "Start AeternaVault quietly when I sign in to Windows",
    keep_running: "Keep running in the notification area when the window is closed",
    schedule_only_while_running: "Automatic backups only run while AeternaVault is running. With both options above they also continue after closing the window and after a restart.",
    schedule_new_title: "New automatic backup",
    schedule_edit_title: "Automatic backup",
    schedule_when: "When",
    schedule_what: "What",
    schedule_name: "Name (optional)",
    scope_radio_everything: "All folders ticked under “What is kept safe”",
    scope_radio_only: "Only these folders:",
    starting_at: "starting at",
    tray_open: "Open AeternaVault",
    tray_quit: "Quit AeternaVault",
    close_hint_title: "AeternaVault keeps running",
    close_hint_text: "Automatic backups continue in the background. To quit, right-click this icon.",
    legacy_task_removed: "Automatic backups are now run by AeternaVault itself. The Windows task of the previous version was removed, and AeternaVault now starts quietly with Windows.",
};

pub static DE: Tr = Tr {
    window_title: "AeternaVault — Deine Daten, für die Ewigkeit bewahrt",
    slogan: "Deine Daten – für die Ewigkeit bewahrt.",

    nav_backup: "Sichern",
    nav_restore: "Wiederherstellen",
    nav_apps: "Anwendungen",
    nav_settings: "Einstellungen",
    nav_activity: "Protokoll",
    switch_to_dark: "Zur dunklen Darstellung wechseln",
    switch_to_light: "Zur hellen Darstellung wechseln",
    switch_language: "English",

    sources_title: "Was bewahrt wird",
    sources_empty: "Noch keine Ordner ausgewählt. Füge die Ordner hinzu, die bewahrt werden sollen.",
    add_folder: "Ordner hinzufügen…",
    remove_source: "Aus der Liste entfernen (es wird nichts gelöscht)",
    source_missing: "Ordner nicht gefunden",
    calculating: "wird berechnet…",
    drop_hint: "Ordner lassen sich auch auf dieses Fenster ziehen.",
    destination_title: "Wo es aufbewahrt wird",
    choose: "Auswählen…",
    destination_not_set: "Noch kein Ziel ausgewählt",
    destination_unreachable: "Zurzeit nicht erreichbar. Ist das Laufwerk angeschlossen?",
    mode_title: "Art der Sicherung",
    mode_incremental: "Inkrementell",
    mode_incremental_hint: "Nur neue und geänderte Dateien werden kopiert. Jede Sicherung bleibt dennoch vollständig.",
    mode_full: "Vollständig",
    mode_full_hint: "Alle Dateien werden erneut kopiert. Dauert länger und braucht mehr Platz.",
    last_backup_title: "Letzte Sicherung",
    last_backup_none: "Noch keine Sicherung vorhanden.",
    preview: "Vorschau",
    back_up_now: "Jetzt sichern",
    restore_ellipsis: "Wiederherstellen…",

    status_complete: "abgeschlossen",
    status_warnings: "mit Hinweisen abgeschlossen",
    status_cancelled: "abgebrochen",
    status_failed: "nicht abgeschlossen",
    status_unfinished: "unvollständig",

    restore_choose_title: "Sicherung auswählen",
    restore_selected: "Ausgewählte Sicherung",
    restore_none: "Am Ziel wurden keine Sicherungen gefunden.",
    refresh: "Aktualisieren",
    restore_target_title: "Wiederherstellen nach",
    restore_original: "An die ursprünglichen Orte",
    restore_folder: "In einen anderen Ordner",
    advanced: "Erweitert",
    conflict_title: "Wenn eine Datei bereits existiert",
    conflict_replace: "Ersetzen, wenn sie abweicht",
    conflict_keep: "Behalten",
    conflict_newer: "Behalten, wenn sie neuer ist",
    verify_checksums: "Prüfsummen beim Wiederherstellen prüfen",
    restore: "Wiederherstellen",
    choose_folder_first: "Bitte zuerst einen Ordner auswählen.",
    loading: "Wird geladen…",

    preview_backup_title: "Vorschau der Sicherung",
    preview_restore_title: "Vorschau der Wiederherstellung",
    preview_read_only: "Dies ist eine Vorschau. Es wurde nichts verändert.",
    full_mode_note: "Vollständige Sicherung: Alle Dateien werden erneut kopiert.",
    removed_note: "Nicht mehr vorhandene Dateien bleiben in früheren Sicherungen erhalten. Es wird nichts gelöscht.",
    filter_all: "Alle",
    filter_hint: "Nach Pfad filtern…",
    col_status: "Status",
    col_source: "Quelle",
    col_path: "Pfad",
    col_size: "Größe",
    export_text: "Als Text exportieren…",
    export_csv: "Als CSV exportieren…",
    back: "Zurück",
    start_backup: "Sicherung starten",
    start_restore: "Wiederherstellung starten",
    nothing_for_filter: "Für diesen Filter gibt es nichts anzuzeigen.",

    kind_new: "Neu",
    kind_changed: "Geändert",
    kind_unchanged: "Unverändert",
    kind_removed: "Nicht mehr vorhanden",
    kind_skipped: "Übersprungen",
    kind_create: "Neu anlegen",
    kind_replace: "Ersetzen",
    kind_identical: "Bereits identisch",

    note_link: "Verknüpfung, wird nicht verfolgt",
    note_online_only: "nur online verfügbar, nicht auf diesem Computer gespeichert",
    note_inside_destination: "dies ist das Sicherungsziel",
    note_source_missing: "Ordner nicht gefunden",
    note_no_access: "kein Zugriff",
    note_keep_existing: "die vorhandene Datei bleibt erhalten",
    note_target_newer: "die vorhandene Datei ist neuer",
    note_target_is_folder: "ein Ordner mit diesem Namen existiert",
    note_backup_file_missing: "fehlt in der Sicherung",
    note_unsafe_path: "ungültiger Pfad im Sicherungsindex",

    confirm_backup_title: "Sicherung starten?",
    confirm_restore_title: "Wiederherstellung starten?",
    confirm_overwrite: "Vorhandene Dateien werden mit der Version aus der Sicherung überschrieben.",
    start: "Starten",
    show_details: "Details anzeigen",
    cancel: "Abbrechen",

    working_scan_backup: "Dateien werden gesichtet",
    working_scan_restore: "Abgleich mit der Sicherung",
    working_backup: "Sicherung läuft",
    working_restore: "Wiederherstellung läuft",
    working_finishing: "Wird abgeschlossen",
    stopping: "Wird angehalten…",

    done_backup: "Die Sicherung ist abgeschlossen.",
    done_backup_notes: "Die Sicherung ist abgeschlossen, mit einigen Hinweisen.",
    done_backup_cancelled: "Die Sicherung wurde abgebrochen. Bereits kopierte Dateien bleiben erhalten.",
    done_restore: "Die Wiederherstellung ist abgeschlossen.",
    done_restore_notes: "Die Wiederherstellung ist abgeschlossen, mit einigen Hinweisen.",
    done_restore_cancelled: "Die Wiederherstellung wurde abgebrochen.",
    done_failed: "Der Vorgang konnte nicht abgeschlossen werden.",
    notes_title: "Hinweise",
    to_overview: "Zur Übersicht",
    open_backup_folder: "Sicherungsordner öffnen",

    apps_installed_title: "Installierte Anwendungen",
    apps_installed_hint: "Mit jeder Sicherung wird eine Liste dieser Programme gespeichert – als Checkliste für die Neuinstallation auf einem neuen Computer.",
    search: "Suchen…",
    open_install_folder: "Installationsordner öffnen",

    settings_general: "Allgemein",
    language: "Sprache",
    language_auto: "Automatisch",
    appearance: "Darstellung",
    appearance_system: "Wie Windows",
    appearance_dark: "Dunkel",
    appearance_light: "Hell",
    exclusions_title: "Ausschlüsse",
    exclusions_hint: "Ein Muster pro Zeile, zum Beispiel *.tmp oder node_modules. Gilt für Datei- und Ordnernamen in allen Quellen.",
    apply: "Übernehmen",
    reset_defaults: "Standard wiederherstellen",
    adv_skip_online: "Nur online verfügbare Cloud-Dateien überspringen (zum Beispiel OneDrive)",
    adv_hardlinks: "Unveränderte Dateien verknüpfen, statt sie erneut zu speichern",
    adv_confirm: "Vor dem Start einer Sicherung oder Wiederherstellung nachfragen",
    config_title: "Konfigurationsdatei",
    config_hint: "Alle Einstellungen stehen in dieser Datei und lassen sich auch von Hand bearbeiten.",
    open_in_editor: "Im Editor öffnen",
    reload: "Neu laden",
    open_log_folder: "Protokollordner öffnen",
    portable_mode: "Portabler Modus",
    config_reloaded: "Die Konfiguration wurde neu geladen.",
    config_created: "Willkommen. Eine Konfiguration mit sinnvollen Standardwerten wurde angelegt. Bitte prüfe die Ordner und das Ziel.",

    activity_empty: "Bisher ist nichts geschehen.",
    unexpected_problem: "Ein unerwartetes Problem ist aufgetreten. Details stehen im Protokoll.",
    csv_columns: ["status", "quelle", "pfad", "groesse_bytes", "hinweis"],

    apps_card_title: "Anwendungseinstellungen",
    apps_card_empty: "Noch keine Anwendungen ausgewählt.",
    choose_apps: "Anwendungen auswählen…",
    apps_hint: "Wähle aus, welche Einstellungen bewahrt werden. Auf einem neuen Computer landen sie wieder am richtigen Ort, auch unter einem anderen Benutzernamen.",
    select_all_found: "Alle gefundenen auswählen",
    show_not_found: "Auch nicht installierte Anwendungen zeigen",
    not_found: "auf diesem Computer nicht gefunden",
    app_open: "geöffnet",
    no_matches: "Nichts passt zur Suche.",
    categories: [
        "Browser",
        "E-Mail",
        "Kommunikation",
        "Büro und Notizen",
        "Entwicklung",
        "Foto, Video und Musik",
        "Spiele",
        "Passwörter und Schlüssel",
        "Werkzeuge",
        "Windows-Einstellungen",
    ],

    select_all: "Alles auswählen",
    select_none: "Nichts auswählen",
    partial_selection: "Nur die angehakten Einträge werden bewahrt.",
    choose_contents: "Auswählen, was aus diesem Ordner bewahrt wird",
    folder_empty: "Dieser Ordner ist leer.",

    encrypt_backups: "Sicherungen verschlüsseln",
    encrypt_hint_on: "Mit deiner Passphrase geschützt. Dateinamen und Inhalte sind ohne sie nicht lesbar.",
    encrypt_hint_off: "Empfohlen für Cloud-Ordner und sensible Daten.",
    encryption_disabled_note: "Neue Sicherungen werden nicht mehr verschlüsselt. Bestehende verschlüsselte Sicherungen bleiben geschützt.",
    enc_create_title: "Verschlüsselung einrichten",
    enc_create_hint: "Wähle eine Passphrase. Ein paar Wörter, die nichts miteinander zu tun haben, sind leicht zu merken und schwer zu erraten.",
    passphrase: "Passphrase",
    passphrase_repeat: "Passphrase wiederholen",
    strength: ["Zu kurz", "Schwach", "Mittel", "Gut", "Stark"],
    passphrases_differ: "Die beiden Passphrasen unterscheiden sich.",
    passphrase_too_weak: "Bitte mindestens 10 Zeichen verwenden.",
    remember_on_computer: "Auf diesem Computer merken (nötig für automatische Sicherungen)",
    enc_warning: "Ohne Passphrase oder Wiederherstellungsschlüssel kann niemand diese Sicherungen wiederherstellen – auch du nicht.",
    set_up: "Einrichten",
    recovery_title: "Dein Wiederherstellungsschlüssel",
    recovery_hint: "Falls du die Passphrase einmal vergisst, entsperrt dieser Schlüssel deine Sicherungen. Schreibe ihn auf oder drucke ihn aus und bewahre ihn sicher auf. Er wird nur jetzt angezeigt.",
    copy: "Kopieren",
    recovery_confirm: "Ich habe den Wiederherstellungsschlüssel sicher aufbewahrt",
    done: "Fertig",
    unlock_title: "Verschlüsselte Sicherungen entsperren",
    unlock_hint: "Gib die Passphrase oder den Wiederherstellungsschlüssel ein.",
    unlock: "Entsperren…",
    wrong_passphrase: "Das hat nicht geklappt. Bitte prüfe Passphrase oder Wiederherstellungsschlüssel.",
    change_title: "Passphrase ändern",
    change_hint: "Bestehende Sicherungen bleiben lesbar, und der Wiederherstellungsschlüssel bleibt gültig.",
    new_passphrase: "Neue Passphrase",
    save: "Speichern",
    passphrase_changed: "Die Passphrase wurde geändert.",
    enc_settings_title: "Verschlüsselung",
    enc_status_on: "Sicherungen an dieses Ziel werden verschlüsselt.",
    enc_status_off: "Sicherungen werden nicht verschlüsselt. Die Verschlüsselung lässt sich in der Ansicht „Sichern“ einschalten.",
    change_passphrase: "Passphrase ändern…",
    lock_now: "Jetzt sperren",
    encrypted_label: "verschlüsselt",
    locked_backup: "Verschlüsselt – entsperren, um Details zu sehen",

    schedule_title: "Automatische Sicherungen",
    schedule_hint: "AeternaVault führt diese Sicherungen selbst aus – leise und mit niedriger Priorität, solange es geöffnet ist oder im Infobereich wartet. Verpasste Sicherungen werden nachgeholt.",
    freq_daily: "Jeden Tag",
    freq_weekly: "Jede Woche",
    freq_hourly: "Alle paar Stunden",
    freq_at_start: "Beim Start von AeternaVault",
    at_time: "um",
    on_day: "am",
    weekdays: [
        "Montag",
        "Dienstag",
        "Mittwoch",
        "Donnerstag",
        "Freitag",
        "Samstag",
        "Sonntag",
    ],
    catch_up: "Verpasste Sicherungen nachholen",
    only_ac: "Nur mit Netzteil (Laptops)",
    at_start_hint: "Einige Minuten nach dem Start von AeternaVault – mit „AeternaVault bei der Anmeldung starten“ also kurz nach der Anmeldung.",
    schedule_needs_key: "Verschlüsselte automatische Sicherungen brauchen den Schlüssel: auf diesem Computer merken oder die Sicherungen entsperren, solange AeternaVault läuft.",
    remember_now: "Merken…",

    restore_what_title: "Was wiederhergestellt wird",
    restore_folders: "Ordner",
    restore_registry_note: "Registry-Einstellungen werden für dein aktuelles Windows-Konto übernommen.",
    open_program_list: "Liste der installierten Programme öffnen",
    adv_program_list: "Mit jeder Sicherung eine Liste der installierten Programme speichern",
    selected_backup_locked: "Gesperrt",

    show_secret: "Passphrase anzeigen",
    hide_secret: "Passphrase verbergen",
    copied: "In die Zwischenablage kopiert",
    save_as_file: "Als Textdatei speichern…",
    recovery_file_name: "AeternaVault Wiederherstellungsschlüssel.txt",
    recovery_file_saved: "Der Wiederherstellungsschlüssel wurde gespeichert. Bewahre die Datei getrennt von den Sicherungen auf – zum Beispiel ausgedruckt oder auf einem USB-Stick in der Schublade.",
    destination_app_folder: "Im gewählten Ordner einen Ordner „AeternaVault“ anlegen",
    interface_size: "Größe der Oberfläche",
    adv_compatibility_graphics: "Kompatible Grafik (OpenGL)",
    adv_compatibility_hint: "Hilft, wenn das Fenster verzerrt aussieht oder flackert. Wirkt beim nächsten Start.",

    weekdays_every: [
        "Jeden Montag",
        "Jeden Dienstag",
        "Jeden Mittwoch",
        "Jeden Donnerstag",
        "Jeden Freitag",
        "Jeden Samstag",
        "Jeden Sonntag",
    ],
    scope_everything: "alles Ausgewählte",
    scope_all_folders: "alle ausgewählten Ordner",
    scope_nothing: "Für diese Sicherung ist nichts ausgewählt.",
    schedules_empty: "Noch keine automatischen Sicherungen.",
    add_schedule: "Hinzufügen…",
    edit: "Bearbeiten…",
    run_now: "Jetzt ausführen",
    remove_schedule: "Diese automatische Sicherung entfernen",
    stop: "Anhalten",
    start_with_windows: "AeternaVault bei der Anmeldung an Windows leise starten",
    keep_running: "Beim Schließen des Fensters im Infobereich weiterlaufen",
    schedule_only_while_running: "Automatische Sicherungen laufen nur, solange AeternaVault läuft. Mit beiden Optionen oben laufen sie auch nach dem Schließen des Fensters und nach einem Neustart weiter.",
    schedule_new_title: "Neue automatische Sicherung",
    schedule_edit_title: "Automatische Sicherung",
    schedule_when: "Wann",
    schedule_what: "Was",
    schedule_name: "Name (optional)",
    scope_radio_everything: "Alle unter „Was bewahrt wird“ ausgewählten Ordner",
    scope_radio_only: "Nur diese Ordner:",
    starting_at: "ab",
    tray_open: "AeternaVault öffnen",
    tray_quit: "AeternaVault beenden",
    close_hint_title: "AeternaVault läuft weiter",
    close_hint_text: "Automatische Sicherungen laufen im Hintergrund weiter. Zum Beenden mit der rechten Maustaste auf dieses Symbol klicken.",
    legacy_task_removed: "Automatische Sicherungen führt AeternaVault jetzt selbst aus. Die Windows-Aufgabe der vorigen Version wurde entfernt, und AeternaVault startet nun leise mit Windows.",
};

#[cfg(test)]
mod tests {
    use super::Lang;

    #[test]
    fn formats_numbers_per_language() {
        assert_eq!(Lang::En.count(1234567), "1,234,567");
        assert_eq!(Lang::De.count(1234567), "1.234.567");
        assert_eq!(Lang::En.bytes(13_314_398_618), "12.4 GB");
        assert_eq!(Lang::De.bytes(13_314_398_618), "12,4 GB");
        assert_eq!(Lang::De.bytes(512), "512 B");
    }
}
