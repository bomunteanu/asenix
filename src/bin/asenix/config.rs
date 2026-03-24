use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentCred {
    pub agent_id: String,
    pub api_token: String,
    pub hub: String,
    pub domain: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthConfig {
    pub hub: String,
    pub token: String,
    pub expires_at: String,
}

/// App data directory. Resolves to the OS-native location:
///   macOS  → ~/Library/Application Support/asenix
///   Linux  → ~/.local/share/asenix  (or $XDG_DATA_HOME/asenix)
///   other  → ~/.asenix
///
/// Override with `ASENIX_DATA_DIR` (used in tests).
pub fn asenix_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ASENIX_DATA_DIR") {
        return PathBuf::from(dir);
    }
    // Legacy override kept for backward compatibility
    if let Ok(dir) = std::env::var("ASENIX_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_dir()
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local").join("share")
        })
        .join("asenix")
}

pub fn agent_creds_dir(hostname: &str) -> PathBuf {
    asenix_config_dir().join(hostname)
}

pub fn logs_dir() -> PathBuf {
    asenix_config_dir().join("logs")
}

pub fn auth_path() -> PathBuf {
    asenix_config_dir().join("auth.toml")
}

pub fn log_path(n: usize) -> PathBuf {
    logs_dir().join(format!("agent_{}.log", n))
}

pub fn domains_dir() -> PathBuf {
    asenix_config_dir().join("domains")
}

pub fn domain_pack_dir(name: &str) -> PathBuf {
    domains_dir().join(name)
}

/// Per-domain agent number: scans for `<domain>_agent_*.toml` in the hostname dir.
pub fn next_agent_n_for_domain(hostname: &str, domain: &str) -> usize {
    let dir = agent_creds_dir(hostname);
    if !dir.exists() {
        return 1;
    }
    let prefix = format!("{}_agent_", domain);
    let mut n = 1usize;
    while dir.join(format!("{}{}.toml", prefix, n)).exists() {
        n += 1;
    }
    n
}

/// Save agent credentials with domain-prefixed filename: `<domain>_agent_<n>.toml`.
pub fn save_agent_cred_for_domain(
    hostname: &str,
    domain: &str,
    n: usize,
    cred: &AgentCred,
) -> Result<PathBuf> {
    let dir = agent_creds_dir(hostname);
    fs::create_dir_all(&dir).context("failed to create credentials directory")?;
    let path = dir.join(format!("{}_agent_{}.toml", domain, n));
    let content = toml::to_string(cred).context("failed to serialize credentials")?;
    fs::write(&path, content).context("failed to write credentials file")?;
    Ok(path)
}

/// Working directory for an agent: /tmp/asenix/<domain>/<n>/
pub fn workdir_path(domain: &str, n: usize) -> PathBuf {
    std::env::temp_dir()
        .join("asenix")
        .join(domain)
        .join(n.to_string())
}

/// PID file for a background agent: /tmp/asenix/<domain>/<n>/agent.pid
pub fn pid_path(domain: &str, n: usize) -> PathBuf {
    workdir_path(domain, n).join("agent.pid")
}

/// Timestamped log path for a domain agent run.
pub fn domain_log_path(domain: &str, n: usize, timestamp: &str) -> PathBuf {
    logs_dir().join(format!("{}_agent_{}_{}.log", domain, n, timestamp))
}

/// Returns the next available agent number under the given hostname directory.
pub fn next_agent_n(hostname: &str) -> usize {
    let dir = agent_creds_dir(hostname);
    if !dir.exists() {
        return 1;
    }
    let mut n = 1usize;
    while dir.join(format!("agent_{}.toml", n)).exists() {
        n += 1;
    }
    n
}

/// Save agent credentials to disk; returns the path written.
pub fn save_agent_cred(hostname: &str, n: usize, cred: &AgentCred) -> Result<PathBuf> {
    let dir = agent_creds_dir(hostname);
    fs::create_dir_all(&dir).context("failed to create credentials directory")?;
    let path = dir.join(format!("agent_{}.toml", n));
    let content = toml::to_string(cred).context("failed to serialize credentials")?;
    fs::write(&path, content).context("failed to write credentials file")?;
    Ok(path)
}

