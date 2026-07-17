use rskit_ai::{Capabilities, Model, Provider as ModelProvider, Usage};
use rskit_inference::{PredictRequest, PredictResponse, PredictStatus, Value};
use serde::{Deserialize, Serialize};

use crate::{Config, TGI_KIND};

pub(crate) fn tgi_chat_body(adapter: &Config, request: &PredictRequest) -> OaiChatRequest {
    let prompt = request
        .inputs
        .get("prompt")
        .or_else(|| request.inputs.get("text"))
        .and_then(|value| match value {
            Value::Text { text } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let model = if request.model_name.is_empty() {
        adapter.model.clone()
    } else {
        request.model_name.clone()
    };

    let max_tokens = request
        .parameters
        .get("max_tokens")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or(adapter.max_tokens);

    let temperature = request
        .parameters
        .get("temperature")
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32);

    OaiChatRequest {
        model,
        messages: vec![OaiMessage {
            role: "user".to_string(),
            content: prompt,
        }],
        max_tokens,
        temperature,
        stream: false,
    }
}

pub(crate) fn tgi_predict_response(
    oai: OaiChatResponse,
    model_version: Option<String>,
) -> PredictResponse {
    let generated = oai
        .choices
        .first()
        .and_then(|choice| choice.message.content.clone())
        .unwrap_or_default();
    let finish = oai
        .choices
        .first()
        .and_then(|choice| choice.finish_reason.as_deref())
        .map(|reason| ("finish_reason".to_string(), reason.to_string()))
        .into_iter()
        .collect();

    PredictResponse {
        outputs: std::collections::HashMap::from([(
            "text".to_string(),
            Value::Text { text: generated },
        )]),
        usage: Usage {
            input_tokens: oai.usage.prompt_tokens as u64,
            output_tokens: oai.usage.completion_tokens as u64,
            ..Usage::default()
        },
        model: Model {
            name: oai.model,
            provider: ModelProvider::Custom(TGI_KIND.to_string()),
            version: model_version,
            capabilities: Capabilities::default(),
        },
        status: PredictStatus::Success,
        metadata: finish,
    }
}

#[derive(Serialize)]
pub(crate) struct OaiChatRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<OaiMessage>,
    pub(crate) max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f32>,
    pub(crate) stream: bool,
}

#[derive(Serialize)]
pub(crate) struct OaiMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

#[derive(Deserialize)]
pub(crate) struct OaiChatResponse {
    pub(crate) model: String,
    pub(crate) choices: Vec<OaiChatChoice>,
    pub(crate) usage: OaiUsage,
}

#[derive(Deserialize)]
pub(crate) struct OaiChatChoice {
    pub(crate) message: OaiChatMessage,
    pub(crate) finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OaiChatMessage {
    pub(crate) content: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OaiUsage {
    pub(crate) prompt_tokens: u32,
    pub(crate) completion_tokens: u32,
}
