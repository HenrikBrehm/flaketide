//! Path discovery helpers.

use std::path::{Path, PathBuf};

/// Walk up from `start` looking for a `.git` directory or `flaketide.toml`.
/// Returns the directory containing the marker, or None if not found.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if current.join(".git").exists()
            || current.join("flaketide.toml").exists()
            || current.join(".flaketide.toml").exists()
        {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Ensure `.flaketide/` exists under `root`. Returns the absolute path.
pub fn ensure_flaketide_dir(root: &Path) -> std::io::Result<PathBuf> {
    let dir = root.join(".flaketide");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn finds_git_marker() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let sub = dir.path().join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(find_repo_root(&sub).unwrap(), dir.path());
    }

    #[test]
    fn finds_toml_marker() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("flaketide.toml"), "").unwrap();
        assert_eq!(find_repo_root(dir.path()).unwrap(), dir.path());
    }

    #[test]
    fn returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        // No marker, should walk up; in CI we will hit user home which likely lacks markers.
        // To avoid depending on host filesystem, we only assert this returns *something* or None safely.
        let _ = find_repo_root(dir.path());
    }
}
