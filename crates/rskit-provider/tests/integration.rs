use rskit_errors::AppResult;
use rskit_provider::traits::{Provider, RequestResponse};

struct EchoProvider;

#[async_trait::async_trait]
impl Provider for EchoProvider {
    fn name(&self) -> &'static str {
        "echo"
    }
}

#[async_trait::async_trait]
impl RequestResponse<String, String> for EchoProvider {
    async fn execute(&self, input: String) -> AppResult<String> {
        Ok(input.to_uppercase())
    }
}

#[tokio::test]
async fn echo_provider_uppercases_input() {
    let p = EchoProvider;
    let result = p.execute("hello".to_string()).await.unwrap();
    assert_eq!(result, "HELLO");
}
