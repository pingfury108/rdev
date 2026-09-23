mod config;
mod project;
mod setup;
mod ssh;
mod sync;
mod task;

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use config::{validate_root, Config, Server};

#[derive(Parser)]
#[command(
    name = "rdev",
    version,
    about = "remote dev proxy: run commands on a remote server"
)]
struct Cli {
    /// skip the pre-run sync
    #[arg(long, global = true)]
    no_sync: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// sync project and open a remote shell in the project dir
    Shell,
    /// sync project and run a script from stdin in the project dir (e.g. rdev sh <<'EOF' ... EOF)
    Sh,
    /// sync project to the remote server only
    Sync,
    /// print the config file path
    Config,
    /// pull a file/dir from the remote project dir
    Pull {
        /// path relative to the remote project dir
        path: String,
        /// local destination (default: same relative path in the local project)
        dest: Option<String>,
    },
    /// start a background task on the remote (log: .rdev/task.log)
    Start {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        cmd: Vec<String>,
    },
    /// show the latest background task log
    Logs {
        /// follow the log (like tail -f)
        #[arg(short, long)]
        follow: bool,
        /// number of lines to show
        #[arg(short = 'n', long, default_value = "100")]
        lines: u32,
    },
    /// show the latest background task status
    Status,
    /// manage remote servers
    Server {
        #[command(subcommand)]
        action: Option<ServerCmd>,
    },
    /// run a command on the remote server (default)
    #[command(external_subcommand)]
    Run(Vec<String>),
}

#[derive(Subcommand)]
enum ServerCmd {
    /// add a server; the first one becomes current
    Add {
        name: String,
        /// ssh host: alias in ~/.ssh/config or user@ip
        host: String,
        /// remote workspace root (default: ~/rdev)
        #[arg(long)]
        root: Option<String>,
        /// overwrite an existing entry
        #[arg(long)]
        force: bool,
    },
    /// switch the current server
    Use { name: String },
    /// list servers
    Ls,
    /// remove a server
    Rm { name: String },
    /// provision a server (bash/rsync/mise); defaults to current
    Setup { name: Option<String> },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => {
            Cli::command().print_help()?;
            Ok(())
        }
        Some(Commands::Run(args)) => run_cmd(&args, cli.no_sync),
        Some(Commands::Shell) => shell(cli.no_sync),
        Some(Commands::Sh) => sh(cli.no_sync),
        Some(Commands::Sync) => sync_only(),
        Some(Commands::Config) => {
            let path = Config::path()?;
            println!("{}", path.display());
            if !path.exists() {
                eprintln!("(not created yet; run: rdev server add <name> <host>)");
            }
            Ok(())
        }
        Some(Commands::Server { action }) => server(action.unwrap_or(ServerCmd::Ls)),
        Some(Commands::Pull { path, dest }) => pull(&path, dest.as_deref()),
        Some(Commands::Start { cmd }) => start(&cmd, cli.no_sync),
        Some(Commands::Logs { follow, lines }) => logs(follow, lines),
        Some(Commands::Status) => status(),
    }
}

/// load config + detect project; shared by all project-scoped commands
fn ctx() -> Result<(Server, project::Project)> {
    let cfg = Config::load()?;
    Ok((cfg.current_server()?.clone(), project::detect()?))
}

fn run_cmd(args: &[String], no_sync: bool) -> Result<()> {
    if args.is_empty() {
        bail!("no command given");
    }
    let (server, proj) = ctx()?;
    if !no_sync {
        sync::push(&server, &proj)?;
    }
    let code = ssh::exec(&server, &proj, args)?;
    std::process::exit(code);
}

fn shell(no_sync: bool) -> Result<()> {
    let (server, proj) = ctx()?;
    if !no_sync {
        sync::push(&server, &proj)?;
    }
    let code = ssh::shell(&server, &proj)?;
    std::process::exit(code);
}

fn sync_only() -> Result<()> {
    let (server, proj) = ctx()?;
    sync::push(&server, &proj)
}

fn sh(no_sync: bool) -> Result<()> {
    let (server, proj) = ctx()?;
    if !no_sync {
        sync::push(&server, &proj)?;
    }
    let code = ssh::sh(&server, &proj)?;
    std::process::exit(code);
}

