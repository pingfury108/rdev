mod config;
mod project;
mod setup;
mod ssh;
mod sync;

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
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// sync project and open a remote shell in the project dir
    Shell,
    /// sync project to the remote server only
    Sync,
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
        Some(Commands::Run(args)) => run_cmd(&args),
        Some(Commands::Shell) => shell(),
        Some(Commands::Sync) => sync_only(),
        Some(Commands::Server { action }) => server(action.unwrap_or(ServerCmd::Ls)),
    }
}

fn run_cmd(args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("no command given");
    }
    let cfg = Config::load()?;
    let server = cfg.current_server()?;
    let proj = project::detect()?;
    sync::push(server, &proj)?;
    let code = ssh::exec(server, &proj, args)?;
    std::process::exit(code);
}

fn shell() -> Result<()> {
    let cfg = Config::load()?;
    let server = cfg.current_server()?;
    let proj = project::detect()?;
    sync::push(server, &proj)?;
    let code = ssh::shell(server, &proj)?;
    std::process::exit(code);
}

fn sync_only() -> Result<()> {
    let cfg = Config::load()?;
    let server = cfg.current_server()?;
    let proj = project::detect()?;
    sync::push(server, &proj)
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
