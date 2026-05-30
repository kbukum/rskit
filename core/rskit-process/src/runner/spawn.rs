use std::io;

use tokio::process::Command as TokioCommand;

use crate::{Command, ProcessConfig};

pub(in crate::runner) fn configure_command(
    cmd: &mut TokioCommand,
    command: &Command,
    config: &ProcessConfig,
    observe_stdout: bool,
    observe_stderr: bool,
) {
    cmd.args(&command.args);

    if let Some(dir) = &command.dir {
        cmd.current_dir(dir);
    }

    if command.scrub_env || !config.inherit_env {
        cmd.env_clear();
    }
    for (key, value) in &command.env {
        cmd.env(key, value);
    }

    if config.capture_output || observe_stdout {
        cmd.stdout(std::process::Stdio::piped());
    }
    if config.capture_output || observe_stderr {
        cmd.stderr(std::process::Stdio::piped());
    }
    if command.stdin.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    } else {
        cmd.stdin(std::process::Stdio::null());
    }

    isolate(cmd);
}

fn isolate(cmd: &mut TokioCommand) {
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs in the child process after fork and before exec.
    // The closure only calls the async-signal-safe `setpgid` libc function and
    // returns an `io::Error` on failure, which is the supported usage pattern.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}
