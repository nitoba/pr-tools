# Repository Guidelines

## Project Structure & Module Organization

`apps/rust/` contains the `prt` CLI crate. Application features live in `apps/rust/src/features/`, integrations in `src/azure/`, AI provider code in `src/ai/`, Git/configuration helpers in `src/git/` and `src/config/`, and Ratatui UI code in `src/ui/`. Unit tests are colocated with modules; CLI integration tests are in `apps/rust/tests/`. TUI reference snapshots are versioned under `apps/rust/src/tui/snapshots/`. Repository automation and installers are in `scripts/`; CI and release workflows are in `.github/workflows/`.

## Build, Test, and Development Commands

Run these from the repository root:

```bash
cargo fmt --manifest-path apps/rust/Cargo.toml -- --check
cargo clippy --manifest-path apps/rust/Cargo.toml --locked --all-targets -- -D clippy::correctness
cargo test --manifest-path apps/rust/Cargo.toml --locked
cargo build --manifest-path apps/rust/Cargo.toml --locked --all-targets
./scripts/build-rust.sh [linux-x64|linux-arm64|macos-arm64|windows-x64]
```

The first four commands match CI checks. The build script runs verification before producing `apps/rust/dist/prt-rust-<platform>`; pass `--no-verify` only when checks were already run.

## Coding Style & Naming Conventions

Use Rust 2024 with four-space indentation and `rustfmt`; keep formatting and Clippy clean. Follow Rust naming conventions: `snake_case` for functions/modules, `CamelCase` for types and enum variants, and `SCREAMING_SNAKE_CASE` for constants. Prefer focused feature modules and explicit error propagation with `Result`; do not add secrets or local configuration files to the repository.

## Testing Guidelines

Tests use Rust’s built-in framework, Tokio tests for async paths, and Insta for TUI snapshots. Name tests descriptively by behavior, such as `doctor_reports_missing_pat`. For intentional UI changes, generate candidates with `INSTA_UPDATE=new cargo test --manifest-path apps/rust/Cargo.toml --locked`, review them with `cargo insta review` from `apps/rust/`, then rerun with `INSTA_UPDATE=no`. Commit accepted `.snap` files with the code change. No separate coverage threshold is configured.

## Commit & Pull Request Guidelines

Use short imperative Conventional Commit-style subjects with an optional scope, matching history examples such as `feat(tui): ...`, `fix: ...`, `refactor(rust): ...`, and `chore(ui): ...`. PRs should explain the user-visible or behavioral change, link the relevant issue or work item when available, list validation commands, and include terminal screenshots or updated snapshots for TUI changes. Keep unrelated cleanup out of the PR.

## Security & Configuration

Never commit Azure PATs, API keys, `.env` files, or generated local configuration. Use `prt init` for local setup and `prt doctor` to diagnose provider, Git, and Azure DevOps access.
