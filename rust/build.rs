//! Build script mínimo: expõe o hash do commit via `PRT_COMMIT`.
//!
//! Usado pelo `--version` (`option_env!("PRT_COMMIT")`).
//! O crate vive em `rust/`, então o Git é consultado a partir da raiz do
//! monorepo. Sem lógica de build — só resolve o hash, com fallback `unknown`.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../.git/HEAD");
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    let repository_root = manifest_dir.parent().unwrap_or(Path::new("."));
    let short = Command::new("git")
        .arg("-C")
        .arg(repository_root)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    if let Some(hash) = short {
        println!("cargo:rustc-env=PRT_COMMIT={hash}");
    } else {
        println!("cargo:rustc-env=PRT_COMMIT=unknown");
    }
}
