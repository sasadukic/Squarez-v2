// src/recovery.rs
//
// Crash-recovery snapshots. Autosave writes every unsaved-changes tab here —
// never to the user's own file, which stays untouched until an explicit Save —
// and the app offers the snapshot back on the next launch if it survived
// (i.e. the app did not exit cleanly with everything saved).
//
// Snapshots are always .sqr (bincode keeps mesh3d), one file per tab, plus a
// JSON manifest recording each tab's display name and, when it had one, the
// path Save should keep writing to after a restore.

use std::path::{Path, PathBuf};

use crate::project::Project;

/// One recoverable tab, as listed in the manifest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecoveryEntry {
    /// Snapshot file, relative to the recovery dir in the manifest.
    pub file: PathBuf,
    /// Display name for the restore prompt.
    pub name: String,
    /// The file this tab was editing, if it had been saved before.
    pub original_path: Option<PathBuf>,
}

const MANIFEST: &str = "manifest.json";

/// Default recovery directory.
pub fn default_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".squarez").join("recovery"))
        .unwrap_or_else(|| std::env::temp_dir().join("squarez_recovery"))
}

/// Remove every snapshot and the manifest. Missing dir is fine.
pub fn wipe(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let ours = p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n == MANIFEST
                    || (n.starts_with("tab") && n.ends_with(".sqr"))
                    // Files from the pre-manifest autosave scheme, which
                    // nothing ever read back — clear them out as we go.
                    || (n.starts_with("recovery_") && n.ends_with(".sqr"))
            });
            if ours {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

/// Replace the snapshot set with one file per given tab.
///
/// Callers pass only tabs with unsaved changes; a tab whose work is safe in
/// its own file needs no snapshot. Returns how many tabs were written.
pub fn write_snapshot(
    dir: &Path,
    tabs: &[(&Project, Option<&Path>)],
) -> std::io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    wipe(dir);
    let mut manifest: Vec<RecoveryEntry> = Vec::new();
    for (i, (project, original)) in tabs.iter().enumerate() {
        let file = PathBuf::from(format!("tab{i}.sqr"));
        if crate::io::sqr::save_sqr(project, &dir.join(&file)).is_err() {
            continue;
        }
        manifest.push(RecoveryEntry {
            file,
            name: project.name.clone(),
            original_path: original.map(|p| p.to_path_buf()),
        });
    }
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(dir.join(MANIFEST), json)?;
    Ok(manifest.len())
}

/// Snapshots waiting from a previous session, with entries whose files went
/// missing filtered out. Empty when there is nothing to offer.
pub fn pending(dir: &Path) -> Vec<RecoveryEntry> {
    let Ok(json) = std::fs::read_to_string(dir.join(MANIFEST)) else {
        return Vec::new();
    };
    let Ok(entries) = serde_json::from_str::<Vec<RecoveryEntry>>(&json) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter(|e| dir.join(&e.file).is_file())
        .collect()
}
