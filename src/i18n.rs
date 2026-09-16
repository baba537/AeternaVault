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

    pub fn backups_count(self, n: usize) -> String {
        match (self, n) {
            (Lang::En, 1) => "1 backup".to_string(),
            (Lang::En, n) => format!("{} backups", self.count(n as u64)),
            (Lang::De, 1) => "1 Sicherung".to_string(),
            (Lang::De, n) => format!("{} Sicherungen", self.count(n as u64)),
        }
    }

    pub fn stored_bytes(self, bytes: u64) -> String {
        match self {
            Lang::En => format!("{} stored", self.bytes(bytes)),
            Lang::De => format!("{} gespeichert", self.bytes(bytes)),
        }
    }

    pub fn delete_n(self, n: usize) -> String {
        match self {
            Lang::En => format!("Delete {n}…"),
            Lang::De => format!("{n} löschen…"),
        }
    }

    pub fn copy_n_to(self, n: usize) -> String {
        match self {
            Lang::En => format!("Copy {n} to…"),
            Lang::De => format!("{n} kopieren nach…"),
        }
    }

    pub fn contents_of(self, title: &str) -> String {
        match self {
            Lang::En => format!("Backup of {title}"),
            Lang::De => format!("Sicherung vom {title}"),
        }
    }

    // --- 0.4 ------------------------------------------------------------------

    /// Tooltip of the passphrase rating: which criteria are met.
    pub fn passphrase_criteria(self, a: &crate::engine::passphrase::Assessment) -> String {
        use crate::engine::passphrase::{FAIR_LENGTH, GOOD_LENGTH};
        let mark = |ok: bool| if ok { "✓" } else { "✗" };
        let length_ok = a.length >= FAIR_LENGTH;
        let classes = a.classes();
        let no_pattern = !(a.keyboard_pattern || a.sequence);
        let not_predictable = !(a.predictable || a.personal);
        match self {
            Lang::En => format!(
                "{} Length: {} characters (at least {FAIR_LENGTH}, better {GOOD_LENGTH})\n\
                 {} Kinds of characters: {classes} of 4 (upper case, lower case, digits, special characters such as ? ! # $)\n\
                 {} No keyboard rows or sequences such as “qwertz” or “12345”\n\
                 {} No dates, years, common words or names\n\
                 Any passphrase is accepted; this is only a hint.",
                mark(length_ok),
                a.length,
                mark(classes >= 3),
                mark(no_pattern),
                mark(not_predictable),
            ),
            Lang::De => format!(
                "{} Länge: {} Zeichen (mindestens {FAIR_LENGTH}, besser {GOOD_LENGTH})\n\
                 {} Zeichenarten: {classes} von 4 (Groß- und Kleinbuchstaben, Ziffern, Sonderzeichen wie ? ! # $)\n\
                 {} Keine Tastaturreihen oder Folgen wie „qwertz“ oder „12345“\n\
                 {} Keine Daten, Jahreszahlen, gängigen Wörter oder Namen\n\
                 Jede Passphrase wird akzeptiert; das ist nur ein Hinweis.",
                mark(length_ok),
                a.length,
                mark(classes >= 3),
                mark(no_pattern),
                mark(not_predictable),
            ),
        }
    }

    fn kdf_name(self, kdf: crate::engine::crypto::KdfParams) -> &'static str {
        use crate::engine::crypto::KdfParams;
        let t = self.t();
        let full = if kdf == KdfParams::STRONG {
            t.kdf_strong
        } else if kdf == KdfParams::VERY_STRONG {
            t.kdf_very_strong
        } else {
            t.kdf_standard
        };
        full.split(['—', '–']).next().unwrap_or(full).trim()
    }

    pub fn method_for_new_vault(
        self,
        cipher: crate::engine::crypto::Cipher,
        kdf: crate::engine::crypto::KdfParams,
    ) -> String {
        let kdf = self.kdf_name(kdf);
        match self {
            Lang::En => format!(
                "Method: {} with {kdf} key derivation. It can be chosen in the settings before setting up.",
                cipher.display_name()
            ),
            Lang::De => format!(
                "Verfahren: {} mit Schlüsselableitung „{kdf}“. Es lässt sich vor dem Einrichten in den Einstellungen wählen.",
                cipher.display_name()
            ),
        }
    }

    /// "XChaCha20-Poly1305 · Standard" (folded choice in the settings).
    pub fn method_summary(
        self,
        cipher: crate::engine::crypto::Cipher,
        kdf: crate::engine::crypto::KdfParams,
    ) -> String {
        format!("{} · {}", cipher.display_name(), self.kdf_name(kdf))
    }

    pub fn method_fixed(self, cipher: &str) -> String {
        match self {
            Lang::En => format!(
                "{cipher}. The method of an existing vault stays the same; a different one can be chosen for a new destination."
            ),
            Lang::De => format!(
                "{cipher}. Ein bestehender Tresor behält sein Verfahren; für ein neues Ziel lässt sich ein anderes wählen."
            ),
        }
    }

    pub fn folder_added(self, name: &str, already_listed: bool) -> String {
        match (self, already_listed) {
            (Lang::En, false) => format!("“{name}” was added to the folders to back up."),
            (Lang::En, true) => format!("“{name}” is already in the list of folders to back up."),
            (Lang::De, false) => {
                format!("„{name}“ wurde zu den zu sichernden Ordnern hinzugefügt.")
            }
            (Lang::De, true) => {
                format!("„{name}“ ist bereits in der Liste der zu sichernden Ordner.")
            }
        }
    }

    pub fn viewer_banner(self, folder: &str) -> String {
        match self {
            Lang::En => {
                format!("Viewing the backups in {folder}. Your own settings are not changed.")
            }
            Lang::De => format!(
                "Ansicht der Sicherungen in {folder}. Deine eigenen Einstellungen bleiben unverändert."
            ),
        }
    }

    pub fn removed_around(self, time: DateTime<Local>) -> String {
        match self {
            Lang::En => format!("removed around {}", time.format("%-d %b %Y")),
            Lang::De => format!("wird etwa am {} entfernt", time.format("%d.%m.%Y")),
        }
    }

    /// "Wednesday, 16 September 2026"
    pub fn weekday_date_only(self, time: DateTime<Local>) -> String {
        let weekday = time.weekday().num_days_from_monday() as usize;
        match self {
            Lang::En => format!(
                "{}, {}",
                [
                    "Monday",
                    "Tuesday",
                    "Wednesday",
                    "Thursday",
                    "Friday",
                    "Saturday",
                    "Sunday"
                ][weekday],
                time.format("%-d %B %Y")
            ),
            Lang::De => {
                let month = [
                    "Januar",
                    "Februar",
                    "März",
                    "April",
                    "Mai",
                    "Juni",
                    "Juli",
                    "August",
                    "September",
                    "Oktober",
                    "November",
                    "Dezember",
                ][time.month0() as usize];
                format!(
                    "{}, {}. {month} {}",
                    [
                        "Montag",
                        "Dienstag",
                        "Mittwoch",
                        "Donnerstag",
                        "Freitag",
                        "Samstag",
                        "Sonntag"
                    ][weekday],
                    time.day(),
                    time.year()
                )
            }
        }
    }

    fn history_outcome(self, outcome: crate::history::Outcome) -> &'static str {
        use crate::history::Outcome as O;
        match (self, outcome) {
            (Lang::En, O::Complete) => "completed",
            (Lang::De, O::Complete) => "abgeschlossen",
            (Lang::En, O::Notes) => "completed with notes",
            (Lang::De, O::Notes) => "mit Hinweisen abgeschlossen",
            (Lang::En, O::Cancelled) => "cancelled",
            (Lang::De, O::Cancelled) => "abgebrochen",
            (Lang::En, O::Skipped) => "skipped, destination not connected",
            (Lang::De, O::Skipped) => "übersprungen, Ziel nicht angeschlossen",
            (Lang::En, O::NeedsPassphrase) => "not run, passphrase needed",
            (Lang::De, O::NeedsPassphrase) => "nicht ausgeführt, Passphrase benötigt",
            (Lang::En, O::Failed) => "failed",
            (Lang::De, O::Failed) => "fehlgeschlagen",
        }
    }

    /// Title, detail line and marking of one history entry.
    pub fn history_entry(
        self,
        event: &crate::history::Event,
    ) -> (String, String, crate::history::Severity) {
        use crate::history::{Event as E, Severity};
        let en = self == Lang::En;
        let pick = |en_text: String, de_text: String| if en { en_text } else { de_text };
        let join = |parts: Vec<String>| {
            parts
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        };
        match event {
            E::Backup {
                job,
                command_line,
                outcome,
                files,
                bytes,
                snapshot,
                message,
            } => {
                let what = if !job.is_empty() {
                    pick(
                        format!("Automatic backup “{job}”"),
                        format!("Automatische Sicherung „{job}“"),
                    )
                } else if *command_line {
                    pick(
                        "Backup from the command line".into(),
                        "Sicherung über die Kommandozeile".into(),
                    )
                } else {
                    pick("Backup".into(), "Sicherung".into())
                };
                let counts = if *files > 0 || *bytes > 0 {
                    format!("{}, {}", self.files(*files), self.bytes(*bytes))
                } else {
                    String::new()
                };
                (
                    format!("{what} {}", self.history_outcome(*outcome)),
                    join(vec![snapshot.clone(), counts, message.clone()]),
                    outcome.severity(),
                )
            }
            E::Restore {
                outcome,
                snapshot,
                target,
                files,
                bytes,
                message,
            } => {
                let place = if target.is_empty() {
                    pick(
                        "to the original places".into(),
                        "an die ursprünglichen Orte".into(),
                    )
                } else {
                    pick(format!("to {target}"), format!("nach {target}"))
                };
                (
                    pick(
                        format!("Restore {}", self.history_outcome(*outcome)),
                        format!("Wiederherstellung {}", self.history_outcome(*outcome)),
                    ),
                    join(vec![
                        snapshot.clone(),
                        place,
                        format!("{}, {}", self.files(*files), self.bytes(*bytes)),
                        message.clone(),
                    ]),
                    outcome.severity(),
                )
            }
            E::BackupsDeleted {
                snapshots,
                by_rules,
                message,
            } => {
                let n = snapshots.len();
                let title = match (en, *by_rules, n) {
                    (true, true, 1) => "1 old backup removed by the rules".to_string(),
                    (true, true, n) => format!("{n} old backups removed by the rules"),
                    (true, false, 1) => "1 backup deleted".to_string(),
                    (true, false, n) => format!("{n} backups deleted"),
                    (false, true, 1) => "1 alte Sicherung nach den Regeln entfernt".to_string(),
                    (false, true, n) => format!("{n} alte Sicherungen nach den Regeln entfernt"),
                    (false, false, 1) => "1 Sicherung gelöscht".to_string(),
                    (false, false, n) => format!("{n} Sicherungen gelöscht"),
                };
                (
                    title,
                    join(vec![snapshots.join(", "), message.clone()]),
                    Severity::Neutral,
                )
            }
            E::BackupMoved { snapshot, to } => (
                pick("Backup moved".into(), "Sicherung verschoben".into()),
                pick(
                    format!("{snapshot} to {to}"),
                    format!("{snapshot} nach {to}"),
                ),
                Severity::Neutral,
            ),
            E::FilesCopied {
                snapshot,
                files,
                to,
            } => (
                pick(
                    "Files copied out of a backup".into(),
                    "Dateien aus einer Sicherung kopiert".into(),
                ),
                pick(
                    format!("{} from {snapshot} to {to}", self.files(*files)),
                    format!("{} aus {snapshot} nach {to}", self.files(*files)),
                ),
                Severity::Neutral,
            ),
            E::Verified {
                snapshot,
                files,
                damaged,
                missing,
            } => {
                let ok = *damaged == 0 && *missing == 0;
                (
                    match (en, ok) {
                        (true, true) => "Backup checked: everything readable".to_string(),
                        (true, false) => "Backup checked: problems found".to_string(),
                        (false, true) => "Sicherung geprüft: alles lesbar".to_string(),
                        (false, false) => "Sicherung geprüft: Probleme gefunden".to_string(),
                    },
                    if ok {
                        join(vec![snapshot.clone(), self.files(*files)])
                    } else {
                        join(vec![
                            snapshot.clone(),
                            pick(
                                format!("{damaged} damaged, {missing} missing"),
                                format!("{damaged} beschädigt, {missing} fehlen"),
                            ),
                        ])
                    },
                    if ok {
                        Severity::Good
                    } else {
                        Severity::Problem
                    },
                )
            }
            E::JobCreated { job } => (
                pick("Backup job created".into(), "Backup-Job angelegt".into()),
                job.clone(),
                Severity::Neutral,
            ),
            E::JobChanged { job } => (
                pick("Backup job changed".into(), "Backup-Job geändert".into()),
                job.clone(),
                Severity::Neutral,
            ),
            E::JobRemoved { job } => (
                pick("Backup job removed".into(), "Backup-Job entfernt".into()),
                job.clone(),
                Severity::Neutral,
            ),
            E::JobSwitched { job, on } => (
                match (en, on) {
                    (true, true) => "Backup job switched on".to_string(),
                    (true, false) => "Backup job switched off".to_string(),
                    (false, true) => "Backup-Job eingeschaltet".to_string(),
                    (false, false) => "Backup-Job ausgeschaltet".to_string(),
                },
                job.clone(),
                Severity::Neutral,
            ),
            E::EncryptionSetUp { cipher } => (
                pick(
                    "Encryption set up".into(),
                    "Verschlüsselung eingerichtet".into(),
                ),
                cipher.clone(),
                Severity::Neutral,
            ),
            E::EncryptionSwitched { on } => (
                match (en, on) {
                    (true, true) => "Encryption switched on".to_string(),
                    (true, false) => "Encryption switched off".to_string(),
                    (false, true) => "Verschlüsselung eingeschaltet".to_string(),
                    (false, false) => "Verschlüsselung ausgeschaltet".to_string(),
                },
                String::new(),
                Severity::Neutral,
            ),
            E::PassphraseChanged => (
                pick("Passphrase changed".into(), "Passphrase geändert".into()),
                String::new(),
                Severity::Neutral,
            ),
            E::RecoveryKeyReplaced => (
                pick(
                    "New recovery key created".into(),
                    "Neuer Wiederherstellungsschlüssel erstellt".into(),
                ),
                String::new(),
                Severity::Neutral,
            ),
            E::DestinationChanged { destination } => (
                pick("Destination changed".into(), "Ziel geändert".into()),
                destination.clone(),
                Severity::Neutral,
            ),
            E::StartWithWindows { on } => (
                match (en, on) {
                    (true, true) => "Start with Windows switched on".to_string(),
                    (true, false) => "Start with Windows switched off".to_string(),
                    (false, true) => "Start mit Windows eingeschaltet".to_string(),
                    (false, false) => "Start mit Windows ausgeschaltet".to_string(),
                },
                String::new(),
                Severity::Neutral,
            ),
            E::Other => (
                pick("Other event".into(), "Anderes Ereignis".into()),
                String::new(),
                Severity::Neutral,
            ),
        }
    }

    pub fn retention_preview(self, n: usize) -> String {
        match (self, n) {
            (Lang::En, 0) => "With these rules, no backup would be removed at the moment.".into(),
            (Lang::En, 1) => "With these rules, 1 older backup would be removed now.".into(),
            (Lang::En, n) => format!("With these rules, {n} older backups would be removed now."),
            (Lang::De, 0) => "Mit diesen Regeln würde zurzeit keine Sicherung entfernt.".into(),
            (Lang::De, 1) => "Mit diesen Regeln würde jetzt 1 ältere Sicherung entfernt.".into(),
            (Lang::De, n) => {
                format!("Mit diesen Regeln würden jetzt {n} ältere Sicherungen entfernt.")
            }
        }
    }

    pub fn retention_removed(self, n: usize) -> String {
        match (self, n) {
            (Lang::En, 1) => "1 old backup was removed by the retention rules.".into(),
            (Lang::En, n) => format!("{n} old backups were removed by the retention rules."),
            (Lang::De, 1) => "1 alte Sicherung wurde nach den Aufbewahrungsregeln entfernt.".into(),
            (Lang::De, n) => {
                format!("{n} alte Sicherungen wurden nach den Aufbewahrungsregeln entfernt.")
            }
        }
    }

    pub fn encrypted_part_files(self, n: u64) -> String {
        match self {
            Lang::En => format!("{} of them in the encrypted part.", self.files(n)),
            Lang::De => format!("Davon {} im verschlüsselten Teil.", self.files(n)),
        }
    }

    pub fn verify_result(self, files: u64, bytes: u64, duration: std::time::Duration) -> String {
        match self {
            Lang::En => format!(
                "{} with {} read and compared with their checksums in {}.",
                self.files(files),
                self.bytes(bytes),
                self.duration(duration)
            ),
            Lang::De => format!(
                "{} mit {} gelesen und mit ihren Prüfsummen verglichen, in {}.",
                self.files(files),
                self.bytes(bytes),
                self.duration(duration)
            ),
        }
    }

    pub fn verify_problems(self, damaged: usize, missing: usize) -> String {
        match self {
            Lang::En => format!(
                "{damaged} damaged, {missing} missing. The affected files cannot be restored from this backup; an older or newer backup may still have them."
            ),
            Lang::De => format!(
                "{damaged} beschädigt, {missing} fehlend. Die betroffenen Dateien lassen sich aus dieser Sicherung nicht wiederherstellen; eine ältere oder neuere Sicherung hat sie vielleicht noch."
            ),
        }
    }

    pub fn delete_result(self, report: &crate::engine::manage::DeleteReport) -> String {
        let mut text = match self {
            Lang::En => format!("{} deleted.", self.backups_count(report.deleted.len())),
            Lang::De => format!("{} gelöscht.", self.backups_count(report.deleted.len())),
        };
        if report.rehomed_files > 0 {
            text.push(' ');
            text.push_str(&self.rehomed_note(report.rehomed_files));
        }
        if report.freed_bytes > 0 {
            text.push_str(&match self {
                Lang::En => format!(
                    " {} of encrypted data freed.",
                    self.bytes(report.freed_bytes)
                ),
                Lang::De => format!(
                    " {} verschlüsselte Daten freigegeben.",
                    self.bytes(report.freed_bytes)
                ),
            });
        }
        if report.deleted_permanently > 0 {
            text.push_str(match self {
                Lang::En => " The drive has no recycle bin, so they were deleted permanently.",
                Lang::De => {
                    " Das Laufwerk hat keinen Papierkorb, daher wurden sie endgültig gelöscht."
                }
            });
        }
        text
    }

    pub fn rehomed_note(self, n: u64) -> String {
        match self {
            Lang::En => format!(
                "{} still needed by newer backups were copied into them first.",
                self.files(n)
            ),
            Lang::De => format!(
                "{}, die neuere Sicherungen noch brauchten, wurden vorher dorthin kopiert.",
                self.files(n)
            ),
        }
    }

    pub fn transfer_result(self, files: u64, bytes: u64, duration: std::time::Duration) -> String {
        match self {
            Lang::En => format!(
                "{} with {} copied and checked in {}; the original was removed.",
                self.files(files),
                self.bytes(bytes),
                self.duration(duration)
            ),
            Lang::De => format!(
                "{} mit {} kopiert und geprüft in {}; das Original wurde entfernt.",
                self.files(files),
                self.bytes(bytes),
                self.duration(duration)
            ),
        }
    }

    pub fn extract_result(self, files: u64, bytes: u64, duration: std::time::Duration) -> String {
        match self {
            Lang::En => format!(
                "{} with {} copied in {}.",
                self.files(files),
                self.bytes(bytes),
                self.duration(duration)
            ),
            Lang::De => format!(
                "{} mit {} kopiert in {}.",
                self.files(files),
                self.bytes(bytes),
                self.duration(duration)
            ),
        }
    }

    pub fn confirm_delete(self, names: &[String]) -> String {
        let list = self.list_names(names);
        match self {
            Lang::En => format!("These backups will be deleted: {list}."),
            Lang::De => format!("Diese Sicherungen werden gelöscht: {list}."),
        }
    }

    pub fn confirm_transfer(self, name: &str, target: &str) -> String {
        match self {
            Lang::En => format!(
                "The backup of {name} is copied to {target} and checked. Only then is the original removed. Newer backups that still need its files keep their own copies."
            ),
            Lang::De => format!(
                "Die Sicherung vom {name} wird nach {target} kopiert und geprüft. Erst danach wird das Original entfernt. Neuere Sicherungen, die noch Dateien daraus brauchen, behalten eigene Kopien."
            ),
        }
    }

    pub fn confirm_extract(self, files: Option<usize>, target: &str) -> String {
        match (self, files) {
            (Lang::En, None) => format!("All files of the backup are copied to {target}."),
            (Lang::En, Some(n)) => format!("{n} files are copied to {target}."),
            (Lang::De, None) => format!("Alle Dateien der Sicherung werden nach {target} kopiert."),
            (Lang::De, Some(n)) => format!("{n} Dateien werden nach {target} kopiert."),
        }
    }

    pub fn cipher_summary(self, cipher: &str) -> String {
        match self {
            Lang::En => format!("{cipher} with 256-bit keys, in 1 MiB authenticated chunks"),
            Lang::De => {
                format!("{cipher} mit 256-Bit-Schlüsseln, in authentifizierten 1-MiB-Abschnitten")
            }
        }
    }

    pub fn kdf_summary(self, memory_kib: u32, iterations: u32) -> String {
        let mib = memory_kib / 1024;
        match self {
            Lang::En => format!(
                "Argon2id with {mib} MiB and {iterations} passes; a recovery key as second way in"
            ),
            Lang::De => format!(
                "Argon2id mit {mib} MiB und {iterations} Durchläufen; ein Wiederherstellungsschlüssel als zweiter Zugang"
            ),
        }
    }

    pub fn cipher_hint(self, cipher: crate::engine::crypto::Cipher) -> &'static str {
        use crate::engine::crypto::Cipher;
        match (self, cipher) {
            (Lang::En, Cipher::XChaCha20Poly1305) => {
                "Modern and fast on every processor. Used by WireGuard, age and many messengers."
            }
            (Lang::De, Cipher::XChaCha20Poly1305) => {
                "Modern und auf jedem Prozessor schnell. Genutzt von WireGuard, age und vielen Messengern."
            }
            (Lang::En, Cipher::Aes256Gcm) => {
                "The widely standardised choice (FIPS); fast on processors with AES support."
            }
            (Lang::De, Cipher::Aes256Gcm) => {
                "Die weit standardisierte Wahl (FIPS); schnell auf Prozessoren mit AES-Unterstützung."
            }
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
    pub strength: [&'static str; 3],
    pub passphrases_differ: &'static str,
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
    pub change_passphrase: &'static str,
    pub lock_now: &'static str,
    pub encrypted_label: &'static str,
    pub locked_backup: &'static str,

    // --- 0.2: automatic backups ------------------------------------------------
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

    // --- 0.3: encryption method and recovery key ----------------------------------
    pub enc_method_title: &'static str,
    pub recommended_suffix: &'static str,
    pub kdf_title: &'static str,
    pub kdf_standard: &'static str,
    pub kdf_strong: &'static str,
    pub kdf_very_strong: &'static str,
    pub test_recovery_title: &'static str,
    pub test_recovery_hint: &'static str,
    pub test_recovery_ok: &'static str,
    pub test_recovery_is_passphrase: &'static str,
    pub test_recovery_wrong: &'static str,
    pub test_now: &'static str,

    // --- 0.3: managing backups ----------------------------------------------------
    pub nav_backups: &'static str,
    pub backups_title: &'static str,
    pub backups_select_hint: &'static str,
    pub some_backups_locked: &'static str,
    pub partly_locked: &'static str,
    pub partly_encrypted_label: &'static str,
    pub browse: &'static str,
    pub check_backup: &'static str,
    pub copy_files_to: &'static str,
    pub move_to: &'static str,
    pub move_backup: &'static str,
    pub open_folder: &'static str,
    pub delete_backup: &'static str,
    pub retention_title: &'static str,
    pub retention_auto: &'static str,
    pub retention_hint: &'static str,
    pub retention_keep: &'static str,
    pub retention_newest: &'static str,
    pub retention_days: &'static str,
    pub retention_weeks: &'static str,
    pub retention_months: &'static str,
    pub clean_up_now: &'static str,
    pub open: &'static str,
    pub open_copy_hint: &'static str,
    pub copy_all_to: &'static str,
    pub copies_are_decrypted: &'static str,
    pub working_verify: &'static str,
    pub working_delete: &'static str,
    pub working_transfer: &'static str,
    pub working_extract: &'static str,
    pub done_verify_ok: &'static str,
    pub done_verify_problems: &'static str,
    pub done_verify_cancelled: &'static str,
    pub damaged_label: &'static str,
    pub missing_label: &'static str,
    pub done_delete: &'static str,
    pub done_transfer: &'static str,
    pub done_extract: &'static str,
    pub done_extract_notes: &'static str,
    pub confirm_delete_title: &'static str,
    pub confirm_delete_plain: &'static str,
    pub confirm_delete_encrypted: &'static str,
    pub confirm_transfer_title: &'static str,
    pub confirm_extract_title: &'static str,

    // --- 0.3: choosing what is encrypted, and how ---------------------------------
    pub encrypt_item: &'static str,
    pub scope_encrypt_everything: &'static str,
    pub scope_encrypt_selected: &'static str,
    pub scope_selected_hint: &'static str,
    pub scope_nothing_marked: &'static str,
    pub encrypt_app_settings: &'static str,
    pub enc_status_selected: &'static str,
    pub test_recovery: &'static str,
    pub replace_recovery: &'static str,
    pub replace_recovery_hint: &'static str,
    pub open_by_double_click: &'static str,
    pub open_by_double_click_hint: &'static str,
    pub enc_info_title: &'static str,
    pub enc_info_cipher: &'static str,
    pub enc_info_passphrase: &'static str,
    pub enc_info_hidden: &'static str,
    pub enc_info_hidden_value: &'static str,
    pub enc_info_key: &'static str,
    pub enc_info_key_remembered: &'static str,
    pub enc_info_key_not_remembered: &'static str,
    pub enc_info_location: &'static str,
    pub enc_without_app_title: &'static str,
    pub enc_without_app_text: &'static str,
    pub enc_without_app_script: &'static str,

    // --- 0.4: calmer layout, backup jobs, history ----------------------------------
    pub passphrase_empty: &'static str,
    pub back_arrow: &'static str,
    pub close_viewer: &'static str,
    pub vault_missing: &'static str,
    pub nav_jobs: &'static str,
    pub jobs_title: &'static str,
    pub open_settings: &'static str,
    pub change_in_settings: &'static str,
    pub scope_selected_short: &'static str,
    pub make_automatic: &'static str,
    pub make_automatic_hint: &'static str,
    pub enc_status_off_settings: &'static str,
    pub enc_scope_title: &'static str,
    pub background_title: &'static str,
    pub start_with_windows_hint: &'static str,
    pub keep_running_hint: &'static str,
    pub only_ac_hint: &'static str,
    pub forecast_title: &'static str,
    pub forecast_assumes_jobs: &'static str,
    pub forecast_assumes_daily: &'static str,
    pub forecast_empty: &'static str,
    pub removed_next_backup: &'static str,
    pub kept_long: &'static str,
    pub show_technical_log: &'static str,
    pub show_history: &'static str,
    pub history_hint: &'static str,
    pub technical_log_hint: &'static str,
    pub history_empty: &'static str,
    pub filter_activity: &'static str,
    pub explorer_menu: &'static str,
    pub explorer_menu_hint: &'static str,
    pub explorer_menu_label: &'static str,
    pub verify_after: &'static str,
    pub verify_after_hint: &'static str,
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
    strength: ["Weak", "Fair", "Good"],
    passphrases_differ: "The two passphrases are different.",
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
    change_passphrase: "Change passphrase…",
    lock_now: "Lock now",
    encrypted_label: "encrypted",
    locked_backup: "Encrypted — unlock to see the details",

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
    schedule_only_while_running: "Backup jobs only run while AeternaVault is running. Turn on “start with Windows” and “keep running in the notification area” in the settings so they also run after closing the window and after a restart.",
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

    enc_method_title: "Encryption method",
    recommended_suffix: " (recommended)",
    kdf_title: "Protection of the passphrase against guessing",
    kdf_standard: "Standard — 64 MiB, unlocks in about half a second",
    kdf_strong: "Strong — 256 MiB, about two seconds",
    kdf_very_strong: "Very strong — 1 GiB, several seconds; needs a recent computer",
    test_recovery_title: "Test the recovery key",
    test_recovery_hint: "Type the recovery key from your printout or file. Nothing is changed; this only checks that the key still works.",
    test_recovery_ok: "The recovery key works. Keep it in its safe place.",
    test_recovery_is_passphrase: "That is the passphrase, not the recovery key. It works as well, but please check the recovery key itself.",
    test_recovery_wrong: "This recovery key does not unlock these backups. If it was replaced, use the newer key.",
    test_now: "Test",

    nav_backups: "Backups",
    backups_title: "Backups at the destination",
    backups_select_hint: "Tick one backup to browse, check, copy or move it; tick several to delete them.",
    some_backups_locked: "Some backups are encrypted. Unlock them to see and manage all details.",
    partly_locked: "details locked",
    partly_encrypted_label: "partly encrypted",
    browse: "Browse…",
    check_backup: "Check",
    copy_files_to: "Copy files to…",
    move_to: "Move to…",
    move_backup: "Move",
    open_folder: "Open folder",
    delete_backup: "Delete…",
    retention_title: "Keeping old backups",
    retention_auto: "Remove old backups automatically",
    retention_hint: "After each backup. Only backups of this computer are considered, and the newest backup always stays.",
    retention_keep: "Keep",
    retention_newest: "newest,",
    retention_days: "days,",
    retention_weeks: "weeks,",
    retention_months: "months",
    clean_up_now: "Remove now…",
    open: "Open",
    open_copy_hint: "Opens a copy from a temporary folder; the backup stays unchanged. The copy is removed the next time AeternaVault starts.",
    copy_all_to: "Copy all to…",
    copies_are_decrypted: "The copies are stored decrypted in the chosen folder.",
    working_verify: "Checking the backup",
    working_delete: "Deleting",
    working_transfer: "Moving the backup",
    working_extract: "Copying files",
    done_verify_ok: "The backup is intact.",
    done_verify_problems: "The check found problems.",
    done_verify_cancelled: "The check was cancelled.",
    damaged_label: "damaged",
    missing_label: "missing",
    done_delete: "The backups were deleted.",
    done_transfer: "The backup was moved.",
    done_extract: "The files were copied.",
    done_extract_notes: "The files were copied, with a few notes.",
    confirm_delete_title: "Delete backups?",
    confirm_delete_plain: "Backup folders go to the recycle bin where the drive has one; otherwise they are deleted permanently. Newer backups that still need files from them get their own copies first.",
    confirm_delete_encrypted: "Encrypted data that no other backup uses is deleted permanently.",
    confirm_transfer_title: "Move the backup?",
    confirm_extract_title: "Copy files?",

    encrypt_item: "Encrypt",
    scope_encrypt_everything: "Encrypt everything",
    scope_encrypt_selected: "Encrypt only marked folders and files",
    scope_selected_hint: "Click the lock next to a folder or file (open a folder with the arrow to see its contents). The rest stays a normal, browsable backup.",
    scope_nothing_marked: "Nothing is marked yet, so nothing is encrypted.",
    encrypt_app_settings: "Also encrypt application settings (recommended: they can contain sign-ins)",
    enc_status_selected: "Marked folders and files are encrypted; everything else is backed up normally.",
    test_recovery: "Test recovery key…",
    replace_recovery: "New recovery key…",
    replace_recovery_hint: "Creates a new recovery key. The old one stops working, the passphrase stays.",
    open_by_double_click: "Open encrypted backups by double-click",
    open_by_double_click_hint: "Double-clicking “Open with AeternaVault.avault” in a backup folder asks for the passphrase and shows the files.",
    enc_info_title: "How the backups are encrypted",
    enc_info_cipher: "Encryption",
    enc_info_passphrase: "Passphrase",
    enc_info_hidden: "Hidden",
    enc_info_hidden_value: "file names, folders, contents and sizes of single files",
    enc_info_key: "Key on this computer",
    enc_info_key_remembered: "remembered, protected by your Windows account (DPAPI)",
    enc_info_key_not_remembered: "not stored; only in memory while unlocked",
    enc_info_location: "Stored in",
    enc_without_app_title: "Getting at the files without AeternaVault",
    enc_without_app_text: "Nothing depends on this installation: copy the whole folder anywhere, and a downloaded AeternaVault.exe (no installation needed) opens it with the passphrase or the recovery key — by double-click on “Open with AeternaVault.avault”, or on the command line:",
    enc_without_app_script: "The format is openly documented, and a short independent Python script can decrypt it as well, without AeternaVault.",
    passphrase_empty: "Please enter a passphrase.",
    back_arrow: "← Back",
    close_viewer: "Close",
    vault_missing: "The encrypted vault is no longer at the destination (the folder may have been deleted or moved). Set up encryption again to continue, or turn encryption off.",
    nav_jobs: "Backup jobs",
    jobs_title: "Automatic backups",
    open_settings: "Settings",
    change_in_settings: "Change in Settings",
    scope_selected_short: "Folders and files with a lock are encrypted.",
    make_automatic: "Repeat automatically…",
    make_automatic_hint: "Creates a backup job that backs up the same folders and application settings on a schedule.",
    enc_status_off_settings: "Backups are not encrypted. The switch is in the Backup view; everything below applies once it is on.",
    enc_scope_title: "What is encrypted",
    background_title: "Startup and background",
    start_with_windows_hint: "AeternaVault stays listed in Windows’ startup apps; this switch turns it on or off there.",
    keep_running_hint: "Closing the window leaves AeternaVault in the notification area, so backup jobs keep running. Quit from the icon’s menu.",
    only_ac_hint: "On a laptop running on battery, backup jobs wait until it is plugged in. “Run now” always runs.",
    forecast_title: "When backups will be removed",
    forecast_assumes_jobs: "Estimated from the switched-on backup jobs.",
    forecast_assumes_daily: "No backup job is on, so one backup per day is assumed.",
    forecast_empty: "There are no complete backups of this computer yet.",
    removed_next_backup: "removed after the next backup",
    kept_long: "kept for a long time (beyond the forecast)",
    show_technical_log: "Technical log",
    show_history: "History",
    history_hint: "Everything AeternaVault did on this computer, also in earlier sessions.",
    technical_log_hint: "Detailed messages of this session. Older ones are in the log folder.",
    history_empty: "Nothing has happened yet.",
    filter_activity: "Search…",
    explorer_menu: "Show “Back up with AeternaVault” when right-clicking a folder in Explorer",
    explorer_menu_hint: "The folder is added to the list of folders to back up. On Windows 11 the entry is under “Show more options”.",
    explorer_menu_label: "Back up with AeternaVault",
    verify_after: "Check the backup afterwards",
    verify_after_hint: "Reads every file of the new backup again and compares it with its checksum. Takes about as long as reading the backup.",
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
    strength: ["Schwach", "Mittel", "Gut"],
    passphrases_differ: "Die beiden Passphrasen unterscheiden sich.",
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
    change_passphrase: "Passphrase ändern…",
    lock_now: "Jetzt sperren",
    encrypted_label: "verschlüsselt",
    locked_backup: "Verschlüsselt – entsperren, um Details zu sehen",

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
    schedule_only_while_running: "Backup-Jobs laufen nur, solange AeternaVault läuft. Schalte in den Einstellungen „mit Windows starten“ und „im Infobereich weiterlaufen“ ein, damit sie auch nach dem Schließen und nach einem Neustart laufen.",
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

    enc_method_title: "Verschlüsselungsverfahren",
    recommended_suffix: " (empfohlen)",
    kdf_title: "Schutz der Passphrase gegen Erraten",
    kdf_standard: "Standard – 64 MiB, entsperrt in etwa einer halben Sekunde",
    kdf_strong: "Stark – 256 MiB, etwa zwei Sekunden",
    kdf_very_strong: "Sehr stark – 1 GiB, mehrere Sekunden; braucht einen neueren Computer",
    test_recovery_title: "Wiederherstellungsschlüssel testen",
    test_recovery_hint: "Tippe den Wiederherstellungsschlüssel von deinem Ausdruck oder aus der Datei ab. Es wird nichts verändert; hier wird nur geprüft, ob der Schlüssel noch funktioniert.",
    test_recovery_ok: "Der Wiederherstellungsschlüssel funktioniert. Bewahre ihn weiter sicher auf.",
    test_recovery_is_passphrase: "Das ist die Passphrase, nicht der Wiederherstellungsschlüssel. Sie funktioniert auch, aber bitte prüfe den Wiederherstellungsschlüssel selbst.",
    test_recovery_wrong: "Dieser Wiederherstellungsschlüssel entsperrt diese Sicherungen nicht. Falls er ersetzt wurde, nimm den neueren Schlüssel.",
    test_now: "Testen",

    nav_backups: "Sicherungen",
    backups_title: "Sicherungen am Ziel",
    backups_select_hint: "Eine Sicherung ankreuzen, um sie zu durchsuchen, zu prüfen, zu kopieren oder zu verschieben; mehrere ankreuzen, um sie zu löschen.",
    some_backups_locked: "Einige Sicherungen sind verschlüsselt. Entsperre sie, um alle Details zu sehen und sie zu verwalten.",
    partly_locked: "Details gesperrt",
    partly_encrypted_label: "teilweise verschlüsselt",
    browse: "Durchsuchen…",
    check_backup: "Prüfen",
    copy_files_to: "Dateien kopieren nach…",
    move_to: "Verschieben nach…",
    move_backup: "Verschieben",
    open_folder: "Ordner öffnen",
    delete_backup: "Löschen…",
    retention_title: "Alte Sicherungen aufbewahren",
    retention_auto: "Alte Sicherungen automatisch entfernen",
    retention_hint: "Nach jeder Sicherung. Berücksichtigt werden nur Sicherungen dieses Computers, und die neueste bleibt immer erhalten.",
    retention_keep: "Behalten:",
    retention_newest: "neueste,",
    retention_days: "Tage,",
    retention_weeks: "Wochen,",
    retention_months: "Monate",
    clean_up_now: "Jetzt entfernen…",
    open: "Öffnen",
    open_copy_hint: "Öffnet eine Kopie aus einem temporären Ordner; die Sicherung bleibt unverändert. Die Kopie wird beim nächsten Start von AeternaVault entfernt.",
    copy_all_to: "Alles kopieren nach…",
    copies_are_decrypted: "Die Kopien liegen entschlüsselt im gewählten Ordner.",
    working_verify: "Sicherung wird geprüft",
    working_delete: "Wird gelöscht",
    working_transfer: "Sicherung wird verschoben",
    working_extract: "Dateien werden kopiert",
    done_verify_ok: "Die Sicherung ist unversehrt.",
    done_verify_problems: "Die Prüfung hat Probleme gefunden.",
    done_verify_cancelled: "Die Prüfung wurde abgebrochen.",
    damaged_label: "beschädigt",
    missing_label: "fehlt",
    done_delete: "Die Sicherungen wurden gelöscht.",
    done_transfer: "Die Sicherung wurde verschoben.",
    done_extract: "Die Dateien wurden kopiert.",
    done_extract_notes: "Die Dateien wurden kopiert, mit einigen Hinweisen.",
    confirm_delete_title: "Sicherungen löschen?",
    confirm_delete_plain: "Sicherungsordner kommen in den Papierkorb, wenn das Laufwerk einen hat; sonst werden sie endgültig gelöscht. Neuere Sicherungen, die noch Dateien daraus brauchen, bekommen vorher eigene Kopien.",
    confirm_delete_encrypted: "Verschlüsselte Daten, die keine andere Sicherung mehr nutzt, werden endgültig gelöscht.",
    confirm_transfer_title: "Sicherung verschieben?",
    confirm_extract_title: "Dateien kopieren?",

    encrypt_item: "Verschlüsseln",
    scope_encrypt_everything: "Alles verschlüsseln",
    scope_encrypt_selected: "Nur markierte Ordner und Dateien verschlüsseln",
    scope_selected_hint: "Klicke auf das Schloss neben einem Ordner oder einer Datei (mit dem Pfeil siehst du den Inhalt eines Ordners). Der Rest bleibt eine normale, durchsuchbare Sicherung.",
    scope_nothing_marked: "Noch nichts markiert, daher wird nichts verschlüsselt.",
    encrypt_app_settings: "Auch Anwendungseinstellungen verschlüsseln (empfohlen: sie können Anmeldungen enthalten)",
    enc_status_selected: "Markierte Ordner und Dateien werden verschlüsselt; alles andere wird normal gesichert.",
    test_recovery: "Wiederherstellungsschlüssel testen…",
    replace_recovery: "Neuer Wiederherstellungsschlüssel…",
    replace_recovery_hint: "Erstellt einen neuen Wiederherstellungsschlüssel. Der alte funktioniert dann nicht mehr, die Passphrase bleibt.",
    open_by_double_click: "Verschlüsselte Sicherungen per Doppelklick öffnen",
    open_by_double_click_hint: "Ein Doppelklick auf „Open with AeternaVault.avault“ in einem Sicherungsordner fragt nach der Passphrase und zeigt die Dateien.",
    enc_info_title: "So werden die Sicherungen verschlüsselt",
    enc_info_cipher: "Verschlüsselung",
    enc_info_passphrase: "Passphrase",
    enc_info_hidden: "Verborgen",
    enc_info_hidden_value: "Dateinamen, Ordner, Inhalte und Größen einzelner Dateien",
    enc_info_key: "Schlüssel auf diesem Computer",
    enc_info_key_remembered: "gemerkt, geschützt durch dein Windows-Konto (DPAPI)",
    enc_info_key_not_remembered: "nicht gespeichert; nur im Arbeitsspeicher, solange entsperrt",
    enc_info_location: "Gespeichert in",
    enc_without_app_title: "An die Dateien kommen ohne AeternaVault",
    enc_without_app_text: "Nichts hängt von dieser Installation ab: Kopiere den ganzen Ordner irgendwohin, und eine heruntergeladene AeternaVault.exe (ohne Installation) öffnet ihn mit der Passphrase oder dem Wiederherstellungsschlüssel – per Doppelklick auf „Open with AeternaVault.avault“ oder auf der Kommandozeile:",
    enc_without_app_script: "Das Format ist offen dokumentiert, und ein kurzes, unabhängiges Python-Skript kann es ebenfalls entschlüsseln – ganz ohne AeternaVault.",
    passphrase_empty: "Bitte eine Passphrase eingeben.",
    back_arrow: "← Zurück",
    close_viewer: "Schließen",
    vault_missing: "Der verschlüsselte Tresor ist nicht mehr am Ziel (der Ordner wurde vielleicht gelöscht oder verschoben). Richte die Verschlüsselung neu ein, um fortzufahren, oder schalte sie aus.",
    nav_jobs: "Backup-Jobs",
    jobs_title: "Automatische Sicherungen",
    open_settings: "Einstellungen",
    change_in_settings: "In den Einstellungen ändern",
    scope_selected_short: "Ordner und Dateien mit Schloss werden verschlüsselt.",
    make_automatic: "Automatisch wiederholen…",
    make_automatic_hint: "Legt einen Backup-Job an, der dieselben Ordner und Anwendungseinstellungen nach Zeitplan sichert.",
    enc_status_off_settings: "Sicherungen werden nicht verschlüsselt. Der Schalter ist in der Ansicht „Sichern“; alles hier gilt, sobald er an ist.",
    enc_scope_title: "Was verschlüsselt wird",
    background_title: "Start und Hintergrund",
    start_with_windows_hint: "AeternaVault bleibt in den Windows-Autostart-Apps eingetragen; dieser Haken schaltet den Eintrag dort an oder aus.",
    keep_running_hint: "Beim Schließen bleibt AeternaVault im Infobereich, damit Backup-Jobs weiterlaufen. Beenden über das Menü des Symbols.",
    only_ac_hint: "Läuft ein Laptop auf Akku, warten Backup-Jobs, bis das Netzteil angeschlossen ist. „Jetzt ausführen“ läuft immer.",
    forecast_title: "Wann Sicherungen entfernt werden",
    forecast_assumes_jobs: "Geschätzt anhand der eingeschalteten Backup-Jobs.",
    forecast_assumes_daily: "Kein Backup-Job ist eingeschaltet, daher wird eine Sicherung pro Tag angenommen.",
    forecast_empty: "Es gibt noch keine vollständigen Sicherungen dieses Computers.",
    removed_next_backup: "wird nach der nächsten Sicherung entfernt",
    kept_long: "bleibt lange erhalten (über die Vorschau hinaus)",
    show_technical_log: "Technisches Protokoll",
    show_history: "Verlauf",
    history_hint: "Alles, was AeternaVault auf diesem Computer getan hat, auch in früheren Sitzungen.",
    technical_log_hint: "Ausführliche Meldungen dieser Sitzung. Ältere stehen im Log-Ordner.",
    history_empty: "Noch ist nichts passiert.",
    filter_activity: "Suchen…",
    explorer_menu: "„Mit AeternaVault sichern“ im Explorer-Kontextmenü von Ordnern anzeigen",
    explorer_menu_hint: "Der Ordner wird zur Liste der zu sichernden Ordner hinzugefügt. Unter Windows 11 steht der Eintrag unter „Weitere Optionen anzeigen“.",
    explorer_menu_label: "Mit AeternaVault sichern",
    verify_after: "Sicherung danach prüfen",
    verify_after_hint: "Liest jede Datei der neuen Sicherung erneut und vergleicht sie mit ihrer Prüfsumme. Dauert etwa so lange wie das Lesen der Sicherung.",
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
