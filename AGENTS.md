# Repository Guidelines

## Project Structure & Module Organization

`apps/rust/` contains the `prt` CLI crate. Features live in `src/features/`; integrations in `src/azure/`, `src/ai/`, `src/git/`, and `src/config/`; UI code in `src/ui/`. Unit tests are colocated; integration tests are in `apps/rust/tests/`; TUI snapshots are in `apps/rust/src/tui/snapshots/`. Scripts and workflows are in `scripts/` and `.github/workflows/`.

## CLI Tooling

The CLI is a Rust 2024 binary built with Cargo. `clap` handles arguments; `ratatui`/`crossterm` handle terminal flows; `tokio` handles async work; `reqwest`/`aisdk` support API providers; and `insta`/`cargo-insta` cover TUI snapshots. Bash, PowerShell Core, and GitHub Actions handle installation, packaging, CI, and releases.

## Build, Test, and Development Commands

Run these from the repository root:

```bash
cargo fmt --manifest-path apps/rust/Cargo.toml -- --check
cargo clippy --manifest-path apps/rust/Cargo.toml --locked --all-targets -- -D clippy::correctness
cargo test --manifest-path apps/rust/Cargo.toml --locked
cargo build --manifest-path apps/rust/Cargo.toml --locked --all-targets
./scripts/build-rust.sh [linux-x64|linux-arm64|macos-arm64|windows-x64]
```

The first four commands match CI. The build script verifies first and writes `apps/rust/dist/prt-rust-<platform>`; use `--no-verify` only after checks have run.

## Rust Skills for Contributors

For Rust changes, consult `.agents/skills/rust-best-practices/SKILL.md` for ownership, errors, tests, docs, and Clippy. Invoke `/rust-skills` for design and read matching `.agents/skills/rust-skills/rules/` prefixes such as `own-*`, `err-*`, `async-*`, `test-*`, `lint-*`, or `proj-*`. For Ratatui, crossterm, or CLI changes, consult `.agents/skills/tui-design/SKILL.md` plus `references/ecosystem-rust.md` or `references/cli-basics.md`. Project tests are authoritative.

## Coding Style & Naming Conventions

Use Rust 2024 with four-space indentation and `rustfmt`; keep formatting and Clippy clean. Follow Rust naming conventions: `snake_case` for functions/modules, `CamelCase` for types and enum variants, and `SCREAMING_SNAKE_CASE` for constants. Prefer focused feature modules and `Result`-based error propagation.

## Testing Guidelines

Tests use Rust’s built-in framework, `#[tokio::test]`, and Insta snapshots. Name tests by behavior, such as `doctor_reports_missing_pat`. For UI changes, generate candidates with `INSTA_UPDATE=new cargo test --manifest-path apps/rust/Cargo.toml --locked`, review via `cargo insta review` from `apps/rust/`, then rerun with `INSTA_UPDATE=no`. Commit accepted `.snap` files; no coverage threshold is configured.

## Commit & Pull Request Guidelines

Use short imperative Conventional Commit-style subjects with an optional scope, matching history examples such as `feat(tui): ...`, `fix: ...`, `refactor(rust): ...`, and `chore(ui): ...`. PRs should explain the user-visible or behavioral change, link the relevant issue or work item when available, list validation commands, and include terminal screenshots or updated snapshots for TUI changes. Keep unrelated cleanup out of the PR.

## Security & Configuration

Never commit Azure PATs, API keys, `.env` files, or generated configuration. Use `prt init` and `prt doctor` to configure and diagnose access.
