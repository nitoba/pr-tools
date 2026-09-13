//! Erros tipados da CLI (`thiserror` no lib, `anyhow` só no binário).
//!
//! Hierarquia espelha `AppFailure` do Dart: cada variante carrega mensagem
//! humana + `exit_code` para `main`.

use thiserror::Error;

/// Erro raiz da aplicação.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AppError {
    /// Falha de CLI / argumentos inválidos.
    #[error("{message}")]
    Cli {
        /// Mensagem para o usuário (minúscula, sem pontuação final — `err-lowercase-msg`).
        message: String,
        /// Código de saída do processo.
        exit_code: i32,
    },
    /// Falha de configuração (arquivo, env, validação).
    #[error("configuração inválida: {message}")]
    Config {
        /// Detalhe do problema.
        message: String,
    },
    /// Falha ao coletar contexto Git.
    #[error("git: {message}")]
    Git {
        /// Detalhe do problema.
        message: String,
    },
    /// Falha da API do Azure DevOps.
    #[error("azure devops (http {status}): {message}")]
    Azure {
        /// Status HTTP (0 = erro de transporte).
        status: u16,
        /// Corpo/mensagem resumida.
        message: String,
    },
    /// O contexto funcional não pôde ser carregado antes da geração.
    #[error("contexto funcional: {message}")]
    FunctionalContext {
        /// Mensagem acionável sem payload bruto do Work Item.
        message: String,
    },
    /// Falha de geração via IA.
    #[error("ia ({provider}): {message}")]
    Ai {
        /// Provider que falhou (`codex`, `opencode`, `openai-compatible`).
        provider: String,
        /// Detalhe.
        message: String,
    },
    /// Descrição excede o limite do Azure (< 4000 chars).
    #[error("a descrição do pr excede o limite do azure devops: {length} caracteres (máximo 3999)")]
    DescriptionTooLong {
        /// Tamanho observado.
        length: usize,
    },
    /// Operação cancelada pelo usuário.
    #[error("operação cancelada")]
    Cancelled,
    /// Erro de IO.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Erro HTTP imprevisto.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// Falha ao baixar ou instalar uma atualização.
    #[error("atualização: {message}")]
    Update {
        /// Detalhe da falha.
        message: String,
    },
}

impl AppError {
    /// Código de saída do processo para este erro.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Cli { exit_code, .. } => *exit_code,
            Self::Cancelled => 130,
            _ => 1,
        }
    }

    /// Construtor para erro de CLI.
    pub fn cli(message: impl Into<String>) -> Self {
        Self::Cli {
            message: message.into(),
            exit_code: 2,
        }
    }
}

/// Resultado padrão do lib.
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_error_should_carry_exit_code_2() {
        let err = AppError::cli("opção desconhecida: --foo");
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.to_string(), "opção desconhecida: --foo");
    }

    #[test]
    fn description_too_long_should_mention_limit() {
        let err = AppError::DescriptionTooLong { length: 5000 };
        assert!(err.to_string().contains("3999"));
        assert_eq!(err.exit_code(), 1);
    }
}
