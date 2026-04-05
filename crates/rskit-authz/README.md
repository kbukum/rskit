# rskit-authz — Authorization Engine

RBAC and ABAC authorization engine with deny-first evaluation.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-authz.svg)](https://crates.io/crates/rskit-authz)
[![docs.rs](https://docs.rs/rskit-authz/badge.svg)](https://docs.rs/rskit-authz)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `Checker` trait — async policy enforcement point (`check(subject, action, resource)`)
- `RbacChecker` — role-based access control with wildcard matching and deny-first evaluation
- `AbacChecker` — attribute-based access control with pluggable `AbacRule` chain
- `Policy` with `Effect::Allow` / `Effect::Deny`
- Deny-by-default security model

## Usage

```toml
[dependencies]
rskit-authz = "0.1"
```

```rust
use rskit_authz::{Checker, RbacChecker, Policy, Effect};

async fn example() {
    let rbac = RbacChecker::new(vec![
        Policy { subject: "admin".into(), action: "*".into(), resource: "*".into(), effect: Effect::Allow },
        Policy { subject: "viewer".into(), action: "read".into(), resource: "doc".into(), effect: Effect::Allow },
    ]);

    rbac.check("admin", "delete", "doc").await.unwrap(); // allowed
    assert!(rbac.check("viewer", "delete", "doc").await.is_err()); // denied
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
