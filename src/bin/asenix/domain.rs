use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;

// ─── Domain Pack Format ───────────────────────────────────────────────────────
//
//  ~/.config/asenix/domains/<name>/
//    domain.toml       ← name, description, working_dir
//    CLAUDE.md         ← verbatim agent instructions
//    bounty.json       ← full publish_atoms body (no credentials)
//    requirements.txt  ← optional Python deps
//    files/            ← files copied into each agent's working dir

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DomainToml {
    pub name: String,
    pub description: String,
    #[serde(default = "default_working_dir")]
    pub working_dir: String,
}

fn default_working_dir() -> String {
    ".".to_string()
}

#[derive(Debug)]
pub struct PackInfo {
    pub name: String,
    pub description: String,
    pub file_count: usize,
    pub has_bounty: bool,
    pub has_requirements: bool,
}

/// Load and validate a domain pack from the installed location.
pub fn load_pack(name: &str) -> Result<DomainToml> {
    let dir = config::domain_pack_dir(name);
    let toml_path = dir.join("domain.toml");

    if !dir.exists() {
        anyhow::bail!(
            "domain pack '{}' is not installed\n  \
             hint: Run `asenix domain install <path>` first",
            name
        );
    }

    let content = fs::read_to_string(&toml_path)
        .with_context(|| format!("domain.toml not found in {}", dir.display()))?;

    toml::from_str::<DomainToml>(&content)
        .with_context(|| format!("domain.toml in '{}' is malformed", name))
}

/// Copy a domain pack from `src` into `~/.config/asenix/domains/<name>/`.
/// Returns the installed domain name.
pub fn install_pack(src: &Path) -> Result<String> {
    let toml_path = src.join("domain.toml");

    if !toml_path.exists() {
        anyhow::bail!(
            "domain.toml not found in {}\n  \
             hint: A domain pack must contain a domain.toml with name and description fields",
            src.display()
        );
    }

    let content = fs::read_to_string(&toml_path)
        .context("failed to read domain.toml")?;

    let pack: DomainToml = toml::from_str(&content).with_context(|| {
        format!(
            "domain.toml is malformed\n  hint: Required fields are name (string) and description (string)"
        )
    })?;

    if pack.name.is_empty() {
        anyhow::bail!("domain.toml: 'name' field must not be empty");
    }
    // Reject names with path separators
    if pack.name.contains('/') || pack.name.contains('\\') {
        anyhow::bail!("domain.toml: 'name' must not contain path separators");
    }

    let dest = config::domain_pack_dir(&pack.name);
    copy_dir_all(src, &dest)
        .with_context(|| format!("failed to copy pack to {}", dest.display()))?;

    Ok(pack.name)
}

/// Return info for every valid installed domain pack.
pub fn list_packs() -> Result<Vec<PackInfo>> {
    let dir = config::domains_dir();
    let mut packs = Vec::new();

    if !dir.exists() {
        return Ok(packs);
    }

    for entry in fs::read_dir(&dir).context("failed to read domains directory")? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let toml_path = entry.path().join("domain.toml");
        if !toml_path.exists() {
            continue;
        }
        let content = match fs::read_to_string(&toml_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let pack: DomainToml = match toml::from_str(&content) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let file_count = count_files(&entry.path().join("files"));
        let has_bounty = entry.path().join("bounty.json").exists();
        let has_requirements = entry.path().join("requirements.txt").exists();

        packs.push(PackInfo {
            name: pack.name,
            description: pack.description,
            file_count,
            has_bounty,
            has_requirements,
        });
    }

    packs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packs)
}

/// Recursively copy `src` directory into `dst`, creating `dst` if needed.
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create directory {}", dst.display()))?;

    for entry in fs::read_dir(src)
        .with_context(|| format!("failed to read directory {}", src.display()))?
    {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path).with_context(|| {
                format!("failed to copy {}", entry.path().display())
            })?;
        }
    }
    Ok(())
}

fn count_files(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()).count())
        .unwrap_or(0)
}

