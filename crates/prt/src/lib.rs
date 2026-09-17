//! `prt` — CLI para gerar descrições de PR e Test Cases a partir do contexto Git.
//!
//! O crate é organizado por responsabilidade, mantendo uma fachada pública
//! estável (`prt::ai`, `prt::azure`, `prt::git`, `prt::tui`, ...):
//!
//! - `core/`: primitivas compartilhadas e infraestrutura transversal mínima;
//! - `integrations/`: adaptadores para IA, Azure DevOps e Git;
//! - `features/`: casos de uso da aplicação;
//! - `tui/`: apresentação e fluxos interativos;
//! - `config/` e `cli.rs`: configuração e contrato de linha de comando.
//!
//! A organização física pode evoluir sem obrigar os consumidores internos a
//! conhecer esses detalhes. Os nomes públicos existentes são preservados para
//! manter compatibilidade e reduzir o risco de regressão nesta refatoração.

#[path = "integrations/ai/mod.rs"]
pub mod ai;
#[path = "integrations/azure/mod.rs"]
pub mod azure;
pub mod cli;
pub mod config;
#[path = "core/error.rs"]
pub mod error;
pub mod features;
#[path = "integrations/git/mod.rs"]
pub mod git;
#[path = "core/process.rs"]
pub(crate) mod process;
pub mod tui;
