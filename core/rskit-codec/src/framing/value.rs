//! Typed value framing: encode/decode one [`Codec`] payload per frame.

use std::io::{Read, Write};

use rskit_errors::{AppError, AppResult};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::codec::{Codec, decode, encode};

use super::frame::{read_frame, write_frame};

/// Encode `value` with `codec` and write it as one length-delimited frame.
///
/// # Errors
///
/// Returns a typed [`AppError`] (cause preserved) if encoding or the frame write fails,
/// or the payload exceeds `max_bytes`.
pub fn write_value<W, T>(
    writer: &mut W,
    codec: &dyn Codec,
    value: &T,
    max_bytes: usize,
) -> AppResult<()>
where
    W: Write,
    T: Serialize + ?Sized,
{
    let text = encode(codec, value)?;
    write_frame(writer, text.as_bytes(), max_bytes)
}

/// Read one frame and decode it into `T` through `codec`.
///
/// Returns `Ok(None)` on a clean end-of-stream between frames.
///
/// # Errors
///
/// Returns a typed [`AppError`] (cause preserved) on a transport failure
/// or a payload that does not decode into `T`.
pub fn read_value<R, T>(reader: &mut R, codec: &dyn Codec, max_bytes: usize) -> AppResult<Option<T>>
where
    R: Read,
    T: DeserializeOwned,
{
    let Some(payload) = read_frame(reader, max_bytes)? else {
        return Ok(None);
    };
    decode_value(codec, &payload).map(Some)
}

/// Decode an already-read frame `payload` into `T` through `codec`.
///
/// Split from [`read_value`]
/// so a caller that must inspect one frame as more than one shape can decode the same bytes without re-reading the stream.
///
/// # Errors
///
/// Returns a typed [`AppError`] (cause preserved) if the payload is not valid UTF-8
/// or does not decode into `T`.
pub fn decode_value<T>(codec: &dyn Codec, payload: &[u8]) -> AppResult<T>
where
    T: DeserializeOwned,
{
    let text = std::str::from_utf8(payload).map_err(|error| {
        AppError::invalid_input("frame", "frame payload is not valid UTF-8").with_cause(error)
    })?;
    decode(codec, text)
}

#[cfg(test)]
mod tests {
    use super::super::frame::DEFAULT_MAX_FRAME_BYTES;
    use super::{read_value, write_value};
    use crate::JsonCodec;

    #[test]
    fn value_round_trips_as_one_frame() {
        let mut buffer = Vec::new();
        let sent = vec!["a".to_string(), "b".to_string()];
        write_value(
            &mut buffer,
            &JsonCodec::default(),
            &sent,
            DEFAULT_MAX_FRAME_BYTES,
        )
        .expect("write");
        let mut cursor = std::io::Cursor::new(buffer);
        let got: Vec<String> =
            read_value(&mut cursor, &JsonCodec::default(), DEFAULT_MAX_FRAME_BYTES)
                .expect("read")
                .expect("present");
        assert_eq!(got, sent);
    }

    #[test]
    fn non_utf8_payload_is_invalid_input() {
        let mut buffer = Vec::new();
        super::write_frame(&mut buffer, &[0xff, 0xfe], DEFAULT_MAX_FRAME_BYTES).expect("write");
        let mut cursor = std::io::Cursor::new(buffer);
        let error =
            read_value::<_, String>(&mut cursor, &JsonCodec::default(), DEFAULT_MAX_FRAME_BYTES)
                .unwrap_err();
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }
}
