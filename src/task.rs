use anyhow::Result;
use std::path::Path;

use crate::config::Server;
use crate::project::Project;
use crate::ssh;

/// remote per-project task state dir (inside the remote project dir)
const TASK_DIR: &str = ".rdev";

fn remote_dir(server: &Server, proj: &Project) -> String {
    ssh::shell_path(&server.root, &proj.name, Path::new(""))
}

/// start a background task; refuses while a previous one is still running
pub fn start(server: &Server, proj: &Project, args: &[String]) -> Result<i32> {
    let dir = remote_dir(server, proj);
    let cmdline = args
        .iter()
        .map(|a| ssh::shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ");
    let mut inner = cmdline.clone();
    if proj.uses_mise() {
        inner = format!("export PATH=\"$HOME/.local/bin:$PATH\"; {inner}");
    }
    // exit-code capture differs per shell: fish has no $? or VAR=value syntax
    let epilogue = if server.shell().ends_with("fish") {
        format!("set code $status; echo $code > {TASK_DIR}/task.exit")
    } else {
        format!("code=$?; echo $code > {TASK_DIR}/task.exit")
    };
    inner = format!("{inner}; {epilogue}");
    let sh = ssh::shell_quote(server.shell());
    let script = format!(
        "cd {dir} || exit 1\n\
         mkdir -p {TASK_DIR}\n\
         if [ -f {TASK_DIR}/task.pid ] && kill -0 \"$(cat {TASK_DIR}/task.pid)\" 2>/dev/null; then\n\
           echo \"task already running (pid $(cat {TASK_DIR}/task.pid))\"\n\
           exit 2\n\
         fi\n\
         rm -f {TASK_DIR}/task.exit\n\
         printf '%s\\n' {cmd} > {TASK_DIR}/task.cmd\n\
         nohup {sh} -lc {inner} > {TASK_DIR}/task.log 2>&1 < /dev/null &\n\
         echo $! > {TASK_DIR}/task.pid\n\
         echo \"started (pid $(cat {TASK_DIR}/task.pid)), log: {TASK_DIR}/task.log\"\n",
        cmd = ssh::shell_quote(&cmdline),
        inner = ssh::shell_quote(&inner),
    );
    ssh::run_sh(server, &script)
}

/// status of the latest task. exit code: task's code when done, 2 running, 3 none
pub fn status(server: &Server, proj: &Project) -> Result<i32> {
    let dir = remote_dir(server, proj);
    let script = format!(
        "cd {dir} || exit 1\n\
         if [ ! -f {TASK_DIR}/task.pid ]; then\n\
           echo \"no task\"\n\
           exit 3\n\
         fi\n\
         pid=$(cat {TASK_DIR}/task.pid)\n\
         cmd=$(cat {TASK_DIR}/task.cmd 2>/dev/null)\n\
         if [ -f {TASK_DIR}/task.exit ]; then\n\
           code=$(cat {TASK_DIR}/task.exit)\n\
           echo \"done (exit $code): $cmd\"\n\
           exit \"$code\"\n\
         elif kill -0 \"$pid\" 2>/dev/null; then\n\
           echo \"running (pid $pid): $cmd\"\n\
           exit 2\n\
         else\n\
           echo \"finished without exit record (pid $pid gone): $cmd\"\n\
           exit 1\n\
         fi\n"
    );
    ssh::run_sh(server, &script)
}

/// tail the latest task log
pub fn logs(server: &Server, proj: &Project, lines: u32, follow: bool) -> Result<i32> {
    let dir = remote_dir(server, proj);
    let f = if follow { " -f" } else { "" };
    let script = format!(
        "cd {dir} || exit 1\n\
         if [ ! -f {TASK_DIR}/task.log ]; then\n\
           echo \"no task log\"\n\
           exit 1\n\
         fi\n\
         tail -n {lines}{f} {TASK_DIR}/task.log\n"
    );
    ssh::run_sh(server, &script)
}
