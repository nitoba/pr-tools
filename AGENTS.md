# Repository Guidelines

## Project Structure & Module Organization

`crates/prt/` contains the `prt` CLI crate. The physical layout follows explicit architectural boundaries: shared primitives in `src/core/`, external integrations in `src/integrations/`, application use cases in `src/features/`, configuration in `src/config/`, and UI code in `src/tui/`. `src/lib.rs` preserves the stable public module names (`prt::ai`, `prt::azure`, `prt::git`, etc.) while mapping them to those boundaries. Unit tests are colocated; integration tests are in `crates/prt/tests/`; TUI snapshots are in `crates/prt/src/tui/snapshots/`. See `ARCHITECTURE.md` before adding new top-level modules. Scripts and workflows are in `scripts/` and `.github/workflows/`.

## CLI Tooling

The CLI is a Rust 2024 binary built with Cargo. `clap` handles arguments; `ratatui`/`crossterm` handle terminal flows; `tokio` handles async work; `reqwest`/`aisdk` support API providers; and `insta`/`cargo-insta` cover TUI snapshots. Bash, PowerShell Core, and GitHub Actions handle installation, packaging, CI, and releases.

## Build, Test, and Development Commands

Run these from the repository root:

```bash
cargo fmt --manifest-path crates/prt/Cargo.toml -- --check
cargo clippy --manifest-path crates/prt/Cargo.toml --locked --all-targets -- -D clippy::correctness
cargo test --manifest-path crates/prt/Cargo.toml --locked
cargo build --manifest-path crates/prt/Cargo.toml --locked --all-targets
./scripts/build-rust.sh [linux-x64|linux-arm64|macos-arm64|windows-x64]
```

The first four commands match CI. The build script verifies first and writes `crates/prt/dist/prt-rust-<platform>`; use `--no-verify` only after checks have run.

## Rust Skills for Contributors

For Rust changes, consult `.agents/skills/rust-best-practices/SKILL.md` for ownership, errors, tests, docs, and Clippy. Invoke `/rust-skills` for design and read matching `.agents/skills/rust-skills/rules/` prefixes such as `own-*`, `err-*`, `async-*`, `test-*`, `lint-*`, or `proj-*`. For Ratatui, crossterm, or CLI changes, consult `.agents/skills/tui-design/SKILL.md` plus `references/ecosystem-rust.md` or `references/cli-basics.md`. Project tests are authoritative.

## Architecture Boundaries

Keep dependency direction intentional. `core/` contains only cross-cutting primitives; `integrations/` owns Git, Azure DevOps, and AI adapters; `features/` owns product use cases and orchestration; `tui/` owns presentation and interaction. New external services belong in `integrations/`, not directly in TUI code. New business flows belong in `features/`, not in `main.rs`. Split a new Cargo crate only when a boundary has an independent lifecycle, test surface, or reuse need; do not create crates merely to make files smaller.

## Coding Style & Naming Conventions

Use Rust 2024 with four-space indentation and `rustfmt`; keep formatting and Clippy clean. Follow Rust naming conventions: `snake_case` for functions/modules, `CamelCase` for types and enum variants, and `SCREAMING_SNAKE_CASE` for constants. Prefer focused feature modules and `Result`-based error propagation.

## Testing Guidelines

Tests use Rust’s built-in framework, `#[tokio::test]`, and Insta snapshots. Name tests by behavior, such as `doctor_reports_missing_pat`. For UI changes, generate candidates with `INSTA_UPDATE=new cargo test --manifest-path crates/prt/Cargo.toml --locked`, review via `cargo insta review` from `crates/prt/`, then rerun with `INSTA_UPDATE=no`. Commit accepted `.snap` files; no coverage threshold is configured.

## Commit & Pull Request Guidelines

Use short imperative Conventional Commit-style subjects with an optional scope, matching history examples such as `feat(tui): ...`, `fix: ...`, `refactor(rust): ...`, and `chore(ui): ...`. PRs should explain the user-visible or behavioral change, link the relevant issue or work item when available, list validation commands, and include terminal screenshots or updated snapshots for TUI changes. Keep unrelated cleanup out of the PR.

## Security & Configuration

Never commit Azure PATs, API keys, `.env` files, or generated configuration. Use `prt init` and `prt doctor` to configure and diagnose access.
