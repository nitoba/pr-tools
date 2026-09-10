//! Eventos do backend → TUI (`mpsc`), sem segurar lock através de `.await`.

use crate::ai::PrDescription;
use crate::azure::pull_requests::PublishedPr;

/// Evento emitido pela tarefa de geração para a UI.
#[derive(Debug)]
pub enum BackendEvent {
    /// Linha de log (ex.: "tentando codex (gpt-5.6-luna)").
    Log(String),
    /// Chunk de texto do stream (efeito typing).
    Token(String),
    /// Progresso 0.0–1.0 + rótulo.
    Progress(f64, String),
    /// Fase textual ("gerando", "reescrevendo", "publicando").
    Phase(String),
    /// Geração concluída.
    Finished(PrDescription, String),
    /// Publicação concluída (um item por target).
    Published(Vec<PublishedPr>),
    /// Um PR foi criado durante uma publicação multi-target.
    PublishedOne(PublishedPr),
    /// Falha terminal.
    Failed(String),
}

/// Resultado final do runner interativo.
#[derive(Debug)]
pub enum LiveOutcome {
    /// Usuário saiu com a descrição pronta (+ PRs publicados, se houver).
    Done {
        /// Descrição gerada.
        desc: PrDescription,
        /// PRs publicados (vazio se saiu sem publicar).
        published: Vec<PublishedPr>,
    },
    /// Usuário abortou (q/Esc/Ctrl-C).
    Aborted,
    /// Falha exibida na TUI.
    Failed(String),
}
