use rskit_errors::AppResult;
use rskit_provider::traits::RequestResponse;

struct EchoProvider;

#[async_trait::async_trait]
impl RequestResponse<String, String> for EchoProvider {
    async fn call(&self, input: String) -> AppResult<String> {
        Ok(input.to_uppercase())
    }
}

#[tokio::test]
async fn echo_provider_uppercases_input() {
    let p = EchoProvider;
    let result = p.call("hello".to_string()).await.unwrap();
    assert_eq!(result, "HELLO");
}
