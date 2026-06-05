# rskit-authz

Canonical RBAC + ABAC authorization engine with role hierarchy, deny override, and default deny.

## Highlights

- `Engine` evaluates RBAC roles and ABAC policies together
- role inheritance with wildcard resource/action permissions
- object-level role permissions via permission-local attribute conditions
- explicit deny policies always win
- unmatched requests resolve to `default deny`
- `AuthorizationLayer` composes policy checks as Tower middleware and denies missing request context
- `match_pattern` / `match_any` mirror gokit and pykit wildcard semantics

## Example

```rust
use std::collections::HashMap;

use rskit_authz::{Checker, Engine, Permission, Request, Resource, Role, Subject};

let engine = Engine::new(
    vec![Role {
        name: "editor".into(),
        inherits: vec!["viewer".into()],
        permissions: vec![Permission {
            resource: "article".into(),
            action: "write".into(),
            conditions: vec![],
        }],
    }],
    vec![],
)?;

let allowed = engine.check(&Request {
    subject: Subject {
        id: "user-1".into(),
        roles: vec!["editor".into()],
        attributes: HashMap::new(),
    },
    resource: Resource {
        resource_type: "article".into(),
        id: "article-1".into(),
        attributes: HashMap::new(),
    },
    action: "write".into(),
    context: HashMap::new(),
});

assert!(allowed);
# Ok::<(), rskit_errors::AppError>(())
```

Object-level constraints that limit a role grant should be attached to the
`Permission::conditions` for that grant. Use explicit deny policies for
constraints that must override every broad role or policy allow.
