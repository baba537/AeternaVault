# Command line

`AeternaVault.exe` is both the window and a command-line tool. Without a command,
the window opens; with one of the commands below, AeternaVault runs in the
console and exits when it is done. The command line uses the same
configuration, backups and remembered keys as the window, and always answers in
English.

```text
AeternaVault.exe [--background] [COMMAND]

Commands:
  backup     Back up all enabled folders and applications
  snapshots  List the backups at the destination
  restore    Restore a backup
  open       Browse backups in a window, without changing any settings
  add        Add a folder to the folders to back up and show the window
  paths      Show where configuration and logs are stored
  help       Show help for a command
```

`AeternaVault.exe --help` and `AeternaVault.exe <command> --help` print the same
information.

## Running it from PowerShell or cmd

The release executable is a Windows GUI program, so that no console window flashes
up when the window starts. Started from a console with a command, it attaches to
that console and prints there — but the shell does not wait for it and reports no
exit code. For scripts, wait for it explicitly:

```powershell
# PowerShell: output in the console, exit code in $p.ExitCode
$p = Start-Process AeternaVault.exe -ArgumentList 'backup' -NoNewWindow -Wait -PassThru
$p.ExitCode
```

```bat
:: cmd
start "" /wait AeternaVault.exe backup
echo %ERRORLEVEL%
```

## `backup`

Backs up with the saved settings: the ticked folders (including their chosen
sub-folders and files), the chosen application settings, the destination,
the kind of backup and encryption, exactly as set up in the window.

```powershell
AeternaVault.exe backup                        # back up now
AeternaVault.exe backup --dry-run              # only show what would be copied
AeternaVault.exe backup --dry-run --csv plan.csv   # the full plan as a CSV file
AeternaVault.exe backup --full                 # copy every file again this time
AeternaVault.exe backup --scheduled            # quiet, low priority, recorded like an automatic backup
```

| Option | Meaning |
|---|---|
| `--dry-run` | Plan only. Nothing is written. |
| `--full` | Copy every file instead of only new and changed ones (for this run). |
| `--csv FILE` | Write the plan (every file and what happens to it) to a CSV file. |
| `--scheduled` | For your own scheduler (for example the Windows Task Scheduler): runs with low priority, never asks anything, skips quietly if the destination is not connected, removes old backups if the retention rules are on, and records the result so the window shows it next time. |

The summary shows how many files are new, changed, unchanged or left out; the
last line names the new backup folder.

## `snapshots`

Lists the backups at the configured destination, or in any other folder.

```powershell
AeternaVault.exe snapshots
AeternaVault.exe snapshots --destination E:\AeternaVault
```