fn pull(path: &str, dest: Option<&str>) -> Result<()> {
    let (server, proj) = ctx()?;
    sync::pull(&server, &proj, path, dest)
}

fn start(cmd: &[String], no_sync: bool) -> Result<()> {
    let (server, proj) = ctx()?;
    if !no_sync {
        sync::push(&server, &proj)?;
    }
    let code = task::start(&server, &proj, cmd)?;
    std::process::exit(code);
}

fn logs(follow: bool, lines: u32) -> Result<()> {
    let (server, proj) = ctx()?;
    let code = task::logs(&server, &proj, lines, follow)?;
    std::process::exit(code);
}

fn status() -> Result<()> {
    let (server, proj) = ctx()?;
    let code = task::status(&server, &proj)?;
    std::process::exit(code);
}

fn server(cmd: ServerCmd) -> Result<()> {
    let mut cfg = Config::load()?;
    match cmd {
        ServerCmd::Add {
            name,
            host,
            root,
            force,
        } => {
            if cfg.servers.contains_key(&name) && !force {
                bail!("server \"{name}\" already exists; use --force to overwrite");
            }
            let root = root.unwrap_or_else(config::default_root);
            validate_root(&root)?;
            cfg.servers.insert(
                name.clone(),
                Server {
                    host: host.clone(),
                    root,
                    shell: None,
                },
            );
            let switched = cfg.current.is_none();
            if switched {
                cfg.current = Some(name.clone());
            }
            cfg.save()?;
            println!("added \"{name}\"");
            if switched {
                println!("switched to \"{name}\"");
            }
            if ssh::probe(&host) {
                println!("connection ok");
                if let Some(sh) = ssh::detect_shell(&host) {
                    if let Some(s) = cfg.servers.get_mut(&name) {
                        s.shell = Some(sh.clone());
                    }
                    cfg.save()?;
                    println!("remote shell: {sh}");
                }
                if !ssh::remote_has(&host, "rsync") {
                    eprintln!("warning: rsync not found on remote; run: rdev server setup");
                }
            } else {
                eprintln!("warning: cannot reach \"{host}\" (saved anyway)");
            }
            Ok(())
        }
        ServerCmd::Use { name } => {
            if !cfg.servers.contains_key(&name) {
                bail!("server \"{name}\" not found; run: rdev server ls");
            }
            cfg.current = Some(name.clone());
            cfg.save()?;
            println!("switched to \"{name}\"");
            Ok(())
        }
        ServerCmd::Ls => {
            if cfg.servers.is_empty() {
                println!("no servers; run: rdev server add <name> <host>");
                return Ok(());
            }
            for (name, s) in &cfg.servers {
                let mark = if cfg.current.as_deref() == Some(name) {
                    "*"
                } else {
                    " "
                };
                println!("{mark} {name:<16} {:<24} {}", s.host, s.root);
            }
            Ok(())
        }
        ServerCmd::Rm { name } => {
            if cfg.servers.remove(&name).is_none() {
                bail!("server \"{name}\" not found");
            }
            let was_current = cfg.current.as_deref() == Some(name.as_str());
            if was_current {
                cfg.current = None;
            }
            cfg.save()?;
            println!("removed \"{name}\"");
            if was_current {
                println!("no current server now; run: rdev server use <name>");
            }
            Ok(())
        }
        ServerCmd::Setup { name } => {
            let key = match &name {
                Some(n) => n.clone(),
                None => cfg
                    .current
                    .clone()
                    .context("no server selected; run: rdev server add <name> <host>")?,
            };
            let server = cfg
                .servers
                .get(&key)
                .with_context(|| format!("server \"{key}\" not found; run: rdev server ls"))?
                .clone();
            let code = setup::setup(&server)?;
            if code == 0 {
                if let Some(sh) = ssh::detect_shell(&server.host) {
                    if let Some(s) = cfg.servers.get_mut(&key) {
                        s.shell = Some(sh);
                    }
                    cfg.save()?;
                }
            }
            std::process::exit(code);
        }
    }
}
