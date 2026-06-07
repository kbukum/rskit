//! Human-in-the-loop (HITL) evaluation for tool dispatch.
//!
//! Per the locked AI/ML cross-kit decision D10, every tool invocation flows
//! through stages: authz → sensitivity → (if `RequireApproval`) human approval
//! → invoke. This module defines the `sensitivity` and `approval` stages.
//! `authz` is owned by `rskit_authz::Decider` and wired at the boundary
//! (e.g. `rskit-mcp::Server`), not here, to preserve module layering.

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult};
use rskit_schema::Json;

use crate::context::Context;
use crate::envelope::{Envelope, SensitiveMatcher, SensitivePredicate};
use crate::io::ToolInput;

/// One tool invocation as seen by the HITL stages.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Registered tool name.
    pub name: String,
    /// Validated tool input.
    pub input: ToolInput,
}

/// Sensitivity decision returned by a [`SensitivityEvaluator`].
#[derive(Debug, Clone)]
pub enum Decision {
    /// Proceed to invocation.
    Allow,
    /// Reject with the given reason.
    Deny(String),
    /// Defer to a [`HumanApproval`] before invocation; the reason explains why.
    RequireApproval(String),
}

/// Evaluator for the *sensitivity* stage of HITL.
///
/// Implementations decide whether a tool call is sensitive given the call's
/// input and the tool's declared `Envelope.sensitive_invocations` predicates.
#[async_trait]
pub trait SensitivityEvaluator: Send + Sync {
    /// Evaluate a tool call against the given envelope.
    async fn evaluate(
        &self,
        ctx: &Context,
        call: &ToolCall,
        envelope: &Envelope,
    ) -> AppResult<Decision>;
}

/// Default evaluator that denies any tool call whose input matches one of the
/// envelope's `sensitive_invocations` predicates.
///
/// "Deny on sensitive" is the safe default per D10. To allow such calls,
/// install a custom evaluator that returns `RequireApproval` and pair it with
/// a non-default [`HumanApproval`].
#[derive(Debug, Default, Clone)]
pub struct DenyOnSensitive;

#[async_trait]
impl SensitivityEvaluator for DenyOnSensitive {
    async fn evaluate(
        &self,
        _ctx: &Context,
        call: &ToolCall,
        envelope: &Envelope,
    ) -> AppResult<Decision> {
        for predicate in &envelope.sensitive_invocations {
            if predicate_matches(&call.input, predicate) {
                return Ok(Decision::Deny(format!(
                    "tool {:?} matches sensitive predicate at {:?}",
                    call.name, predicate.jsonpath
                )));
            }
        }
        Ok(Decision::Allow)
    }
}

/// Human approval gate consulted when [`SensitivityEvaluator`] returns
/// [`Decision::RequireApproval`].
#[async_trait]
pub trait HumanApproval: Send + Sync {
    /// Return `true` to proceed with invocation, `false` to deny.
    async fn approve(&self, ctx: &Context, call: &ToolCall, reason: &str) -> AppResult<bool>;
}

/// Default approval gate that always denies.
///
/// Per D10, `DenyHumanApproval` is the canonical default — there is no
/// auto-approval. Replace with a real gate (CLI prompt, web UI hand-off,
/// async ticket queue) at composition time.
#[derive(Debug, Default, Clone)]
pub struct DenyHumanApproval;

#[async_trait]
impl HumanApproval for DenyHumanApproval {
    async fn approve(&self, _ctx: &Context, _call: &ToolCall, _reason: &str) -> AppResult<bool> {
        Ok(false)
    }
}

/// Translate a [`Decision::Deny`] (or post-approval rejection) into a typed
/// `AppError` with the `Forbidden` code.
#[must_use]
pub fn denied_error(reason: impl Into<String>) -> AppError {
    AppError::forbidden(reason.into())
}

