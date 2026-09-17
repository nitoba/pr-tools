//! Eventos do backend → TUI (`mpsc`), sem segurar lock através de `.await`.

use crate::ai::PrDescription;
use crate::azure::pull_requests::{PublishedPr, PullRequestCandidate};
use crate::features::describe::PublishFailure;
use crate::features::test_card::TestCardLaunchContext;

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
    /// Um target entrou em processamento.
    PublishingTarget(String),
    /// Falha recuperável durante a publicação.
    PublishFailed(PublishFailure),
    /// Resultado da busca de possíveis PRs já criados.
    CandidatesLoaded {
        /// Candidatos encontrados.
        candidates: Vec<PullRequestCandidate>,
        /// Falha opcional da consulta.
        error: Option<String>,
    },
    /// Falha terminal.
    Failed(String),
}

/// Resultado final do runner interativo.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum LiveOutcome {
    /// Usuário saiu com a descrição pronta (+ PRs publicados, se houver).
    Done {
        /// Descrição gerada.
        desc: PrDescription,
        /// PRs publicados (vazio se saiu sem publicar).
        published: Vec<PublishedPr>,
    },
    /// Handoff explícito da receipt publicada para um único Test Case.
    PrepareTestCase {
        /// Descrição original, usada para reportar a receipt após o segundo fluxo.
        desc: PrDescription,
        /// Contexto estruturado do PR selecionado.
        launch_context: TestCardLaunchContext,
        /// Todos os PRs publicados, inclusive os não selecionados.
        published: Vec<PublishedPr>,
    },
    /// Usuário abortou (q/Esc/Ctrl-C).
    Aborted,
    /// Usuário descartou explicitamente a sessão local.
    Discarded,
    /// Falha exibida na TUI.
    Failed(String),
}
