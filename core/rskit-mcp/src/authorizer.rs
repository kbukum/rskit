//! Default [`ToolAuthorizer`] implementation backed by `rskit-authz`.
//!
//! Maps an MCP `ToolAuthorizationRequest` to a canonical `AuthzRequest`
//! so the injected [`Decider`] can enforce policies for every `tools/call`.
//!
//! The mapping is fixed (D14):
//!   - `principal`  ← authorizer configuration, defaulting to `"anonymous"`
//!   - `action`     ← `"tools/call"` (or the override supplied by the caller)
//!   - `resource`   ← `format!("mcp:tool:{}", request.tool_name)`
//!   - `attributes` ← `{ "mcp_name": ..., "arguments": ... }`

use std::sync::Arc;

use async_trait::async_trait;
use rskit_authz::{AuthzDecision, AuthzRequest, Decider};
use serde_json::json;

use crate::authz::{ToolAuthorizationDecision, ToolAuthorizationRequest, ToolAuthorizer};

/// Default tool authorizer that delegates to a canonical `Decider`.
pub struct DeciderToolAuthorizer {
    decider: Arc<dyn Decider>,
    action: String,
    principal: String,
}

impl DeciderToolAuthorizer {
    /// Construct a new authorizer with the canonical MCP action `tools/call`.
    /// `principal` is used when the request does not carry one (anonymous transports).
    /// Use [`Self::with_principal`] to override.
    #[must_use]
    pub fn new(decider: Arc<dyn Decider>) -> Self {
        Self {
            decider,
            action: "tools/call".to_owned(),
            principal: String::new(),
        }
    }

    /// Override the MCP action used in the canonical request.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }

    /// Override the default principal used when the request lacks one.
    #[must_use]
    pub fn with_principal(mut self, principal: impl Into<String>) -> Self {
        self.principal = principal.into();
        self
    }

    fn build_request(&self, request: &ToolAuthorizationRequest) -> AuthzRequest {
        AuthzRequest {
            principal: if self.principal.is_empty() {
                "anonymous".to_owned()
            } else {
                self.principal.clone()
            },
            action: self.action.clone(),
            resource: format!("mcp:tool:{}", request.tool_name),
            scopes: Vec::new(),
            attributes: json!({
                "mcp_name": request.mcp_name,
                "arguments": request.arguments,
            }),
        }
    }
}

#[async_trait]
impl ToolAuthorizer for DeciderToolAuthorizer {
    async fn authorize_tool(
        &self,
        request: &ToolAuthorizationRequest,
    ) -> Result<ToolAuthorizationDecision, String> {
        let canonical = self.build_request(request);
        let decision = self.decider.decide(&canonical);
        Ok(match decision {
            AuthzDecision::Allow => ToolAuthorizationDecision {
                allowed: true,
                reason: "allow".to_owned(),
            },
            AuthzDecision::Deny(reason) => ToolAuthorizationDecision {
                allowed: false,
                reason,
            },
            AuthzDecision::RequiresHumanApproval(reason) => ToolAuthorizationDecision {
                allowed: false,
                reason: format!("requires human approval: {reason}"),
            },
            other => ToolAuthorizationDecision {
                allowed: false,
                reason: format!("unsupported authz decision: {other:?}"),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct StaticDecider {
        decision: AuthzDecision,
    }
    impl Decider for StaticDecider {
        fn decide(&self, _req: &AuthzRequest) -> AuthzDecision {
            self.decision.clone()
        }
    }

    fn req(name: &str) -> ToolAuthorizationRequest {
        ToolAuthorizationRequest {
            tool_name: name.to_owned(),
            mcp_name: name.to_owned(),
            arguments: rskit_tool::ToolInput::empty(),
        }
    }

    #[tokio::test]
    async fn allow_passes_through() {
        let auth = DeciderToolAuthorizer::new(Arc::new(StaticDecider {
            decision: AuthzDecision::Allow,
        }));
        let d = auth.authorize_tool(&req("ping")).await.unwrap();
        assert!(d.allowed);
    }

    #[tokio::test]
    async fn deny_returns_reason() {
        let auth = DeciderToolAuthorizer::new(Arc::new(StaticDecider {
            decision: AuthzDecision::Deny("nope".into()),
        }));
        let d = auth.authorize_tool(&req("ping")).await.unwrap();
        assert!(!d.allowed);
        assert_eq!(d.reason, "nope");
    }

    #[tokio::test]
    async fn requires_human_approval_is_treated_as_deny_at_authz_stage() {
        let auth = DeciderToolAuthorizer::new(Arc::new(StaticDecider {
            decision: AuthzDecision::RequiresHumanApproval("policy".into()),
        }));
        let d = auth.authorize_tool(&req("ping")).await.unwrap();
        assert!(!d.allowed);
        assert!(d.reason.contains("policy"));
    }

    #[tokio::test]
    async fn maps_resource_with_mcp_prefix() {
        struct Capture;
        impl Decider for Capture {
            fn decide(&self, req: &AuthzRequest) -> AuthzDecision {
                if req.resource == "mcp:tool:ping" && req.action == "tools/call" {
                    AuthzDecision::Allow
                } else {
                    AuthzDecision::Deny(format!(
                        "unexpected: action={} resource={}",
                        req.action, req.resource
                    ))
                }
            }
        }
        let auth = DeciderToolAuthorizer::new(Arc::new(Capture));
        let d = auth.authorize_tool(&req("ping")).await.unwrap();
        assert!(d.allowed, "{}", d.reason);
    }
}
