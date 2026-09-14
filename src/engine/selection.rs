//! Choosing individual sub-folders and files of a source.
//!
//! A selection is a set of decisions: "this path is included" or "this path
//! is excluded". The nearest decision on the way from a path up to the source
//! root wins; without any decision everything is included. `"."` is the root.
//!
//! Examples:
//! * `exclude_paths = ["Games"]` — everything except the `Games` folder
//! * `exclude_paths = ["."]`, `include_paths = ["Projects", "notes.txt"]` — only those two

/// Tri-state for checkboxes in the folder tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Checked,
    Unchecked,
    Mixed,
}

pub const ROOT: &str = ".";

/// Normalises user or UI input: `/` separators, no leading/trailing slashes,
/// `.` for the root.
pub fn normalize(path: &str) -> String {
    let cleaned: Vec<&str> = path
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if cleaned.is_empty() {
        ROOT.to_string()
    } else {
        cleaned.join("/")
    }
}

fn key(path: &str) -> String {
    normalize(path).to_lowercase()
}

fn parent(path: &str) -> Option<String> {
    if path == ROOT {
        return None;
    }
    Some(match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => ROOT.to_string(),
    })
}

fn is_descendant(candidate: &str, ancestor: &str) -> bool {
    if ancestor == ROOT {
        return candidate != ROOT;
    }
    candidate.len() > ancestor.len()
        && candidate.starts_with(ancestor)
        && candidate.as_bytes()[ancestor.len()] == b'/'
}

/// Read-only view on the two decision lists of a source.
pub struct Selection<'a> {
    include: &'a [String],
    exclude: &'a [String],
}

impl<'a> Selection<'a> {
    pub fn new(include: &'a [String], exclude: &'a [String]) -> Self {
        Self { include, exclude }
    }

    pub fn is_everything(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    fn decision(&self, lower: &str) -> Option<bool> {
        if self.exclude.iter().any(|p| key(p) == lower) {
            Some(false)
        } else if self.include.iter().any(|p| key(p) == lower) {
            Some(true)
        } else {
            None
        }
    }

    /// Whether `path` itself is included.
    pub fn is_included(&self, path: &str) -> bool {
        if self.is_everything() {
            return true;
        }
        let mut current = Some(key(path));
        while let Some(p) = current {
            if let Some(decision) = self.decision(&p) {
                return decision;
            }
            current = parent(&p);
        }
        true
    }

    /// Whether some path below `path` has an explicit decision `value`.
    pub fn has_descendant(&self, path: &str, value: bool) -> bool {
        let lower = key(path);
        let list = if value { self.include } else { self.exclude };
        list.iter().any(|p| is_descendant(&key(p), &lower))
    }

    /// Whether a folder must be entered while scanning.
    pub fn should_enter(&self, path: &str) -> bool {
        self.is_included(path) || self.has_descendant(path, true)
    }

    pub fn state(&self, path: &str) -> CheckState {
        let included = self.is_included(path);
        if self.has_descendant(path, !included) {
            CheckState::Mixed
        } else if included {
            CheckState::Checked
        } else {
            CheckState::Unchecked
        }
    }

    /// Top-level starting points for a scan when the root is excluded:
    /// included paths that are not below another included path.
    pub fn scan_roots(&self) -> Option<Vec<String>> {
        if self.is_included(ROOT) {
            return None;
        }
        let mut roots: Vec<String> = self
            .include
            .iter()
            .map(|p| normalize(p))
            .filter(|p| self.is_included(p))
            .collect();
        roots.sort_by_key(|p| p.len());
        let mut result: Vec<String> = Vec::new();
        for root in roots {
            let lower = root.to_lowercase();
            if !result
                .iter()
                .any(|r| is_descendant(&lower, &r.to_lowercase()) || r.eq_ignore_ascii_case(&root))
            {
                result.push(root);
            }
        }
        Some(result)
    }
}

/// Toggles a node the way a tree checkbox would: checked → unchecked,
/// unchecked or mixed → checked. Decisions below the node are cleared.
pub fn toggle(include: &mut Vec<String>, exclude: &mut Vec<String>, path: &str) {
    let path = normalize(path);
    let lower = path.to_lowercase();
    let new_value = Selection::new(include, exclude).state(&path) != CheckState::Checked;
    let clear = |list: &mut Vec<String>| {
        list.retain(|p| {
            let k = key(p);
            k != lower && !is_descendant(&k, &lower)
        })
    };
    clear(include);
    clear(exclude);
    let inherited = match parent(&lower) {
        Some(parent) => Selection::new(include, exclude).is_included(&parent),
        None => true,
    };
    if inherited != new_value {
        if new_value {
            include.push(path);
        } else {
            exclude.push(path);
        }
    }
}

pub fn select_all(include: &mut Vec<String>, exclude: &mut Vec<String>) {
    include.clear();
    exclude.clear();
}

pub fn select_none(include: &mut Vec<String>, exclude: &mut Vec<String>) {
    include.clear();
    exclude.clear();
    exclude.push(ROOT.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_decision_wins() {
        let include = vec!["Projects/Keep".to_string()];
        let exclude = vec!["Projects".to_string()];
        let s = Selection::new(&include, &exclude);
        assert!(s.is_included("notes.txt"));
        assert!(!s.is_included("projects/old.txt"));
        assert!(s.is_included("Projects/Keep/a.txt"));
        assert_eq!(s.state("Projects"), CheckState::Mixed);
        assert!(s.should_enter("Projects"));
        assert_eq!(s.state("."), CheckState::Mixed);
    }

    #[test]
    fn toggling_builds_minimal_decisions() {
        let mut include = Vec::new();
        let mut exclude = Vec::new();
        toggle(&mut include, &mut exclude, "Games");
        assert_eq!(exclude, vec!["Games"]);
        toggle(&mut include, &mut exclude, "Games");
        assert!(include.is_empty() && exclude.is_empty());

        select_none(&mut include, &mut exclude);
        toggle(&mut include, &mut exclude, "Projects");
        let s = Selection::new(&include, &exclude);
        assert!(s.is_included("Projects/x"));
        assert!(!s.is_included("Other"));
        assert_eq!(s.scan_roots(), Some(vec!["Projects".to_string()]));

        // Checking the mixed root selects everything again.
        toggle(&mut include, &mut exclude, ".");
        assert!(include.is_empty() && exclude.is_empty());
    }

    #[test]
    fn prefix_names_are_not_descendants() {
        let exclude = vec!["Game".to_string()];
        let s = Selection::new(&[], &exclude);
        assert!(s.is_included("Games/x"));
        assert!(!s.is_included("Game/x"));
    }
}
