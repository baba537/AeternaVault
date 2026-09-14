//! Automatic backups via the Windows Task Scheduler.
//!
//! Why the Task Scheduler instead of an autostart entry with a program that
//! keeps running in the background:
//! * nothing runs between backups — no memory use, no tray icon,
//! * missed backups (computer was off) are caught up automatically,
//! * conditions such as "only on mains power" come for free,
//! * no administrator rights are needed for a task of the current user.
//!
//! The task runs `AeternaVault.exe backup --scheduled`, which shows no window,
//! lowers its priority and records the result for the next app start.

use std::io;
use std::path::Path;
use std::process::Command;

use chrono::{Local, NaiveTime};

use crate::config::{Frequency, Schedule, Weekday};

pub const TASK_FOLDER: &str = "AeternaVault";
pub const TASK_NAME: &str = "Automatic backup";

pub fn task_path() -> String {
    format!("\\{TASK_FOLDER}\\{TASK_NAME}")
}

/// Builds the Task Scheduler XML definition.
pub fn task_xml(schedule: &Schedule, exe: &Path, user: &str) -> String {
    let time = NaiveTime::parse_from_str(&schedule.time, "%H:%M")
        .unwrap_or_else(|_| NaiveTime::from_hms_opt(20, 0, 0).unwrap_or_default());
    let start = Local::now()
        .date_naive()
        .and_time(time)
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    let trigger = match schedule.frequency {
        Frequency::Daily => format!(
            "<CalendarTrigger><StartBoundary>{start}</StartBoundary><Enabled>true</Enabled>\
             <ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay></CalendarTrigger>"
        ),
        Frequency::Weekly => format!(
            "<CalendarTrigger><StartBoundary>{start}</StartBoundary><Enabled>true</Enabled>\
             <ScheduleByWeek><DaysOfWeek><{day} /></DaysOfWeek><WeeksInterval>1</WeeksInterval>\
             </ScheduleByWeek></CalendarTrigger>",
            day = weekday_element(schedule.weekday)
        ),
        Frequency::Hourly => format!(
            "<TimeTrigger><Repetition><Interval>PT{hours}H</Interval>\
             <StopAtDurationEnd>false</StopAtDurationEnd></Repetition>\
             <StartBoundary>{start}</StartBoundary><Enabled>true</Enabled></TimeTrigger>",
            hours = schedule.every_hours.clamp(1, 23)
        ),
        Frequency::AtLogon => format!(
            "<LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT5M</Delay></LogonTrigger>",
            user = xml_escape(user)
        ),
    };

    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Author>AeternaVault</Author>
    <Description>Automatic backup by AeternaVault. Change or turn off in AeternaVault.</Description>
    <URI>{uri}</URI>
  </RegistrationInfo>
  <Triggers>
    {trigger}
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>{on_ac}</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>{catch_up}</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT12H</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>backup --scheduled</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        uri = xml_escape(&task_path()),
        user = xml_escape(user),
        on_ac = schedule.only_on_ac_power,
        catch_up = schedule.catch_up,
        exe = xml_escape(&exe.display().to_string()),
    )
}

fn weekday_element(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn current_user() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
        _ => user,
    }
}

fn schtasks() -> Command {
    let mut command = Command::new("schtasks.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn run(command: &mut Command) -> io::Result<()> {
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(io::Error::other(if text.is_empty() {
            format!("schtasks exited with {}", output.status)
        } else {
            text
        }))
    }
}

/// Creates or replaces the task.
pub fn install(schedule: &Schedule, exe: &Path, work_dir: &Path) -> io::Result<()> {
    let xml = task_xml(schedule, exe, &current_user());
    std::fs::create_dir_all(work_dir)?;
    let file = work_dir.join("scheduled-task.xml");
    // schtasks expects UTF-16 with a byte order mark.
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(xml.encode_utf16().flat_map(|u| u.to_le_bytes()));
    std::fs::write(&file, bytes)?;
    let result = run(schtasks()
        .args(["/Create", "/F", "/TN"])
        .arg(task_path())
        .arg("/XML")
        .arg(&file));
    let _ = std::fs::remove_file(&file);
    result?;
    tracing::info!("automatic backup task installed ({:?})", schedule.frequency);
    Ok(())
}

/// Removes the task; succeeds if it does not exist.
pub fn remove() -> io::Result<()> {
    if !is_installed() {
        return Ok(());
    }
    run(schtasks().args(["/Delete", "/F", "/TN"]).arg(task_path()))?;
    tracing::info!("automatic backup task removed");
    Ok(())
}

pub fn is_installed() -> bool {
    schtasks()
        .args(["/Query", "/TN"])
        .arg(task_path())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Approximate next run time for display (the Task Scheduler is authoritative).
pub fn next_run(
    schedule: &Schedule,
    now: chrono::DateTime<Local>,
) -> Option<chrono::DateTime<Local>> {
    let time = NaiveTime::parse_from_str(&schedule.time, "%H:%M").ok()?;
    match schedule.frequency {
        Frequency::AtLogon => None,
        Frequency::Hourly => {
            let hours = i64::from(schedule.every_hours.clamp(1, 23));
            let anchor = now
                .date_naive()
                .and_time(time)
                .and_local_timezone(Local)
                .single()?;
            let mut next = anchor;
            while next <= now {
                next += chrono::Duration::hours(hours);
            }
            while next - chrono::Duration::hours(hours) > now {
                next -= chrono::Duration::hours(hours);
            }
            Some(next)
        }
        Frequency::Daily | Frequency::Weekly => {
            use chrono::Datelike;
            for offset in 0..8 {
                let date = now.date_naive() + chrono::Duration::days(offset);
                if schedule.frequency == Frequency::Weekly
                    && date.weekday().num_days_from_monday() != schedule.weekday as u32
                {
                    continue;
                }
                let candidate = date.and_time(time).and_local_timezone(Local).single()?;
                if candidate > now {
                    return Some(candidate);
                }
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_contains_the_chosen_trigger() {
        let mut schedule = Schedule {
            enabled: true,
            frequency: Frequency::Weekly,
            weekday: Weekday::Sunday,
            time: "19:30".into(),
            ..Schedule::default()
        };
        let xml = task_xml(
            &schedule,
            Path::new(r"C:\Tools & More\AeternaVault.exe"),
            r"PC\anna",
        );
        assert!(xml.contains("<Sunday />"));
        assert!(xml.contains("T19:30:00"));
        assert!(xml.contains(r"C:\Tools &amp; More\AeternaVault.exe"));
        assert!(xml.contains("<Arguments>backup --scheduled</Arguments>"));

        schedule.frequency = Frequency::AtLogon;
        let xml = task_xml(&schedule, Path::new("a.exe"), r"PC\anna");
        assert!(xml.contains("<LogonTrigger>"));
    }

    #[test]
    fn next_run_daily() {
        let schedule = Schedule {
            enabled: true,
            frequency: Frequency::Daily,
            time: "20:00".into(),
            ..Schedule::default()
        };
        let now = Local::now();
        let next = next_run(&schedule, now).unwrap();
        assert!(next > now && next - now <= chrono::Duration::days(1));
    }
}
