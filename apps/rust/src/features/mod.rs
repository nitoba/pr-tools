//! Features — `desc`, `test`, `init`, `doctor` e `update`.
//!
//! Cada módulo espelha o fluxo do Dart, mas com TUI Ratatui e
//! `Result<T, AppError>` + `?` (sem `unwrap` em produção).

pub mod describe;
pub mod doctor;
pub mod init;
pub mod process_profiles;
pub mod session;
pub mod test_card;
pub mod update;
pub mod update_pull_request;
