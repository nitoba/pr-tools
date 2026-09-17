//! Composição do executável e dispatch dos comandos da CLI.
//!
//! `runtime.rs` contém a orquestração existente. Ele é incluído como um
//! submódulo privado para que a refatoração mova a lógica sem alterá-la.

mod runtime {
    include!("runtime.rs");

    pub(super) fn run() {
        main();
    }
}

pub fn run() {
    runtime::run();
}
