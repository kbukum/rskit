//! Process specification, I/O modes, and execution policy.

mod config;
mod io;
mod redaction;
mod signal;
mod spec;

pub use config::ProcessConfig;
pub use io::{
    CapturedIo, DEFAULT_MAX_OUTPUT_BYTES, InheritedIo, InputPolicy, ObservedIo, OutputPolicy,
    ProcessIo,
};
pub use redaction::ArgRedaction;
pub use signal::SignalPolicy;
pub use spec::{EnvPolicy, ProcessSpec, command};

#[cfg(test)]
mod tests;
