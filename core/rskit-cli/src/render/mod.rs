//! Structured, non-interactive terminal display.
//!
//! Everything a command emits when it is *reporting* rather than prompting:
//!
//! - [`table`] — aligned [`OutputTable`]s for row/column data.
//! - [`keyvalue`] — [`OutputKV`] blocks for headers and summaries.
//! - [`error`] — [`OutputFormat`], the shared [`ExitCode`] convention, and an
//!   [`ErrorRenderer`] that turns an `AppError` into consistent text/JSON/YAML.
//! - [`status`] — one-off [`StatusReporter`] feedback lines (success/warn/step/
//!   heading) for guided, multi-step flows.

pub mod error;
pub mod keyvalue;
pub mod status;
pub mod table;

pub use error::{ErrorRenderer, ExitCode, OutputFormat};
pub use keyvalue::OutputKV;
pub use status::StatusReporter;
pub use table::OutputTable;
