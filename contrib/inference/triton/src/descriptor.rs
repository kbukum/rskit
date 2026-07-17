use rskit_authz::{AuthzDecision, AuthzRequest, Decider};
use rskit_inference::{InferenceDescriptor, InferenceError, PredictRequest, ServingProtocol};
use rskit_tool::{Envelope, NetworkPolicy, NetworkRule};
use serde_json::json;

use crate::Config;

pub(crate) fn descriptor_from_config(config: &Config) -> InferenceDescriptor {
    InferenceDescriptor {
        name: config.name.clone(),
        description: config.description.clone(),
        serving_protocol: ServingProtocol::KServeV2Http,
        envelope: Envelope {
            scopes: config.scopes.clone(),
            network: NetworkPolicy::AllowList {
                rules: vec![NetworkRule {
                    host: config.network_host.clone(),
                    port: config.network_port,
                    scheme: Some(config.network_scheme.clone()),
                }],
            },
            ..Envelope::default()
        },
    }
}

pub(crate) fn authorize_prediction(
    decider: Option<&dyn Decider>,
    descriptor: &InferenceDescriptor,
    request: &PredictRequest,
) -> Result<(), InferenceError> {
    let Some(decider) = decider else {
        return Ok(());
    };
    let principal = request
        .metadata
        .get("principal")
        .cloned()
        .unwrap_or_else(|| "anonymous".to_owned());
    let decision = decider.decide(&AuthzRequest {
        principal,
        action: "inference:predict".to_owned(),
        resource: format!("inference:{}:{}", descriptor.name, request.model_name),
        scopes: descriptor.envelope.scopes.clone(),
        attributes: json!({
            "model_name": request.model_name,
            "model_version": request.model_version,
            "serving_protocol": descriptor.serving_protocol,
        }),
    });
    match decision {
        AuthzDecision::Allow => Ok(()),
        AuthzDecision::Deny(reason) | AuthzDecision::RequiresHumanApproval(reason) => {
            Err(InferenceError::Authorization(reason))
        }
        _ => Err(InferenceError::Authorization(
            "unsupported decision".to_owned(),
        )),
    }
}
