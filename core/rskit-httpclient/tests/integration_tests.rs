//! Integration tests for rskit-httpclient.

#[cfg(test)]
mod integration_tests {
    use std::time::Duration;

    use rskit_httpclient::{Auth, DestinationPolicy, HttpClient, HttpClientConfig, Request};
    use rskit_resilience::{ConstantBackoff, Policy, RetryPolicy};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_simple_get_request() {
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
    async fn test_post_with_json_body() {
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
    async fn test_bearer_token_auth() {
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
    async fn test_error_response() {
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
    async fn test_resilience_policy_retries_transport_execution() {
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
                .with_jitter(false),
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

        assert!(result.is_err());
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

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn redirect_limit_is_enforced_with_custom_policy() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/loop"))
            .respond_with(ResponseTemplate::new(302).append_header("location", "/loop"))
            .mount(&mock_server)
            .await;

        let config = HttpClientConfig::new()
            .with_base_url(mock_server.uri())
            .with_max_redirects(1);
        let client = HttpClient::new(config).unwrap();

        let result = client.get("/loop").await;

        assert!(result.is_err());
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

        assert!(result.is_err());
    }

    #[test]
    fn test_bearer_auth_creation() {
        let auth = Auth::bearer("token123");
        assert_eq!(auth.to_string(), "Bearer");
    }

    #[test]
    fn test_basic_auth_creation() {
        let auth = Auth::basic("user", "pass");
        assert_eq!(auth.to_string(), "Basic");
    }

    #[test]
    fn test_api_key_auth_creation() {
        let auth = Auth::api_key("X-API-Key", "key123");
        assert_eq!(auth.to_string(), "ApiKey(X-API-Key)");
    }
}
