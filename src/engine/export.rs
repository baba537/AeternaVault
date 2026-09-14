//! Exporting a preview as plain text or CSV.
//!
//! Labels are passed in already translated, so the engine stays language-neutral.

use std::io::{self, Write};
use std::path::Path;

use super::plan::{ItemKind, Plan, PlanItem};

pub struct ExportLabels<'a> {
    pub title: &'a str,
    pub kind: &'a dyn Fn(ItemKind) -> String,
    pub note: &'a dyn Fn(&PlanItem) -> String,
    pub columns: [&'a str; 5],
}

pub fn write_csv(plan: &dyn Plan, labels: &ExportLabels<'_>, path: &Path) -> io::Result<()> {
    let mut out = io::BufWriter::new(std::fs::File::create(path)?);
    // UTF-8 byte order mark: lets Excel detect the encoding (umlauts etc.).
    out.write_all(b"\xEF\xBB\xBF")?;
    writeln!(out, "{}", labels.columns.map(csv_field).join(","))?;
    for item in plan.items() {
        let row = [
            (labels.kind)(item.kind),
            plan.sources()[item.source].name.clone(),
            plan.item_path(item).display().to_string(),
            item.size.to_string(),
            (labels.note)(item),
        ];
        writeln!(
            out,
            "{}",
            row.iter()
                .map(|s| csv_field(s))
                .collect::<Vec<_>>()
                .join(",")
        )?;
    }
    out.flush()
}

pub fn write_text(
    plan: &dyn Plan,
    labels: &ExportLabels<'_>,
    summary: &[String],
    path: &Path,
) -> io::Result<()> {
    let mut out = io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(out, "{}", labels.title)?;
    writeln!(out, "{}", "=".repeat(labels.title.chars().count()))?;
    writeln!(out)?;
    for line in summary {
        writeln!(out, "{line}")?;
    }
    for kind in ItemKind::ALL {
        let mut items = plan.items().iter().filter(|i| i.kind == kind).peekable();
        if items.peek().is_none() {
            continue;
        }
        writeln!(out)?;
        writeln!(out, "## {}", (labels.kind)(kind))?;
        for item in items {
            let note = (labels.note)(item);
            if note.is_empty() {
                writeln!(out, "  {}", plan.item_path(item).display())?;
            } else {
                writeln!(out, "  {}  ({note})", plan.item_path(item).display())?;
            }
        }
    }
    out.flush()
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
