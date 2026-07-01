---
name: rust-code-style
description: Apply Rust code style, module organization, error handling, async design, testing, and dependency discipline for this repository. Use when writing, reviewing, or refactoring Rust code, adding crates, designing public/internal APIs, or improving code quality in this Project DM-D.S.S earthquake notification system.
---

# Rust Code Style

Use this skill when changing Rust code in this repository.

## Required First Steps

1. Read `AGENTS.md`.
2. Inspect `Cargo.toml` and nearby source files before adding crates or changing structure.
3. Preserve existing style unless it conflicts with this skill or `AGENTS.md`.

## Formatting And Naming

- Run `cargo fmt` before finishing.
- Use idiomatic Rust names: `snake_case` for functions and variables, `PascalCase` for types and traits, `SCREAMING_SNAKE_CASE` for constants.
- Prefer clear domain names over abbreviations except for established terms such as EEW, API, URL, ID, and TTS.
- Keep functions small enough that error paths and side effects are easy to see.
- Avoid unnecessary comments; add comments only for non-obvious domain rules or failure handling.

## Module Boundaries

- Keep external API DTOs separate from internal domain models.
- Put parsing and conversion near the API client or model boundary.
- Keep notification, TTS, configuration, and state management behind narrow interfaces.
- Avoid global mutable state.
- Prefer explicit dependencies passed through constructors or function parameters.

## Error Handling

- Return `Result<T, E>` for recoverable failures.
- Prefer domain-specific error enums with `thiserror` for reusable modules.
- Use `anyhow` only at executable boundaries or orchestration layers where rich context is more useful than typed matching.
- Add context to IO, network, config, and parse errors.
- Do not panic in long-running background tasks except for impossible programmer errors.

## Async And Runtime Style

- Prefer async IO for network clients and long-lived receivers.
- Avoid blocking inside async tasks; use `spawn_blocking` or a worker thread for synchronous TTS or OS notification calls.
- Use bounded channels for task communication.
- Make reconnect loops cancellable and testable.
- Add backoff and jitter for remote service reconnects.
- Log task start, stop, reconnect, and adapter failures with `tracing`.

## Data And Config

- Deserialize remote payloads with `serde`.
- Use explicit types for timestamps, IDs, severities, intensities, and cancellation/final states.
- Validate config at startup and fail with actionable errors.
- Do not commit secrets or real endpoint-specific credentials.
- Prefer sample config files with placeholders when examples are needed.

## Testing Rules

- Unit-test pure logic first: parsing, conversion, deduplication, severity decisions, and readout text.
- Use fixture payloads instead of live network calls.
- Use fake notification and TTS adapters in tests.
- Keep timing tests deterministic.
- Add regression tests for bugs found in event handling or duplicate suppression.

## Dependency Rules

- Prefer standard library features before adding crates.
- Add crates only when they reduce meaningful complexity or improve reliability.
- Prefer actively maintained crates with clear Rust async ecosystem compatibility.
- Explain new dependencies in the change summary.

## Validation Checklist

- `cargo fmt`
- `cargo test --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

If a command cannot be run, report the reason and the residual risk.
