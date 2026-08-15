use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default)]
    pub servers: BTreeMap<String, Server>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub host: String,
    #[serde(default = "default_root")]
    pub root: String,
    /// remote login shell, auto-detected on add/setup; fallback: bash
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

impl Server {
    pub fn shell(&self) -> &str {
        self.shell.as_deref().unwrap_or("bash")
    }
}

pub fn default_root() -> String {
    "~/rdev".to_string()
}

pub fn validate_root(root: &str) -> Result<()> {
    let r = root.trim_end_matches('/');
    if r.is_empty() || r == "/" || r == "~" {
        bail!("invalid root \"{root}\": too broad");
    }
    Ok(())
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let dir = dirs::config_dir().context("cannot locate config dir")?;
        Ok(dir.join("rdev").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, toml::to_string_pretty(self)?)
            .with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn current_server(&self) -> Result<&Server> {
        let name = self
            .current
            .as_deref()
            .context("no server selected; run: rdev server add <name> <host>")?;
        self.servers
            .get(name)
            .with_context(|| format!("server \"{name}\" not found; run: rdev server ls"))
    }
}
