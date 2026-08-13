use rskit_errors::{AppError, AppResult, ErrorCode};
use std::collections::HashSet;

use super::ShutdownSignal;

pub(super) struct SignalStreams {
    streams: Vec<tokio::signal::unix::Signal>,
}

impl SignalStreams {
    pub(super) fn install(signals: &[ShutdownSignal]) -> AppResult<Self> {
        let mut streams = Vec::new();
        for (signal, number) in unique_signal_numbers(signals) {
            streams.push(install_signal(
                signal,
                tokio::signal::unix::SignalKind::from_raw(number),
            )?);
        }

        Ok(Self { streams })
    }

    pub(super) async fn recv(&mut self) {
        recv_any(&mut self.streams).await;
    }
}

fn unique_signal_numbers(signals: &[ShutdownSignal]) -> Vec<(ShutdownSignal, i32)> {
    let mut seen = HashSet::new();
    signals
        .iter()
        .filter_map(|signal| {
            let number = match *signal {
                ShutdownSignal::Interrupt => libc::SIGINT,
                ShutdownSignal::Terminate => libc::SIGTERM,
                ShutdownSignal::Hangup => libc::SIGHUP,
                ShutdownSignal::UnixRaw(number) => number,
            };
            seen.insert(number).then_some((*signal, number))
        })
        .collect()
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

async fn recv_any(streams: &mut [tokio::signal::unix::Signal]) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_signal_numbers_are_canonicalized() {
        let signals = [
            ShutdownSignal::Interrupt,
            ShutdownSignal::unix_raw(libc::SIGINT),
            ShutdownSignal::Terminate,
            ShutdownSignal::unix_raw(libc::SIGTERM),
            ShutdownSignal::Hangup,
            ShutdownSignal::unix_raw(libc::SIGHUP),
        ];

        assert_eq!(
            unique_signal_numbers(&signals),
            vec![
                (ShutdownSignal::Interrupt, libc::SIGINT),
                (ShutdownSignal::Terminate, libc::SIGTERM),
                (ShutdownSignal::Hangup, libc::SIGHUP),
            ]
        );
    }
}
