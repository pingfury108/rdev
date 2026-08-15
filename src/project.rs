use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};

pub struct Project {
    /// sync source: git repo root, or cwd if not in a repo
    pub root: PathBuf,
    /// cwd relative to root (empty when cwd == root)
    pub rel_cwd: PathBuf,
    /// root dir basename, used as remote dir name
    pub name: String,
}

pub fn detect() -> Result<Project> {
    let cwd = env::current_dir().context("cannot get current dir")?;
    let mut dir: &Path = &cwd;
    loop {
        if dir.join(".git").exists() {
            return make(dir, &cwd);
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return make(&cwd, &cwd),
        }
    }
}

impl Project {
    pub fn uses_mise(&self) -> bool {
        ["mise.toml", ".mise.toml", ".tool-versions"]
            .iter()
            .any(|f| self.root.join(f).exists())
    }
}

fn make(root: &Path, cwd: &Path) -> Result<Project> {
    let name = root
        .file_name()
        .context("cannot determine project name from current dir")?
        .to_string_lossy()
        .into_owned();
    let rel_cwd = cwd.strip_prefix(root).unwrap_or(Path::new("")).to_path_buf();
    Ok(Project {
        root: root.to_path_buf(),
        rel_cwd,
        name,
    })
}
