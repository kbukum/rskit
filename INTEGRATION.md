# rskit: Integration Patterns

This document shows how rskit modules compose together to solve common microservice challenges. Each pattern demonstrates a practical workflow combining multiple crates using Rust's trait-based abstractions.

## Pattern 1: Server + Discovery

**Problem**: Start an HTTP or gRPC server and automatically register it with a discovery service (Consul, etcd, etc.) for automatic deregistration on shutdown.

**Solution**: Use `DiscoveryServer<T>` from `rskit-discovery::server` to wrap your `HttpServer` or `GrpcServer` and handle automatic registration/deregistration via the component lifecycle.

**Code example**:

```rust
use rskit_discovery::{Discovery, Registry, ServiceInstance, server::DiscoveryServer};
use rskit_server::http::HttpServer;
use rskit_logging::Logger;
use rskit_di::Container;

async fn setup_discovery_server(
    http_server: HttpServer,
    registry: Box<dyn Registry>,
    log: Logger,
) -> Result<DiscoveryServer<HttpServer>, Box<dyn std::error::Error>> {
    let service_instance = ServiceInstance {
        id: "payment-svc-1".to_string(),
        name: "payment-service".to_string(),
        address: "127.0.0.1".to_string(),
        port: 8080,
        tags: vec!["v1".to_string(), "prod".to_string()],
        metadata: Default::default(),
    };

    let discovery_server = DiscoveryServer::new(http_server, registry, service_instance, log)?;
    Ok(discovery_server)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log = Logger::new();

    // Create HTTP server
    let http_server = HttpServer::new("0.0.0.0:8080")?;

    // Setup discovery registry (e.g., Consul)
    let registry: Box<dyn Registry> = Box::new(/* consul registry */);

    // Wrap with discovery
    let disc_server = setup_discovery_server(http_server, registry, log).await?;

    // Start (auto-registers on Start, deregisters on Stop)
    disc_server.start().await?;

    // Keep running
    tokio::signal::ctrl_c().await?;
    disc_server.stop().await?;

    Ok(())
}
```

**Modules involved**:
- `rskit-discovery` — `Discovery`, `Registry`, `ServiceInstance`, `DiscoveryServer`
- `rskit-server` — `HttpServer`, `GrpcServer`
- `rskit-logging` — `Logger`
- `tokio` — async runtime

---

## Pattern 2: Messaging + Middleware Stack

**Problem**: Process messages from a topic with automatic retry, metrics tracking, tracing, circuit breaker protection, and dead-letter handling without manually nesting middleware.

**Solution**: Use trait-based middleware composition from `rskit-messaging` with a builder pattern. Stack retry, circuit breaker, and metrics middleware in a predictable order.

**Code example**:

```rust
use rskit_messaging::{MessageHandler, Message, Handler};
use rskit_messaging::kafka::{KafkaConsumer, KafkaConfig};
use rskit_resilience::{RetryPolicy, CircuitBreaker};
use rskit_observability::Tracer;
use rskit_logging::Logger;
use std::sync::Arc;

struct OrderHandler;

#[async_trait::async_trait]
impl MessageHandler for OrderHandler {
    async fn handle(&self, msg: &Message) -> Result<(), Box<dyn std::error::Error>> {
        // Process order event
        println!("Processing order: {:?}", msg.data);
        Ok(())
    }
}

async fn setup_message_handler(tracer: Arc<Tracer>, log: Logger) -> Arc<dyn MessageHandler> {
    let base_handler = Arc::new(OrderHandler);

    // Wrap with retry middleware
    let with_retry = rskit_messaging::middleware::retry_handler(
        base_handler,
        RetryPolicy {
            max_attempts: 3,
            backoff_ms: 100,
            dlq_topic: Some("orders.dlq".to_string()),
        },
    );

    // Wrap with circuit breaker
    let with_cb = rskit_messaging::middleware::circuit_breaker_handler(
        with_retry,
        CircuitBreaker {
            failure_threshold: 5,
            success_threshold: 2,
            timeout_seconds: 30,
        },
    );

    // Wrap with metrics
    let with_metrics = rskit_messaging::middleware::metrics_handler(
        with_cb,
        "orders.created",
        "order-processor",
    );

    // Wrap with tracing (outermost)
    let with_tracing = rskit_messaging::middleware::tracing_handler(
        with_metrics,
        tracer,
    );

    with_tracing
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log = Logger::new();
    let tracer = Arc::new(Tracer::new("order-service"));

    // Setup message handler with middleware
    let handler = setup_message_handler(tracer.clone(), log.clone()).await;

    // Create Kafka consumer with wrapped handler
    let config = KafkaConfig {
        brokers: vec!["localhost:9092".to_string()],
        group_id: "order-processor".to_string(),
        ..Default::default()
    };

    let consumer = KafkaConsumer::new(config, "orders.created", handler)?;

    // Run consumer
    consumer.start().await?;

    // Graceful shutdown
    tokio::signal::ctrl_c().await?;
    consumer.stop().await?;

    Ok(())
}
```

