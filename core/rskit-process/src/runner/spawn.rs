use std::io;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command as TokioCommand};

use crate::{EnvPolicy, ProcessConfig, ProcessSpec, process_group::isolate_async};

/// Spawn `cmd`, retrying transient `ETXTBSY` ("text file busy") failures.
///
/// Executing a file that was just written races against concurrent
/// `fork`/`exec` on other threads: a peer that forked while this process still
/// held a writable descriptor to the target keeps that descriptor open in its
/// child until it execs, so the kernel reports `ETXTBSY` on our `exec`. The
/// window is microseconds and closes on its own, so a bounded backoff turns the
/// spurious failure into a successful spawn. Rust's std leaves this to the
/// caller; Go's runtime performs the same retry. Non-`ETXTBSY` errors (missing
/// or non-executable binary) surface immediately and are never masked.
#[cfg(unix)]
pub(in crate::runner) async fn spawn_with_etxtbsy_retry(
    cmd: &mut TokioCommand,
) -> io::Result<Child> {
    spawn_retrying_etxtbsy(|| cmd.spawn()).await
}

/// Retry `spawn` on transient `ETXTBSY` with bounded exponential backoff.
///
/// Split from [`spawn_with_etxtbsy_retry`] so the policy can be exercised with
/// an injected spawn result instead of a live `fork`/`exec` race.
#[cfg(unix)]
async fn spawn_retrying_etxtbsy<F>(mut spawn: F) -> io::Result<Child>
where
    F: FnMut() -> io::Result<Child>,
{
    const MAX_ATTEMPTS: u32 = 10;
    const MAX_BACKOFF: Duration = Duration::from_millis(50);

    let mut backoff = Duration::from_millis(1);
    for _ in 1..MAX_ATTEMPTS {
        match spawn() {
            Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) => {
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
            result => return result,
        }
    }
    spawn()
}

/// Non-Unix platforms do not surface `ETXTBSY`, so spawn once.
#[cfg(not(unix))]
pub(in crate::runner) async fn spawn_with_etxtbsy_retry(
    cmd: &mut TokioCommand,
) -> io::Result<Child> {
    cmd.spawn()
}

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

#[cfg(all(test, unix))]
mod tests {
    use std::cell::Cell;
    use std::io;

    use super::{Child, TokioCommand, spawn_retrying_etxtbsy};

    fn etxtbsy() -> io::Error {
        io::Error::from_raw_os_error(libc::ETXTBSY)
    }

    #[tokio::test(start_paused = true)]
    async fn retries_transient_etxtbsy_until_the_spawn_succeeds() {
        let attempts = Cell::new(0u32);
        let child = spawn_retrying_etxtbsy(|| {
            let attempt = attempts.get() + 1;
            attempts.set(attempt);
            if attempt < 3 {
                Err(etxtbsy())
            } else {
                TokioCommand::new("/bin/sh").arg("-c").arg("exit 0").spawn()
            }
        })
        .await
        .expect("spawn succeeds once the busy window closes");

        assert_eq!(
            attempts.get(),
            3,
            "spawn is retried past the ETXTBSY window"
        );
        let mut child = child;
        child.wait().await.expect("child is reaped");
    }

    #[tokio::test(start_paused = true)]
    async fn non_etxtbsy_errors_are_not_retried() {
        let attempts = Cell::new(0u32);
        let error = spawn_retrying_etxtbsy(|| {
            attempts.set(attempts.get() + 1);
            Err::<Child, _>(io::Error::from_raw_os_error(libc::ENOENT))
        })
        .await
        .expect_err("a missing binary is surfaced immediately");

        assert_eq!(error.raw_os_error(), Some(libc::ENOENT));
        assert_eq!(attempts.get(), 1, "non-ETXTBSY failures are not retried");
    }
}