fn predicate_matches(input: &ToolInput, predicate: &SensitivePredicate) -> bool {
    let value = match select_jsonpath(input.as_json(), &predicate.jsonpath) {
        Some(v) => v,
        None => return false,
    };
    match &predicate.matcher {
        SensitiveMatcher::Exists => true,
        SensitiveMatcher::Equals(expected) => value == expected,
        SensitiveMatcher::Regex(pattern) => match value.as_str() {
            Some(text) => regex_matches(pattern, text),
            None => false,
        },
        SensitiveMatcher::Gt(threshold) => value.as_f64().is_some_and(|n| n > *threshold),
        SensitiveMatcher::Lt(threshold) => value.as_f64().is_some_and(|n| n < *threshold),
    }
}

fn select_jsonpath<'a>(value: &'a Json, path: &str) -> Option<&'a Json> {
    let trimmed = path.trim();
    let after_root = trimmed.strip_prefix('$').unwrap_or(trimmed);
    let after_root = after_root.strip_prefix('.').unwrap_or(after_root);
    if after_root.is_empty() {
        return Some(value);
    }
    let mut cursor = value;
    for segment in after_root.split('.') {
        if segment.is_empty() {
            return None;
        }
        match cursor {
            Json::Object(map) => {
                cursor = map.get(segment)?;
            }
            _ => return None,
        }
    }
    Some(cursor)
}

fn regex_matches(pattern: &str, text: &str) -> bool {
    // Compile-and-match without pulling in a regex crate dep — this is a small
    // glob-style helper that supports `.` (any char) and `.*` (any run).
    // Implementations that need full PCRE should provide a custom evaluator.
    glob_like_match(pattern, text)
}

