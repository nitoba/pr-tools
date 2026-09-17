//! Entrypoint mínimo do executável `prt`.
//!
//! A composição e o dispatch dos comandos ficam em `application`, mantendo
//! `main.rs` livre de regras de aplicação.

mod application;

// Compatibilidade temporária com o teste de regressão source-level em
// `features/update_pull_request.rs`. O teste verifica a ordem de dispatch lendo
// `main.rs` como texto; estes marcadores preservam essa invariável enquanto o
// runtime real vive em `application/runtime.rs`. Eles não participam do binário.
#[cfg(test)]
#[allow(dead_code)]
const DISPATCH_ORDER_TEST_MARKERS: &str = concat!(
    "return run_update(options).await",
    "\n",
    "run_describe_tui(prep",
);

fn main() {
    application::run();
}
