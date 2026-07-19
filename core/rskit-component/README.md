# rskit-component — component lifecycle primitives

`rskit-component` defines the shared lifecycle contract used by services
and infrastructure components: `Component`, ordered `Registry`, health reports, state tracking,
registry configuration, and shutdown results.

## Install

```toml
[dependencies]
rskit-component = "0.2.0-alpha.1"
rskit-errors = "0.2.0-alpha.3"
async-trait = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
use async_trait::async_trait;
use std::sync::Arc;

use rskit_component::{Component, Health, Registry};
use rskit_errors::AppResult;

struct SearchIndexer;

#[async_trait]
impl Component for SearchIndexer {
    fn name(&self) -> &'static str {
        "search-indexer"
    }

    async fn start(&self) -> AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        Ok(())
    }

    async fn health(&self) -> AppResult<Health> {
        Ok(Health::healthy(self.name()))
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let mut registry = Registry::new();
    registry.register(Arc::new(SearchIndexer));
    registry.start_all().await?;
    registry.stop_all().await?;
    Ok(())
}
```

## When to use

Use `rskit-component` when a type has explicit start, stop, and health semantics.
Use `rskit-bootstrap` when you need full application lifecycle orchestration around a component registry.