/// Check that `claude` is findable on PATH without spawning a process.
pub fn claude_in_path() -> bool {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            if Path::new(dir).join("claude").exists() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn write_domain_toml(dir: &Path, name: &str, desc: &str) {
        fs::write(
            dir.join("domain.toml"),
            format!("name = \"{}\"\ndescription = \"{}\"\n", name, desc),
        )
        .unwrap();
    }

    fn with_config_dir<F: FnOnce()>(f: F) {
        let tmp = TempDir::new().unwrap();
        std::env::set_var("ASENIX_DATA_DIR", tmp.path());
        f();
        std::env::remove_var("ASENIX_DATA_DIR");
    }

    #[test]
    #[serial]
    fn install_creates_pack_in_domains_dir() {
        with_config_dir(|| {
            let src = TempDir::new().unwrap();
            write_domain_toml(src.path(), "test_domain", "A test domain");
            fs::create_dir(src.path().join("files")).unwrap();
            fs::write(src.path().join("files").join("train.py"), "# placeholder").unwrap();
            fs::write(src.path().join("CLAUDE.md"), "Do research.").unwrap();

            let name = install_pack(src.path()).unwrap();
            assert_eq!(name, "test_domain");

            let dest = config::domain_pack_dir("test_domain");
            assert!(dest.join("domain.toml").exists());
            assert!(dest.join("CLAUDE.md").exists());
            assert!(dest.join("files").join("train.py").exists());
        });
    }

    #[test]
    #[serial]
    fn install_fails_without_domain_toml() {
        with_config_dir(|| {
            let src = TempDir::new().unwrap();
            // No domain.toml
            let result = install_pack(src.path());
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("domain.toml not found"));
        });
    }

    #[test]
    #[serial]
    fn install_fails_with_malformed_toml() {
        with_config_dir(|| {
            let src = TempDir::new().unwrap();
            fs::write(src.path().join("domain.toml"), "this is not valid toml ][").unwrap();
            let result = install_pack(src.path());
            assert!(result.is_err());
        });
    }

    #[test]
    #[serial]
    fn install_fails_with_missing_name_field() {
        with_config_dir(|| {
            let src = TempDir::new().unwrap();
            fs::write(src.path().join("domain.toml"), "description = \"No name field\"\n").unwrap();
            let result = install_pack(src.path());
            assert!(result.is_err());
        });
    }

    #[test]
    #[serial]
    fn load_pack_roundtrip() {
        with_config_dir(|| {
            let src = TempDir::new().unwrap();
            write_domain_toml(src.path(), "roundtrip", "Round trip test");
            install_pack(src.path()).unwrap();

            let pack = load_pack("roundtrip").unwrap();
            assert_eq!(pack.name, "roundtrip");
            assert_eq!(pack.description, "Round trip test");
        });
    }

    #[test]
    #[serial]
    fn load_pack_fails_for_missing_domain() {
        with_config_dir(|| {
            let result = load_pack("nonexistent");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("not installed"));
        });
    }

    #[test]
    #[serial]
    fn list_packs_returns_installed_packs() {
        with_config_dir(|| {
            let src1 = TempDir::new().unwrap();
            write_domain_toml(src1.path(), "alpha", "Alpha domain");
            install_pack(src1.path()).unwrap();

            let src2 = TempDir::new().unwrap();
            write_domain_toml(src2.path(), "beta", "Beta domain");
            fs::write(src2.path().join("bounty.json"), "{}").unwrap();
            fs::write(src2.path().join("requirements.txt"), "torch\n").unwrap();
            install_pack(src2.path()).unwrap();

            let packs = list_packs().unwrap();
            assert_eq!(packs.len(), 2);
            assert_eq!(packs[0].name, "alpha");
            assert_eq!(packs[1].name, "beta");
            assert!(packs[1].has_bounty);
            assert!(packs[1].has_requirements);
            assert!(!packs[0].has_bounty);
        });
    }

    #[test]
    #[serial]
    fn list_packs_empty_when_none_installed() {
        with_config_dir(|| {
            let packs = list_packs().unwrap();
            assert!(packs.is_empty());
        });
    }

    #[test]
    fn copy_dir_all_copies_nested_files() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        let subdir = src.path().join("sub");
        fs::create_dir(&subdir).unwrap();
        fs::write(src.path().join("a.txt"), "a").unwrap();
        fs::write(subdir.join("b.txt"), "b").unwrap();

        copy_dir_all(src.path(), dst.path()).unwrap();

        assert!(dst.path().join("a.txt").exists());
        assert!(dst.path().join("sub").join("b.txt").exists());
    }
}
