use std::io;
use std::process::Stdio;

use tokio::process::Command as TokioCommand;

use crate::{EnvPolicy, ProcessConfig, ProcessSpec};

/// The three OS pipe handles a child process is spawned with.
pub(in crate::runner) struct PipeStdio {
    pub(in crate::runner) stdin: Stdio,
    pub(in crate::runner) stdout: Stdio,
    pub(in crate::runner) stderr: Stdio,
}

pub(in crate::runner) fn configure_command(
    cmd: &mut TokioCommand,
    spec: &ProcessSpec,
    config: &ProcessConfig,
    stdio: PipeStdio,
) {
    cmd.args(&spec.args)
        .stdin(stdio.stdin)
        .stdout(stdio.stdout)
        .stderr(stdio.stderr);

    if let Some(dir) = &spec.dir {
        cmd.current_dir(dir);
    }

    if matches!(spec.env_policy, EnvPolicy::Empty) {
        cmd.env_clear();
    }
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }

    if config.signal.create_process_group {
        isolate(cmd);
    }
}

fn isolate(cmd: &mut TokioCommand) {
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs in the child process after fork and before exec.
    // The closure only calls the async-signal-safe `setpgid` libc function
    // and returns an `io::Error` on failure, which is the supported usage pattern.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}