/// Load every agent credential found under all hostname subdirectories.
pub fn load_all_agents() -> Result<Vec<(usize, AgentCred, PathBuf)>> {
    let base = asenix_config_dir();
    let mut result = Vec::new();
    if !base.exists() {
        return Ok(result);
    }
    for entry in fs::read_dir(&base).context("failed to read config directory")? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if !ft.is_dir() {
            continue;
        }
        if entry.file_name() == "logs" {
            continue;
        }
        for file in fs::read_dir(entry.path())? {
            let file = file?;
            let name = file.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("agent_") || !name_str.ends_with(".toml") {
                continue;
            }
            let n: usize = name_str
                .strip_prefix("agent_")
                .and_then(|s| s.strip_suffix(".toml"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let content = fs::read_to_string(file.path())?;
            let cred: AgentCred =
                toml::from_str(&content).context("failed to parse credentials file")?;
            result.push((n, cred, file.path()));
        }
    }
    result.sort_by_key(|(n, _, _)| *n);
    Ok(result)
}

pub fn save_auth(auth: &AuthConfig) -> Result<()> {
    let dir = asenix_config_dir();
    fs::create_dir_all(&dir).context("failed to create config directory")?;
    let content = toml::to_string(auth).context("failed to serialize auth")?;
    fs::write(auth_path(), content).context("failed to write auth file")?;
    Ok(())
}

pub fn load_auth() -> Result<AuthConfig> {
    let path = auth_path();
    let content = fs::read_to_string(&path)
        .with_context(|| format!("auth file not found at {}", path.display()))?;
    toml::from_str(&content).context("failed to parse auth file")
}

/// Delete all agent credential files under the given hostname directory.
/// Returns the number of files deleted.
pub fn delete_host_data(hostname: &str) -> Result<usize> {
    let dir = agent_creds_dir(hostname);
    let mut count = 0;
    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().ends_with(".toml") {
                fs::remove_file(entry.path())?;
                count += 1;
            }
        }
        let _ = fs::remove_dir(&dir);
    }
    Ok(count)
}

/// Delete all log files in the logs directory. Returns the count deleted.
pub fn delete_logs() -> Result<usize> {
    let dir = logs_dir();
    let mut count = 0;
    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().ends_with(".log") {
                fs::remove_file(entry.path())?;
                count += 1;
            }
        }
        let _ = fs::remove_dir(&dir);
    }
    Ok(count)
}

