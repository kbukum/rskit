use std::process::Stdio;

use tokio::process::Command as TokioCommand;

use crate::{EnvPolicy, ProcessConfig, ProcessSpec, process_group::isolate_async};

/// The stdin/stdout/stderr configuration a child process is spawned with.
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

    if config.lifecycle.isolate_process_group {
        isolate_async(cmd);
    }
}
