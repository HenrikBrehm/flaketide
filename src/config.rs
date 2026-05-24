//! Configuration loader: merges flaketide.toml + env vars + CLI overrides.

use std::path::{Path, PathBuf};

use crate::domain::Config;
use crate::error::{FlaketideError, Result};
use crate::util::paths::find_repo_root;

/// File names searched in repo-root, in order.
const CONFIG_FILENAMES: &[&str] = &["flaketide.toml", ".flaketide.toml"];

pub fn load_config(explicit: Option<&Path>, start: &Path) -> Result<(Config, Option<PathBuf>)> {
    if let Some(p) = explicit {
        if !p.exists() {
            return Err(FlaketideError::Config(format!(
                "config file not found: {}",
                p.display()
            )));
        }
        let cfg = read_config_file(p)?;
        return Ok((cfg, Some(p.to_path_buf())));
    }

    let root = find_repo_root(start).unwrap_or_else(|| start.to_path_buf());
    for name in CONFIG_FILENAMES {
        let candidate = root.join(name);
        if candidate.exists() {
            let cfg = read_config_file(&candidate)?;
            return Ok((cfg, Some(candidate)));
        }
    }
    Ok((Config::default(), None))
}

pub fn read_config_file(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)?;
    let cfg: Config = toml::from_str(&raw)?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn defaults_when_no_file() {
        let dir = TempDir::new().unwrap();
        let (cfg, found) = load_config(None, dir.path()).unwrap();
        assert_eq!(cfg.runs, 10);
        assert!(found.is_none());
    }

    #[test]
    fn loads_minimal_toml() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("flaketide.toml");
        std::fs::write(&p, r#"runs = 42
parallel = 4
"#).unwrap();
        let (cfg, found) = load_config(None, dir.path()).unwrap();
        assert_eq!(cfg.runs, 42);
        assert_eq!(cfg.parallel, 4);
        assert_eq!(found.as_deref(), Some(p.as_path()));
    }

    #[test]
    fn rejects_unknown_keys() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("flaketide.toml");
        std::fs::write(&p, "unknown_key = 1\n").unwrap();
        let err = load_config(None, dir.path()).unwrap_err();
        assert!(matches!(err, FlaketideError::Toml(_)), "got {err:?}");
    }
}
