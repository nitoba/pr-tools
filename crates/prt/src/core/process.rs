//! Resolução de comandos externos que também funcionam com shims `.cmd` no Windows.

/// Retorna os nomes possíveis para um comando instalado no PATH.
///
/// No Windows, CLIs instalados por npm normalmente expõem um `.cmd`, enquanto
/// CLIs nativos podem ser encontrados como `.exe` quando o sufixo é omitido.
pub(crate) fn command_candidates(program: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let mut candidates = vec![program.to_owned()];
        if !program.to_ascii_lowercase().ends_with(".cmd") {
            candidates.push(format!("{program}.cmd"));
        }
        candidates
    }

    #[cfg(not(windows))]
    {
        vec![program.to_owned()]
    }
}

#[cfg(test)]
mod tests {
    use super::command_candidates;

    #[cfg(not(windows))]
    #[test]
    fn does_not_add_windows_shim_on_unix() {
        assert_eq!(command_candidates("codex"), vec!["codex"]);
    }

    #[cfg(windows)]
    #[test]
    fn adds_cmd_shim_on_windows() {
        assert_eq!(command_candidates("codex"), vec!["codex", "codex.cmd"]);
    }
}
