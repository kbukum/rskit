# Architecture Decision Records (ADR)

ADRs document **significant** architectural choices: tradeoffs considered,
why we picked one option, and the consequences. They are immutable —
superseded ADRs are kept and linked from the replacement.

## When to write an ADR

- Adding or replacing a foundation-tier crate
- Introducing a new layer in the cargo-deny / cargo-machete contract
- Changing the lifecycle / boot model
- Anything that locks downstream callers into a pattern that is costly to
  reverse

## Format

Each ADR is a Markdown file named `NNNN-short-kebab-title.md` where NNNN
is the next zero-padded sequence number. Use [`0000-template.md`](0000-template.md)
as the starting point.

## Index

| #     | Title                                              | Status   |
|-------|----------------------------------------------------|----------|
| 0001  | [Layered crate architecture](0001-layered-crate-architecture.md) | Accepted |
