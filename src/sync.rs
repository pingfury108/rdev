use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::config::Server;
use crate::project::Project;
use crate::ssh;

const BUILTIN_EXCLUDES: &[&str] = &[
    ".git/",
    "target/",
    "node_modules/",
    "build/",
    "dist/",
    "__pycache__/",
];

pub fn push(server: &Server, proj: &Project) -> Result<()> {
    let dir = ssh::shell_path(&server.root, &proj.name, Path::new(""));
    let target = format!("{}:{}", server.host, dir);

    let transport = std::iter::once("ssh".to_string())
        .chain(ssh::control_args()?)
        .map(|a| ssh::shell_quote(&a))
        .collect::<Vec<_>>()
        .join(" ");

    let mut c = Command::new("rsync");
    c.arg("-az");
    for pat in excludes(&proj.root) {
        c.arg(format!("--exclude={pat}"));
    }
    c.arg("-e")
        .arg(transport)
        .arg(format!("--rsync-path=mkdir -p {dir} && rsync"))
        .arg(format!("{}/", proj.root.display()))
        .arg(&target);

    eprintln!("sync -> {target}");
    let status = c.status().context("failed to spawn rsync")?;
    if !status.success() {
        anyhow::bail!("rsync failed ({status})");
    }
    Ok(())
}

fn excludes(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    if let Ok(text) = std::fs::read_to_string(root.join(".gitignore")) {
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') || l.starts_with('!') {
                continue;
            }
            v.push(l.to_string());
        }
    }
    v
}
