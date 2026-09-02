//! Behavioral tests for HTTP client transport, request building, and response handling.

#[cfg(test)]
mod client_transport_behavior {
    use std::time::Duration;

    use base64::Engine;
    use rskit_errors::AppError;
    use rskit_errors::ErrorCode;
    use rskit_httpclient::{
        Auth, DestinationPolicy, HttpClient, HttpClientConfig, Request, RequestBody,
        TransportErrorKind,
    };
    use rskit_resilience::{ConstantBackoff, Policy, RetryPolicy};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use wiremock::matchers::{body_string, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn simple_get_request_returns_successful_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/users"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"[{"id": 1, "name": "Alice"}]"#),
            )
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new().with_base_url(mock_server.uri());

        let client = HttpClient::new(config).unwrap();
        let resp = client.get("/api/users").await.unwrap();

        assert!(resp.is_success());
        let body = resp.text().unwrap();
        assert!(body.contains("Alice"));
    }

    #[tokio::test]
    async fn post_with_json_body_returns_created_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/users"))
            .respond_with(
                ResponseTemplate::new(201).set_body_string(r#"{"id": 42, "name": "Bob"}"#),
            )
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new().with_base_url(mock_server.uri());

        let client = HttpClient::new(config).unwrap();

        let body = serde_json::json!({"name": "Bob"});
        let resp = client.post("/api/users", &body).await.unwrap();

        assert_eq!(resp.status().as_u16(), 201);
        let result: serde_json::Value = resp.json().unwrap();
        assert_eq!(result["id"], 42);
    }

    #[tokio::test]
    async fn bearer_token_auth_sends_authorized_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/protected"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"data": "secret"}"#))
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new().with_base_url(mock_server.uri());

        let client = HttpClient::new(config).unwrap();
        let resp = client
            .send(Request::get("/api/protected").bearer_token("secret-token"))
            .await
            .unwrap();

        assert!(resp.is_success());
    }

    #[tokio::test]
    async fn error_response_can_be_checked_for_status_failure() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string(r#"{"error": "not found"}"#))
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new().with_base_url(mock_server.uri());

        let client = HttpClient::new(config).unwrap();
        let resp = client.get("/api/missing").await.unwrap();

        assert!(!resp.is_success());
        assert_eq!(resp.status().as_u16(), 404);

        // error_for_status should return an error
        let result = resp.error_for_status();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resilience_policy_retries_transport_execution() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut first, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = first.read(&mut buffer).await;
            drop(first);

            let (mut second, _) = listener.accept().await.unwrap();
            let _ = second.read(&mut buffer).await;
            second
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok")
                .await
                .unwrap();
            second.shutdown().await.unwrap();
        });

        let policy = Policy::new().with_retry(
            RetryPolicy::new()
                .with_max_attempts(2)
                .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
                .with_jitter(0.0),
        );
        let config = HttpClientConfig::new()
            .with_base_url(format!("http://{address}"))
            .with_resilience_policy(policy);

        let client = HttpClient::new(config).unwrap();
        let response = client.get("/retry").await.unwrap();

        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().unwrap(), "ok");

        server.await.unwrap();
    }

    #[tokio::test]
    async fn response_body_limit_rejects_oversized_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/large"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(32)))
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new()
            .with_base_url(mock_server.uri())
            .with_max_response_body_bytes(16);
        let client = HttpClient::new(config).unwrap();

        let result = client.get("/large").await;

        let error = result.unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(!error.is_retryable());
        assert_eq!(
            TransportErrorKind::classify(&error),
            Some(TransportErrorKind::ResponseTooLarge)
        );
        assert!(error.details().contains_key("max_response_body_bytes"));
    }

    #[tokio::test]
    async fn redirect_destination_policy_rejects_metadata_target() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(
                ResponseTemplate::new(302)
                    .append_header("location", "http://169.254.169.254/latest/meta-data"),
            )
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new().with_base_url(mock_server.uri());
        let client = HttpClient::new(config).unwrap();

        let result = client.get("/redirect").await;

        let error = result.unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(!error.is_retryable());
        assert!(
            error
                .message()
                .contains("metadata service destinations are blocked")
        );
    }

    #[tokio::test]
    async fn redirect_limit_rejects_more_than_configured_hops() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(ResponseTemplate::new(302).append_header("location", "/middle"))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/middle"))
            .respond_with(ResponseTemplate::new(302).append_header("location", "/final"))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/final"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new()
            .with_base_url(mock_server.uri())
            .with_max_redirects(1);
        let client = HttpClient::new(config).unwrap();

        let result = client.get("/redirect").await;

        let error = result.unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(!error.is_retryable());
        assert!(error.message().contains("max_redirects"));
    }

    #[tokio::test]
    async fn redirect_limit_allows_exactly_configured_hops() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(ResponseTemplate::new(302).append_header("location", "/final"))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/final"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new()
            .with_base_url(mock_server.uri())
            .with_max_redirects(1);
        let client = HttpClient::new(config).unwrap();

        let response = client.get("/redirect").await.unwrap();

        assert_eq!(response.text().unwrap(), "ok");
    }

    #[tokio::test]
    async fn allow_list_rejects_disallowed_host() {
        let mock_server = MockServer::start().await;
        let config = HttpClientConfig::new()
            .with_base_url(mock_server.uri())
            .with_destination_policy(
                DestinationPolicy::new().with_allowed_hosts(["api.example.com"]),
            );
        let client = HttpClient::new(config).unwrap();

        let result = client.get("/api/users").await;

        let error = result.unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(!error.is_retryable());
        assert!(error.message().contains("not allowed"));
    }

    #[tokio::test]
    async fn convenience_methods_parse_json_and_checked_statuses() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"value":1}"#))
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"created":true}"#))
            .mount(&mock_server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"updated":true}"#))
            .mount(&mock_server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"patched":true}"#))
            .mount(&mock_server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/gone"))
            .respond_with(ResponseTemplate::new(204).set_body_string(""))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/conflict"))
            .respond_with(ResponseTemplate::new(409).set_body_string("duplicate"))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/invalid-json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client =
            HttpClient::new(HttpClientConfig::new().with_base_url(mock_server.uri())).unwrap();

        let get: serde_json::Value = client.get_json("/json").await.unwrap();
        assert_eq!(get["value"], 1);
        let post: serde_json::Value = client
            .post_json("/json", &serde_json::json!({"name":"new"}))
            .await
            .unwrap();
        assert_eq!(post["created"], true);
        let put: serde_json::Value = client
            .put_json("/json", &serde_json::json!({"name":"updated"}))
            .await
            .unwrap();
        assert_eq!(put["updated"], true);
        let patch: serde_json::Value = client
            .patch_json("/json", &serde_json::json!({"name":"patched"}))
            .await
            .unwrap();
        assert_eq!(patch["patched"], true);
        assert_eq!(client.delete("/gone").await.unwrap().status_u16(), 204);

        let conflict = client
            .send_checked(Request::get("/conflict"))
            .await
            .unwrap_err();
        assert_eq!(conflict.code(), ErrorCode::Conflict);
        assert_eq!(
            conflict
                .details()
                .get("body")
                .and_then(serde_json::Value::as_str),
            Some("duplicate")
        );

        let invalid_json = client
            .get("/invalid-json")
            .await
            .unwrap()
            .checked_json::<serde_json::Value>()
            .unwrap_err();
        assert_eq!(invalid_json.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn request_builder_applies_headers_queries_auth_and_text_or_byte_bodies() {
        let mock_server = MockServer::start().await;

        let basic_user = format!("user-{:08x}", rand::random::<u32>());
        let basic_password = format!("pw-{:08x}", rand::random::<u32>());
        let expected_basic = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{basic_user}:{basic_password}"))
        );

        Mock::given(method("POST"))
            .and(path("/text"))
            .and(query_param("mode", "text"))
            .and(header("x-custom", "override"))
            .and(header("authorization", expected_basic.as_str()))
            .and(body_string("hello"))
            .respond_with(ResponseTemplate::new(200).set_body_string("text-ok"))
            .mount(&mock_server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/bytes"))
            .and(header("x-default", "default"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0xff]))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(
            HttpClientConfig::new()
                .with_base_url(mock_server.uri())
                .with_header("x-default", "default")
                .with_auth(Auth::bearer("default-token")),
        )
        .unwrap();

        let text_response = client
            .send(
                Request::new(http::Method::POST, "/old")
                    .path("/text")
                    .header("x-custom", "override")
                    .query_param("mode", "text")
                    .basic_auth(&basic_user, &basic_password)
                    .body(RequestBody::text("hello")),
            )
            .await
            .unwrap();
        assert_eq!(text_response.checked_text().unwrap(), "text-ok");

        let bytes_response = client
            .send(Request::patch("/bytes").body(RequestBody::bytes(vec![1_u8, 2, 3])))
            .await
            .unwrap();
        assert_eq!(bytes_response.status_u16(), 200);
        assert_eq!(bytes_response.text_or_diagnostic(), "<non-utf8 body>");
    }

    #[tokio::test]
    async fn checked_helpers_support_custom_error_mapping_and_body_consumption() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/teapot"))
            .respond_with(
                ResponseTemplate::new(418)
                    .append_header("x-request-id", "req-1")
                    .set_body_string("short and stout"),
            )
            .mount(&mock_server)
            .await;
        Mock::given(method("HEAD"))
            .and(path("/head"))
            .respond_with(ResponseTemplate::new(200).append_header("x-answer", "42"))
            .mount(&mock_server)
            .await;

        let client =
            HttpClient::new(HttpClientConfig::new().with_base_url(mock_server.uri())).unwrap();

        let mapped = client
            .get("/teapot")
            .await
            .unwrap()
            .checked_text_with(|response| {
                AppError::new(ErrorCode::ExternalService, "mapped teapot")
                    .with_detail("status", response.status.as_u16().to_string())
                    .with_detail(
                        "request_id",
                        response.header("x-request-id").cloned().unwrap(),
                    )
                    .with_detail("body", response.body)
            })
            .unwrap_err();
        assert_eq!(mapped.code(), ErrorCode::ExternalService);
        assert_eq!(
            mapped
                .details()
                .get("request_id")
                .and_then(serde_json::Value::as_str),
            Some("req-1")
        );

        let head = client.send(Request::head("/head")).await.unwrap();
        assert!(head.is_success());
        assert_eq!(head.header("X-Answer").map(String::as_str), Some("42"));
        assert!(head.headers().contains_key("x-answer") || head.headers().contains_key("X-Answer"));
        assert!(head.into_bytes().is_empty());
    }

    #[tokio::test]
    async fn head_convenience_method_returns_headers_without_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .and(path("/status"))
            .respond_with(ResponseTemplate::new(204).append_header("x-status", "ok"))
            .mount(&mock_server)
            .await;

        let client =
            HttpClient::new(HttpClientConfig::new().with_base_url(mock_server.uri())).unwrap();

        let response = client.head("/status").await.unwrap();

        assert_eq!(response.status_u16(), 204);
        assert_eq!(response.header("x-status").map(String::as_str), Some("ok"));
        assert!(response.into_bytes().is_empty());
    }

    #[tokio::test]
    async fn unsupported_method_and_invalid_headers_are_rejected_before_transport() {
        let mock_server = MockServer::start().await;
        let client =
            HttpClient::new(HttpClientConfig::new().with_base_url(mock_server.uri())).unwrap();

        let unsupported = client
            .send(Request::new(http::Method::OPTIONS, "/options"))
            .await
            .unwrap_err();
        assert_eq!(unsupported.code(), ErrorCode::InvalidInput);
        assert!(unsupported.message().contains("unsupported http method"));

        let invalid_header_name = client
            .send(Request::get("/").header("bad header", "value"))
            .await
            .unwrap_err();
        assert_eq!(invalid_header_name.code(), ErrorCode::InvalidInput);

        let invalid_header_value = client
            .send(Request::get("/").header("x-bad", "bad\nvalue"))
            .await
            .unwrap_err();
        assert_eq!(invalid_header_value.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn bearer_auth_display_redacts_token_value() {
        let auth = Auth::bearer("token123");
        assert_eq!(auth.to_string(), "Bearer");
    }

    #[test]
    fn basic_auth_display_redacts_credentials() {
        let password = format!("pw-{:08x}", rand::random::<u32>());
        let auth = Auth::basic("user", password);
        assert_eq!(auth.to_string(), "Basic");
    }

    #[test]
    fn api_key_auth_display_redacts_key_value() {
        let auth = Auth::api_key("X-API-Key", "key123");
        assert_eq!(auth.to_string(), "ApiKey(X-API-Key)");
    }
}
