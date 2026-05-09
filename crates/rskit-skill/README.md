> **Note:** This `skill` primitive borrows the `SKILL.md` filename and progressive-disclosure model from Anthropic Agent Skills. It is a distinct primitive and makes no interop claim with Claude Code or the Anthropic runtime.

# rskit-skill

SDK-free skill manifests, progressive-disclosure loaders, registries, providers, and verifier contracts for rskit agentic applications.

The manifest file is `kit.skill.yaml`. `scripts/` and `references/` are inert assets: the loader records path and SHA-256 and never executes them.

## Architecture

```mermaid
graph TD
    skill[rskit-skill]
    manifest[manifest schema]
    loader[loader + registry]
    verifier[signature verifier]
    assets[references/scripts metadata]
    agent[rskit-agent]
    mcp[rskit-mcp]

    skill --> manifest
    skill --> loader
    skill --> verifier
    skill --> assets
    loader --> agent
    loader --> mcp
```
