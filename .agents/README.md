# Project Agent Skills

This directory contains Codex skills that are specific to this repository.

## Skills

### `$eew-quake-notify-system`

Use this skill for the Rust background system that receives Project DM-D.S.S earthquake information and performs local desktop notifications and VOICEVOX readouts.

Primary scope:

- DM-D.S.S client / WebSocket / reconnect behavior
- Event parsing and internal event models
- Deduplication
- Desktop notification
- VOICEVOX TTS
- Config / logging
- Runtime pipeline and tests

Important constraints:

- Keep the system scoped to personal local use.
- Do not implement secondary distribution features such as Discord, Slack, LINE, webhooks, email, remote push, or feed relay outputs.
- Verify official Project DM-D.S.S documentation before changing API behavior.

Path: `skills/eew-quake-notify-system/SKILL.md`

### `$rust-code-style`

Use this skill for Rust code style, module boundaries, error handling, async design, testing, and dependency discipline in this project.

Primary scope:

- Rust code writing / refactoring / review
- Module organization
- Typed errors and executable-boundary errors
- Async runtime rules
- Unit tests and fixture tests
- Dependency selection

Standard validation:

- `cargo fmt`
- `cargo test --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

Path: `skills/rust-code-style/SKILL.md`

## Usage Notes

- Prefer the root `AGENTS.md` for always-on project rules.
- Use `$rust-code-style` when implementing or reviewing Rust code.
- Use `$eew-quake-notify-system` for domain-specific design or implementation work.
- After adding or changing a skill, validate that skill with `quick_validate.py`.
