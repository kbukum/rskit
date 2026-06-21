# rskit-codec

Pluggable structured-text codecs over one canonical value tree, plus value-tree merge.

`rskit-codec` is the single owner of "encode/decode a structured text format" and "deep-merge two documents" for the rskit ecosystem. Crates that read or write config files, manifests, or documents reuse it instead of re-implementing `bounded read → parse → typed error` per format.

## Value model

[`serde_json::Value`] is the canonical in-memory tree — format-neutral and the de-facto Rust interchange type. Every codec decodes into it and the merge operates on it.

One consequence: types without a JSON equivalent (notably TOML datetimes) are not part of the model. Represent such values as strings.

## Codecs

| Codec | Feature | Notes |
| --- | --- | --- |
| `JsonCodec` | always on | `serde_json` backs the value model |
| `TomlCodec` | `toml` (default) | top-level value must be a table |

[`Codec`] is object-safe, so a codec can be held as `Arc<dyn Codec>` and chosen at runtime — see [`select::codec_for_path`] for extension-based selection. The generic conveniences [`encode`] / [`decode`] take `&dyn Codec` and work with any `#[derive(Serialize, Deserialize)]` type; `decode::<T>` honors `#[serde(deny_unknown_fields)]`.

```rust
use rskit_codec::{TomlCodec, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Settings { name: String, retries: u8 }

let codec = TomlCodec;
let parsed: Settings = decode(&codec, "name = \"svc\"\nretries = 3").unwrap();
let text = encode(&codec, &parsed).unwrap();
```

## Value-tree merge

[`value::merge`] deep-merges two value trees (objects merge recursively, last-wins scalars, arrays replaced). [`value::merge_with`] lets the caller choose, per key, whether arrays replace or concatenate — so layered-document systems (config includes, overlays) express their own policy without the merge knowing what any key means.

```rust
use rskit_codec::value::{merge_with, ArrayStrategy};
use serde_json::json;

let merged = merge_with(
    json!({ "groups": [{ "name": "a" }] }),
    json!({ "groups": [{ "name": "b" }] }),
    |key| if key == "groups" { ArrayStrategy::Concat } else { ArrayStrategy::Replace },
);
```

YAML and other formats drop in as additional `Codec` implementations behind their own features without changing the contract or the merge.
