/// Transport-agnostic authorization decision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum AuthzDecision {
    /// Request is allowed.
    Allow,
    /// Request is denied with a reason.
    Deny(String),
    /// Request requires human approval.
    RequiresHumanApproval(String),
}

/// Transport-agnostic authorization request for a principal, action, and resource.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthzRequest {
    /// Principal identifier.
    pub principal: String,
    /// Action being performed.
    pub action: String,
    /// Resource identifier.
    pub resource: String,
    /// Scopes relevant to the request.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Additional structured attributes.
    #[serde(default)]
    pub attributes: serde_json::Value,
}

/// Object-safe authorization decider used at integration boundaries.
pub trait Decider: Send + Sync {
    /// Decide one authorization request.
    fn decide(&self, request: &AuthzRequest) -> AuthzDecision;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authz_decision_serde_pins_variant_wire_shape() {
        assert_eq!(serde_json::to_value(AuthzDecision::Allow).unwrap(), "Allow");
        assert_eq!(
            serde_json::to_value(AuthzDecision::Deny("no access".into())).unwrap(),
            serde_json::json!({ "Deny": "no access" })
        );
        assert_eq!(
            serde_json::to_value(AuthzDecision::RequiresHumanApproval("review".into())).unwrap(),
            serde_json::json!({ "RequiresHumanApproval": "review" })
        );

        let deny: AuthzDecision =
            serde_json::from_value(serde_json::json!({ "Deny": "no access" })).unwrap();
        assert_eq!(deny, AuthzDecision::Deny("no access".into()));
        let approval: AuthzDecision =
            serde_json::from_value(serde_json::json!({ "RequiresHumanApproval": "review" }))
                .unwrap();
        assert_eq!(
            approval,
            AuthzDecision::RequiresHumanApproval("review".into())
        );
    }

    #[test]
    fn authz_request_serde_pins_fields_and_defaults() {
        let request: AuthzRequest = serde_json::from_value(serde_json::json!({
            "principal": "user:1",
            "action": "read",
            "resource": "doc:42"
        }))
        .unwrap();

        assert_eq!(request.principal, "user:1");
        assert_eq!(request.action, "read");
        assert_eq!(request.resource, "doc:42");
        assert!(request.scopes.is_empty());
        assert_eq!(request.attributes, serde_json::Value::Null);

        let value = serde_json::to_value(&AuthzRequest {
            principal: "user:1".into(),
            action: "read".into(),
            resource: "doc:42".into(),
            scopes: vec!["docs.read".into()],
            attributes: serde_json::json!({ "tenant": "acme" }),
        })
        .unwrap();

        assert_eq!(value["principal"], "user:1");
        assert_eq!(value["action"], "read");
        assert_eq!(value["resource"], "doc:42");
        assert_eq!(value["scopes"][0], "docs.read");
        assert_eq!(value["attributes"]["tenant"], "acme");
    }
}
