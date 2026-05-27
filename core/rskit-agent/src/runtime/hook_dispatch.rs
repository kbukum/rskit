//! Hook emission helpers for runtime lifecycle events.

use rskit_hook::{CancellationToken, Event, HookRegistry};

use crate::hooks;

pub(crate) fn emit_hook<E: Event>(
    hooks: &HookRegistry,
    event: &E,
    token: CancellationToken,
) -> bool {
    match hooks.emit(event, token.clone()) {
        Ok(()) => false,
        Err(error) => {
            let fatal = error.is_fatal();
            let _ = hooks.emit(
                &hooks::OnError {
                    error: error.to_string(),
                    source: event.event_type().to_string(),
                },
                token,
            );
            fatal
        }
    }
}
