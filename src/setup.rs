use anyhow::{Context, Result};
use std::io::IsTerminal;
use std::process::Command;

use crate::config::Server;
use crate::ssh;

/// posix script executed via `sh -c` so it works regardless of the remote login shell (bash/zsh/fish)
/// note: keep the script free of single quotes (it is single-quoted as one ssh argument)
const SCRIPT: &str = r#"
set -u
have() { command -v "$1" >/dev/null 2>&1; }

if have bash; then echo "ok: bash"; else echo "MISSING: bash (install it manually)"; fi

if have rsync; then
  echo "ok: rsync"
else
  echo "missing: rsync, installing..."
  if have apt-get; then sudo apt-get update -qq && sudo apt-get install -y rsync
  elif have dnf; then sudo dnf install -y rsync
  elif have yum; then sudo yum install -y rsync
  elif have pacman; then sudo pacman -S --noconfirm rsync
  elif have brew; then brew install rsync
  else echo "FAILED: no known package manager; install rsync manually"
  fi
  if have rsync; then echo "ok: rsync"; else echo "FAILED: rsync"; fi
fi

if have mise || [ -x "$HOME/.local/bin/mise" ]; then
  echo "ok: mise"
else
  echo "missing: mise, installing..."
  if have curl; then curl -fsSL https://mise.run | sh
  elif have wget; then wget -qO- https://mise.run | sh
  else echo "FAILED: need curl or wget to install mise"
  fi
  if [ -x "$HOME/.local/bin/mise" ]; then echo "ok: mise"; else echo "FAILED: mise"; fi
fi
"#;

/// provision a remote server: bash/rsync/mise; idempotent. returns remote exit code
pub fn setup(server: &Server) -> Result<i32> {
    let mut c = Command::new("ssh");
    c.args(ssh::control_args()?);
    if std::io::stdout().is_terminal() {
        c.arg("-t"); // allow interactive sudo password prompt
    }
    c.arg(&server.host)
        .arg(format!("sh -c {}", ssh::shell_quote(SCRIPT)));
    let status = c.status().context("failed to spawn ssh")?;
    Ok(status.code().unwrap_or(1))
}
