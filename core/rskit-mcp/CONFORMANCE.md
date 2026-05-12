# rskit-mcp MCP 2025-06-18 conformance

This document tracks `rskit-mcp` conformance to the
[Model Context Protocol](https://modelcontextprotocol.io/) revision
**2025-06-18**.

Transports are locked to `stdio` and `streamable_http`; `sse` is not exposed as a peer transport.

| Capability | Status | Notes |
|---|---|---|
| tools/list + tools/call | Present | Registry-backed tool exposure via `rskit-tool`. |
| prompts/list + prompts/get | Present | Static prompt entries; `rskit-ai/prompt` owns templates. |
| resources/list + resources/read | Present | Static resources and template matching. |
| resource templates | Present | Basic URI-template matching. |
| cancellation | Partial | SDK/session cancellation propagates; explicit compliance vectors remain SDK-limited. |
| progress/logging/pagination/completion | Partial | Native SDK request handlers can be added without protocol forks. |
| structured tool output | Present | `rskit-schema` validation runs before MCP result conversion and `ToolResult.output` maps to `structuredContent`. |
| tool annotations | Present | Derived from `rskit-tool::Envelope` and local annotations at the MCP boundary. |
| roots/sampling/elicitation | Partial | Client-side/server-side seams depend on upstream SDK capability exposure. |
| stdio transport | Present | Canonical name `stdio`. |
| streamable_http transport | Present | Canonical name `streamable_http`; SSE is not a separate transport. |
| Origin validation | Present | Transport helper validates configured origins. |
| localhost bind | Present | Security helper defaults to `127.0.0.1`. |
| payload limits | Present | Server `max_input_bytes`/`max_result_bytes`; HTTP helper max payload policy. |
| OAuth 2.1 + PKCE | Partial | Helper config and default requirement seam; full authorization server integration is composition-owned. |

Remote MCP servers are tool sources, not skills. Skill manifests live in `rskit-skill`.