**Modules involved**:
- `rskit-messaging` — `MessageHandler`, `Message`, middleware builders
- `rskit-messaging::kafka` — `KafkaConsumer`, `KafkaConfig`
- `rskit-resilience` — `RetryPolicy`, `CircuitBreaker`
- `rskit-observability` — `Tracer`
- `tokio` — async runtime

---

## Pattern 3: gRPC Client + Discovery

**Problem**: Create a gRPC client that dynamically discovers and connects to a remote service with automatic load balancing and connection pooling.

**Solution**: Use `rskit-grpc` with discovery integration. `DiscoveryChannel` resolves service endpoints via `rskit-discovery`, then hands the connected tonic `Channel` to your generated client.

**Code example**:

```rust
// Cargo.toml
// rskit-grpc = { version = "0.1.0-alpha.1", features = ["client", "discovery"] }

use rskit_grpc::{DiscoveryChannel, GrpcClientConfig};
use rskit_discovery::Discovery;
use std::sync::Arc;

// Generated gRPC client stubs
use analysis::analysis_service_client::AnalysisServiceClient;
use analysis::AnalyzeRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create discovery client (e.g., Consul)
    let discovery: Arc<dyn Discovery> = Arc::new(/* ConsulDiscovery */);

    // Resolve and connect lazily through the aligned grpc transport
    let channel = DiscoveryChannel::new(
        discovery,
        "analysis-service",
        GrpcClientConfig::new("analysis-service:50051"),
    );

    // Create typed gRPC client from the resolved tonic channel
    let mut client = AnalysisServiceClient::new(channel.channel().await?);

    // First call triggers service discovery and connection
    let response = client
        .analyze(tonic::Request::new(AnalyzeRequest {
            data: "hello world".to_string(),
        }))
        .await?;

    println!("Analysis result: {:?}", response.into_inner());

    Ok(())
}
```

**Modules involved**:
- `rskit-grpc` — `GrpcChannel`, `DiscoveryChannel`, `GrpcClientConfig`
- `rskit-discovery` — `Discovery`, backends, load balancing
- `tonic` — gRPC codegen and runtime
- `tokio` — async runtime

---

## Pattern 4: HTTP Client + Resilience

**Problem**: Make HTTP calls to external APIs with retry logic, timeout handling, and circuit breaker protection.

**Solution**: Use `rskit-httpclient` paired with `rskit-resilience` for a resilient HTTP client that automatically handles transient failures.

**Code example**:

```rust
use rskit_httpclient::{HttpClient, HttpClientConfig};
use rskit_resilience::{CbConfig, ConstantBackoff, Policy, RetryPolicy};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compose resilience in rskit-resilience, then inject it into the client
    let policy = Policy::new()
        .with_timeout(Duration::from_secs(10))
        .with_circuit_breaker(CbConfig::new("external-api").with_max_failures(5))?
        .with_retry(
            RetryPolicy::new()
                .with_max_attempts(3)
                .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(100)))
                .with_jitter(false),
        );

    let config = HttpClientConfig::new()
        .with_base_url("https://api.example.com")
        .with_timeout(Duration::from_secs(10))
        .with_resilience_policy(policy);

    let client = HttpClient::new(config)?;

    let resp = client.get("/users/123").await?;

    println!("Status: {}", resp.status);
    println!("Body: {}", resp.text()?);

    Ok(())
}
```

**Modules involved**:
- `rskit-httpclient` — `HttpClient`, `HttpClientConfig`, `Request`, `Response`
- `rskit-resilience` — `Policy`, `RetryPolicy`, `CbConfig`
- `reqwest` — underlying HTTP library
- `tokio` — async runtime

---

## Pattern 5: EventPublisher + Messaging

**Problem**: Publish domain events with automatic envelope construction and routing without manually handling serialization and envelope metadata.

**Solution**: Use `rskit-messaging::EventPublisher` to wrap a Kafka producer. The facade handles event envelope creation (ID, timestamp, source) automatically.

**Code example**:

```rust
use rskit_messaging::{EventPublisher, Producer, Event};
use rskit_messaging::kafka::KafkaProducer;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct OrderCreatedEvent {
    order_id: String,
    customer_id: String,
    amount: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create Kafka producer
    let producer = KafkaProducer::new("localhost:9092")?;

    // Wrap with EventPublisher for auto-envelope
    let event_pub = EventPublisher::new(producer, "order-service");

    // Publish event (no manual envelope needed)
    let order_event = OrderCreatedEvent {
        order_id: "order-123".to_string(),
        customer_id: "cust-456".to_string(),
        amount: 99.99,
    };

    event_pub
        .publish(
            "orders.created",
            "order.created.v1",
            &order_event,
        )
        .await?;

    // Publish with partition key for ordering
    event_pub
        .publish_keyed(
            "orders.created",
            "order.created.v1",
            &order_event,
            "cust-456",  // key ensures ordering per customer
        )
        .await?;

    Ok(())
}
```

**Modules involved**:
- `rskit-messaging` — `EventPublisher`, `Producer`, `Event`
- `rskit-messaging::kafka` — `KafkaProducer`
- `serde` — serialization
- `tokio` — async runtime