Each line shows the backup name (use it with `restore`), its status, the number
of files and the size. Encrypted backups are only described in detail when they
can be unlocked (see [Encrypted backups](#encrypted-backups)); otherwise they are
listed as `encrypted (locked)`.

## `restore`

Restores a backup — `latest` or a name as printed by `snapshots`.

```powershell
# See what would happen first
AeternaVault.exe restore latest --to D:\Restored --dry-run

# Restore into a separate folder
AeternaVault.exe restore latest --to D:\Restored --yes

# Back to the original places, never overwriting existing files
AeternaVault.exe restore "2026-09-16 20-00" --conflict keep-existing --yes

# Backups copied from another computer or an old drive
AeternaVault.exe restore latest --destination E:\AeternaVault --to D:\Restored --yes
```

| Option | Meaning |
|---|---|
| `--to DIR` | Restore into this folder instead of the original locations. |
| `--destination DIR` | Read the backups from this folder instead of the configured destination. |
| `--dry-run` | Plan only. Nothing is written. |
| `--yes` | Required to actually write files. Without it, the plan is shown and the exit code is `2`. |
| `--conflict POLICY` | What to do with files that already exist: `replace-changed` (default) replaces files that differ, `keep-existing` never touches existing files, `keep-newer` replaces only files that are older than the backed-up copy. |

`latest` means the newest usable backup of this computer; if this computer has
none (for example on a new PC), the newest usable backup of any computer.
Checksums are verified while restoring if that is switched on in the settings
(it is by default).

Application settings stored in the backup are restored as well, including their
registry values. Close the applications first.

## `open`

Opens a window that shows only the backups at the given path — a backup folder,
or the `Open with AeternaVault.avault` file inside an encrypted vault — and asks
for the passphrase if needed. Nothing in your own configuration is changed. This
is what a double-click on `Open with AeternaVault.avault` runs.

```powershell
AeternaVault.exe open E:\AeternaVault
AeternaVault.exe open "E:\AeternaVault\AeternaVault Encrypted\Open with AeternaVault.avault"
```

## `add`

Adds a folder to the list of folders to back up and shows the window (or brings
the running one to the front, which then shows the folder). This is what "Back up
with AeternaVault" in the Explorer menu of folders runs; that menu entry can be
switched on in *Settings → Startup and background*.

```powershell
AeternaVault.exe add D:\Projects
```

## `paths`

```text
> AeternaVault.exe paths
configuration: C:\Users\you\AppData\Roaming\AeternaVault\config.toml
logs:          C:\Users\you\AppData\Local\AeternaVault\logs
portable:      false
```

## `--background`

Starts AeternaVault without a window, in the notification area, where it runs the
backup jobs. This is what "Start AeternaVault quietly when I sign in to Windows"
uses. If AeternaVault is already running, nothing happens; starting it again
without `--background` brings the existing window to the front.

## Encrypted backups

For encrypted backups, the key is taken from, in this order:

1. the key remembered on this computer ("Remember on this computer" in the
   window; protected by your Windows account),
2. the environment variable `AETERNAVAULT_PASSPHRASE`,
3. a prompt in the console (`snapshots` and `restore` only; typing is hidden,
   three attempts). The recovery key works here as well.

`backup` never prompts: without a remembered key or `AETERNAVAULT_PASSPHRASE`,
it stops with an error (or, with `--scheduled`, records that the passphrase is
needed).

```powershell
$env:AETERNAVAULT_PASSPHRASE = Read-Host -AsSecureString "Passphrase" |
    ForEach-Object { [Runtime.InteropServices.Marshal]::PtrToStringUni(
        [Runtime.InteropServices.Marshal]::SecureStringToGlobalAllocUnicode($_)) }
AeternaVault.exe restore latest --destination E:\AeternaVault --to D:\Restored --yes
Remove-Item Env:AETERNAVAULT_PASSPHRASE
```

Avoid putting the passphrase itself into scripts or scheduled tasks.

The encrypted format is documented byte by byte in [ENCRYPTION.md](ENCRYPTION.md);
[`tools/aeterna-decrypt.py`](../tools/aeterna-decrypt.py) decrypts it without
AeternaVault.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success (also: `--scheduled` skipped because the destination is not connected). |
| `1` | Error — the reason is printed and written to the log. |
| `2` | Completed with notes (for example files that could not be read), or `restore` without `--yes`. |

## Environment variables

| Variable | Effect |
|---|---|
| `AETERNAVAULT_PASSPHRASE` | Passphrase or recovery key for encrypted backups. |
| `AETERNAVAULT_HOME` | Keep configuration, state, history and logs in this folder instead of `%APPDATA%` / `%LOCALAPPDATA%`. |
| `AETERNAVAULT_LOG` | Log detail: `debug`, `trace` or `warn` (default: `info`). |
| `AETERNAVAULT_RENDERER` | `glow` draws the window with OpenGL instead of Direct3D 12. |

A file named `AeternaVault.toml` next to the executable switches to portable mode:
that file is the configuration, and logs go to a `logs` folder beside it.

## Activity history

Backups and restores started from the command line appear in the window's
Activity view, together with everything done in the window and by backup jobs.
The history is stored as one JSON object per line in `history.jsonl` next to the
configuration file.
