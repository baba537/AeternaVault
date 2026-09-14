# Design

> Entering a well-kept archive, not launching a game.

This is a starting point for AeternaVault's visual identity. It is meant to be
refined — every value below lives in `src/gui/theme.rs` and `src/gui/widgets.rs`
and can be changed in one place.

## Principles

1. **Calm.** One accent color, generous spacing, no motion that asks for attention.
2. **Enduring.** Serif headings and warm neutrals, like labels in a museum.
3. **Clear.** Every screen answers: *what* is kept, *where*, *when*, and *how it went*.
4. **Honest.** Previews are marked as previews; nothing happens without confirmation.

## Name and mark

- *aeterna* (Latin: eternal) + *vault* — data kept safely for the long term.
- **Slogan:** "Your data, kept for eternity." / „Deine Daten – für die Ewigkeit bewahrt."
- **Mark:** a vault door seen from the front — outer ring, dial ticks, and an
  infinity sign as the handle. Drawn with lines only; gold on anthracite.
- The app draws the mark as vector shapes (`widgets::logo`); the Windows icon is
  rendered by `tools/make-icon.ps1` into `assets/icon/`.

## Color

### Dark (default when Windows is dark)

| Token | Hex | Use |
|---|---|---|
| background | `#14161A` | window ground |
| panel | `#1C1F24` | cards |
| raised | `#252930` | inputs, hover, selected rows |
| border | `#2E3239` | hairlines around cards |
| text | `#EDE6D6` | primary text (ivory) |
| text secondary | `#9A958A` | paths, hints, labels |
| accent | `#C9A227` | the one main button, active tab, focus |
| accent hover | `#E0B93A` | hover on accent |
| success | `#8BA877` | completed, "new" (text-safe variant of `#6E8B5A`) |
| warning | `#D19A3E` | notes, skipped (text-safe variant of `#C08A2E`) |
| error | `#C7675A` | failures (text-safe variant of `#A84A3C`) |

### Light

| Token | Hex |
|---|---|
| background | `#F6F1E7` |
| panel | `#FBF8F1` |
| raised | `#FFFDF8` |
| border | `#E3DACA` |
| text | `#1C1F24` |
| text secondary | `#6E6A60` |
| accent | `#8C6A1F` |
| accent hover | `#B08A2E` |
| success | `#55703F` |
| warning | `#94661B` |
| error | `#9A3F32` |

Status colors are slightly lighter (dark) or darker (light) than the original
fill tones so they stay readable as small text. Gold is used sparingly: one
primary button per screen, the active navigation tab, and the short rule under
section titles.

## Typography

| Role | Face | Size |
|---|---|---|
| App title | Georgia (serif) | 28 |
| Screen heading | Georgia | 24–26 |
| Slogan | Georgia Italic | 14 |
| Section label | Segoe UI Semibold, UPPERCASE, +1.4 letter spacing | 11.5 |
| Body, buttons | Segoe UI / Segoe UI Semibold | 15 |
| Secondary | Segoe UI | 12.5–13.5 |
| Paths in settings, log | monospace | 13.5 |

Georgia and Segoe UI ship with every Windows installation, so nothing is bundled.
**Optional:** set `[fonts] heading` / `body` in `config.toml` to use EB Garamond,
Lora, Inter or Source Sans (all under the SIL Open Font License). Bundling them in
the executable would add ~1 MB and require shipping their licence texts.

## Layout

```text
┌──────────────────────────────────────────────────────────────────┐
│  ◎  AeternaVault                                  Deutsch   ◐    │
│     Your data, kept for eternity.                                │
│  Backup   Restore   Applications   Settings   Activity           │
│  ‾‾‾‾‾‾                                                          │
├──────────────────────────────────────────────────────────────────┤
│   ┌────────────────────────────────────────────────────────┐     │
│   │ WHAT IS KEPT SAFE                                      │     │
│   │ ─                                                      │     │
│   │ ☑ Documents                                  12.4 GB × │     │
│   │   C:\Users\Anna\Documents                              │     │
│   │ ☐ Downloads                                   8.7 GB × │     │
│   │ [Add folder…]  Suggestions from applications           │     │
│   └────────────────────────────────────────────────────────┘     │
│   ┌────────────────────────────────────────────────────────┐     │
│   │ WHERE IT IS KEPT                                       │     │
│   │ D:\AeternaVault\Backups                    [Choose…]   │     │
│   │ 812 GB free                                            │     │
│   │ KIND OF BACKUP                                         │     │
│   │ ● Incremental   ○ Full                                 │     │
│   └────────────────────────────────────────────────────────┘     │
│   ┌────────────────────────────────────────────────────────┐     │
│   │ LAST BACKUP   ● 2 days ago, 14:32 — completed          │     │
│   └────────────────────────────────────────────────────────┘     │
│                      Restore…   [ Preview ]  [▓ Back up now ▓]   │
│  AeternaVault 0.1.0                                   DESKTOP-1  │
└──────────────────────────────────────────────────────────────────┘
```

- Content column: max. 860 px, centered. The preview uses up to 1100 px.
- Cards: 6 px radius, 1 px border, 22 × 18 px padding, 14 px apart.
- Buttons: 4 px radius, 36 px high. Primary = gold fill; secondary = outline;
  quiet = text only.
- Rows: 48 px for sources, 26 px in the preview table (virtualised).

## Motion

- 0.18 s fades for hover and collapsing sections. No bounce, no scaling.
- Scanning shows a single gold segment travelling slowly (2.6 s per pass).

## Iconography

- Line-based, drawn with the painter: the vault mark, status dots, the
  half-filled circle for appearance. No emoji, no cartoon icons.
- Future icons should use 1.3–1.6 px strokes with round caps.

## Voice

| Instead of | Write |
|---|---|
| "Backup failed!" | "The backup could not be completed." |
| "ERROR: Access denied (os error 5)" | "No access" (details in the notes) |
| "Delete old snapshots?" | "Remove backups older than one year?" |

- Sentences end with a period, never an exclamation mark.
- Say what happened and what the person can do next.
- German uses the informal *du*, matching the friendly tone.

## Accessibility

- Primary text reaches > 12:1 contrast in both themes; secondary text > 4.5:1.
- Everything is reachable with the keyboard through egui's focus handling;
  AccessKit exposes widgets to screen readers.
- Status is never shown by color alone — every dot has a text label.
