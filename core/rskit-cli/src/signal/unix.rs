use rskit_errors::{AppError, AppResult, ErrorCode};

use super::ShutdownSignal;

pub(super) struct SignalStreams {
    interrupt: Option<tokio::signal::unix::Signal>,
    terminate: Option<tokio::signal::unix::Signal>,
    hangup: Option<tokio::signal::unix::Signal>,
    raw: Vec<tokio::signal::unix::Signal>,
}

impl SignalStreams {
    pub(super) fn install(signals: &[ShutdownSignal]) -> AppResult<Self> {
        let mut streams = Self {
            interrupt: None,
            terminate: None,
            hangup: None,
            raw: Vec::new(),
        };

        for signal in signals {
            match *signal {
                ShutdownSignal::Interrupt => {
                    streams.interrupt = Some(install_signal(
                        *signal,
                        tokio::signal::unix::SignalKind::interrupt(),
                    )?);
                }
                ShutdownSignal::Terminate => {
                    streams.terminate = Some(install_signal(
                        *signal,
                        tokio::signal::unix::SignalKind::terminate(),
                    )?);
                }
                ShutdownSignal::Hangup => {
                    streams.hangup = Some(install_signal(
                        *signal,
                        tokio::signal::unix::SignalKind::hangup(),
                    )?);
                }
                ShutdownSignal::UnixRaw(raw) => {
                    streams.raw.push(install_signal(
                        *signal,
                        tokio::signal::unix::SignalKind::from_raw(raw),
                    )?);
                }
            }
        }

        Ok(streams)
    }

    pub(super) async fn recv(&mut self) {
        tokio::select! {
            () = recv_optional(&mut self.interrupt) => {}
            () = recv_optional(&mut self.terminate) => {}
            () = recv_optional(&mut self.hangup) => {}
            () = recv_raw(&mut self.raw) => {}
        }
    }
}

fn install_signal(
    signal: ShutdownSignal,
    kind: tokio::signal::unix::SignalKind,
) -> AppResult<tokio::signal::unix::Signal> {
    tokio::signal::unix::signal(kind).map_err(|err| {
        AppError::new(
            ErrorCode::Internal,
            format!("install shutdown signal stream for {signal:?}"),
        )
        .with_cause(err)
    })
}

async fn recv_optional(stream: &mut Option<tokio::signal::unix::Signal>) {
    match stream {
        Some(stream) => {
            let _ = stream.recv().await;
        }
        None => std::future::pending::<()>().await,
    }
}

async fn recv_raw(streams: &mut [tokio::signal::unix::Signal]) {
    if streams.is_empty() {
        std::future::pending::<()>().await;
        return;
    }

    std::future::poll_fn(|cx| {
        for stream in streams.iter_mut() {
            if std::pin::Pin::new(stream).poll_recv(cx).is_ready() {
                return std::task::Poll::Ready(());
            }
        }
        std::task::Poll::Pending
    })
    .await;
}
