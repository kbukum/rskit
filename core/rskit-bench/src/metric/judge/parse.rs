//! Untrusted judge-reply parsing and the trust-boundary error contract.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::FinishReason;

use super::verdict::JudgeVerdict;

/// Hard cap on the byte length of a judge reply accepted for parsing.
///
/// `max_tokens` is only a *request* hint the provider may ignore, so it does not bound the response an untrusted or misbehaving judge actually returns. This is the local resource boundary on model output: a reply longer than this is rejected as a typed [`ErrorCode::InvalidInput`] error before it is parsed, rather than being copied and deserialized in full. The verdict is a tiny JSON object, so this bound is generous while still finite.
pub(crate) const MAX_REPLY_BYTES: usize = 64 * 1024;

/// Rejects a completion that did not end cleanly before its body is trusted as a verdict.
///
/// Only a natural stop is treated as a complete reply. A [`FinishReason::Length`] truncation, a [`FinishReason::ContentFilter`] block, a provider [`FinishReason::Error`] or [`FinishReason::Cancelled`], or an unexpected [`FinishReason::ToolUse`] (the judge is called without tools) means generation did not finish normally, so its body — even if it happens to be valid JSON — is an untrusted-reply fault (external-service, see [`invalid_judge_reply`]) rather than a score. A `None` reason means the provider did not report one; that is accepted rather than fabricated into a failure, since parsing still rejects any body that is not a well-formed verdict.
pub(crate) fn ensure_complete_reason(stop_reason: Option<FinishReason>) -> AppResult<()> {
    match stop_reason {
        None | Some(FinishReason::Stop) => Ok(()),
        Some(reason) => Err(invalid_judge_reply(format!(
            "llm_judge: judge completion did not finish normally (stop reason {reason:?}); its reply is not a trustworthy verdict"
        ))),
    }
}

/// Builds a typed error for an untrusted judge *reply* fault.
///
/// The caller supplied a valid request; the untrusted model returned an unusable response (non-JSON, missing/out-of-range score, incomplete finish, over-long, duplicate verdict key, or an incomparable model resolution). That is a fault of the external judge service, not caller input, so it carries [`ErrorCode::ExternalService`] (a 5xx-equivalent). Provider *transport* faults (timeout, cancellation, unavailability) are not routed here — they already carry their own codes from the resilience policy and provider.
pub(crate) fn invalid_judge_reply(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::ExternalService, message)
}

/// Rejects a verdict object that repeats a top-level `score` or `rationale` key.
///
/// JSON binds struct fields case-insensitively and `serde_json` keeps the last value for a repeated key, so `{"score":0,"score":1}` — or a case variant like `{"score":0,"Score":1}` — would let an untrusted judge silently choose its own score. Only the top-level object is scanned and only the two verdict fields are folded; nested objects are opaque judge metadata and are left alone, matching gokit's precise scope. A body that is not a JSON object is left for [`parse_verdict`] to reject with its canonical message.
fn reject_duplicate_verdict_keys(body: &str) -> AppResult<()> {
    struct DuplicateKeyScan;

    impl<'de> serde::de::Visitor<'de> for DuplicateKeyScan {
        type Value = Option<&'static str>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a JSON verdict object")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            // Drain the whole object before returning so the underlying parser reaches the closing `}`; returning early would leave it mid-token and mask the scan behind a spurious parse error. Only the first duplicate is reported.
            let mut duplicate: Option<&'static str> = None;
            let mut seen_score = false;
            let mut seen_rationale = false;
            while let Some(key) = map.next_key::<String>()? {
                match key.to_ascii_lowercase().as_str() {
                    "score" => {
                        if seen_score {
                            duplicate.get_or_insert("score");
                        }
                        seen_score = true;
                    }
                    "rationale" => {
                        if seen_rationale {
                            duplicate.get_or_insert("rationale");
                        }
                        seen_rationale = true;
                    }
                    _ => {}
                }
                map.next_value::<serde::de::IgnoredAny>()?;
            }
            Ok(duplicate)
        }
    }

    let mut de = serde_json::Deserializer::from_str(body);
    // A non-object body makes `deserialize_map` fail; defer that to `parse_verdict`, which owns the canonical "not a valid JSON verdict" message.
    use serde::Deserializer as _;
    if let Ok(Some(duplicate)) = de.deserialize_map(DuplicateKeyScan) {
        return Err(invalid_judge_reply(format!(
            "llm_judge: judge reply repeats the `{duplicate}` verdict key; an untrusted judge must not choose its score by duplicating a field"
        )));
    }
    Ok(())
}

/// Parses an untrusted judge reply into a validated [`JudgeVerdict`].
///
/// The reply is expected to be a single JSON object; a surrounding Markdown code fence is tolerated, but any other prose, a non-JSON body, a repeated top-level `score`/`rationale` key, a missing `score`, or a non-finite or out-of-range score is an untrusted-reply fault (external-service, see [`invalid_judge_reply`]) rather than a trusted score. This is the trust boundary for model output: it enforces the reply shape and score range, but it is not a prompt-injection detector — a syntactically valid `{"score": 1}` still parses, so the injection defense is the data-only framing in the prompt, not this parser.
pub(crate) fn parse_verdict(reply: &str) -> AppResult<JudgeVerdict> {
    let body = strip_code_fence(reply.trim());
    reject_duplicate_verdict_keys(body)?;
    let verdict: JudgeVerdict = serde_json::from_str(body).map_err(|error| {
        invalid_judge_reply(format!(
            "llm_judge: judge reply was not a valid JSON verdict: {error}"
        ))
        .with_cause(error)
    })?;
    if !verdict.score.is_finite() || !(0.0..=1.0).contains(&verdict.score) {
        return Err(invalid_judge_reply(format!(
            "llm_judge: judge score {} is out of the required range [0, 1]",
            verdict.score
        )));
    }
    Ok(verdict)
}

/// Strips a single surrounding Markdown code fence (```` ```json `` … `` ``` ````), returning the inner body; input without a fence is returned unchanged.
fn strip_code_fence(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    // Drop an optional language tag on the opening fence line (for example ```json).
    let after_open = rest.find('\n').map_or(rest, |newline| &rest[newline + 1..]);
    after_open
        .trim_end()
        .strip_suffix("```")
        .map_or(text, str::trim)
}
