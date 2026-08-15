use anyhow::{Context, Result};
use std::io::IsTerminal;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::config::Server;
use crate::project::Project;

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// shell-ready remote path; a leading ~ is left unquoted so the remote shell expands it
pub fn shell_path(root: &str, name: &str, rel: &Path) -> String {
    let mut p = format!("{}/{}", root.trim_end_matches('/'), name);
    let rel = rel.to_string_lossy();
    if !rel.is_empty() {
        p.push('/');
        p.push_str(&rel);
    }
    match p.strip_prefix("~/") {
        Some(rest) => format!("~/{}", shell_quote(rest)),
        None => shell_quote(&p),
    }
}

pub fn control_args() -> Result<Vec<String>> {
    let dir = dirs::cache_dir()
        .context("cannot locate cache dir")?
        .join("rdev");
    std::fs::create_dir_all(&dir)?;
    Ok(vec![
        "-o".into(),
        "ControlMaster=auto".into(),
        "-o".into(),
        format!("ControlPath={}/cm-%C", dir.display()),
        "-o".into(),
        "ControlPersist=10m".into(),
    ])
}

fn base_cmd(host: &str) -> Result<Command> {
    let mut c = Command::new("ssh");
    c.args(control_args()?).arg(host);
    Ok(c)
}

fn wait(c: &mut Command) -> Result<i32> {
    let status = c.status().context("failed to spawn ssh")?;
    Ok(status.code().unwrap_or(1))
}

/// run a command remotely; returns its exit code
pub fn exec(server: &Server, proj: &Project, args: &[String]) -> Result<i32> {
    let dir = shell_path(&server.root, &proj.name, &proj.rel_cwd);
    let cmdline = args
        .iter()
        .map(|a| shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ");
    let inner = if proj.uses_mise() {
        format!("export PATH=\"$HOME/.local/bin:$PATH\"; exec mise x -- {cmdline}")
    } else {
        cmdline
    };
    let sh = shell_quote(server.shell());
    let script = format!("mkdir -p {dir} && cd {dir} && exec {sh} -lc {}", shell_quote(&inner));
    let mut c = base_cmd(&server.host)?;
    if std::io::stdout().is_terminal() {
        c.arg("-t");
    }
    c.arg(script);
    wait(&mut c)
}

/// open an interactive login shell in the remote project dir
pub fn shell(server: &Server, proj: &Project) -> Result<i32> {
    let dir = shell_path(&server.root, &proj.name, &proj.rel_cwd);
    let sh = shell_quote(server.shell());
    let script = format!("mkdir -p {dir} && cd {dir} && exec {sh} -l");
    let mut c = base_cmd(&server.host)?;
    c.arg("-t").arg(script);
    wait(&mut c)
}

pub fn probe(host: &str) -> bool {
    remote_ok(host, "true")
}

pub fn remote_has(host: &str, cmd: &str) -> bool {
    remote_ok(host, &format!("command -v {cmd}"))
}

/// detect the remote login shell via $SHELL (set by sshd from the passwd entry)
pub fn detect_shell(host: &str) -> Option<String> {
    let out = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5"])
        .arg(host)
        .arg("echo $SHELL")
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn remote_ok(host: &str, remote_cmd: &str) -> bool {
    Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5"])
        .arg(host)
        .arg(remote_cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
