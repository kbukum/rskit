use std::io::ErrorKind;
use std::process::Stdio;

use tokio::{process::ChildStdin, task::JoinHandle};

use crate::{
    AppError, AppResult, CapturedIo, ErrorCode, InputPolicy, ObservedIo, OutputPolicy,
    ProcessConfig,
};

pub(super) trait PipeIo {
    fn input(&self) -> &InputPolicy;
    fn output(&self) -> &OutputPolicy;
}

impl PipeIo for CapturedIo {
    fn input(&self) -> &InputPolicy {
        &self.input
    }

    fn output(&self) -> &OutputPolicy {
        &self.output
    }
}

impl PipeIo for ObservedIo {
    fn input(&self) -> &InputPolicy {
        &self.input
    }

    fn output(&self) -> &OutputPolicy {
        &self.output
    }
}

pub(super) fn stdin_stdio(input: &InputPolicy) -> Stdio {
    match input {
        InputPolicy::Closed => Stdio::null(),
        InputPolicy::Bytes(_) => Stdio::piped(),
        InputPolicy::Inherit => Stdio::inherit(),
    }
}

pub(super) fn pipe_stdin_stdio(input: &InputPolicy) -> AppResult<Stdio> {
    match input {
        InputPolicy::Closed => Ok(Stdio::null()),
        InputPolicy::Bytes(_) => Ok(Stdio::piped()),
        InputPolicy::Inherit => Err(AppError::invalid_input(
            "process.io.input",
            "inherited stdin requires inherited I/O mode; pipe-backed interactive stdin is not supported",
        )),
    }
}

pub(super) fn inherited_config(config: &ProcessConfig) -> ProcessConfig {
    let mut config = config.clone();
    config.lifecycle = config
        .lifecycle
        .with_isolate_process_group(false)
        .with_terminate_descendants(false);
    config
}

pub(super) fn spawn_stdin_writer(
    stdin: Option<ChildStdin>,
    input: &InputPolicy,
) -> Option<JoinHandle<AppResult<()>>> {
    let InputPolicy::Bytes(bytes) = input else {
        return None;
    };
    let mut stdin = stdin?;
    let bytes = bytes.clone();
    Some(tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;

        match stdin.write_all(&bytes).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(AppError::new(
                ErrorCode::Internal,
                format!("failed to write to stdin: {error}"),
            )),
        }
    }))
}