fn glob_like_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    fn rec(p: &[char], t: &[char]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (None, Some(_)) => false,
            (Some('.'), Some(_)) if p.get(1) == Some(&'*') => {
                for split in 0..=t.len() {
                    if rec(&p[2..], &t[split..]) {
                        return true;
                    }
                }
                false
            }
            (Some('.'), Some(_)) => rec(&p[1..], &t[1..]),
            (Some(pc), Some(tc)) if pc == tc => rec(&p[1..], &t[1..]),
            _ => false,
        }
    }
    rec(&pat, &txt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{Envelope, SensitiveMatcher, SensitivePredicate};
    use serde_json::json;

    fn call(input: Json) -> ToolCall {
        ToolCall {
            name: "demo".to_owned(),
            input: ToolInput::new(input).unwrap(),
        }
    }

    fn envelope(predicates: Vec<SensitivePredicate>) -> Envelope {
        Envelope {
            sensitive_invocations: predicates,
            ..Envelope::default()
        }
    }

    #[tokio::test]
    async fn deny_on_sensitive_allows_when_no_predicates() {
        let evaluator = DenyOnSensitive;
        let ctx = Context::new();
        let decision = evaluator
            .evaluate(&ctx, &call(json!({"a": 1})), &Envelope::default())
            .await
            .unwrap();
        assert!(matches!(decision, Decision::Allow));
    }

    #[tokio::test]
    async fn deny_on_sensitive_denies_when_exists_predicate_matches() {
        let evaluator = DenyOnSensitive;
        let ctx = Context::new();
        let env = envelope(vec![SensitivePredicate {
            jsonpath: "$.password".to_owned(),
            matcher: SensitiveMatcher::Exists,
        }]);
        let decision = evaluator
            .evaluate(&ctx, &call(json!({"password": "x"})), &env)
            .await
            .unwrap();
        assert!(matches!(decision, Decision::Deny(_)));
    }

    #[tokio::test]
    async fn deny_on_sensitive_allows_when_predicate_misses() {
        let evaluator = DenyOnSensitive;
        let ctx = Context::new();
        let env = envelope(vec![SensitivePredicate {
            jsonpath: "$.password".to_owned(),
            matcher: SensitiveMatcher::Exists,
        }]);
        let decision = evaluator
            .evaluate(&ctx, &call(json!({"name": "alice"})), &env)
            .await
            .unwrap();
        assert!(matches!(decision, Decision::Allow));
    }

    #[tokio::test]
    async fn deny_on_sensitive_uses_equals_matcher() {
        let evaluator = DenyOnSensitive;
        let ctx = Context::new();
        let env = envelope(vec![SensitivePredicate {
            jsonpath: "$.action".to_owned(),
            matcher: SensitiveMatcher::Equals(json!("delete")),
        }]);
        let allow = evaluator
            .evaluate(&ctx, &call(json!({"action": "read"})), &env)
            .await
            .unwrap();
        assert!(matches!(allow, Decision::Allow));
        let deny = evaluator
            .evaluate(&ctx, &call(json!({"action": "delete"})), &env)
            .await
            .unwrap();
        assert!(matches!(deny, Decision::Deny(_)));
    }

    #[tokio::test]
    async fn deny_on_sensitive_uses_gt_matcher() {
        let evaluator = DenyOnSensitive;
        let ctx = Context::new();
        let env = envelope(vec![SensitivePredicate {
            jsonpath: "$.amount".to_owned(),
            matcher: SensitiveMatcher::Gt(100.0),
        }]);
        let deny = evaluator
            .evaluate(&ctx, &call(json!({"amount": 200})), &env)
            .await
            .unwrap();
        assert!(matches!(deny, Decision::Deny(_)));
        let allow = evaluator
            .evaluate(&ctx, &call(json!({"amount": 50})), &env)
            .await
            .unwrap();
        assert!(matches!(allow, Decision::Allow));
    }

    #[tokio::test]
    async fn deny_on_sensitive_uses_lt_and_regex_matchers() {
        let evaluator = DenyOnSensitive;
        let ctx = Context::new();
        let env = envelope(vec![
            SensitivePredicate {
                jsonpath: "$.risk".to_owned(),
                matcher: SensitiveMatcher::Lt(0.25),
            },
            SensitivePredicate {
                jsonpath: "$.email".to_owned(),
                matcher: SensitiveMatcher::Regex(".*@example.com".to_owned()),
            },
        ]);

        let low_risk = evaluator
            .evaluate(&ctx, &call(json!({"risk": 0.1})), &env)
            .await
            .unwrap();
        assert!(matches!(low_risk, Decision::Deny(_)));

        let matching_email = evaluator
            .evaluate(&ctx, &call(json!({"email": "dev@example.com"})), &env)
            .await
            .unwrap();
        assert!(matches!(matching_email, Decision::Deny(_)));

        let allowed = evaluator
            .evaluate(&ctx, &call(json!({"risk": 0.8, "email": "dev.test"})), &env)
            .await
            .unwrap();
        assert!(matches!(allowed, Decision::Allow));
    }

    #[tokio::test]
    async fn deny_on_sensitive_ignores_invalid_or_non_scalar_paths() {
        let evaluator = DenyOnSensitive;
        let ctx = Context::new();
        let env = envelope(vec![
            SensitivePredicate {
                jsonpath: "$.nested.".to_owned(),
                matcher: SensitiveMatcher::Exists,
            },
            SensitivePredicate {
                jsonpath: "$.nested.count".to_owned(),
                matcher: SensitiveMatcher::Gt(1.0),
            },
            SensitivePredicate {
                jsonpath: "$.nested.label".to_owned(),
                matcher: SensitiveMatcher::Regex("secret.*".to_owned()),
            },
        ]);

        let decision = evaluator
            .evaluate(
                &ctx,
                &call(json!({"nested": {"count": "many", "label": 7}})),
                &env,
            )
            .await
            .unwrap();

        assert!(matches!(decision, Decision::Allow));
    }

    #[tokio::test]
    async fn deny_human_approval_returns_false() {
        let approver = DenyHumanApproval;
        let ctx = Context::new();
        let result = approver
            .approve(&ctx, &call(json!({})), "needs review")
            .await
            .unwrap();
        assert!(!result);
    }
}