---

## Pattern 6: Process + Error Handling

**Problem**: Execute external processes (shell commands, FFmpeg, etc.) with timeout, signal handling, and comprehensive error classification.

**Solution**: Use `rskit-process` with error mapping from `rskit-errors` to handle subprocesses safely and classify failures uniformly.

**Code example**:

```rust
use rskit_process::{ProcessConfig, ProcessSpec, run_with_cancel};
use rskit_errors::{AppError, ErrorCode};
use rskit_logging::Logger;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log = Logger::new();

    // Configure subprocess execution
    let config = ProcessConfig::default().with_timeout(Some(Duration::from_secs(30)));

    // Execute FFmpeg command for video processing
    let spec = ProcessSpec::new("ffmpeg")
        .args(&[
            "-i", "input.mp4",
            "-vf", "scale=1280:720",
            "-c:v", "libx264",
            "-preset", "fast",
            "output.mp4",
        ]);

    match run_with_cancel(&spec, &config, CancellationToken::new()).await {
        Ok(result) => {
            println!("Process succeeded");
            println!("Exit code: {:?}", result.exit_code);
            println!("Stdout: {}", result.stdout);
        }
        Err(e) => {
            // Map process errors to AppError for consistent handling
            let app_err = AppError::new(
                ErrorCode::Internal,
                format!("Video encoding failed: {}", e),
            );
            log.error(&app_err.to_string());
            return Err(app_err.into());
        }
    }

    Ok(())
}
```

**Modules involved**:
- `rskit-process` — `ProcessSpec`, `ProcessConfig`, `run_with_cancel`
- `rskit-errors` — `AppError`, `ErrorCode`
- `rskit-logging` — `Logger`
- `tokio` — async runtime

---

## Cross-Pattern Composition

All six patterns work together in a complete microservice:

1. **Service Registration** (Pattern 1) makes your service discoverable.
2. **gRPC Clients** (Pattern 3) use discovery to call downstream services.
3. **Message Processing** (Pattern 2) uses trait-based middleware to handle async events.
4. **HTTP Clients** (Pattern 4) call external APIs with resilience.
5. **Event Publishing** (Pattern 5) broadcasts domain events to consumers.
6. **Process Execution** (Pattern 6) handles subprocess tasks safely.

**Example architecture**:

```rust
// main.rs — complete microservice wiring
use rskit_discovery::*;
use rskit_server::http::HttpServer;
use rskit_messaging::*;
use rskit_messaging::kafka::*;
use rskit_grpc::*;
use rskit_httpclient::*;
use rskit_logging::Logger;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log = Logger::new();

    // 1. HTTP server with discovery
    let http_server = HttpServer::new("0.0.0.0:8080")?;
    let registry: Box<dyn Registry> = Box::new(/* ConsulRegistry */);
    let disc_server = DiscoveryServer::new(
        http_server,
        registry,
        ServiceInstance { /* ... */ },
        log.clone(),
    )?;
    disc_server.start().await?;

    // 2. Message handler with middleware
    let handler = setup_message_handler(log.clone()).await;
    let consumer = KafkaConsumer::new(KafkaConfig::default(), "orders.created", handler)?;
    consumer.start().await?;

    // 3. Event publisher
    let producer = KafkaProducer::new("localhost:9092")?;
    let event_pub = EventPublisher::new(producer, "order-service");

    // 4. gRPC client with discovery
    let discovery: Arc<dyn Discovery> = Arc::new(/* ConsulDiscovery */);
    let grpc_channel = DiscoveryChannel::new(
        discovery,
        "analysis-service",
        GrpcClientConfig::new("analysis-service:50051"),
    );

    // 5. HTTP client with resilience
    let http_config = HttpClientConfig::new().with_base_url("https://api.example.com");
    let http_client = HttpClient::new(http_config)?;

    // Keep running
    tokio::signal::ctrl_c().await?;

    consumer.stop().await?;
    disc_server.stop().await?;

    Ok(())
}
```

This architecture provides:
- ✅ **Discoverability** — other services find and call you
- ✅ **Resilience** — retries, circuit breakers for messaging and HTTP
- ✅ **Observability** — metrics and tracing through middleware
- ✅ **Type safety** — trait-based abstractions with compile-time checks
- ✅ **Event-driven communication** — publish and consume async events
- ✅ **Process safety** — timeouts and signal handling for subprocesses

---

## Best Practices

1. **Use trait-based abstractions** — implement `Discovery`, `Registry`, `MessageHandler`, `Producer` to swap implementations easily.
2. **Prefer DiscoveryChannel** for gRPC to enable dynamic service discovery without hardcoding addresses.
3. **Stack middleware in a predictable order** — retry → circuit breaker → metrics → tracing (inner to outer).
4. **Use EventPublisher for consistency** — ensures all events have proper envelopes with IDs, timestamps, and source.
5. **Handle process timeouts** — always set `timeout` in `ProcessConfig` to prevent hanging subprocesses.
6. **Classify errors with AppError** — use `rskit-errors::ErrorCode` for uniform error handling across services.
