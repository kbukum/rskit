use rskit_errors::AppResult;

use crate::error::GitError;
use crate::types::Oid;

pub(crate) fn parse_oid(hex: &str) -> AppResult<Oid> {
    let hex = hex.trim();
    if hex.len() != 40 {
        return Err(GitError::InvalidOid {
            value: hex.to_string(),
        }
        .into());
    }

    let mut bytes = [0u8; 20];
    for i in 0..20 {
        bytes[i] =
            u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| GitError::InvalidOid {
                value: hex.to_string(),
            })?;
    }

    Ok(Oid::from_bytes(bytes))
}
