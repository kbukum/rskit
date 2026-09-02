use std::collections::BTreeMap;

use rskit_ai::chat::{
    AssistantMessage, Message, ToolResultMessage, assistant, count_tokens_approx, system,
    tool_result_msg, user,
};
use rskit_ai::{
    Builder, ContentPart, PromptError, PromptTemplate, Registry, RenderContext, RenderToMessage,
    ValidationFindingKind, VariableDecl, VariableType, render, text_content, text_of, validate,
};
use semver::Version;
use serde_json::{Map, json};

fn context(values: &[(&str, serde_json::Value)]) -> RenderContext {
    values
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

#[test]
fn chat_helpers_preserve_roles_text_and_tool_aliases() {
    let messages = [
        user("hello"),
        assistant("world"),
        system("follow policy"),
        tool_result_msg("call-1", "done", true),
    ];

    assert_eq!(
        messages.iter().map(Message::role).collect::<Vec<_>>(),
        ["user", "assistant", "system", "tool"]
    );

    let Message::Assistant(reply) = &messages[1] else {
        panic!("assistant helper must create assistant message");
    };
    assert_eq!(reply.text(), "world");
    assert!(!reply.has_tool_calls());

    let wire = json!({
        "role": "tool",
        "tool_use_id": "call-2",
        "content": "result",
        "is_error": false
    });
    let decoded: Message = serde_json::from_value(wire).unwrap();
    assert_eq!(
        decoded,
        Message::Tool(ToolResultMessage {
            tool_use_id: "call-2".to_owned(),
            content: "result".to_owned(),
            is_error: false,
        })
    );
}

#[test]
fn content_helpers_ignore_non_text_and_size_multimodal_blocks_conservatively() {
    let mut input = Map::new();
    input.insert("query".to_owned(), json!("rust"));

    let blocks = vec![
        ContentPart::Text {
            text: "hello".to_owned(),
        },
        ContentPart::Image {
            source: "https://example.test/image.png".to_owned(),
            mime_type: "image/png".to_owned(),
            data: None,
        },
        ContentPart::Audio {
            source: "audio-1".to_owned(),
            mime_type: "audio/mpeg".to_owned(),
            data: Some("base64".to_owned()),
        },
        ContentPart::Video {
            source: "video-1".to_owned(),
            mime_type: "video/mp4".to_owned(),
            data: None,
        },
        ContentPart::File {
            source: "file-1".to_owned(),
            mime_type: "application/pdf".to_owned(),
            data: None,
        },
        ContentPart::ToolUse {
            id: "call-1".to_owned(),
            name: "search".to_owned(),
            input: input.clone(),
        },
        ContentPart::ToolResult {
            id: "call-1".to_owned(),
            content: "result".to_owned(),
            is_error: false,
        },
        ContentPart::Text {
            text: "world".to_owned(),
        },
    ];

    assert_eq!(
        text_content("single"),
        vec![ContentPart::Text {
            text: "single".to_owned()
        }]
    );
    assert_eq!(text_of(&blocks), "helloworld");
    assert_eq!(blocks[0].approx_chars(), 5);
    assert_eq!(blocks[1].approx_chars(), 256);
    assert_eq!(blocks[2].approx_chars(), 256);
    assert_eq!(blocks[3].approx_chars(), 256);
    assert_eq!(blocks[4].approx_chars(), 256);
    assert_eq!(blocks[5].approx_chars(), json!(input).to_string().len());
    assert_eq!(blocks[6].approx_chars(), 6);
}

#[test]
fn token_counter_handles_empty_multimodal_tool_and_length_mismatch_edges() {
    let mut input = Map::new();
    input.insert("k".to_owned(), json!("value"));
    let messages = vec![
        Message::User(Default::default()),
        Message::User(rskit_ai::chat::UserMessage {
            content: vec![
                ContentPart::Text {
                    text: "abcd".to_owned(),
                },
                ContentPart::Image {
                    source: "img".to_owned(),
                    mime_type: "image/png".to_owned(),
                    data: None,
                },
            ],
        }),
        Message::Assistant(AssistantMessage {
            content: vec![ContentPart::Text {
                text: "abcdefgh".to_owned(),
            }],
            tool_calls: vec![rskit_ai::ToolUseBlock {
                id: "call-1".to_owned(),
                name: "search".to_owned(),
                input,
            }],
            usage: None,
        }),
        system("12345678"),
        tool_result_msg("call-1", "12345678", false),
    ];

    assert_eq!(count_tokens_approx(&messages), 4 + (2 + 65 + 4) + 7 + 6 + 6);
}

#[test]
fn prompt_builder_requires_identity_and_body_while_preserving_metadata() {
    assert!(matches!(
        Builder::new("  ").body("hello").build(),
        Err(PromptError::MissingField("name"))
    ));
    assert!(matches!(
        Builder::new("named").build(),
        Err(PromptError::MissingField("body"))
    ));

    let prompt = PromptTemplate::builder("summarize")
        .version(Version::new(1, 2, 3))
        .description("Summarize content")
        .body("Summarize {{topic}}")
        .variable("topic")
        .output_schema(json!({"type": "object"}))
        .build()
        .unwrap();

    assert_eq!(prompt.name, "summarize");
    assert_eq!(prompt.version, Version::new(1, 2, 3));
    assert_eq!(prompt.description, "Summarize content");
    assert_eq!(prompt.output_schema, Some(json!({"type": "object"})));
    assert_eq!(
        prompt.variables,
        vec![VariableDecl {
            name: "topic".to_owned(),
            kind: VariableType::Any,
            required: true,
            default: None,
        }]
    );
}

#[test]
fn prompt_rendering_supports_defaults_json_values_and_literal_invalid_placeholders() {
    let prompt = PromptTemplate {
        name: "qa".to_owned(),
        version: Version::new(1, 0, 0),
        template: "Ask {{ topic }} with {{count}} examples; keep {{ invalid.name }}".to_owned(),
        variables: vec![
            VariableDecl {
                name: "topic".to_owned(),
                kind: VariableType::String,
                required: true,
                default: None,
            },
            VariableDecl {
                name: "count".to_owned(),
                kind: VariableType::Number,
                required: false,
                default: Some(json!(3)),
            },
        ],
        output_schema: None,
        description: String::new(),
    };

    let rendered = prompt
        .render(&context(&[("topic", json!("Rust"))]))
        .unwrap();
    assert_eq!(
        rendered,
        "Ask Rust with 3 examples; keep {{ invalid.name }}"
    );

    let message = prompt
        .render_to_message(&context(&[("topic", json!("Rust"))]))
        .unwrap();
    assert_eq!(
        message,
        system("Ask Rust with 3 examples; keep {{ invalid.name }}")
    );

    assert!(matches!(
        prompt.render(&BTreeMap::new()),
        Err(PromptError::MissingVariable(name)) if name == "topic"
    ));
    assert_eq!(
        render("dangling {{ topic", &RenderContext::new()).unwrap(),
        "dangling {{ topic"
    );
    assert_eq!(
        render("empty {{ }} ok", &RenderContext::new()).unwrap(),
        "empty {{ }} ok"
    );
}

#[test]
fn prompt_validation_reports_missing_and_unused_variables_in_stable_order() {
    let prompt = PromptTemplate {
        name: "review".to_owned(),
        version: Version::new(1, 0, 0),
        template: "{{b}} {{a}} {{valid-name}} {{ invalid.name }}".to_owned(),
        variables: vec![
            VariableDecl {
                name: "a".to_owned(),
                kind: VariableType::String,
                required: true,
                default: None,
            },
            VariableDecl {
                name: "z".to_owned(),
                kind: VariableType::Boolean,
                required: false,
                default: Some(json!(true)),
            },
        ],
        output_schema: None,
        description: String::new(),
    };

    let findings = validate(&prompt);
    assert_eq!(findings.len(), 3);
    assert_eq!(findings[0].kind, ValidationFindingKind::MissingVariable);
    assert_eq!(findings[0].variable, "b");
    assert_eq!(findings[1].kind, ValidationFindingKind::MissingVariable);
    assert_eq!(findings[1].variable, "valid-name");
    assert_eq!(findings[2].kind, ValidationFindingKind::UnusedVariable);
    assert_eq!(findings[2].variable, "z");
}

#[test]
fn registry_rejects_bad_versions_duplicates_and_returns_latest_semver() {
    let mut registry = Registry::new();

    assert!(matches!(
        registry.register("prompt", "not-semver", "{{topic}}", None),
        Err(PromptError::InvalidVersion { version, .. }) if version == "not-semver"
    ));

    let old = registry
        .register(
            "prompt",
            "1.0.0",
            "{{topic}}",
            Some(json!({"type": "string"})),
        )
        .unwrap();
    let latest = registry
        .register("prompt", "2.0.0", "{{other}}", None)
        .unwrap();
    registry.register("other", "1.0.0", "static", None).unwrap();

    assert!(matches!(
        registry.register("prompt", "1.0.0", "duplicate", None),
        Err(PromptError::AlreadyRegistered { name, version })
            if name == "prompt" && version == Version::new(1, 0, 0)
    ));
    assert!(matches!(
        registry.register_template(old.clone()),
        Err(PromptError::AlreadyRegistered { name, version })
            if name == "prompt" && version == Version::new(1, 0, 0)
    ));

    assert_eq!(
        registry.lookup("prompt", &Version::new(1, 0, 0)).unwrap(),
        &old
    );
    assert_eq!(registry.lookup_latest("prompt").unwrap(), &latest);
    assert_eq!(
        registry.versions("prompt"),
        vec![Version::new(1, 0, 0), Version::new(2, 0, 0)]
    );
    assert_eq!(
        registry
            .list()
            .into_iter()
            .map(|identity| (identity.name, identity.version))
            .collect::<Vec<_>>(),
        vec![
            ("other".to_owned(), Version::new(1, 0, 0)),
            ("prompt".to_owned(), Version::new(1, 0, 0)),
            ("prompt".to_owned(), Version::new(2, 0, 0)),
        ]
    );
    assert!(matches!(
        registry.lookup("missing", &Version::new(1, 0, 0)),
        Err(PromptError::NotFound { name, version })
            if name == "missing" && version == Version::new(1, 0, 0)
    ));
    assert!(matches!(
        registry.lookup_latest("missing"),
        Err(PromptError::NameNotFound(name)) if name == "missing"
    ));
}

#[test]
fn vector_helpers_cover_mismatch_zero_and_negative_value_edges() {
    assert_eq!(rskit_ai::vector::dot_product(&[1.0, 2.0], &[3.0]), 0.0);
    assert!(rskit_ai::vector::euclidean_distance(&[1.0, 2.0], &[3.0]).is_nan());
    assert_eq!(
        rskit_ai::vector::cosine_similarity(&[0.0, 0.0], &[1.0, 2.0]),
        0.0
    );
    assert_eq!(rskit_ai::vector::normalize(&[0.0, 0.0]), None);
    assert_eq!(rskit_ai::vector::mean_pooling(&[]), None);
    assert_eq!(rskit_ai::vector::max_pooling(&[]), None);
    assert_eq!(
        rskit_ai::vector::mean_pooling(&[vec![1.0], vec![1.0, 2.0]]),
        None
    );
    assert_eq!(
        rskit_ai::vector::max_pooling(&[vec![1.0], vec![1.0, 2.0]]),
        None
    );

    assert_eq!(
        rskit_ai::vector::dot_product(&[1.0, -2.0], &[3.0, 4.0]),
        -5.0
    );
    assert_eq!(
        rskit_ai::vector::euclidean_distance(&[1.0, 2.0], &[4.0, 6.0]),
        5.0
    );
    assert!(
        (rskit_ai::vector::cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < f32::EPSILON
    );
    assert_eq!(
        rskit_ai::vector::mean_pooling(&[vec![1.0, 3.0], vec![3.0, 5.0]]),
        Some(vec![2.0, 4.0])
    );
    assert_eq!(
        rskit_ai::vector::max_pooling(&[vec![-1.0, 3.0], vec![3.0, 2.0]]),
        Some(vec![3.0, 3.0])
    );
    assert_eq!(
        rskit_ai::vector::normalize(&[3.0, 4.0]),
        Some(vec![0.6, 0.8])
    );
}
