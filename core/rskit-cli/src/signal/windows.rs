use rskit_errors::{AppError, AppResult, ErrorCode};

use super::ShutdownSignal;

pub(super) struct SignalStreams {
    ctrl_c: Option<tokio::signal::windows::CtrlC>,
    ctrl_close: Option<tokio::signal::windows::CtrlClose>,
}

impl SignalStreams {
    pub(super) fn install(signals: &[ShutdownSignal]) -> AppResult<Self> {
        let mut streams = Self {
            ctrl_c: None,
            ctrl_close: None,
        };

        for signal in signals {
            match *signal {
                ShutdownSignal::Interrupt => {
                    streams.ctrl_c = Some(
                        tokio::signal::windows::ctrl_c()
                            .map_err(|err| signal_error(*signal, err))?,
                    );
                }
                ShutdownSignal::Close => {
                    streams.ctrl_close = Some(
                        tokio::signal::windows::ctrl_close()
                            .map_err(|err| signal_error(*signal, err))?,
                    );
                }
            }
        }

        Ok(streams)
    }

    pub(super) async fn recv(&mut self) {
        tokio::select! {
            () = recv_ctrl_c(&mut self.ctrl_c) => {}
            () = recv_ctrl_close(&mut self.ctrl_close) => {}
        }
    }
}

fn signal_error(signal: ShutdownSignal, err: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("install shutdown signal stream for {signal:?}"),
    )
    .with_cause(err)
}

async fn recv_ctrl_c(stream: &mut Option<tokio::signal::windows::CtrlC>) {
    match stream {
        Some(stream) => {
            let _ = stream.recv().await;
        }
        None => std::future::pending::<()>().await,
    }
}

async fn recv_ctrl_close(stream: &mut Option<tokio::signal::windows::CtrlClose>) {
    match stream {
        Some(stream) => {
            let _ = stream.recv().await;
        }
        None => std::future::pending::<()>().await,
    }
}
