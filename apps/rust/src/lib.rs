//! `prt` — CLI para gerar descrições de PR e Test Cases a partir do contexto Git.
//!
//! Espelha a CLI Dart `pr-tools` (`prt` v4): comandos `desc`, `test`,
//! `init` e `doctor`, com acesso ao Azure DevOps via REST e geração de
//! conteúdo via Codex / `OpenCode` / endpoint OpenAI-compatible (via `aisdk`).
//!
//! # Examples
//!
//! ```no_run
//! use prt::cli::parse_cli;
//! let opts = parse_cli(["prt", "desc", "--dry-run"]);
//! assert!(opts.is_ok());
//! ```

pub mod ai;
pub mod azure;
pub mod cli;
pub mod config;
pub mod error;
pub mod features;
pub mod git;
pub mod tui;
