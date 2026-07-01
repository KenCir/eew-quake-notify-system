---
name: eew-quake-notify-system
description: Build and maintain this Rust background service for Project DM-D.S.S earthquake information, automatic notifications, and text-to-speech readouts. Use when implementing the DM-D.S.S client, event parsing, alert deduplication, notification adapters, TTS output, config, logging, retry/reconnect behavior, or tests for this repository.
---

# EEW Quake Notify System

Use this skill when working on the Rust earthquake notification and readout system in this repository.

## Required First Steps

1. Read `AGENTS.md`.
2. Inspect `Cargo.toml` and the affected source files before choosing crates or structure.
3. If touching Project DM-D.S.S integration behavior, verify current official documentation for endpoints, authentication, payload schemas, and limits before coding.
4. Keep credentials and environment-specific values out of committed files.
5. Do not implement secondary distribution features such as webhooks, chat notifications, email forwarding, public feeds, or relay services.

## Implementation Workflow

1. Define the smallest internal event model needed by the feature.
2. Add or update external DTOs separately from the internal model.
3. Convert external payloads into normalized internal events with explicit error handling.
4. Pass normalized events through deduplication before notification or speech.
5. Keep desktop notification and TTS code behind traits so tests can use fakes and future platforms can be added cleanly.
6. Add focused tests for parsing, normalization, deduplication, and speech text.
7. Run `cargo fmt`, then run tests and clippy when available.

## Runtime Design Rules

- Prefer async IO for network work.
- Use bounded channels for communication between receiver, notification, and TTS tasks.
- Add cancellation paths for long-running loops.
- Use retry backoff for reconnectable failures.
- Log enough context to debug remote payload, connection, and adapter failures without logging secrets.
- Treat notification and TTS failures as adapter failures, not as reasons to stop receiving earthquake data.
- Keep runtime configuration file based so a future GUI can read and write the same settings.
- Favor cross-platform abstractions where practical, but do not delay the initial local desktop implementation for unsupported platforms.

## Distribution Restrictions

- Keep this system scoped to personal local use.
- Do not add Discord, Slack, LINE, webhook, email, remote push, multi-user broadcast, or feed relay outputs.
- Do not design APIs that expose received EEW or earthquake information for other users or systems.
- If a feature could be interpreted as secondary distribution of Project DM-D.S.S data, do not implement it without explicit project-level review.

## Speech And Notification Rules

- Generate concise Japanese readout text.
- Avoid speaking unchanged updates repeatedly.
- Speak cancellation, final report, maximum intensity, magnitude, epicenter, and affected area when available.
- Keep notification titles short and bodies structured.
- Make severity decisions explicit and testable.
- Use VOICEVOX as the initial speech engine, assuming a local VOICEVOX HTTP API unless the config says otherwise.

## Validation Checklist

- `cargo fmt`
- `cargo test --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

If a command cannot be run, report why and mention the remaining risk.
