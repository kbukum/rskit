# Maintainers

This file lists the people responsible for the rskit project. Maintainers are responsible for code review, releases, and project direction.

## Core Maintainers

| Name      | GitHub      | Areas             |
|-----------|-------------|-------------------|
| K. Bukum  | @kbukum     | All crates        |

## Bus Factor: 1 — Co-Maintainers Wanted

rskit currently has a **single core maintainer**. This is a known sustainability risk for a project of this size (40+ crates). We are actively looking for contributors interested in becoming co-maintainers, particularly in the following areas:

- **Foundation**: `rskit-errors`, `rskit-config`, `rskit-logging`, `rskit-validation`
- **Patterns**: `rskit-provider`, `rskit-resilience`, `rskit-di`, `rskit-bootstrap`, `rskit-observability`
- **Data & Flow**: `rskit-pipeline`, `rskit-dag`, `rskit-worker`, `rskit-sse`, `rskit-chain`
- **Transport**: `rskit-server`, `rskit-grpc`, `rskit-http`, `rskit-httpclient`
- **Storage / Infra**: `rskit-database`, `rskit-cache`, `rskit-storage`, `rskit-storage-s3`, `rskit-storage-gcs`, `rskit-messaging`
- **AI/ML**: `rskit-ai`, `rskit-llm`, `rskit-llm-openai`, `rskit-llm-anthropic`, `rskit-llm-gemini`, `rskit-llm-ollama`, `rskit-bench`, `rskit-dataset`, `rskit-embedding`, `rskit-inference`, `rskit-vectorstore`, `rskit-agent`, `rskit-tool`, `rskit-mcp`
- **Media**: `rskit-media`, `rskit-media-audio`, `rskit-media-image`, `rskit-media-ffmpeg`
- **Security**: `rskit-auth`, `rskit-authz`, `rskit-encryption`

If you are interested, please open an issue using the [engineering review template](.github/ISSUE_TEMPLATE/) describing your area of interest and recent contributions, or start by picking up issues labelled `good-first-issue` / `help-wanted`.

## How Maintainers Are Added

New maintainers are added by the existing core maintainers via a pull request that updates this file. Candidates are typically long-term contributors who have demonstrated:

- A track record of high-quality contributions across multiple areas of the codebase.
- Familiarity with project conventions, the split workspace layout, the layering rules (enforced by topology and dependency policy checks), and the release process.
- A commitment to responsive code review.

## Responsibilities

Maintainers are expected to:

- Review pull requests within a reasonable timeframe.
- Triage issues and security reports (see [SECURITY.md](SECURITY.md)).
- Cut releases following the process documented in [docs/RELEASING.md](docs/RELEASING.md).
- Uphold the [Code of Conduct](CODE_OF_CONDUCT.md).
- Maintain sibling parity with [`gokit`](https://github.com/kbukum/gokit) and [`pykit`](https://github.com/kbukum/pykit).

## Becoming Inactive / Stepping Down

A maintainer who has been inactive for 6 months may be moved to an "Emeritus" section by the remaining maintainers. Maintainers are encouraged to step down explicitly by opening a PR to update this file.

## Emeritus Maintainers

_No emeritus maintainers yet._

## Contact

For routine project communication, use GitHub issues or discussions. For security issues, see [SECURITY.md](SECURITY.md).
