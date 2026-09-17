//! Entrypoint mínimo do executável `prt`.
//!
//! A composição e o dispatch dos comandos ficam em `application`, mantendo
//! `main.rs` livre de regras de aplicação.

mod application;

fn main() {
    application::run();
}
