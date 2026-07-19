//! Bounded length-delimited framing for streaming structured-text payloads.
//!
//! A frame is a 4-byte big-endian unsigned length prefix followed by exactly that many payload bytes.
//! Every read is bounded by an explicit maximum so a malformed
//! or hostile peer can never make a reader allocate without limit.
//! Use this to carry one [`Codec`](crate::Codec)-encoded value per frame over any blocking `Read`/`Write` transport (a pipe, a socket, a subprocess's stdio).
//!
//! [`write_value`] / [`read_value`] encode and decode typed values through an injected codec;
//! [`write_frame`] / [`read_frame`] move raw payload bytes when the caller owns serialization.

mod frame;
mod value;

pub use frame::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame};
pub use value::{decode_value, read_value, write_value};
