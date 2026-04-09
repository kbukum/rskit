//! Integration tests for rskit-httpclient.

#[cfg(test)]
mod integration_tests {
    use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
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