/// Best-effort hostname detection.
pub fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "local".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn with_temp_dir<F: FnOnce()>(f: F) {
        let dir = tempfile::TempDir::new().unwrap();
        std::env::set_var("ASENIX_DATA_DIR", dir.path());
        f();
        std::env::remove_var("ASENIX_DATA_DIR");
        // dir is dropped here, cleaning up temp files
    }

    #[test]
    #[serial]
    fn next_agent_n_returns_1_when_empty() {
        with_temp_dir(|| {
            assert_eq!(next_agent_n("testhost"), 1);
        });
    }

    #[test]
    #[serial]
    fn next_agent_n_increments_past_existing() {
        with_temp_dir(|| {
            let cred = AgentCred {
                agent_id: "abc".to_string(),
                api_token: "tok".to_string(),
                hub: "http://localhost:3000".to_string(),
                domain: "ml".to_string(),
                created_at: "2025-01-01T00:00:00Z".to_string(),
            };
            save_agent_cred("testhost", 1, &cred).unwrap();
            assert_eq!(next_agent_n("testhost"), 2);
            save_agent_cred("testhost", 2, &cred).unwrap();
            assert_eq!(next_agent_n("testhost"), 3);
        });
    }

    #[test]
    #[serial]
    fn save_and_load_agent_cred_roundtrip() {
        with_temp_dir(|| {
            let cred = AgentCred {
                agent_id: "agent-123".to_string(),
                api_token: "mote_abc123".to_string(),
                hub: "http://localhost:3000".to_string(),
                domain: "biology".to_string(),
                created_at: "2025-03-15T10:00:00Z".to_string(),
            };
            let path = save_agent_cred("myhost", 1, &cred).unwrap();
            assert!(path.exists());

            let loaded = load_all_agents().unwrap();
            assert_eq!(loaded.len(), 1);
            let (n, loaded_cred, _) = &loaded[0];
            assert_eq!(*n, 1);
            assert_eq!(loaded_cred.agent_id, "agent-123");
            assert_eq!(loaded_cred.domain, "biology");
        });
    }

    #[test]
    #[serial]
    fn save_and_load_auth_roundtrip() {
        with_temp_dir(|| {
            let auth = AuthConfig {
                hub: "http://localhost:3000".to_string(),
                token: "jwt_token_here".to_string(),
                expires_at: "2025-03-16T10:00:00Z".to_string(),
            };
            save_auth(&auth).unwrap();
            let loaded = load_auth().unwrap();
            assert_eq!(loaded.token, "jwt_token_here");
            assert_eq!(loaded.hub, "http://localhost:3000");
        });
    }

    #[test]
    #[serial]
    fn load_auth_errors_when_missing() {
        with_temp_dir(|| {
            let result = load_auth();
            assert!(result.is_err());
        });
    }

    #[test]
    #[serial]
    fn delete_host_data_removes_files() {
        with_temp_dir(|| {
            let cred = AgentCred {
                agent_id: "x".to_string(),
                api_token: "y".to_string(),
                hub: "http://localhost:3000".to_string(),
                domain: "ml".to_string(),
                created_at: "2025-01-01T00:00:00Z".to_string(),
            };
            save_agent_cred("h", 1, &cred).unwrap();
            save_agent_cred("h", 2, &cred).unwrap();
            let deleted = delete_host_data("h").unwrap();
            assert_eq!(deleted, 2);
            assert!(load_all_agents().unwrap().is_empty());
        });
    }

    #[test]
    #[serial]
    fn next_agent_n_for_domain_returns_1_when_empty() {
        with_temp_dir(|| {
            assert_eq!(next_agent_n_for_domain("host", "cifar10"), 1);
        });
    }

    #[test]
    #[serial]
    fn next_agent_n_for_domain_independent_per_domain() {
        with_temp_dir(|| {
            let cred = AgentCred {
                agent_id: "x".to_string(),
                api_token: "y".to_string(),
                hub: "http://localhost:3000".to_string(),
                domain: "cifar10".to_string(),
                created_at: "2025-01-01T00:00:00Z".to_string(),
            };
            save_agent_cred_for_domain("host", "cifar10", 1, &cred).unwrap();
            save_agent_cred_for_domain("host", "cifar10", 2, &cred).unwrap();
            save_agent_cred_for_domain("host", "ml", 1, &cred).unwrap();

            // cifar10 is at 3, ml is at 2 — fully independent
            assert_eq!(next_agent_n_for_domain("host", "cifar10"), 3);
            assert_eq!(next_agent_n_for_domain("host", "ml"), 2);
        });
    }

    #[test]
    #[serial]
    fn save_agent_cred_for_domain_uses_prefixed_filename() {
        with_temp_dir(|| {
            let cred = AgentCred {
                agent_id: "abc".to_string(),
                api_token: "tok".to_string(),
                hub: "http://localhost:3000".to_string(),
                domain: "physics".to_string(),
                created_at: "2025-01-01T00:00:00Z".to_string(),
            };
            let path = save_agent_cred_for_domain("h", "physics", 1, &cred).unwrap();
            assert!(path.file_name().unwrap().to_string_lossy().contains("physics_agent_1"));
        });
    }

    #[test]
    fn workdir_path_is_in_tmp() {
        let p = workdir_path("cifar10", 3);
        assert!(p.starts_with(std::env::temp_dir()));
        assert!(p.to_string_lossy().contains("cifar10"));
        assert!(p.to_string_lossy().contains('3'));
    }

    #[test]
    #[serial]
    fn load_all_agents_skips_logs_dir() {
        with_temp_dir(|| {
            // Create a logs dir with a .toml file — should not be picked up as a cred
            let logs = logs_dir();
            std::fs::create_dir_all(&logs).unwrap();
            std::fs::write(logs.join("agent_1.toml"), "not a cred").unwrap();

            let agents = load_all_agents().unwrap();
            assert!(agents.is_empty());
        });
    }
}
