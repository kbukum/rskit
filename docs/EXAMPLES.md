# rskit Usage Examples

A tour of common rskit patterns. For per-crate details, see each crate's own `README.md`.

## Hello, lifecycle

```rust
use rskit_bootstrap::{AppBuilder};
use rskit_config::ServiceConfig;
use rskit_errors::AppResult;

#[tokio::main]
async fn main() -> AppResult<()> {
    let config = MyConfig::default();

    AppBuilder::new(config)
        .build()?
        .before_start(|_cancel| async move {
            println!("Starting.");
            Ok(())
        })
        .after_start(|_cancel| async move {
            println!("Ready.");
            Ok(())
        })
        .run()
        .await
}
```

## Resilient HTTP call

```rust
use rskit_resilience::{CircuitBreaker, CbConfig, RetryPolicy};
use std::time::Duration;

let cb = CircuitBreaker::new(CbConfig::default());
let retry = RetryPolicy::builder()
    .max_attempts(3)
    .initial_backoff(Duration::from_millis(100))
    .build();

let result = retry.execute(|| async {
    cb.execute(|| async { call_external_service().await }).await
}).await?;
```

## Stream pipeline

```rust
use rskit_pipeline::{RskitStreamExt, from_slice};
use futures::StreamExt;

let results = from_slice(vec![1u32, 2, 3, 4, 5])
    .rfilter(|&n| async move { n % 2 == 0 })
    .rmap(|n| async move { Ok(n * 10) })
    .collect::<Vec<_>>()
    .await;
// [Ok(20), Ok(40)]
```

## Worker pool

```rust
use rskit_worker::{Handler, Pool, PoolConfig, Event};
use rskit_errors::AppResult;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use std::sync::Arc;

struct MyHandler;

#[async_trait::async_trait]
impl Handler<String, String> for MyHandler {
    async fn handle(
        &self,
        input: String,
        emit: mpsc::Sender<Event<String>>,
        _cancel: CancellationToken,
    ) -> AppResult<String> {
        Ok(input.to_uppercase())
    }
}

let pool = Pool::new(Arc::new(MyHandler), PoolConfig::new("demo"));
let handle = pool.submit("hello".to_string()).await?;
let result = handle.result().await?;
assert_eq!(result, "HELLO");
```

## Errors — typed codes + tonic interop

```rust
match err.code() {
    ErrorCode::NotFound      => 404,
    ErrorCode::Unauthorized  => 401,
    ErrorCode::RateLimited   => 429,
    _                        => 500,
}

let err = AppError::not_found("user", user_id)
    .with_detail("tenant", tenant_id)
    .with_cause(db_error);

let status: tonic::Status = err.into();
```

## Resilience as Tower layers

```rust
use tower::ServiceBuilder;
use rskit_resilience::{CircuitBreakerLayer, RetryLayer};

let svc = ServiceBuilder::new()
    .layer(CircuitBreakerLayer(cb))
    .layer(RetryLayer(policy))
    .service(my_service);
```

## Config loading order

```
1. Programmatic defaults            ← lowest priority
2. TOML file (optional)
3. Profile .env file (optional)
4. .env file (optional, dotenvy)
5. Adapter sources
6. APP__SECTION__KEY env vars
7. Programmatic overrides           ← highest priority
```

```rust
#[derive(Deserialize, Validate)]
struct Config {
    service: ServiceConfig,
    #[validate(range(min = 1, max = 65535))]
    port: u16,
}

impl AppConfig for Config {
    fn apply_defaults(&mut self) {
        if self.port == 0 {
            self.port = 50051;
        }
    }

    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
}

let cfg: Config = ConfigLoader::app()
    .with_config_file("config/app.toml")
    .with_env_prefix("MYAPP")
    .load_app()?;
```

## Pipeline operators

All operators are lazy and non-allocating where possible.

| Operator | Description |
|---|---|
| `rmap` / `rflatmap` | Async map / flat-map |
| `rfilter` | Async predicate filter |
| `rtap` | Side-effect without transforming |
| `rreduce` | Fold to a single value |
| `rparallel` | Bounded concurrent execution |
| `rfan_out` | Broadcast item to N async functions |
| `rbatch` | Collect N items into a `Vec` |
| `rdebounce` | Suppress rapid bursts; emit last after quiet period |
| `rthrottle` | Emit at most once per interval |
| `rtumbling_window` | Fixed non-overlapping time windows |
| `rsliding_window` | Overlapping time windows |
