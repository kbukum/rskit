#![allow(missing_docs)]
#![cfg(feature = "server")]

use std::sync::Arc;

use parking_lot::Mutex;
use rskit_authz::{AuthzDecision, AuthzRequest, Decider};
use rskit_mcp::{
    DeciderToolAuthorizer, TRANSPORT_STDIO, TRANSPORT_STREAMABLE_HTTP, ToolAuthorizationRequest,
    ToolAuthorizer, TransportKind, streamable_http_server_config,
};
use rskit_tool::ToolInput;
use serde_json::json;

struct CapturingDecider {
    decision: AuthzDecision,
    seen: Mutex<Vec<AuthzRequest>>,
}

impl CapturingDecider {
    fn new(decision: AuthzDecision) -> Self {
        Self {
            decision,
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl Decider for CapturingDecider {
    fn decide(&self, request: &AuthzRequest) -> AuthzDecision {
        self.seen.lock().push(request.clone());
        self.decision.clone()
    }
}

fn request() -> ToolAuthorizationRequest {
    ToolAuthorizationRequest {
        tool_name: "lookup".to_owned(),
        mcp_name: "kb.lookup".to_owned(),
        arguments: ToolInput::new(json!({"q":"rust"})).unwrap(),
    }
}

#[test]
fn transport_kind_parses_canonical_names_and_rejects_unknown_values() {
    assert_eq!(
        TransportKind::try_from(TRANSPORT_STDIO).unwrap(),
        TransportKind::Stdio
    );
    assert_eq!(
        TransportKind::try_from(TRANSPORT_STREAMABLE_HTTP).unwrap(),
        TransportKind::StreamableHttp
    );
    assert_eq!(TransportKind::Stdio.as_str(), "stdio");
    assert!(
        TransportKind::try_from("websocket")
            .unwrap_err()
            .contains("unsupported MCP transport")
    );
}

#[test]
fn streamable_http_config_validates_origin_boundaries() {
    let config = streamable_http_server_config(["localhost"], ["https://app.example.com"]).unwrap();
    assert_eq!(config.allowed_hosts, vec!["localhost"]);
    assert_eq!(config.allowed_origins, vec!["https://app.example.com"]);
    assert!(
        streamable_http_server_config(["localhost"], ["ftp://example.com"])
            .unwrap_err()
            .contains("scheme")
    );
    assert!(
        streamable_http_server_config(["localhost"], ["https://user@example.com"])
            .unwrap_err()
            .contains("credentials")
    );
    assert!(
        streamable_http_server_config(["localhost"], ["https://example.com/path"])
            .unwrap_err()
            .contains("path")
    );
    assert!(
        streamable_http_server_config(["localhost"], ["https://example.com?x=1"])
            .unwrap_err()
            .contains("query")
    );
}

#[tokio::test]
async fn decider_authorizer_maps_requests_and_decisions() {
    let decider = Arc::new(CapturingDecider::new(AuthzDecision::Allow));
    let auth = DeciderToolAuthorizer::new(decider.clone())
        .with_action("custom/call")
        .with_principal("alice");
    let decision = auth.authorize_tool(&request()).await.unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.reason, "allow");
    let seen = decider.seen.lock();
    assert_eq!(seen[0].principal, "alice");
    assert_eq!(seen[0].action, "custom/call");
    assert_eq!(seen[0].resource, "mcp:tool:lookup");
    assert_eq!(seen[0].attributes["mcp_name"], "kb.lookup");
    assert_eq!(seen[0].attributes["arguments"], json!({"q":"rust"}));
}

#[tokio::test]
async fn decider_authorizer_converts_denials_and_approval_to_deny() {
    let deny = DeciderToolAuthorizer::new(Arc::new(CapturingDecider::new(AuthzDecision::Deny(
        "no".into(),
    ))));
    let denied = deny.authorize_tool(&request()).await.unwrap();
    assert!(!denied.allowed);
    assert_eq!(denied.reason, "no");

    let approval = DeciderToolAuthorizer::new(Arc::new(CapturingDecider::new(
        AuthzDecision::RequiresHumanApproval("gate".into()),
    )));
    let gated = approval.authorize_tool(&request()).await.unwrap();
    assert!(!gated.allowed);
    assert!(gated.reason.contains("requires human approval"));
}
