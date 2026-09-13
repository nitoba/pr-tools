//! Fluxo TUI `prt test` — gera card de Test Case, revisa e cria.
//!
//! Fases: Preparando → Gerando → Revisão (+settings) → Criando → Pronto/Erro,
//! espelhando o comando Dart: confirmação "Criar este Test Case?" (initial
//! `--create`), settings com validação, criação, e confirmação "Atualizar
//! Work Item pai para Test QA?" com campos de esforço.
//! O backend (`prepare` + `generate` via `features::test_card`) roda em
//! `tokio::spawn` e empurra eventos locais por `mpsc`; a UI redesenha a
//! ~30fps com dirty-flag. A revisão mostra preview Markdown com highlight
//! mais painel de settings com 6 campos de texto editáveis.

use std::collections::VecDeque;
use std::future::Future;
use std::io::IsTerminal;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget, Wrap,
    },
};
use tokio::sync::mpsc;
use unicode_width::UnicodeWidthChar;

use super::content_editor::{
    ContentEditAction, ContentEditState, ContentField, render_content_editor,
};
use super::markdown::{markdown_text, title_line};
use super::{
    StatusHeader, border_type, centered_buttons, modal_frame, status_header, status_layout, theme,
};
use crate::ai::PrDescription;
use crate::azure::WorkItem;
use crate::cli::CliOptions;
use crate::features::test_card::{
    self, CreateFailure, CreateFailureKind, TestCardPrep, TestCardRequest, TestCaseCandidate,
    TestSettings, TestSettingsField,
};

/// Resultado final do fluxo de teste para o `main`.
#[derive(Debug)]
pub enum TestFlowOutcome {
    /// Test Case criado (id + URL p/ recibo fora da TUI).
    Created {
        /// ID do Test Case criado.
        id: i64,
        /// URL do work item p/ abrir no navegador.
        url: String,
    },
    /// Usuário revisou o card mas saiu sem criar.
    Reviewed,
    /// Usuário revisou com `--no-create` (saída normal, sem criar).
    ReviewedNoCreate,
    /// Usuário abortou antes da revisão.
    Aborted,
}

/// Evento local do backend → UI (sem segurar lock através de `.await`).
#[derive(Debug)]
enum TestEvent {
    /// Linha de log (ex.: "pai #11763 resolvido").
    Log(String),
    /// Pedaço de texto p/ animar o preview (efeito typing).
    Token(String),
    /// Progresso 0.0–1.0 + rótulo.
    Progress(f64, String),
    /// Rótulo textual da fase ("preparando…", "gerando…").
    PhaseLabel(String),
    /// Geração concluída (prep + card + settings iniciais).
    Generated {
        /// Contexto preparado (p/ criar e atualizar o pai depois).
        prep: Box<TestCardPrep>,
        /// Título gerado.
        title: String,
        /// Corpo Markdown gerado.
        body: String,
        /// Valores iniciais dos 6 campos de settings (ordem de `field_label`).
        initial: [String; 6],
    },
    /// Criação concluída no Azure.
    CreatedItem(WorkItem),
    /// Falha recuperável na criação, preservando a revisão em tela.
    CreateFailed(CreateFailure),
    /// Resultado da busca por cards possivelmente criados antes de um timeout.
    CandidatesLoaded {
        /// Candidatos encontrados.
        candidates: Vec<TestCaseCandidate>,
        /// Falha opcional da busca.
        error: Option<String>,
    },
    /// Exclusão confirmada de um candidato.
    CandidateDeleted(i64),
    /// Falha ao excluir um candidato.
    CandidateDeleteFailed(String),
    /// Falha terminal de preparação ou geração.
    Failed(String),
}

/// Fase do fluxo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TestPhase {
    /// Coletando contexto git/pr/pai.
    #[default]
    Preparando,
    /// Gerando card via IA.
    Gerando,
    /// Preview + settings editáveis.
    Revisao,
    /// Criando Test Case no Azure.
    Criando,
    /// Criado (mostra id/URL e oferece a atualização do pai).
    Pronto,
    /// Erro terminal (mostra mensagem até sair).
    Erro,
}

impl TestPhase {
    /// Rótulo curto p/ o header.
    fn short(self) -> &'static str {
        match self {
            Self::Preparando => "preparando",
            Self::Gerando => "gerando",
            Self::Revisao => "revisão",
            Self::Criando => "criando",
            Self::Pronto => "pronto",
            Self::Erro => "erro",
        }
    }

    /// Fase ocupada (animação do status ativa)?
    fn busy(self) -> bool {
        matches!(self, Self::Preparando | Self::Gerando | Self::Criando)
    }
}

/// Painel focado na revisão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    /// Preview do card (scroll, copia, cria).
    Preview,
    /// Settings (edição dos 6 campos).
    Settings,
}

/// Total de campos de settings.
const FIELD_COUNT: usize = 6;

/// Diálogo modal do fluxo (confirmações e esforços do Test QA).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestDialog {
    /// "Criar este Test Case?" — bool = Sim selecionado?
    ConfirmCreate(bool),
    /// "Atualizar Work Item pai para Test QA?" — bool = Sim selecionado?
    ConfirmTestQa(bool),
    /// Campos de esforço (Effort + Real Effort).
    QaEfforts,
    /// Ações disponíveis depois de um resultado incerto da criação.
    CreateRecovery(usize),
    /// Candidatos retornados pela busca de duplicidade.
    CandidateList { selected: usize },
    /// Confirmação da exclusão de um candidato.
    DeleteCandidate { selected: usize, yes: bool },
    /// Divergência entre o checkout atual e o snapshot da publicação.
    PublishedContextDivergence(bool),
}

/// Atividade da consulta/exclusão de candidatos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateActivity {
    /// Nenhuma operação remota em andamento.
    Idle,
    /// Consultando possíveis cards.
    Loading,
    /// Movendo um candidato para a lixeira.
    Deleting,
}

/// Disponibilidade das ações após resultado incerto da criação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateRecoveryState {
    /// Não há envio pendente para recuperar.
    Unavailable,
    /// A última tentativa pode ter criado um item.
    Available,
}

/// Rótulo do campo de settings por índice.
fn field_label(idx: usize) -> &'static str {
    match idx {
        0 => "AreaPath",
        1 => "Responsável (AssignedTo)",
        2 => "IterationPath",
        3 => "Prioridade",
        4 => "Team *",
        _ => "Programa *",
    }
}

/// Dica do campo de settings por índice.
fn field_hint(idx: usize) -> &'static str {
    match idx {
        0 => r"ex.: MeuProjeto\Time",
        1 => "email ou vazio",
        2 => r"ex.: MeuProjeto\Sprint 12",
        3 => "número > 0 (ex.: 2)",
        4 => "Custom.Team (obrigatório)",
        _ => "Custom.ProgramasAgrotrace (obrigatório)",
    }
}

/// Editor de linha única mínimo (inserir/apagar/setas/Home/End, sem máscara).
#[derive(Debug, Clone)]
struct LineEditor {
    /// Conteúdo atual.
    value: String,
    /// Cursor em índice de char.
    cursor: usize,
}

impl LineEditor {
    /// Cria editor com cursor no fim.
    fn new(value: String) -> Self {
        let cursor = value.chars().count();
        Self { value, cursor }
    }

    /// Texto aparado p/ validação/envio.
    fn trimmed(&self) -> String {
        self.value.trim().to_owned()
    }

    /// Trata tecla de edição; `true` se consumiu.
    fn handle_key(&mut self, key: event::KeyEvent) -> bool {
        if key
            .modifiers
            .contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        match key.code {
            KeyCode::Char(c) => {
                let byte = char_byte_index(&self.value, self.cursor);
                self.value.insert(byte, c);
                self.cursor += 1;
                true
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let byte = char_byte_index(&self.value, self.cursor);
                    let prev = char_byte_index(&self.value, self.cursor - 1);
                    self.value.drain(prev..byte);
                    self.cursor -= 1;
                }
                true
            }
            KeyCode::Delete => {
                let len = self.value.chars().count();
                if self.cursor < len {
                    let byte = char_byte_index(&self.value, self.cursor);
                    let next = char_byte_index(&self.value, self.cursor + 1);
                    self.value.drain(byte..next);
                }
                true
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                let len = self.value.chars().count();
                self.cursor = (self.cursor + 1).min(len);
                true
            }
            KeyCode::Home => {
                self.cursor = 0;
                true
            }
            KeyCode::End => {
                self.cursor = self.value.chars().count();
                true
            }
            _ => false,
        }
    }
}

/// Índice de byte do n-ésimo char (saturado no fim).
fn char_byte_index(s: &str, char_idx: usize) -> usize {
    for (i, (b, _)) in s.char_indices().enumerate() {
        if i == char_idx {
            return b;
        }
    }
    s.len()
}

/// Estado completo da tela de teste.
struct TestApp {
    /// Fase atual.
    phase: TestPhase,
    /// Rótulo detalhado da fase.
    phase_label: String,
    /// Frame de animação (~30fps).
    tick: u64,
    /// Scroll vertical do preview.
    scroll: u16,
    /// Progresso 0.0–1.0.
    progress: f64,
    /// Rótulo do progresso.
    progress_label: String,
    /// Logs recentes (cap 200).
    logs: VecDeque<String>,
    /// Tokens brutos acumulados (animação durante geração).
    streamed_raw: String,
    /// Título final gerado.
    title: String,
    /// Corpo Markdown final gerado.
    body: String,
    /// Draft temporário de edição de título/body.
    content_edit: Option<ContentEditState>,
    /// Conteúdo aprovado congelado antes da primeira chamada remota.
    frozen_create_content: Option<PrDescription>,
    /// Contexto p/ criar/atualizar (chega no `Generated`).
    prep: Option<TestCardPrep>,
    /// 6 campos de settings (ordem de `field_label`).
    fields: [LineEditor; 6],
    /// Campo focado (0..6).
    field_focus: usize,
    /// Painel focado.
    panel: Panel,
    /// Erro de validação/falha p/ exibir.
    error: Option<String>,
    /// Campo apontado pelo último erro remoto, quando identificado.
    error_field: Option<TestSettingsField>,
    /// A última configuração realmente enviada ao Azure.
    last_create_settings: Option<TestSettings>,
    /// Mantém disponíveis as ações de recuperação após resultado incerto.
    create_recovery: CreateRecoveryState,
    /// Candidatos encontrados após uma resposta perdida.
    candidates: Vec<TestCaseCandidate>,
    /// Atividade remota sobre candidatos.
    candidate_activity: CandidateActivity,
    /// Mensagem da busca/exclusão de candidatos.
    candidate_message: Option<String>,
    /// Criado (id + url).
    created: Option<(i64, String)>,
    /// Pai já atualizado p/ Test QA?
    parent_updated: bool,
    /// Mensagem do update do pai / clipboard.
    parent_msg: Option<String>,
    /// Flash de "copiado" até o tick.
    copied_flash_until: u64,
    /// Valor inicial do "Criar este Test Case?" (vem de `--create`).
    create_initial: bool,
    /// `--no-create`: gera e revisa, sem criar.
    no_create: bool,
    /// Diálogo modal aberto (confirmações / esforços).
    dialog: Option<TestDialog>,
    /// Editor do Effort (Test QA).
    qa_effort: LineEditor,
    /// Editor do Real Effort (Test QA).
    qa_real: LineEditor,
    /// Campo de esforço focado (0 = Effort, 1 = Real Effort).
    qa_focus: usize,
    /// Indica que a escolha de snapshot remoto liberou o backend.
    start_after_divergence: bool,
}

impl TestApp {
    /// Estado inicial (antes do primeiro evento).
    fn new() -> Self {
        Self {
            phase: TestPhase::Preparando,
            phase_label: "preparando contexto…".to_owned(),
            tick: 0,
            scroll: 0,
            progress: 0.0,
            progress_label: "preparando".to_owned(),
            logs: VecDeque::with_capacity(200),
            streamed_raw: String::new(),
            title: String::new(),
            body: String::new(),
            content_edit: None,
            frozen_create_content: None,
            prep: None,
            fields: std::array::from_fn(|_| LineEditor::new(String::new())),
            field_focus: 4,
            panel: Panel::Preview,
            error: None,
            error_field: None,
            last_create_settings: None,
            create_recovery: CreateRecoveryState::Unavailable,
            candidates: Vec::new(),
            candidate_activity: CandidateActivity::Idle,
            candidate_message: None,
            created: None,
            parent_updated: false,
            parent_msg: None,
            copied_flash_until: 0,
            create_initial: false,
            no_create: false,
            dialog: None,
            qa_effort: LineEditor::new(String::new()),
            qa_real: LineEditor::new(String::new()),
            qa_focus: 0,
            start_after_divergence: false,
        }
    }

    /// Estado inicial ajustado ao contrato de entrada selecionado.
    fn for_request(request: &TestCardRequest) -> Self {
        let mut app = Self::new();
        match request {
            TestCardRequest::Cli(options) => {
                app.create_initial = options.create;
                app.no_create = options.no_create;
            }
            TestCardRequest::PublishedPr(_) => {
                app.create_initial = false;
                app.no_create = false;
            }
        }
        app
    }

    /// Avança 1 tick (~33ms).
    fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Rola o preview com clamp simples.
    fn scroll_by(&mut self, delta: i16) {
        let next = i32::from(self.scroll) + i32::from(delta);
        self.scroll = u16::try_from(next.clamp(0, 5000)).unwrap_or(u16::MAX);
    }

    /// Marca flash de "copiado" (~2s a 30fps).
    fn flash_copied(&mut self) {
        self.copied_flash_until = self.tick + 60;
    }

    /// Está no flash de copiado?
    fn is_copied_flash(&self) -> bool {
        self.tick < self.copied_flash_until
    }

    /// Corpo atual p/ copiar (final se houver, senão stream parcial).
    fn copy_body(&self) -> String {
        if let Some(content) = &self.frozen_create_content {
            return content.body.clone();
        }
        if self.body.is_empty() {
            self.streamed_raw.clone()
        } else {
            self.body.clone()
        }
    }

    /// Aplica evento do backend.
    fn on_event(&mut self, ev: TestEvent) {
        match ev {
            TestEvent::Log(line) => self.push_log(line),
            TestEvent::Token(chunk) => {
                self.streamed_raw.push_str(chunk.as_str());
                if self.phase == TestPhase::Preparando {
                    self.phase = TestPhase::Gerando;
                }
            }
            TestEvent::Progress(ratio, label) => {
                self.progress = ratio.clamp(0.0, 1.0);
                self.progress_label = label;
            }
            TestEvent::PhaseLabel(label) => {
                self.phase_label.clone_from(&label);
                if self.logs.len() >= 200 {
                    let _ = self.logs.pop_front();
                }
                self.logs.push_back(label);
                if self.phase == TestPhase::Preparando && self.phase_label.contains("gerando") {
                    self.phase = TestPhase::Gerando;
                }
            }
            TestEvent::Generated {
                prep,
                title,
                body,
                initial,
            } => self.on_generated(*prep, title, body, initial),
            TestEvent::CreatedItem(item) => self.on_created_item(&item),
            TestEvent::CreateFailed(failure) => self.on_create_failed(&failure),
            TestEvent::CandidatesLoaded { candidates, error } => {
                self.on_candidates_loaded(candidates, error);
            }
            TestEvent::CandidateDeleted(id) => self.on_candidate_deleted(id),
            TestEvent::CandidateDeleteFailed(message) => {
                self.candidate_activity = CandidateActivity::Idle;
                self.candidate_message = Some(message);
            }
            TestEvent::Failed(msg) => self.on_terminal_failure(msg.as_str()),
        }
    }

    fn push_log(&mut self, line: String) {
        if self.logs.len() >= 200 {
            let _ = self.logs.pop_front();
        }
        self.logs.push_back(line);
    }

    fn on_generated(
        &mut self,
        prep: TestCardPrep,
        title: String,
        body: String,
        initial: [String; 6],
    ) {
        for (i, val) in initial.into_iter().enumerate() {
            if let Some(slot) = self.fields.get_mut(i) {
                *slot = LineEditor::new(val);
            }
        }
        self.title = title;
        self.body = body;
        self.content_edit = None;
        self.frozen_create_content = None;
        self.prep = Some(prep);
        self.phase = TestPhase::Revisao;
        "revisão".clone_into(&mut self.phase_label);
        self.error = None;
        self.error_field = None;
        self.create_recovery = CreateRecoveryState::Unavailable;
        self.progress = 1.0;
        "pronto p/ revisão".clone_into(&mut self.progress_label);
        self.scroll = 0;
        self.panel = Panel::Preview;
        self.push_log("card pronto — revise, ajuste settings (tab) e crie (enter)".to_owned());
    }

    /// Abre o editor somente no preview e antes da primeira tentativa remota.
    fn open_content_edit(&mut self) -> bool {
        if self.phase != TestPhase::Revisao
            || self.panel != Panel::Preview
            || self.dialog.is_some()
            || self.create_recovery == CreateRecoveryState::Available
            || self.frozen_create_content.is_some()
        {
            return false;
        }
        self.content_edit = Some(ContentEditState::for_test(
            self.title.as_str(),
            self.body.as_str(),
        ));
        true
    }

    fn on_created_item(&mut self, item: &WorkItem) {
        let id = item.id;
        let url = match self.prep.as_ref() {
            Some(p) => test_case_url(p, id),
            None => format!("workitem:{id}"),
        };
        self.created = Some((id, url));
        self.phase = TestPhase::Pronto;
        self.error = None;
        self.error_field = None;
        self.create_recovery = CreateRecoveryState::Unavailable;
        self.candidate_activity = CandidateActivity::Idle;
        self.candidates.clear();
        self.candidate_message = None;
        "test case criado".clone_into(&mut self.phase_label);
        self.progress = 1.0;
        "criado".clone_into(&mut self.progress_label);
        self.push_log(format!("criado #{id}"));
        // A confirmação do pai aparece imediatamente para não encerrar o
        // fluxo antes que o usuário decida sobre a atualização.
        self.dialog = Some(TestDialog::ConfirmTestQa(false));
    }

    fn on_create_failed(&mut self, failure: &CreateFailure) {
        self.phase = TestPhase::Revisao;
        "revisão — falha no envio".clone_into(&mut self.phase_label);
        "envio falhou — ajuste e tente novamente".clone_into(&mut self.progress_label);
        self.error = Some(failure.message.clone());
        self.error_field = failure.field;
        self.create_recovery = if matches!(failure.kind, CreateFailureKind::OutcomeUnknown) {
            CreateRecoveryState::Available
        } else {
            CreateRecoveryState::Unavailable
        };
        self.panel = Panel::Settings;
        if let Some(field) = failure.field {
            self.field_focus = field.index();
        }
        self.candidates.clear();
        self.candidate_activity = CandidateActivity::Idle;
        self.candidate_message = None;
        self.dialog = if matches!(failure.kind, CreateFailureKind::OutcomeUnknown) {
            Some(TestDialog::CreateRecovery(0))
        } else {
            None
        };
        self.push_log(format!("falha ao criar: {}", failure.message));
    }

    fn on_candidates_loaded(&mut self, candidates: Vec<TestCaseCandidate>, error: Option<String>) {
        self.candidate_activity = CandidateActivity::Idle;
        self.candidate_message = error;
        if self.candidate_message.is_none() {
            self.candidates = candidates;
        }
        if let Some(TestDialog::CandidateList { selected }) = self.dialog {
            self.dialog = Some(TestDialog::CandidateList {
                selected: selected.min(self.candidates.len().saturating_sub(1)),
            });
        }
    }

    fn on_candidate_deleted(&mut self, id: i64) {
        self.candidate_activity = CandidateActivity::Idle;
        self.candidates.retain(|candidate| candidate.id != id);
        self.candidate_message = Some(format!(
            "Test Case #{id} movido para a lixeira do Azure DevOps"
        ));
        if let Some(TestDialog::CandidateList { selected }) = self.dialog {
            self.dialog = Some(TestDialog::CandidateList {
                selected: selected.min(self.candidates.len().saturating_sub(1)),
            });
        }
    }

    fn adopt_candidate(&mut self, selected: usize) -> bool {
        let Some(candidate) = self.candidates.get(selected).cloned() else {
            return false;
        };
        self.created = Some((candidate.id, candidate.url));
        self.phase = TestPhase::Pronto;
        "test case encontrado".clone_into(&mut self.phase_label);
        self.progress = 1.0;
        "candidato selecionado".clone_into(&mut self.progress_label);
        self.error = None;
        self.error_field = None;
        self.create_recovery = CreateRecoveryState::Unavailable;
        self.candidate_activity = CandidateActivity::Idle;
        self.candidates.clear();
        self.candidate_message = None;
        self.push_log(format!(
            "candidato #{} selecionado como criado",
            candidate.id
        ));
        self.dialog = Some(TestDialog::ConfirmTestQa(false));
        true
    }

    fn on_terminal_failure(&mut self, msg: &str) {
        self.phase = TestPhase::Erro;
        "erro".clone_into(&mut self.phase_label);
        self.error = Some(msg.to_owned());
        self.error_field = None;
        self.create_recovery = CreateRecoveryState::Unavailable;
        self.push_log(format!("erro: {msg}"));
    }

    /// Valida os 6 campos e monta [`TestSettings`] (erros em PT-BR).
    fn build_settings(&self) -> Result<TestSettings, String> {
        let get = |i: usize| -> String {
            match self.fields.get(i) {
                Some(f) => f.trimmed(),
                None => String::new(),
            }
        };
        let team = get(4);
        if team.trim().is_empty() {
            return Err("team é obrigatório (Custom.Team).".to_owned());
        }
        let program = get(5);
        if program.trim().is_empty() {
            return Err("programa é obrigatório (Custom.ProgramasAgrotrace).".to_owned());
        }
        let assigned = get(1);
        if !optional_email_ok(assigned.as_str()) {
            return Err("responsável: informe um email válido ou deixe vazio.".to_owned());
        }
        let priority = parse_priority_text(get(3).as_str())?;
        Ok(TestSettings {
            area_path: get(0),
            assigned_to: assigned,
            iteration_path: get(2),
            priority,
            team,
            program,
        })
    }
}

impl TestApp {
    /// Abre os esforços do Test QA pré-preenchidos (declarado do pai ou "1").
    fn open_qa_efforts(&mut self) {
        // Reabrir após uma falha deve preservar o que o usuário digitou, em
        // vez de restaurar silenciosamente os defaults do Work Item pai.
        if self.qa_effort.value.is_empty() && self.qa_real.value.is_empty() {
            let (effort, real) = match self.prep.as_ref() {
                Some(p) => parent_effort_defaults(&p.parent),
                None => ("1".to_owned(), "1".to_owned()),
            };
            self.qa_effort = LineEditor::new(effort);
            self.qa_real = LineEditor::new(real);
        }
        self.qa_focus = 0;
        self.dialog = Some(TestDialog::QaEfforts);
        self.error = None;
        self.error_field = None;
    }

    /// Valida os esforços (não-vazios, decimais ≥ 0).
    fn validate_qa_efforts(&self) -> Result<(String, String), String> {
        let check = |label: &str, raw: &str| {
            let t = raw.trim();
            if t.is_empty() {
                return Err(format!("{label} deve ser um número ≥ 0."));
            }
            match t.replace(',', ".").parse::<f64>() {
                Ok(n) if n.is_finite() && n >= 0.0 => Ok(t.to_owned()),
                _ => Err(format!("{label} deve ser um número ≥ 0.")),
            }
        };
        Ok((
            check("effort", &self.qa_effort.trimmed())?,
            check("real effort", &self.qa_real.trimmed())?,
        ))
    }
}

/// Email opcional válido (vazio ok; senão `user@domínio.tld` sem espaços).
fn optional_email_ok(value: &str) -> bool {
    let t = value.trim();
    if t.is_empty() {
        return true;
    }
    if t.contains(' ') || t.contains('\t') {
        return false;
    }
    let mut parts = t.split('@');
    let user = parts.next();
    let domain = parts.next();
    let extra = parts.next();
    match (user, domain, extra) {
        (Some(u), Some(d), None) => !u.is_empty() && d.contains('.') && !d.is_empty(),
        _ => false,
    }
}

/// Interpreta o campo prioridade (vazio = 2.0; vírgula vira ponto).
fn parse_priority_text(raw: &str) -> Result<f64, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(2.0);
    }
    let normalized = t.replace(',', ".");
    let number: f64 = match normalized.parse() {
        Ok(n) => n,
        Err(_) => return Err("prioridade deve ser um número positivo.".to_owned()),
    };
    if number.is_finite() && number > 0.0 {
        Ok(number)
    } else {
        Err("prioridade deve ser um número positivo.".to_owned())
    }
}

/// URL amigável do Test Case criado.
fn test_case_url(prep: &TestCardPrep, id: i64) -> String {
    match prep.context.remote.as_ref() {
        Some(r) => format!(
            "https://dev.azure.com/{}/{}/_workitems/edit/{id}",
            r.organization, r.project
        ),
        None => format!("workitem:{id}"),
    }
}

/// Esforços padrão p/ `update_parent` (esforço do pai ou "1"; real igual).
fn parent_effort_defaults(parent: &WorkItem) -> (String, String) {
    let raw = match parent.fields.get("Microsoft.VSTS.Scheduling.Effort") {
        Some(v) => {
            if let Some(s) = v.as_str() {
                s.trim().to_owned()
            } else if let Some(n) = v.as_i64() {
                n.to_string()
            } else if let Some(f) = v.as_f64() {
                if f.fract() == 0.0 {
                    format!("{f:.0}")
                } else {
                    format!("{f}")
                }
            } else {
                String::new()
            }
        }
        None => String::new(),
    };
    let effort = if raw.trim().is_empty() {
        "1".to_owned()
    } else {
        raw.trim().to_owned()
    };
    (effort.clone(), effort)
}

/// Tarefa de prepare + generate (roda em `tokio::spawn`).
async fn backend_prepare_generate(request: TestCardRequest, tx: mpsc::UnboundedSender<TestEvent>) {
    backend_prepare_generate_with(
        request,
        tx,
        |request| async move { test_card::prepare_request(request).await },
        |prep| async move { test_card::generate(&prep).await },
    )
    .await;
}

/// Executa o backend de prepare + generate com as duas dependências isoladas.
///
/// O fluxo de produção injeta os adaptadores reais acima; os testes de
/// handoff usam o mesmo encadeamento de eventos com Azure/IA controlados, sem
/// fabricar um `Generated` fora do backend.
async fn backend_prepare_generate_with<P, PFut, G, GFut>(
    request: TestCardRequest,
    tx: mpsc::UnboundedSender<TestEvent>,
    prepare: P,
    generate: G,
) where
    P: FnOnce(TestCardRequest) -> PFut,
    PFut: Future<Output = crate::error::Result<TestCardPrep>>,
    G: FnOnce(TestCardPrep) -> GFut,
    GFut: Future<Output = crate::error::Result<PrDescription>>,
{
    let _ = tx.send(TestEvent::PhaseLabel("preparando contexto…".to_owned()));
    let _ = tx.send(TestEvent::Progress(0.05, "coletando git/pr/pai".to_owned()));
    let _ = tx.send(TestEvent::Log(
        "lendo config, git e work item pai…".to_owned(),
    ));
    let options = match &request {
        TestCardRequest::Cli(options) => Some(options.clone()),
        TestCardRequest::PublishedPr(_) => None,
    };
    let prep = match prepare(request).await {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(TestEvent::Failed(e.to_string()));
            return;
        }
    };
    let _ = tx.send(TestEvent::Log(format!("pai #{} resolvido", prep.parent.id)));
    let _ = tx.send(TestEvent::PhaseLabel("gerando card via IA…".to_owned()));
    let _ = tx.send(TestEvent::Progress(0.3, "chamando provider".to_owned()));
    let desc = match generate(prep.clone()).await {
        Ok(d) => d,
        Err(e) => {
            let _ = tx.send(TestEvent::Failed(e.to_string()));
            return;
        }
    };
    let _ = tx.send(TestEvent::PhaseLabel("renderizando preview…".to_owned()));
    let mut buf = String::with_capacity(24);
    for ch in desc.body.chars() {
        buf.push(ch);
        if buf.len() >= 24 || ch == '\n' {
            let chunk = std::mem::take(&mut buf);
            let _ = tx.send(TestEvent::Token(chunk));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    if !buf.is_empty() {
        let _ = tx.send(TestEvent::Token(buf));
    }
    let initial = initial_field_values(options.as_ref(), &prep);
    let _ = tx.send(TestEvent::Progress(1.0, "pronto p/ revisão".to_owned()));
    let _ = tx.send(TestEvent::Generated {
        prep: Box::new(prep),
        title: desc.title,
        body: desc.body,
        initial,
    });
}

/// Valores iniciais dos 6 campos (CLI > config; iteração herdada do pai).
fn initial_field_values(options: Option<&CliOptions>, prep: &TestCardPrep) -> [String; 6] {
    if let Some(settings) = &prep.settings {
        let priority = if settings.priority.fract() == 0.0 {
            format!("{:.0}", settings.priority)
        } else {
            format!("{}", settings.priority)
        };
        return [
            settings.area_path.clone(),
            settings.assigned_to.clone(),
            settings.iteration_path.clone(),
            priority,
            settings.team.clone(),
            settings.program.clone(),
        ];
    }
    let Some(options) = options else {
        return std::array::from_fn(|_| String::new());
    };
    if let Ok(s) = TestSettings::from_cli_or_config(options, &prep.config, &prep.parent) {
        let priority = if s.priority.fract() == 0.0 {
            format!("{:.0}", s.priority)
        } else {
            format!("{}", s.priority)
        };
        [
            s.area_path,
            s.assigned_to,
            s.iteration_path,
            priority,
            s.team,
            s.program,
        ]
    } else {
        let area = match options.area_path.clone() {
            Some(v) => v,
            None => prep.config.test_area_path.clone(),
        };
        let assigned = match options.assigned_to.clone() {
            Some(v) => v,
            None => prep.config.test_assigned_to.clone(),
        };
        let iteration = match options.iteration_path.clone() {
            Some(v) => v,
            None => test_card::work_item_field(&prep.parent, "System.IterationPath").to_owned(),
        };
        let priority = match options.priority.clone() {
            Some(v) => v,
            None => "2".to_owned(),
        };
        let team = match options.team.clone() {
            Some(v) => v,
            None => prep.config.test_team.clone(),
        };
        let program = match options.program.clone() {
            Some(v) => v,
            None => prep.config.test_program.clone(),
        };
        [area, assigned, iteration, priority, team, program]
    }
}

/// Tarefa de criação (disparada pelo enter na revisão).
async fn backend_create(
    prep: TestCardPrep,
    settings: TestSettings,
    title: String,
    body: String,
    tx: mpsc::UnboundedSender<TestEvent>,
) {
    backend_create_with(
        prep,
        settings,
        title,
        body,
        tx,
        |prep, settings, title, body| async move {
            test_card::create_with_current_config(&prep, &settings, title.as_str(), body.as_str())
                .await
        },
    )
    .await;
}

/// Executa a escrita do Test Case com o adaptador de criação isolado.
async fn backend_create_with<C, CFut>(
    prep: TestCardPrep,
    settings: TestSettings,
    title: String,
    body: String,
    tx: mpsc::UnboundedSender<TestEvent>,
    create: C,
) where
    C: FnOnce(TestCardPrep, TestSettings, String, String) -> CFut,
    CFut: Future<Output = crate::error::Result<WorkItem>>,
{
    let _ = tx.send(TestEvent::PhaseLabel("criando test case…".to_owned()));
    let _ = tx.send(TestEvent::Progress(0.5, "enviando ao azure".to_owned()));
    match create(prep, settings, title, body).await {
        Ok(item) => {
            let _ = tx.send(TestEvent::Progress(1.0, "criado".to_owned()));
            let _ = tx.send(TestEvent::CreatedItem(item));
        }
        Err(e) => {
            let _ = tx.send(TestEvent::CreateFailed(test_card::classify_create_error(
                &e,
            )));
        }
    }
}

/// Executa a consulta de duplicidade fora do loop de renderização.
async fn backend_find_candidates(
    prep: TestCardPrep,
    title: String,
    settings: TestSettings,
    tx: mpsc::UnboundedSender<TestEvent>,
) {
    match test_card::find_create_candidates(&prep, title.as_str(), &settings).await {
        Ok(candidates) => {
            let _ = tx.send(TestEvent::CandidatesLoaded {
                candidates,
                error: None,
            });
        }
        Err(error) => {
            let _ = tx.send(TestEvent::CandidatesLoaded {
                candidates: Vec::new(),
                error: Some(format!("não foi possível consultar candidatos: {error}")),
            });
        }
    }
}

/// Executa a exclusão reversível fora do loop de renderização.
async fn backend_delete_candidate(
    prep: TestCardPrep,
    id: i64,
    tx: mpsc::UnboundedSender<TestEvent>,
) {
    match test_card::delete_candidate(&prep, id).await {
        Ok(()) => {
            let _ = tx.send(TestEvent::CandidateDeleted(id));
        }
        Err(error) => {
            let _ = tx.send(TestEvent::CandidateDeleteFailed(format!(
                "não foi possível excluir o Test Case #{id}: {error}"
            )));
        }
    }
}

/// Desenha um frame completo a partir do estado.
impl Widget for &TestApp {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 60 || area.height < 20 {
            render_too_small(area, buf);
            return;
        }
        let [head, body, foot] = status_layout(area);
        Block::new().style(theme().root).render(area, buf);
        render_header(self, head, buf);
        render_body(self, body, buf);
        render_footer(self, foot, buf);
        if let Some(dialog) = &self.dialog {
            render_dialog(self, *dialog, area, buf);
        }
        if let Some(editor) = &self.content_edit {
            render_content_editor(editor, area, buf);
        }
    }
}

/// Piso honesto p/ terminal < 60×20.
fn render_too_small(area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    Block::new().style(theme().root).render(area, buf);
    let msg = format!(
        "terminal muito pequeno — mínimo 60×20 (atual {}×{})",
        area.width, area.height
    );
    let pop_w = area.width.saturating_sub(2).clamp(1, 56).min(area.width);
    let pop_h = 5.min(area.height).max(1).min(area.height);
    let x = area.x.saturating_add(area.width.saturating_sub(pop_w) / 2);
    let y = area.y.saturating_add(area.height.saturating_sub(pop_h) / 2);
    let popup = Rect {
        x,
        y,
        width: pop_w,
        height: pop_h,
    };
    let block = Block::default()
        .title(Span::styled(" ◆ prt ", theme().warning))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(theme().warning);
    let inner = block.inner(popup);
    block.render(popup, buf);
    if inner.width > 0 && inner.height > 0 {
        Paragraph::new(msg)
            .style(theme().muted)
            .alignment(ratatui::layout::Alignment::Center)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}

/// Header com fase, mensagem e progresso global.
fn render_header(app: &TestApp, area: Rect, buf: &mut Buffer) {
    let message = if app.candidate_activity == CandidateActivity::Loading {
        "consultando Test Cases recentes…"
    } else if app.candidate_activity == CandidateActivity::Deleting {
        "excluindo candidato…"
    } else if app.phase == TestPhase::Erro {
        "consulte os detalhes abaixo"
    } else if app.phase == TestPhase::Revisao {
        "card pronto"
    } else if app.phase == TestPhase::Pronto {
        "id e URL disponíveis"
    } else {
        app.phase_label.as_str()
    };
    let style = match app.phase {
        TestPhase::Erro => theme().error,
        TestPhase::Pronto | TestPhase::Revisao => theme().success,
        TestPhase::Preparando | TestPhase::Gerando | TestPhase::Criando => theme().accent,
    };
    status_header(
        area,
        buf,
        StatusHeader {
            command: "test",
            phase: app.phase.short(),
            message,
            progress: if app.candidate_activity == CandidateActivity::Idle {
                Some(app.progress)
            } else {
                None
            },
            tick: app.tick,
            active: app.phase.busy() || app.candidate_activity != CandidateActivity::Idle,
            style,
        },
    );
}

/// Corpo: preview e settings, sem repetir o status global.
fn render_body(app: &TestApp, area: Rect, buf: &mut Buffer) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    if area.width < 100 {
        let half = area.height / 2;
        let top = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: half,
        };
        let bottom = Rect {
            x: area.x,
            y: area.y.saturating_add(half),
            width: area.width,
            height: area.height.saturating_sub(half),
        };
        if top.height > 0 {
            render_preview(app, top, buf);
        }
        if bottom.height > 0 {
            render_settings(app, bottom, buf);
        }
        return;
    }
    let [left, _gap, right] = Layout::horizontal([
        Constraint::Percentage(58),
        Constraint::Length(1),
        Constraint::Percentage(42),
    ])
    .areas(area);
    render_preview(app, left, buf);
    render_settings(app, right, buf);
}

/// Preview do card (Markdown final ou stream parcial).
fn render_preview(app: &TestApp, area: Rect, buf: &mut Buffer) {
    let ready = !app.title.is_empty() || !app.body.is_empty();
    let title = " Card de teste ";
    let block = Block::default()
        .title(Span::styled(
            title,
            if ready {
                theme().success
            } else {
                theme().border
            },
        ))
        .borders(Borders::ALL)
        .border_style(if ready {
            theme().success
        } else {
            theme().border
        })
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if ready {
        let mut lines: Vec<Line> = vec![title_line(app.title.as_str()), Line::from("")];
        lines.extend(markdown_text(app.body.as_str()).lines);
        let total = lines.len();
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0))
            .render(inner, buf);
        let mut state = ScrollbarState::new(total.max(1)).position(app.scroll as usize);
        <Scrollbar as ratatui::widgets::StatefulWidget>::render(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            inner,
            buf,
            &mut state,
        );
    } else {
        let text = if app.streamed_raw.is_empty() {
            String::new()
        } else {
            format!("{}▊", app.streamed_raw)
        };
        let total = text.lines().count();
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0))
            .render(inner, buf);
        let mut state = ScrollbarState::new(total.max(1)).position(app.scroll as usize);
        <Scrollbar as ratatui::widgets::StatefulWidget>::render(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            inner,
            buf,
            &mut state,
        );
    }
}

/// Painel de settings / status final / erro.
fn render_settings(app: &TestApp, area: Rect, buf: &mut Buffer) {
    if app.phase == TestPhase::Pronto {
        render_done(app, area, buf);
        return;
    }
    if app.phase == TestPhase::Erro {
        render_error(app, area, buf);
        return;
    }
    let focused_panel = app.panel == Panel::Settings;
    let block = Block::default()
        .title(Span::styled(
            " ⚙ Settings ",
            if focused_panel {
                theme().accent.add_modifier(Modifier::BOLD)
            } else {
                theme().muted
            },
        ))
        .borders(Borders::ALL)
        .border_style(if focused_panel {
            theme().accent
        } else {
            theme().muted
        })
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let lines = settings_lines(app, inner, focused_panel);
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((0, 0))
        .render(inner, buf);
}

/// Monta apenas os campos visíveis do formulário, mantendo o foco na janela.
fn settings_lines(app: &TestApp, inner: Rect, focused_panel: bool) -> Vec<Line<'static>> {
    let visible_count = (usize::from(inner.height).saturating_add(2) / 3).max(1);
    let max_start = FIELD_COUNT.saturating_sub(visible_count);
    let start = app
        .field_focus
        .saturating_sub(visible_count.saturating_sub(1))
        .min(max_start);
    let end = (start + visible_count).min(FIELD_COUNT);
    let mut lines: Vec<Line> = Vec::new();
    if start > 0 {
        lines.push(Line::from(Span::styled(
            "↑ mais campos acima",
            theme().muted,
        )));
    }
    for (i, slot) in app.fields.iter().enumerate().skip(start).take(end - start) {
        let focused = focused_panel && app.field_focus % FIELD_COUNT == i;
        let label = field_label(i);
        let marker = if focused { "▸ " } else { "  " };
        let field_error = app.error_field.is_some_and(|field| field.index() == i);
        let label_style = if focused {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            theme().muted
        };
        lines.push(Line::from(vec![
            Span::styled(
                if field_error && focused {
                    "✘▸ "
                } else if field_error {
                    "✘ "
                } else {
                    marker
                },
                if field_error {
                    theme().error
                } else if focused {
                    theme().accent
                } else {
                    theme().muted
                },
            ),
            Span::styled(label.to_owned(), label_style),
            Span::styled(format!("  ·  {}", field_hint(i)), theme().muted),
        ]));
        let value = slot.value.clone();
        let cursor = slot.cursor;
        if focused {
            lines.push(editor_line(value.as_str(), cursor, inner.width as usize));
        } else if value.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                "—".to_owned(),
                theme().muted.add_modifier(Modifier::ITALIC),
            )));
        } else {
            lines.push(Line::from(Span::raw(value)));
        }
        lines.push(Line::from(""));
    }
    if end < FIELD_COUNT {
        lines.push(Line::from(Span::styled(
            "↓ mais campos abaixo",
            theme().muted,
        )));
    }
    if let Some(m) = app.parent_msg.as_ref() {
        lines.push(Line::from(Span::styled(m.clone(), theme().muted)));
    }
    if app.is_copied_flash() {
        lines.push(Line::from(Span::styled("✓ copiado!", theme().success)));
    }
    lines
}

/// Diálogos modais do fluxo (despacha p/ um modal por vez).
fn render_dialog(app: &TestApp, dialog: TestDialog, area: Rect, buf: &mut Buffer) {
    match dialog {
        TestDialog::ConfirmCreate(yes) => {
            render_yes_no_dialog(area, buf, " Criar ", "Criar este Test Case?", yes);
        }
        TestDialog::ConfirmTestQa(yes) => {
            render_yes_no_dialog(
                area,
                buf,
                " Test QA ",
                "Atualizar Work Item pai para Test QA?",
                yes,
            );
        }
        TestDialog::QaEfforts => render_qa_efforts_dialog(app, area, buf),
        TestDialog::CreateRecovery(choice) => render_create_recovery_dialog(choice, area, buf),
        TestDialog::CandidateList { selected } => {
            render_candidate_list_dialog(app, selected, area, buf);
        }
        TestDialog::DeleteCandidate { selected, yes } => {
            render_delete_candidate_dialog(app, selected, yes, area, buf);
        }
        TestDialog::PublishedContextDivergence(yes) => {
            render_published_context_divergence(area, buf, yes);
        }
    }
}

/// Aviso explícito antes de gerar quando o checkout deixou de ser o snapshot
/// capturado na publicação.
fn render_published_context_divergence(area: Rect, buf: &mut Buffer, yes: bool) {
    let inner = modal_frame(
        area,
        buf,
        " Contexto local alterado ",
        theme().warning,
        82,
        9,
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let selected = if yes {
        "continuar com snapshot remoto"
    } else {
        "voltar"
    };
    let first = if yes { "▸" } else { " " };
    let second = if yes { " " } else { "▸" };
    Paragraph::new(vec![
        Line::from(Span::styled(
            "O checkout mudou desde a publicação do PR selecionado.",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from("A geração só pode usar o snapshot remoto validado ou ser cancelada."),
        Line::from(""),
        Line::from(Span::styled(
            format!("{first} continuar com snapshot remoto"),
            if yes { theme().accent } else { theme().muted },
        )),
        Line::from(Span::styled(
            format!("{second} voltar"),
            if yes { theme().muted } else { theme().accent },
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("selecionado: {selected}"),
            theme().muted,
        )),
        Line::from(Span::styled(
            "←/→ alternar · y/n escolher · enter confirmar · esc/q voltar",
            theme().muted,
        )),
    ])
    .wrap(Wrap { trim: false })
    .render(inner, buf);
}

/// Modal genérico Sim/Não (base dos confirms de criar e de Test QA).
fn render_yes_no_dialog(area: Rect, buf: &mut Buffer, title: &str, question: &str, yes: bool) {
    // 5 linhas de conteúdo: pergunta, respiro, botões, respiro, dicas.
    let inner = modal_frame(area, buf, title, theme().accent, 60, 5);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    Paragraph::new(vec![
        Line::from(Span::styled(
            question,
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        centered_buttons(yes, inner.width),
        Line::from(""),
        Line::from(Span::styled(
            "←/→ alternar · y sim · n não · enter confirmar · esc voltar",
            theme().muted,
        )),
    ])
    .wrap(Wrap { trim: false })
    .render(inner, buf);
}

/// Ações do popup exibido quando o resultado da criação é incerto.
const RECOVERY_ACTIONS: [&str; 4] = [
    "editar settings",
    "verificar possíveis cards",
    "reenviar mesmo assim",
    "sair",
];

/// Renderiza as ações de recuperação sem fechar a revisão subjacente.
fn render_create_recovery_dialog(choice: usize, area: Rect, buf: &mut Buffer) {
    let inner = modal_frame(
        area,
        buf,
        " Recuperar envio ",
        theme().warning,
        72,
        RECOVERY_ACTIONS.len() + 5,
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = vec![
        Line::from(Span::styled(
            "A resposta do Azure não confirmou se o card foi criado.",
            theme().warning,
        )),
        Line::from(Span::styled(
            "Verifique antes de reenviar para evitar duplicidade.",
            theme().muted,
        )),
        Line::from(""),
    ];
    for (index, action) in RECOVERY_ACTIONS.iter().enumerate() {
        let marker = if index == choice { "▸" } else { " " };
        let style = if index == choice {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {action}"),
            style,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑/↓ escolher · enter confirmar · esc editar",
        theme().muted,
    )));
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// Renderiza candidatos com título exato e indicação de correspondência.
fn render_candidate_list_dialog(app: &TestApp, selected: usize, area: Rect, buf: &mut Buffer) {
    let visible = app.candidates.len().min(6);
    let empty_hint =
        usize::from(app.candidates.is_empty() && app.candidate_activity == CandidateActivity::Idle);
    let content_height =
        3 + visible * 2 + empty_hint + usize::from(app.candidate_message.is_some());
    let inner = modal_frame(
        area,
        buf,
        " Verificar possíveis cards ",
        theme().accent,
        86,
        content_height,
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = Vec::new();
    if app.candidate_activity == CandidateActivity::Idle {
        lines.push(Line::from(Span::styled(
            "a busca usa título exato e relação com o Work Item pai",
            theme().muted,
        )));
    }
    if app.candidates.is_empty() && app.candidate_activity == CandidateActivity::Idle {
        lines.push(Line::from(Span::styled(
            "nenhum candidato encontrado; o retry ainda pode duplicar um card",
            theme().warning,
        )));
    }
    let start = selected
        .saturating_sub(visible.saturating_sub(1))
        .min(app.candidates.len().saturating_sub(visible));
    for (index, candidate) in app.candidates.iter().enumerate().skip(start).take(visible) {
        let is_selected = index == selected;
        let marker = if is_selected { "▸" } else { " " };
        let parent = if candidate.parent_matches {
            "pai ✓"
        } else {
            "pai ?"
        };
        let fields = if candidate.comparable_fields == 0 {
            "campos —".to_owned()
        } else {
            format!(
                "campos {}/{}",
                candidate.matching_fields, candidate.comparable_fields
            )
        };
        let style = if is_selected {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
        };
        let created = if candidate.created_at.is_empty() {
            String::new()
        } else {
            format!(" · {}", candidate.created_at)
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{marker} #{}  {}  · {parent} · {fields}{created}",
                candidate.id, candidate.title,
            ),
            style,
        )));
        lines.push(Line::from(Span::styled(
            format!("    {}", candidate.url),
            theme().muted,
        )));
    }
    if let Some(message) = app.candidate_message.as_ref() {
        lines.push(Line::from(Span::styled(message.clone(), theme().warning)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "enter usar selecionado · d excluir · r atualizar · esc voltar",
        theme().muted,
    )));
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// Confirma a exclusão reversível de um candidato específico.
fn render_delete_candidate_dialog(
    app: &TestApp,
    selected: usize,
    yes: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    let Some(candidate) = app.candidates.get(selected) else {
        return;
    };
    let question = format!(
        "Mover o Test Case #{} ({}) para a lixeira?",
        candidate.id, candidate.title
    );
    render_yes_no_dialog(area, buf, " Excluir candidato ", question.as_str(), yes);
}

/// Modal de esforços do Test QA (Effort + Real Effort).
fn render_qa_efforts_dialog(app: &TestApp, area: Rect, buf: &mut Buffer) {
    // 1 dica + 2×(rótulo + valor) + respiro + [erro] + dicas.
    let height = if app.error.is_some() { 8 } else { 7 };
    let inner = modal_frame(area, buf, " Esforço Test QA ", theme().accent, 62, height);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = vec![Line::from(Span::styled(
        "horas decimais ≥ 0 · tab troca campo",
        theme().muted,
    ))];
    let labels = ["Effort", "Real Effort"];
    let editors = [&app.qa_effort, &app.qa_real];
    for (i, label) in labels.iter().enumerate() {
        let focused = app.qa_focus == i;
        let editor = editors[i];
        lines.push(Line::from(vec![
            Span::styled(
                if focused { "▸ " } else { "  " },
                if focused {
                    theme().accent
                } else {
                    theme().muted
                },
            ),
            Span::styled(
                (*label).to_owned(),
                if focused {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    theme().muted
                },
            ),
        ]));
        lines.push(editor_line(
            editor.value.as_str(),
            editor.cursor,
            usize::from(inner.width),
        ));
    }
    lines.push(Line::from(""));
    if let Some(err) = app.error.as_ref() {
        lines.push(Line::from(vec![
            Span::styled("✘ ", theme().error),
            Span::styled(err.clone(), theme().error),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "tab troca campo · enter confirma · esc volta",
        theme().muted,
    )));
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// Tela Pronto com id/URL + hint do Test QA.
fn render_done(app: &TestApp, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .title(Span::styled(
            " ✔ Pronto ",
            theme().success.add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(theme().success)
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let (id_txt, url_txt) = match app.created.as_ref() {
        Some((id, url)) => (format!("#{id}"), url.clone()),
        None => ("—".to_owned(), "—".to_owned()),
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "✔ Test Case criado!",
            theme().success.add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("id   ", theme().muted),
            Span::styled(id_txt, theme().accent.add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("url  ", theme().muted),
            Span::styled(url_txt, theme().accent),
        ]),
        Line::from(""),
    ];
    if app.parent_updated {
        lines.push(Line::from(Span::styled(
            "pai atualizado p/ Test QA ✓",
            theme().success,
        )));
    } else {
        let action = if app.parent_msg.is_some() {
            "tentar atualizar pai novamente"
        } else {
            "atualizar pai p/ Test QA"
        };
        lines.push(Line::from(vec![
            Span::styled(format!("▸ [ enter ] {action}"), theme().accent),
            Span::styled("   ·   c copia body", theme().muted),
        ]));
    }
    if let Some(m) = app.parent_msg.as_ref() {
        if !app.parent_updated {
            lines.push(Line::from(Span::styled(m.clone(), theme().warning)));
        }
    }
    if app.is_copied_flash() {
        lines.push(Line::from(Span::styled("✓ copiado!", theme().success)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "outra tecla sai",
        theme().muted.add_modifier(Modifier::ITALIC),
    )));
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// Tela de erro honesta.
fn render_error(app: &TestApp, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .title(Span::styled(
            " ✘ Erro ",
            theme().error.add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(theme().error)
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let msg = match app.error.as_ref() {
        Some(e) => e.clone(),
        None => "falha desconhecida".to_owned(),
    };
    let lines = vec![
        Line::from(Span::styled("✘ Falha no fluxo de teste", theme().error)),
        Line::from(""),
        Line::from(Span::styled(msg, Style::new())),
        Line::from(""),
        Line::from(Span::styled(
            "qualquer tecla sai",
            theme().muted.add_modifier(Modifier::ITALIC),
        )),
    ];
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// Linha do editor com cursor reverso e scroll horizontal que segue o cursor.
fn editor_line(value: &str, cursor: usize, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }
    let chars: Vec<char> = value.chars().collect();
    let mut widths: Vec<usize> = Vec::with_capacity(chars.len());
    for ch in &chars {
        widths.push(UnicodeWidthChar::width(*ch).unwrap_or_default());
    }
    let cursor = cursor.min(chars.len());
    let cursor_cell: usize = if cursor < chars.len() {
        widths[cursor].max(1)
    } else {
        1
    };
    let mut start = cursor;
    let mut end = if cursor < chars.len() {
        cursor + 1
    } else {
        chars.len()
    };
    let mut used = cursor_cell;
    while start > 0 {
        let w = widths[start - 1];
        if used.saturating_add(w) > width {
            break;
        }
        start -= 1;
        used += widths[start];
    }
    while end < chars.len() {
        let w = widths[end];
        if used.saturating_add(w) > width {
            break;
        }
        used += w;
        end += 1;
    }
    let mut spans = Vec::new();
    for (i, ch) in chars[start..end].iter().enumerate() {
        let global = start + i;
        let style = if global == cursor {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        spans.push(Span::styled(ch.to_string(), style));
    }
    if cursor >= chars.len() {
        spans.push(Span::styled(
            " ",
            Style::new().add_modifier(Modifier::REVERSED),
        ));
    }
    if spans.is_empty() {
        spans.push(Span::styled(
            " ",
            Style::new().add_modifier(Modifier::REVERSED),
        ));
    }
    Line::from(spans)
}

/// Footer honesto por contexto.
fn render_footer(app: &TestApp, area: Rect, buf: &mut Buffer) {
    let hints = if app.content_edit.is_some() {
        "Tab alterna título/corpo · Ctrl+S salva · Esc cancela"
    } else {
        match &app.dialog {
            Some(TestDialog::ConfirmCreate(_) | TestDialog::ConfirmTestQa(_)) => {
                "←/→ alternar · y sim · n não · enter confirmar · esc voltar"
            }
            Some(TestDialog::QaEfforts) => {
                "digite o número · tab troca campo · enter confirma · esc volta"
            }
            Some(TestDialog::CreateRecovery(_)) => {
                "e editar settings · v verificar cards · r reenviar · enter escolher · esc editar"
            }
            Some(TestDialog::CandidateList { .. }) => {
                "↑/↓ selecionar · d excluir · r atualizar · esc voltar"
            }
            Some(TestDialog::DeleteCandidate { .. }) => {
                "←/→ alternar · y sim · n não · enter confirmar · esc cancelar"
            }
            Some(TestDialog::PublishedContextDivergence(_)) => {
                "←/→ alternar · y/n escolher · enter confirmar · esc/q voltar"
            }
            None => match app.phase {
                TestPhase::Preparando | TestPhase::Gerando => "j/k rolar preview · q/esc abortar",
                TestPhase::Criando => "q/esc abortar",
                TestPhase::Revisao => match app.panel {
                    Panel::Preview => {
                        if app.frozen_create_content.is_none() {
                            "e Editar conteúdo · tab settings · enter continuar · c copia · j/k rola · q sai"
                        } else {
                            "tab settings · enter continuar · c copia · j/k rola · q sai"
                        }
                    }
                    Panel::Settings => {
                        if app.create_recovery == CreateRecoveryState::Available {
                            "digite p/ editar · ↑/↓ campo · enter retry · esc opções · q sai"
                        } else if app.error.is_some() {
                            "digite p/ editar · ↑/↓ campo · enter reenviar · tab preview · q sai"
                        } else {
                            "digite p/ editar · ↑/↓ campo · tab preview · enter continuar · esc volta"
                        }
                    }
                },
                TestPhase::Pronto => "enter Test QA · c copia · outra tecla sai",
                TestPhase::Erro => "qualquer tecla sai",
            },
        }
    };
    let mut spans = vec![Span::styled(hints, theme().muted)];
    if app.is_copied_flash() {
        spans.push(Span::styled("   ✓ copiado!", theme().success));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

/// Roda o fluxo interativo de teste até sair.
///
/// # Errors
///
/// Retorna erro se o terminal não puder ser inicializado ou se o backend
/// falhar (prepare/generate/create); nesse caso a tela de erro é exibida
/// antes de retornar.
pub async fn run_test_flow(options: &CliOptions) -> anyhow::Result<TestFlowOutcome> {
    run_test_flow_request(TestCardRequest::Cli(options.clone())).await
}

/// Roda o fluxo de Test Case para uma entrada standalone ou um handoff
/// estruturado de PR publicado.
pub async fn run_test_flow_request(request: TestCardRequest) -> anyhow::Result<TestFlowOutcome> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("tui requer terminal interativo");
    }
    let mut terminal: DefaultTerminal = ratatui::init();
    let res = run_loop(&mut terminal, request).await;
    ratatui::restore();
    res
}

/// Loop principal: drena backend, ticka a ~30fps e trata teclas.
async fn run_loop(
    terminal: &mut DefaultTerminal,
    request: TestCardRequest,
) -> anyhow::Result<TestFlowOutcome> {
    run_loop_with(
        terminal,
        request,
        || {
            if event::poll(Duration::from_millis(10))? {
                Ok(Some(event::read()?))
            } else {
                Ok(None)
            }
        },
        |request, tx| {
            tokio::spawn(backend_prepare_generate(request, tx));
        },
    )
    .await
}

/// Loop principal com fontes de entrada e backend substituíveis nos testes.
async fn run_loop_with<I, B>(
    terminal: &mut DefaultTerminal,
    request: TestCardRequest,
    mut read_event: I,
    start_backend: B,
) -> anyhow::Result<TestFlowOutcome>
where
    I: FnMut() -> anyhow::Result<Option<Event>>,
    B: Fn(TestCardRequest, mpsc::UnboundedSender<TestEvent>),
{
    let (tx, mut rx) = mpsc::unbounded_channel::<TestEvent>();
    let mut app = TestApp::for_request(&request);
    let mut pending_request = Some(request);
    let published_diverged = pending_request
        .as_ref()
        .is_some_and(published_context_diverged);
    if published_diverged {
        app.dialog = Some(TestDialog::PublishedContextDivergence(false));
    } else if let Some(request) = pending_request.take() {
        let first_tx = tx.clone();
        start_backend(request, first_tx);
    }

    let tick_rate = Duration::from_millis(33);
    let mut last_tick = Instant::now();
    let mut needs_draw = true;

    loop {
        let mut backend_novo = false;
        while let Ok(ev) = rx.try_recv() {
            app.on_event(ev);
            backend_novo = true;
        }
        if backend_novo {
            needs_draw = true;
        }
        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
            needs_draw = true;
        }
        if needs_draw {
            terminal.draw(|f| f.render_widget(&app, f.area()))?;
            needs_draw = false;
        }
        let Some(ev) = read_event()? else {
            tokio::time::sleep(Duration::from_millis(1)).await;
            continue;
        };
        if let Event::Paste(text) = ev {
            if handle_content_paste(&mut app, text.as_str()) {
                needs_draw = true;
            }
            continue;
        }
        let Event::Key(key) = ev else {
            needs_draw = true;
            continue;
        };
        if !test_key_kind_allowed(&app, key) {
            continue;
        }
        if let Some(action) = handle_test_interrupt_key(&mut app, key) {
            if let Some(res) = apply_test_key_action(action, &mut needs_draw) {
                return res;
            }
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('z') {
            #[cfg(unix)]
            {
                super::suspend::suspend_to_shell(&mut *terminal)?;
                needs_draw = true;
            }
            continue;
        }
        // Diálogos modais têm prioridade sobre o resto da tela.
        if let Some(dialog) = app.dialog {
            let action = handle_test_dialog_key(&mut app, terminal, &tx, dialog, key).await?;
            if let Some(res) = apply_test_key_action(action, &mut needs_draw) {
                return res;
            }
            if app.start_after_divergence {
                app.start_after_divergence = false;
                if let Some(request) = pending_request.take() {
                    start_backend(request, tx.clone());
                }
            }
            continue;
        }
        let action = handle_test_phase_key(&mut app, key);
        if let Some(res) = apply_test_key_action(action, &mut needs_draw) {
            return res;
        }
    }
}

/// Exercita uma request publicada até a revisão usando o backend real do
/// fluxo e adaptadores determinísticos para Azure/IA.
#[cfg(test)]
pub(crate) async fn exercise_published_request_for_test(
    request: TestCardRequest,
) -> (usize, bool, bool, bool) {
    let TestCardRequest::PublishedPr(context) = &request else {
        panic!("harness publicado recebeu request CLI");
    };
    let parent = context
        .work_item
        .clone()
        .expect("snapshot de Work Item publicado");
    let prep = TestCardPrep {
        config: context.config.clone(),
        context: crate::git::ChangeContext {
            branch: context.source_ref_name.clone(),
            source_ref: context.source_ref_name.clone(),
            base_branch: context.target_ref_name.clone(),
            sprint_branch: String::new(),
            diff: "diff remoto".to_owned(),
            diff_original_lines: 1,
            log: "log remoto".to_owned(),
            work_item_id: parent.id.to_string(),
            remote: Some(context.remote.clone()),
        },
        parent,
        pr_id: Some(context.published_pr.id.to_string()),
        settings: Some(context.settings.clone()),
        pr_changes: "changes remotas".to_owned(),
        examples_text: String::new(),
        prompt: "prompt remoto".to_owned(),
    };
    let generated = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let generated_by_backend = generated.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    backend_prepare_generate_with(
        request.clone(),
        tx,
        move |_| async move { Ok(prep) },
        move |_| {
            generated_by_backend.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async {
                Ok(PrDescription {
                    title: "Card do PR".to_owned(),
                    body: "## Objetivo\nValidar o PR".to_owned(),
                })
            }
        },
    )
    .await;
    let mut app = TestApp::for_request(&request);
    while let Ok(event) = rx.try_recv() {
        app.on_event(event);
    }
    (
        generated.load(std::sync::atomic::Ordering::Relaxed),
        app.phase == TestPhase::Revisao,
        app.created.is_some(),
        app.parent_updated,
    )
}

fn published_context_diverged(request: &TestCardRequest) -> bool {
    matches!(
        request,
        TestCardRequest::PublishedPr(context) if !context.fingerprint.matches_current()
    )
}

/// Decisão de um handler de tecla do fluxo de teste.
///
/// Extraído de `run_loop` (que estourou `too_many_lines`); cada handler
/// devolve um valor em vez de `continue`/`return` direto no loop.
enum TestKeyAction {
    /// Segue no loop (`bool` = redesenhar a tela).
    Continue(bool),
    /// Encerra o loop com o resultado.
    Done(TestFlowOutcome),
    /// Encerra o loop com erro (fase `Erro`).
    Fail(String),
}

/// Aplica a decisão do handler: atualiza `needs_draw` ou encerra o loop.
fn apply_test_key_action(
    action: TestKeyAction,
    needs_draw: &mut bool,
) -> Option<anyhow::Result<TestFlowOutcome>> {
    match action {
        TestKeyAction::Continue(redraw) => {
            *needs_draw |= redraw;
            None
        }
        TestKeyAction::Done(outcome) => Some(Ok(outcome)),
        TestKeyAction::Fail(msg) => Some(Err(anyhow::Error::msg(msg))),
    }
}

/// Diz se a tecla passa pelo filtro de `Release`/`Repeat` do loop.
///
/// `Repeat` só passa p/ scroll ou p/ edição no painel de settings.
fn test_key_kind_allowed(app: &TestApp, key: event::KeyEvent) -> bool {
    if key.kind == KeyEventKind::Release {
        return false;
    }
    if key.kind != KeyEventKind::Repeat {
        return true;
    }
    let is_scroll = matches!(
        key.code,
        KeyCode::Char('j' | 'k')
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
    );
    let is_edit = matches!(
        key.code,
        KeyCode::Char(_)
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
    );
    is_scroll
        || (app.phase == TestPhase::Revisao
            && ((app.panel == Panel::Settings && is_edit) || app.content_edit.is_some()))
}

/// Trata Ctrl-C conforme a fase (`None` = a tecla não é Ctrl-C).
fn handle_test_interrupt_key(app: &mut TestApp, key: event::KeyEvent) -> Option<TestKeyAction> {
    if !(key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')) {
        return None;
    }
    if app.phase == TestPhase::Pronto {
        if let Some((id, url)) = app.created.take() {
            return Some(TestKeyAction::Done(TestFlowOutcome::Created { id, url }));
        }
        return Some(TestKeyAction::Done(TestFlowOutcome::Reviewed));
    }
    if app.phase == TestPhase::Erro {
        let msg = match app.error.take() {
            Some(m) => m,
            None => "falha desconhecida".to_owned(),
        };
        return Some(TestKeyAction::Fail(msg));
    }
    Some(TestKeyAction::Done(TestFlowOutcome::Aborted))
}

/// Despacha a tecla p/ o diálogo modal aberto.
async fn handle_test_dialog_key(
    app: &mut TestApp,
    terminal: &mut DefaultTerminal,
    tx: &mpsc::UnboundedSender<TestEvent>,
    dialog: TestDialog,
    key: event::KeyEvent,
) -> anyhow::Result<TestKeyAction> {
    match dialog {
        TestDialog::ConfirmCreate(yes) => Ok(handle_confirm_create_key(app, tx, yes, key)),
        TestDialog::ConfirmTestQa(yes) => Ok(handle_confirm_qa_key(app, yes, key)),
        TestDialog::QaEfforts => handle_qa_efforts_key(app, terminal, key).await,
        TestDialog::CreateRecovery(choice) => Ok(handle_create_recovery_key(app, tx, choice, key)),
        TestDialog::CandidateList { selected } => {
            Ok(handle_candidate_list_key(app, tx, selected, key))
        }
        TestDialog::DeleteCandidate { selected, yes } => {
            Ok(handle_delete_candidate_key(app, tx, selected, yes, key))
        }
        TestDialog::PublishedContextDivergence(yes) => {
            Ok(handle_published_context_divergence_key(app, yes, key))
        }
    }
}

/// Tecla do gate de divergência: continuar usa apenas o snapshot remoto;
/// voltar encerra a entrada publicada antes de qualquer geração.
fn handle_published_context_divergence_key(
    app: &mut TestApp,
    yes: bool,
    key: event::KeyEvent,
) -> TestKeyAction {
    if key.kind != KeyEventKind::Press {
        return TestKeyAction::Continue(false);
    }
    match key.code {
        KeyCode::Left | KeyCode::Right => {
            app.dialog = Some(TestDialog::PublishedContextDivergence(!yes));
            TestKeyAction::Continue(true)
        }
        KeyCode::Char('y') => {
            app.dialog = Some(TestDialog::PublishedContextDivergence(true));
            TestKeyAction::Continue(true)
        }
        KeyCode::Char('n') => {
            app.dialog = Some(TestDialog::PublishedContextDivergence(false));
            TestKeyAction::Continue(true)
        }
        KeyCode::Enter if yes => {
            app.dialog = None;
            app.start_after_divergence = true;
            TestKeyAction::Continue(true)
        }
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
            app.dialog = None;
            TestKeyAction::Done(TestFlowOutcome::Aborted)
        }
        _ => TestKeyAction::Continue(false),
    }
}

/// Tecla no popup de recuperação de um envio incerto.
fn handle_create_recovery_key(
    app: &mut TestApp,
    tx: &mpsc::UnboundedSender<TestEvent>,
    choice: usize,
    key: event::KeyEvent,
) -> TestKeyAction {
    if key.kind != KeyEventKind::Press {
        return TestKeyAction::Continue(false);
    }
    let selected = choice.min(RECOVERY_ACTIONS.len().saturating_sub(1));
    let move_choice = |down: bool| -> usize {
        let count = RECOVERY_ACTIONS.len();
        if down {
            (selected + 1) % count
        } else if selected == 0 {
            count - 1
        } else {
            selected - 1
        }
    };
    match key.code {
        KeyCode::Char('e') | KeyCode::Esc => {
            app.dialog = None;
            app.panel = Panel::Settings;
            TestKeyAction::Continue(true)
        }
        KeyCode::Char('v') => {
            start_candidate_search(app, tx);
            TestKeyAction::Continue(true)
        }
        KeyCode::Char('r') => {
            app.dialog = None;
            start_create(app, tx);
            TestKeyAction::Continue(true)
        }
        KeyCode::Char('q') => {
            app.dialog = None;
            TestKeyAction::Done(TestFlowOutcome::Reviewed)
        }
        KeyCode::Up | KeyCode::Left => {
            app.dialog = Some(TestDialog::CreateRecovery(move_choice(false)));
            TestKeyAction::Continue(true)
        }
        KeyCode::Down | KeyCode::Right => {
            app.dialog = Some(TestDialog::CreateRecovery(move_choice(true)));
            TestKeyAction::Continue(true)
        }
        KeyCode::Enter => match selected {
            0 => {
                app.dialog = None;
                app.panel = Panel::Settings;
                TestKeyAction::Continue(true)
            }
            1 => {
                start_candidate_search(app, tx);
                TestKeyAction::Continue(true)
            }
            2 => {
                app.dialog = None;
                start_create(app, tx);
                TestKeyAction::Continue(true)
            }
            _ => {
                app.dialog = None;
                TestKeyAction::Done(TestFlowOutcome::Reviewed)
            }
        },
        _ => TestKeyAction::Continue(false),
    }
}

/// Tecla na lista de candidatos encontrados no Azure.
fn handle_candidate_list_key(
    app: &mut TestApp,
    tx: &mpsc::UnboundedSender<TestEvent>,
    selected: usize,
    key: event::KeyEvent,
) -> TestKeyAction {
    if key.kind != KeyEventKind::Press || app.candidate_activity != CandidateActivity::Idle {
        return TestKeyAction::Continue(false);
    }
    match key.code {
        KeyCode::Esc => {
            app.dialog = Some(TestDialog::CreateRecovery(0));
            TestKeyAction::Continue(true)
        }
        KeyCode::Char('r') => {
            start_candidate_search(app, tx);
            TestKeyAction::Continue(true)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let next = selected.saturating_sub(1);
            app.dialog = Some(TestDialog::CandidateList { selected: next });
            TestKeyAction::Continue(true)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let next = (selected + 1).min(app.candidates.len().saturating_sub(1));
            app.dialog = Some(TestDialog::CandidateList { selected: next });
            TestKeyAction::Continue(true)
        }
        KeyCode::Enter => {
            if app.adopt_candidate(selected) {
                TestKeyAction::Continue(true)
            } else {
                TestKeyAction::Continue(false)
            }
        }
        KeyCode::Char('d') => {
            if app.candidates.get(selected).is_some() {
                app.dialog = Some(TestDialog::DeleteCandidate {
                    selected,
                    yes: false,
                });
                TestKeyAction::Continue(true)
            } else {
                TestKeyAction::Continue(false)
            }
        }
        _ => TestKeyAction::Continue(false),
    }
}

/// Tecla de confirmação da exclusão de um candidato.
fn handle_delete_candidate_key(
    app: &mut TestApp,
    tx: &mpsc::UnboundedSender<TestEvent>,
    selected: usize,
    yes: bool,
    key: event::KeyEvent,
) -> TestKeyAction {
    if key.kind != KeyEventKind::Press {
        return TestKeyAction::Continue(false);
    }
    let set_confirmation = |app: &mut TestApp, value: bool| {
        app.dialog = Some(TestDialog::DeleteCandidate {
            selected,
            yes: value,
        });
    };
    match key.code {
        KeyCode::Left | KeyCode::Right => {
            set_confirmation(app, !yes);
            TestKeyAction::Continue(true)
        }
        KeyCode::Char('y') => {
            set_confirmation(app, true);
            TestKeyAction::Continue(true)
        }
        KeyCode::Enter if yes => {
            let Some(candidate) = app.candidates.get(selected) else {
                app.dialog = Some(TestDialog::CandidateList { selected: 0 });
                return TestKeyAction::Continue(true);
            };
            let id = candidate.id;
            app.candidate_activity = CandidateActivity::Deleting;
            app.candidate_message = None;
            app.dialog = Some(TestDialog::CandidateList { selected });
            if let Some(prep) = app.prep.clone() {
                let tx = tx.clone();
                tokio::spawn(async move { backend_delete_candidate(prep, id, tx).await });
            } else {
                app.candidate_activity = CandidateActivity::Idle;
                app.candidate_message = Some("sem contexto para excluir o candidato".to_owned());
            }
            TestKeyAction::Continue(true)
        }
        KeyCode::Char('n') | KeyCode::Esc | KeyCode::Enter => {
            app.dialog = Some(TestDialog::CandidateList { selected });
            TestKeyAction::Continue(true)
        }
        _ => TestKeyAction::Continue(false),
    }
}

/// Tecla no diálogo "Criar este Test Case?".
fn handle_confirm_create_key(
    app: &mut TestApp,
    tx: &mpsc::UnboundedSender<TestEvent>,
    yes: bool,
    key: event::KeyEvent,
) -> TestKeyAction {
    match (key.code, key.kind) {
        (KeyCode::Left | KeyCode::Right, KeyEventKind::Press) => {
            app.dialog = Some(TestDialog::ConfirmCreate(!yes));
            TestKeyAction::Continue(true)
        }
        (KeyCode::Char('y'), KeyEventKind::Press) => {
            app.dialog = None;
            start_create(app, tx);
            TestKeyAction::Continue(true)
        }
        (KeyCode::Char('n' | 'q') | KeyCode::Esc, KeyEventKind::Press) => {
            app.dialog = None;
            app.logs
                .push_back("criação dispensada — q sai (revisado)".to_owned());
            TestKeyAction::Continue(true)
        }
        (KeyCode::Enter, KeyEventKind::Press) => {
            if yes {
                app.dialog = None;
                start_create(app, tx);
            } else {
                app.dialog = None;
                app.logs
                    .push_back("criação dispensada — q sai (revisado)".to_owned());
            }
            TestKeyAction::Continue(true)
        }
        _ => TestKeyAction::Continue(false),
    }
}

/// Tecla no diálogo "Atualizar Work Item pai para Test QA?".
fn handle_confirm_qa_key(app: &mut TestApp, yes: bool, key: event::KeyEvent) -> TestKeyAction {
    match (key.code, key.kind) {
        (KeyCode::Left | KeyCode::Right, KeyEventKind::Press) => {
            app.dialog = Some(TestDialog::ConfirmTestQa(!yes));
            TestKeyAction::Continue(true)
        }
        (KeyCode::Char('y'), KeyEventKind::Press) => {
            app.open_qa_efforts();
            TestKeyAction::Continue(true)
        }
        (KeyCode::Char('n' | 'q') | KeyCode::Esc, KeyEventKind::Press) => {
            app.dialog = None;
            TestKeyAction::Continue(true)
        }
        (KeyCode::Enter, KeyEventKind::Press) => {
            if yes {
                app.open_qa_efforts();
            } else {
                app.dialog = None;
            }
            TestKeyAction::Continue(true)
        }
        _ => TestKeyAction::Continue(false),
    }
}

/// Tecla no diálogo de esforços do Test QA (único que espera I/O).
async fn handle_qa_efforts_key(
    app: &mut TestApp,
    terminal: &mut DefaultTerminal,
    key: event::KeyEvent,
) -> anyhow::Result<TestKeyAction> {
    match (key.code, key.kind) {
        (KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down, KeyEventKind::Press) => {
            app.qa_focus ^= 1;
            Ok(TestKeyAction::Continue(true))
        }
        (KeyCode::Esc, KeyEventKind::Press) => {
            app.dialog = Some(TestDialog::ConfirmTestQa(false));
            Ok(TestKeyAction::Continue(true))
        }
        (KeyCode::Enter, KeyEventKind::Press) => {
            match app.validate_qa_efforts() {
                Ok((effort, real)) => {
                    app.dialog = None;
                    run_qa_update(app, terminal, effort, real).await?;
                }
                Err(msg) => {
                    app.error = Some(msg);
                }
            }
            Ok(TestKeyAction::Continue(true))
        }
        _ => {
            let editor = if app.qa_focus == 0 {
                &mut app.qa_effort
            } else {
                &mut app.qa_real
            };
            if editor.handle_key(key) {
                app.error = None;
                app.error_field = None;
                Ok(TestKeyAction::Continue(true))
            } else {
                Ok(TestKeyAction::Continue(false))
            }
        }
    }
}

/// Despacha a tecla (fora de diálogo) conforme a fase atual.
fn handle_test_phase_key(app: &mut TestApp, key: event::KeyEvent) -> TestKeyAction {
    if app.content_edit.is_some() {
        return handle_content_edit_key(app, key);
    }
    match app.phase {
        TestPhase::Pronto => handle_pronto_key(app, key),
        TestPhase::Erro => handle_erro_key(app, key),
        TestPhase::Preparando | TestPhase::Gerando | TestPhase::Criando => {
            handle_busy_key(app, key)
        }
        TestPhase::Revisao => {
            if app.panel == Panel::Settings {
                handle_review_settings_key(app, key)
            } else {
                handle_review_preview_key(app, key)
            }
        }
    }
}

/// Trata uma tecla enquanto o editor de conteúdo está aberto.
fn handle_content_edit_key(app: &mut TestApp, key: event::KeyEvent) -> TestKeyAction {
    let Some(editor) = app.content_edit.as_mut() else {
        return TestKeyAction::Continue(false);
    };
    match editor.handle_key(key) {
        ContentEditAction::Saved(content) => {
            app.title = content.title;
            app.body = content.body;
            app.content_edit = None;
            app.error = None;
            app.error_field = None;
            app.scroll = 0;
            app.push_log("conteúdo salvo — preview atualizado".to_owned());
            TestKeyAction::Continue(true)
        }
        ContentEditAction::Cancelled => {
            app.content_edit = None;
            app.error = None;
            app.error_field = None;
            TestKeyAction::Continue(true)
        }
        ContentEditAction::Consumed => TestKeyAction::Continue(true),
        ContentEditAction::Ignored => TestKeyAction::Continue(false),
    }
}

/// Trata paste do terminal enquanto o editor está aberto.
fn handle_content_paste(app: &mut TestApp, text: &str) -> bool {
    let Some(editor) = app.content_edit.as_mut() else {
        return false;
    };
    match editor.field {
        ContentField::Title => editor.title.insert_text(text),
        ContentField::Body => editor.body.insert_text(text),
    }
    editor.error = None;
    true
}

/// Tecla na fase `Pronto` (Test Case já criado).
fn handle_pronto_key(app: &mut TestApp, key: event::KeyEvent) -> TestKeyAction {
    let ctrl_alt = key
        .modifiers
        .contains(KeyModifiers::CONTROL | KeyModifiers::ALT);
    if ctrl_alt {
        return TestKeyAction::Continue(false);
    }
    if key.code == KeyCode::Enter && key.kind == KeyEventKind::Press && !app.parent_updated {
        app.dialog = Some(TestDialog::ConfirmTestQa(false));
        return TestKeyAction::Continue(true);
    }
    if key.code == KeyCode::Char('c') && key.modifiers.is_empty() && key.kind == KeyEventKind::Press
    {
        let body = app.copy_body();
        if crate::features::describe::copy_to_clipboard(body.as_str()) {
            app.flash_copied();
        } else {
            app.parent_msg =
                Some("clipboard indisponível — selecione e copie manualmente".to_owned());
        }
        return TestKeyAction::Continue(true);
    }
    if key.kind != KeyEventKind::Press {
        return TestKeyAction::Continue(false);
    }
    match app.created.take() {
        Some((id, url)) => TestKeyAction::Done(TestFlowOutcome::Created { id, url }),
        None => TestKeyAction::Done(TestFlowOutcome::Reviewed),
    }
}

/// Tecla na fase `Erro` (qualquer tecla sai com o erro).
fn handle_erro_key(app: &mut TestApp, key: event::KeyEvent) -> TestKeyAction {
    if key.kind != KeyEventKind::Press {
        return TestKeyAction::Continue(false);
    }
    let msg = match app.error.take() {
        Some(m) => m,
        None => "falha desconhecida".to_owned(),
    };
    TestKeyAction::Fail(msg)
}

/// Tecla nas fases ocupadas (só scroll e abortar).
fn handle_busy_key(app: &mut TestApp, key: event::KeyEvent) -> TestKeyAction {
    if key
        .modifiers
        .contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return TestKeyAction::Continue(false);
    }
    match (key.code, key.kind) {
        (KeyCode::Char('q') | KeyCode::Esc, KeyEventKind::Press) => {
            TestKeyAction::Done(TestFlowOutcome::Aborted)
        }
        (KeyCode::Char('j') | KeyCode::Down | KeyCode::PageDown, _) => {
            app.scroll_by(3);
            TestKeyAction::Continue(true)
        }
        (KeyCode::Char('k') | KeyCode::Up | KeyCode::PageUp, _) => {
            app.scroll_by(-3);
            TestKeyAction::Continue(true)
        }
        _ => TestKeyAction::Continue(false),
    }
}

/// Tecla na revisão com o painel de settings focado (edição dos 6 campos).
fn handle_review_settings_key(app: &mut TestApp, key: event::KeyEvent) -> TestKeyAction {
    let ctrl_alt = key
        .modifiers
        .contains(KeyModifiers::CONTROL | KeyModifiers::ALT);
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            if key.kind == KeyEventKind::Press {
                app.panel = Panel::Preview;
                TestKeyAction::Continue(true)
            } else {
                TestKeyAction::Continue(false)
            }
        }
        KeyCode::Esc => {
            if key.kind == KeyEventKind::Press && !ctrl_alt {
                if app.create_recovery == CreateRecoveryState::Available {
                    app.dialog = Some(TestDialog::CreateRecovery(0));
                } else {
                    app.panel = Panel::Preview;
                }
                TestKeyAction::Continue(true)
            } else {
                TestKeyAction::Continue(false)
            }
        }
        KeyCode::Up => {
            if ctrl_alt {
                TestKeyAction::Continue(false)
            } else {
                app.field_focus = (app.field_focus + FIELD_COUNT - 1) % FIELD_COUNT;
                TestKeyAction::Continue(true)
            }
        }
        KeyCode::Down => {
            if ctrl_alt {
                TestKeyAction::Continue(false)
            } else {
                app.field_focus = (app.field_focus + 1) % FIELD_COUNT;
                TestKeyAction::Continue(true)
            }
        }
        KeyCode::Enter => {
            if key.kind == KeyEventKind::Press && !ctrl_alt {
                if app.no_create {
                    app.logs
                        .push_back("somente revisão (--no-create)".to_owned());
                    TestKeyAction::Done(TestFlowOutcome::ReviewedNoCreate)
                } else {
                    app.dialog = Some(TestDialog::ConfirmCreate(app.create_initial));
                    TestKeyAction::Continue(true)
                }
            } else {
                TestKeyAction::Continue(false)
            }
        }
        _ => {
            let idx = app.field_focus % FIELD_COUNT;
            let consumed = match app.fields.get_mut(idx) {
                Some(f) => f.handle_key(key),
                None => false,
            };
            if consumed {
                app.error = None;
                app.error_field = None;
                TestKeyAction::Continue(true)
            } else {
                TestKeyAction::Continue(false)
            }
        }
    }
}

/// Tecla na revisão com o preview focado (scroll, copia, cria).
fn handle_review_preview_key(app: &mut TestApp, key: event::KeyEvent) -> TestKeyAction {
    let ctrl_alt = key
        .modifiers
        .contains(KeyModifiers::CONTROL | KeyModifiers::ALT);
    if ctrl_alt {
        return TestKeyAction::Continue(false);
    }
    match (key.code, key.kind) {
        (KeyCode::Tab | KeyCode::BackTab, KeyEventKind::Press) => {
            app.panel = Panel::Settings;
            TestKeyAction::Continue(true)
        }
        (KeyCode::Char('e'), KeyEventKind::Press) if app.frozen_create_content.is_none() => {
            TestKeyAction::Continue(app.open_content_edit())
        }
        (KeyCode::Char('q') | KeyCode::Esc, KeyEventKind::Press) => {
            TestKeyAction::Done(TestFlowOutcome::Reviewed)
        }
        (KeyCode::Char('j') | KeyCode::Down | KeyCode::PageDown, _) => {
            app.scroll_by(3);
            TestKeyAction::Continue(true)
        }
        (KeyCode::Char('k') | KeyCode::Up | KeyCode::PageUp, _) => {
            app.scroll_by(-3);
            TestKeyAction::Continue(true)
        }
        (KeyCode::Home, KeyEventKind::Press) => {
            app.scroll = 0;
            TestKeyAction::Continue(true)
        }
        (KeyCode::Char('c'), KeyEventKind::Press) => {
            let body = app.copy_body();
            if crate::features::describe::copy_to_clipboard(body.as_str()) {
                app.flash_copied();
            } else {
                app.parent_msg =
                    Some("clipboard indisponível — selecione e copie manualmente".to_owned());
            }
            TestKeyAction::Continue(true)
        }
        (KeyCode::Enter, KeyEventKind::Press) => {
            if app.no_create {
                app.logs
                    .push_back("somente revisão (--no-create)".to_owned());
                TestKeyAction::Done(TestFlowOutcome::ReviewedNoCreate)
            } else {
                app.dialog = Some(TestDialog::ConfirmCreate(app.create_initial));
                TestKeyAction::Continue(true)
            }
        }
        _ => TestKeyAction::Continue(false),
    }
}

/// Valida settings e dispara a tarefa de criação (ou mostra o erro).
fn start_create(app: &mut TestApp, tx: &mpsc::UnboundedSender<TestEvent>) {
    if let Err(error) = test_card::validate_card(&app.title, &app.body) {
        let mut editor = ContentEditState::for_test(app.title.as_str(), app.body.as_str());
        if let Err(content_error) = editor.validate() {
            app.error = Some(content_error.to_string());
            editor.set_error(content_error);
        } else {
            app.error = Some(error.to_string());
        }
        app.content_edit = Some(editor);
        app.panel = Panel::Preview;
        return;
    }
    match app.build_settings() {
        Ok(settings) => {
            if let Some(prep) = app.prep.clone() {
                let content = if let Some(content) = app.frozen_create_content.clone() {
                    content
                } else {
                    let content = create_content_for_attempt(app);
                    app.frozen_create_content = Some(content.clone());
                    content
                };
                app.last_create_settings = Some(settings.clone());
                app.create_recovery = CreateRecoveryState::Unavailable;
                app.phase = TestPhase::Criando;
                "criando test case…".clone_into(&mut app.phase_label);
                app.progress = 0.3;
                "enviando…".clone_into(&mut app.progress_label);
                app.error = None;
                app.error_field = None;
                app.candidates.clear();
                app.candidate_message = None;
                let tx2 = tx.clone();
                tokio::spawn(backend_create(
                    prep,
                    settings,
                    content.title,
                    content.body,
                    tx2,
                ));
            } else {
                app.panel = Panel::Settings;
                app.error = Some("sem contexto p/ criar (prepare falhou)".to_owned());
            }
        }
        Err(msg) => {
            app.panel = Panel::Settings;
            app.error_field = settings_field_for_error(msg.as_str());
            if let Some(field) = app.error_field {
                app.field_focus = field.index();
            }
            app.error = Some(msg);
        }
    }
}

/// Associa uma falha de validação local ao campo editável correspondente.
fn settings_field_for_error(message: &str) -> Option<TestSettingsField> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("team") {
        Some(TestSettingsField::Team)
    } else if lower.contains("programa") {
        Some(TestSettingsField::Program)
    } else if lower.contains("responsável") || lower.contains("assigned") {
        Some(TestSettingsField::AssignedTo)
    } else if lower.contains("prioridade") {
        Some(TestSettingsField::Priority)
    } else {
        None
    }
}

/// Inicia a consulta de possíveis cards criados antes de uma resposta perdida.
fn start_candidate_search(app: &mut TestApp, tx: &mpsc::UnboundedSender<TestEvent>) {
    let (Some(prep), Some(settings)) = (app.prep.clone(), app.last_create_settings.clone()) else {
        app.candidate_message =
            Some("sem contexto da última tentativa para consultar o Azure".to_owned());
        app.dialog = Some(TestDialog::CandidateList { selected: 0 });
        return;
    };
    app.candidates.clear();
    app.candidate_activity = CandidateActivity::Loading;
    app.candidate_message = None;
    app.dialog = Some(TestDialog::CandidateList { selected: 0 });
    let title = create_candidate_title(app);
    let tx = tx.clone();
    tokio::spawn(async move {
        backend_find_candidates(prep, title, settings, tx).await;
    });
}

/// Retorna o conteúdo aprovado para esta tentativa de criação.
fn create_content_for_attempt(app: &TestApp) -> PrDescription {
    app.frozen_create_content
        .clone()
        .unwrap_or_else(|| PrDescription {
            title: app.title.clone(),
            body: app.body.clone(),
        })
}

/// Título usado pela busca de duplicidade do Test Case atual.
fn create_candidate_title(app: &TestApp) -> String {
    create_content_for_attempt(app).title
}

/// Executa o update do pai p/ Test QA com esforços já validados.
async fn run_qa_update(
    app: &mut TestApp,
    terminal: &mut DefaultTerminal,
    effort: String,
    real: String,
) -> anyhow::Result<()> {
    app.parent_msg = Some("atualizando pai p/ Test QA…".to_owned());
    terminal.draw(|f| f.render_widget(&*app, f.area()))?;
    run_qa_update_with(app, effort, real, |prep, effort, real| async move {
        test_card::update_parent_with_current_config(&prep, effort.as_deref(), real.as_deref())
            .await
    })
    .await
}

/// Atualiza o estado do pai usando um adaptador de escrita isolado.
async fn run_qa_update_with<U, UFut>(
    app: &mut TestApp,
    effort: String,
    real: String,
    update: U,
) -> anyhow::Result<()>
where
    U: FnOnce(TestCardPrep, Option<String>, Option<String>) -> UFut,
    UFut: Future<Output = crate::error::Result<()>>,
{
    match app.prep.clone() {
        Some(prep) => match update(prep, Some(effort), Some(real)).await {
            Ok(()) => {
                app.parent_updated = true;
                app.parent_msg = Some("pai atualizado p/ Test QA ✓".to_owned());
            }
            Err(e) => {
                app.parent_msg = Some(test_card::describe_parent_update_error(&e));
            }
        },
        None => {
            app.parent_msg = Some("sem contexto do pai p/ atualizar".to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{Receiver, channel};
    use std::thread::{JoinHandle, spawn};

    use super::*;
    use crate::git::{ChangeContext, GitContextFingerprint, RepositoryRemote};
    use ratatui::{
        DefaultTerminal, Terminal, TerminalOptions, Viewport, backend::TestBackend, layout::Rect,
    };

    fn review_app() -> TestApp {
        let mut app = TestApp::new();
        app.title = "Card exemplo".to_owned();
        app.body = "## Objetivo\nX".to_owned();
        app.phase = TestPhase::Revisao;
        app.create_initial = true;
        app
    }

    fn test_terminal() -> DefaultTerminal {
        Terminal::with_options(
            ratatui::backend::CrosstermBackend::new(std::io::stdout()),
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, 100, 30)),
            },
        )
        .expect("terminal de teste")
    }

    fn published_request() -> TestCardRequest {
        let remote = RepositoryRemote {
            organization: "org".to_owned(),
            project: "project".to_owned(),
            repository: "repo".to_owned(),
        };
        let parent: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11763,
            "fields": {
                "System.Title": "Mudança funcional",
                "System.WorkItemType": "User Story",
                "System.IterationPath": "project\\Sprint 12"
            }
        }))
        .expect("snapshot do pai");
        TestCardRequest::PublishedPr(crate::features::test_card::TestCardLaunchContext {
            published_pr: crate::azure::pull_requests::PublishedPr {
                target: "dev".to_owned(),
                id: 99,
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/99".to_owned(),
            },
            remote,
            work_item_id: Some(11763),
            work_item: Some(parent),
            source_ref_name: "refs/heads/feature/11763-exemplo".to_owned(),
            target_ref_name: "refs/heads/dev".to_owned(),
            config: crate::config::Config {
                azure_pat: "pat".to_owned(),
                test_team: "DevOps".to_owned(),
                test_program: "Agrotrace".to_owned(),
                ..crate::config::Config::default()
            },
            settings: TestSettings {
                area_path: "project\\QA".to_owned(),
                assigned_to: "qa@example.com".to_owned(),
                iteration_path: "project\\Sprint 12".to_owned(),
                priority: 2.0,
                team: "DevOps".to_owned(),
                program: "Agrotrace".to_owned(),
            },
            fingerprint: GitContextFingerprint::default(),
        })
    }

    fn published_prep() -> TestCardPrep {
        let TestCardRequest::PublishedPr(context) = published_request() else {
            unreachable!();
        };
        TestCardPrep {
            config: context.config,
            context: ChangeContext {
                branch: "feature/11763-exemplo".to_owned(),
                source_ref: context.source_ref_name,
                base_branch: context.target_ref_name,
                sprint_branch: String::new(),
                diff: "diff".to_owned(),
                diff_original_lines: 1,
                log: "log".to_owned(),
                work_item_id: "11763".to_owned(),
                remote: Some(context.remote),
            },
            parent: context.work_item.expect("pai"),
            pr_id: Some(context.published_pr.id.to_string()),
            settings: Some(context.settings),
            pr_changes: "changes".to_owned(),
            examples_text: "- #5 Exemplo".to_owned(),
            prompt: "prompt com PR e refs".to_owned(),
        }
    }

    fn spawn_http_error_server(
        status: u16,
        body: &str,
    ) -> (String, Receiver<(String, String)>, JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener");
        let address = listener.local_addr().expect("endereço");
        let (sender, receiver) = channel();
        let body = body.to_owned();
        let handle = spawn(move || {
            let (mut stream, _) = listener.accept().expect("conexão");
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0_u8; 4096];
                let read = stream.read(&mut chunk).expect("leitura");
                assert!(read > 0, "cliente encerrou antes dos cabeçalhos");
                bytes.extend_from_slice(&chunk[..read]);
                if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    let line = String::from_utf8_lossy(&bytes[..end]);
                    let mut parts = line
                        .lines()
                        .next()
                        .expect("request line")
                        .split_whitespace();
                    sender
                        .send((
                            parts.next().expect("método").to_owned(),
                            parts.next().expect("target").to_owned(),
                        ))
                        .expect("captura");
                    break;
                }
            }
            let reason = match status {
                401 => "Unauthorized",
                403 => "Forbidden",
                _ => "Error",
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("resposta");
        });
        (format!("http://{address}/org"), receiver, handle)
    }

    fn render_text(app: &TestApp) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|f| f.render_widget(app, f.area()))
            .expect("render");
        let buffer = terminal.backend().buffer();
        let width = usize::from(buffer.area.width);
        let mut text = String::new();
        for (index, cell) in buffer.content().iter().enumerate() {
            if index > 0 && index % width == 0 {
                text.push('\n');
            }
            text.push_str(cell.symbol());
        }
        text
    }

    #[tokio::test]
    async fn published_preparation_failure_should_not_start_generation_or_remote_writes() {
        let mut request = published_request();
        if let TestCardRequest::PublishedPr(context) = &mut request {
            context.settings.team.clear();
        }
        let (tx, mut rx) = mpsc::unbounded_channel();
        backend_prepare_generate(request, tx).await;
        let mut backend_failed = false;
        let mut backend_generated = false;
        while let Ok(event) = rx.try_recv() {
            match &event {
                TestEvent::Failed(message) => {
                    backend_failed = true;
                    assert!(message.contains("Custom.Team"));
                }
                TestEvent::Generated { .. } => backend_generated = true,
                _ => {}
            }
        }
        assert!(backend_failed);
        assert!(!backend_generated);

        for status in [401, 403] {
            let (base_url, request_rx, server) =
                spawn_http_error_server(status, r#"{"message":"access denied"}"#);
            let context = match published_request() {
                TestCardRequest::PublishedPr(context) => context,
                TestCardRequest::Cli(_) => unreachable!(),
            };
            let client = crate::azure::AzureClient::new_for_test(&base_url, "pat");
            let request = TestCardRequest::PublishedPr(context.clone());
            let (tx, mut rx) = mpsc::unbounded_channel();
            backend_prepare_generate_with(
                request,
                tx,
                move |_| async move {
                    crate::features::test_card::prepare_published_pr_with(
                        &context,
                        context.config.clone(),
                        &client,
                        |_, _| panic!("Git não pode começar após falha de autenticação"),
                    )
                    .await
                },
                |_| async { panic!("IA não pode começar após falha de autenticação") },
            )
            .await;
            let mut backend_failed = false;
            let mut backend_generated = false;
            let mut app = TestApp::for_request(&published_request());
            while let Ok(event) = rx.try_recv() {
                backend_failed |= matches!(event, TestEvent::Failed(_));
                backend_generated |= matches!(event, TestEvent::Generated { .. });
                app.on_event(event);
            }
            assert!(backend_failed);
            assert!(!backend_generated);
            assert_eq!(app.phase, TestPhase::Erro);
            assert!(
                app.error
                    .as_deref()
                    .is_some_and(|message| message.contains(&status.to_string()))
            );
            assert!(app.prep.is_none());
            assert!(app.created.is_none());
            assert!(app.last_create_settings.is_none());
            assert!(!app.parent_updated);
            let (method, target) = request_rx.recv().expect("lookup do PR");
            assert_eq!(method, "GET");
            assert!(target.contains("pullRequests/99"));
            server.join().expect("servidor de autenticação");
        }
    }

    #[test]
    fn matching_git_fingerprint_should_skip_divergence_gate() {
        let fingerprint = GitContextFingerprint::capture("", &[]).expect("fingerprint atual");
        let mut request = published_request();
        let TestCardRequest::PublishedPr(context) = &mut request else {
            unreachable!();
        };
        context.fingerprint = fingerprint;
        assert!(!published_context_diverged(&request));
        let mut app = TestApp::for_request(&request);
        assert!(app.dialog.is_none());
        assert!(!app.start_after_divergence);
        app.on_event(TestEvent::Generated {
            prep: Box::new(published_prep()),
            title: "Card remoto".to_owned(),
            body: "## Objetivo\nPR remoto".to_owned(),
            initial: [
                "project\\QA".to_owned(),
                "qa@example.com".to_owned(),
                "project\\Sprint 12".to_owned(),
                "2".to_owned(),
                "DevOps".to_owned(),
                "Agrotrace".to_owned(),
            ],
        });
        assert_eq!(app.phase, TestPhase::Revisao);
        assert_eq!(
            app.prep.as_ref().and_then(|prep| prep.pr_id.as_deref()),
            Some("99")
        );
        assert_eq!(app.prep.as_ref().map(|prep| prep.parent.id), Some(11763));
    }

    #[test]
    fn changed_git_fingerprint_should_open_remote_snapshot_gate() {
        let mut request = published_request();
        let TestCardRequest::PublishedPr(_) = &mut request else {
            unreachable!();
        };
        let matching =
            GitContextFingerprint::capture("", &["main".to_owned()]).expect("fingerprint base");
        let mutations: [fn(&mut GitContextFingerprint); 4] = [
            |fingerprint: &mut GitContextFingerprint| {
                fingerprint.repository = "outro-checkout".to_owned();
            },
            |fingerprint: &mut GitContextFingerprint| {
                fingerprint.source_branch = "outra-branch".to_owned();
            },
            |fingerprint: &mut GitContextFingerprint| {
                fingerprint.source_oid = "outro-source-oid".to_owned();
            },
            |fingerprint: &mut GitContextFingerprint| {
                fingerprint
                    .target_oids
                    .insert("main".to_owned(), "outro-target-oid".to_owned());
            },
        ];
        for mutate in mutations {
            {
                let TestCardRequest::PublishedPr(context) = &mut request else {
                    unreachable!();
                };
                context.fingerprint = matching.clone();
                mutate(&mut context.fingerprint);
            }
            assert!(published_context_diverged(&request));
        }

        let mut app = TestApp::for_request(&request);
        app.dialog = Some(TestDialog::PublishedContextDivergence(false));
        let rendered = render_text(&app);
        assert!(rendered.contains("continuar com snapshot remoto"));
        assert!(rendered.contains("voltar"));
        assert!(rendered.contains("O checkout mudou desde a publicação"));
    }

    #[test]
    fn remote_snapshot_choice_should_never_use_current_checkout_as_context() {
        let request = published_request();
        let TestCardRequest::PublishedPr(context) = &request else {
            unreachable!();
        };
        let mut app = TestApp::for_request(&request);
        app.dialog = Some(TestDialog::PublishedContextDivergence(false));
        let action = handle_published_context_divergence_key(
            &mut app,
            false,
            event::KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        );
        assert!(matches!(action, TestKeyAction::Continue(true)));
        let action = handle_published_context_divergence_key(
            &mut app,
            true,
            event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(matches!(action, TestKeyAction::Continue(true)));
        assert!(app.start_after_divergence);
        assert!(app.prep.is_none());
        assert!(app.created.is_none());
        assert_eq!(context.source_ref_name, "refs/heads/feature/11763-exemplo");
        assert_eq!(context.target_ref_name, "refs/heads/dev");

        app.on_event(TestEvent::Generated {
            prep: Box::new(published_prep()),
            title: "Card remoto".to_owned(),
            body: "## Objetivo\nSnapshot remoto".to_owned(),
            initial: [
                "project\\QA".to_owned(),
                "qa@example.com".to_owned(),
                "project\\Sprint 12".to_owned(),
                "2".to_owned(),
                "DevOps".to_owned(),
                "Agrotrace".to_owned(),
            ],
        });
        let prep = app.prep.as_ref().expect("prep do PR remoto");
        assert_eq!(prep.context.source_ref, "refs/heads/feature/11763-exemplo");
        assert_eq!(prep.context.base_branch, "refs/heads/dev");
        assert_eq!(prep.context.diff, "diff");
        assert_eq!(prep.context.log, "log");
        assert!(prep.prompt.contains("PR e refs"));

        let mut cancelled = TestApp::for_request(&request);
        cancelled.dialog = Some(TestDialog::PublishedContextDivergence(false));
        let action = handle_published_context_divergence_key(
            &mut cancelled,
            false,
            event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(matches!(
            action,
            TestKeyAction::Done(TestFlowOutcome::Aborted)
        ));
        assert!(cancelled.prep.is_none());
        assert!(cancelled.created.is_none());
        assert!(!cancelled.parent_updated);
        let TestCardRequest::PublishedPr(context) = request else {
            unreachable!();
        };
        assert_eq!(context.published_pr.id, 99);
        assert_eq!(context.published_pr.target, "dev");
        assert_eq!(
            context.published_pr.url,
            "https://dev.azure.com/org/project/_git/repo/pullrequest/99"
        );
    }

    #[tokio::test]
    async fn published_request_should_enter_review_without_reprompting_context() {
        let request = published_request();
        let started = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let started_by_backend = started.clone();
        let generated = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let generated_by_backend = generated.clone();
        let mut input = std::collections::VecDeque::from([
            Event::Key(event::KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)),
            Event::Key(event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ]);
        let mut idle_cycles = 0;
        let mut terminal = test_terminal();
        let outcome = run_loop_with(
            &mut terminal,
            request,
            move || {
                if let Some(event) = input.pop_front() {
                    return Ok(Some(event));
                }
                if idle_cycles < 500 {
                    idle_cycles += 1;
                    return Ok(None);
                }
                Ok(Some(Event::Key(event::KeyEvent::new(
                    KeyCode::Char('q'),
                    KeyModifiers::NONE,
                ))))
            },
            move |request, tx| {
                let started = started_by_backend.clone();
                let generated = generated_by_backend.clone();
                tokio::spawn(backend_prepare_generate_with(
                    request,
                    tx,
                    move |request| {
                        started.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        async move {
                            assert!(matches!(request, TestCardRequest::PublishedPr(_)));
                            let prep = published_prep();
                            assert_eq!(prep.pr_id.as_deref(), Some("99"));
                            assert_eq!(prep.context.source_ref, "refs/heads/feature/11763-exemplo");
                            assert_eq!(prep.context.base_branch, "refs/heads/dev");
                            assert_eq!(prep.parent.id, 11763);
                            assert_eq!(
                                prep.settings
                                    .as_ref()
                                    .map(|settings| settings.team.as_str()),
                                Some("DevOps")
                            );
                            Ok(prep)
                        }
                    },
                    move |_| {
                        generated.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        async {
                            Ok(PrDescription {
                                title: "Card do PR".to_owned(),
                                body: "## Objetivo\nValidar o PR".to_owned(),
                            })
                        }
                    },
                ));
            },
        )
        .await
        .expect("loop do handoff publicado");
        assert!(matches!(outcome, TestFlowOutcome::Reviewed));
        assert_eq!(started.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(generated.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn one_handoff_activation_should_prepare_one_test_case_for_multiple_targets() {
        let receipt = [
            crate::azure::pull_requests::PublishedPr {
                target: "sprint/12".to_owned(),
                id: 98,
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/98".to_owned(),
            },
            crate::azure::pull_requests::PublishedPr {
                target: "dev".to_owned(),
                id: 99,
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/99".to_owned(),
            },
        ];
        assert_eq!(receipt.len(), 2);
        let request = published_request();
        let TestCardRequest::PublishedPr(context) = request else {
            unreachable!();
        };
        assert_eq!(context.published_pr.id, 99);
        assert_eq!(context.published_pr.target, "dev");
        assert_eq!(
            context.published_pr.url,
            "https://dev.azure.com/org/project/_git/repo/pullrequest/99"
        );
        assert_eq!(context.source_ref_name, "refs/heads/feature/11763-exemplo");
        assert_eq!(context.target_ref_name, "refs/heads/dev");
        assert_eq!(context.work_item_id, Some(11763));
        assert_eq!(context.published_pr.id, receipt[1].id);
        let app = TestApp::for_request(&TestCardRequest::PublishedPr(context));
        assert!(!app.create_initial);
        assert!(!app.no_create);
        assert_eq!(app.phase, TestPhase::Preparando);
        assert!(app.prep.is_none());
        assert!(app.created.is_none());
        assert!(!app.parent_updated);
    }

    #[tokio::test]
    async fn cancelled_test_case_handoff_should_preserve_published_receipt() {
        let request = published_request();
        let TestCardRequest::PublishedPr(context) = &request else {
            unreachable!();
        };
        let published = [
            crate::azure::pull_requests::PublishedPr {
                target: "sprint/12".to_owned(),
                id: 98,
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/98".to_owned(),
            },
            context.published_pr.clone(),
        ];
        let receipt = published
            .iter()
            .map(|item| format!("PR #{} · {} · {}", item.id, item.target, item.url))
            .collect::<Vec<_>>()
            .join("\n");

        let mut gate_request = request.clone();
        if let TestCardRequest::PublishedPr(context) = &mut gate_request {
            context.fingerprint.repository = "outro-checkout".to_owned();
        }
        let backend_starts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let starts = backend_starts.clone();
        let mut gate_input = std::collections::VecDeque::from([Event::Key(event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ))]);
        let mut gate_terminal = test_terminal();
        let gate_outcome = run_loop_with(
            &mut gate_terminal,
            gate_request,
            move || Ok(gate_input.pop_front()),
            move |_, _| {
                starts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            },
        )
        .await
        .expect("cancelamento do gate");
        assert!(matches!(gate_outcome, TestFlowOutcome::Aborted));
        assert_eq!(backend_starts.load(std::sync::atomic::Ordering::Relaxed), 0);
        for item in &published {
            assert!(receipt.contains(&format!("PR #{}", item.id)));
            assert!(receipt.contains(&item.target));
            assert!(receipt.contains(&item.url));
        }

        let mut review_input = std::collections::VecDeque::from([
            Event::Key(event::KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)),
            Event::Key(event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ]);
        let mut idle_cycles = 0;
        let mut review_terminal = test_terminal();
        let generated = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let generated_by_backend = generated.clone();
        let review_outcome = run_loop_with(
            &mut review_terminal,
            request,
            move || {
                if let Some(event) = review_input.pop_front() {
                    return Ok(Some(event));
                }
                if idle_cycles < 500 {
                    idle_cycles += 1;
                    return Ok(None);
                }
                Ok(Some(Event::Key(event::KeyEvent::new(
                    KeyCode::Esc,
                    KeyModifiers::NONE,
                ))))
            },
            move |request, tx| {
                let generated = generated_by_backend.clone();
                tokio::spawn(backend_prepare_generate_with(
                    request,
                    tx,
                    |_| async { Ok(published_prep()) },
                    move |_| {
                        generated.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        async {
                            Ok(PrDescription {
                                title: "Card".to_owned(),
                                body: "## Objetivo\nX".to_owned(),
                            })
                        }
                    },
                ));
            },
        )
        .await
        .expect("cancelamento na revisão");
        assert!(matches!(review_outcome, TestFlowOutcome::Reviewed));
        assert_eq!(generated.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn failed_test_case_handoff_should_preserve_published_receipt_and_writers() {
        let (base_url, request_rx, server) =
            spawn_http_error_server(403, r#"{"message":"access denied"}"#);
        let context = match published_request() {
            TestCardRequest::PublishedPr(context) => context,
            TestCardRequest::Cli(_) => unreachable!(),
        };
        let client = crate::azure::AzureClient::new_for_test(&base_url, "pat");
        let request = TestCardRequest::PublishedPr(context.clone());
        let (tx, mut rx) = mpsc::unbounded_channel();
        backend_prepare_generate_with(
            request.clone(),
            tx,
            move |_| async move {
                crate::features::test_card::prepare_published_pr_with(
                    &context,
                    context.config.clone(),
                    &client,
                    |_, _| panic!("Git não pode começar após lookup recusado"),
                )
                .await
            },
            |_| async { panic!("IA não pode começar após lookup recusado") },
        )
        .await;
        let mut app = TestApp::for_request(&request);
        while let Ok(event) = rx.try_recv() {
            app.on_event(event);
        }
        let TestCardRequest::PublishedPr(context) = &request else {
            unreachable!();
        };
        assert_eq!(app.phase, TestPhase::Erro);
        assert!(
            app.error
                .as_deref()
                .is_some_and(|message| message.contains("403"))
        );
        assert!(app.created.is_none());
        assert!(!app.parent_updated);
        assert_eq!(context.published_pr.id, 99);
        assert_eq!(context.published_pr.target, "dev");
        assert!(context.published_pr.url.ends_with("/99"));
        let receipt = format!(
            "PR #{} · {} · {}",
            context.published_pr.id, context.published_pr.target, context.published_pr.url
        );
        let reported = format!(
            "{}

{}",
            app.error.as_deref().unwrap_or_default(),
            receipt
        );
        assert!(reported.contains("PR #99 · dev"));
        assert!(reported.contains("pullrequest/99"));
        assert!(!reported.contains("criado:"));
        let (method, target) = request_rx.recv().expect("lookup do PR");
        assert_eq!(method, "GET");
        assert!(target.contains("pullRequests/99"));
        server.join().expect("servidor de erro do handoff");
    }

    #[tokio::test]
    async fn published_flow_should_keep_create_and_test_qa_confirmations_separate() {
        let mut app = TestApp::for_request(&published_request());
        app.on_event(TestEvent::Generated {
            prep: Box::new(published_prep()),
            title: "Card do PR".to_owned(),
            body: "## Objetivo\nValidar o PR".to_owned(),
            initial: [
                "project\\QA".to_owned(),
                "qa@example.com".to_owned(),
                "project\\Sprint 12".to_owned(),
                "2".to_owned(),
                "DevOps".to_owned(),
                "Agrotrace".to_owned(),
            ],
        });
        let pr_before_edit = app.prep.as_ref().and_then(|prep| prep.pr_id.clone());
        assert!(app.open_content_edit());
        assert!(matches!(
            handle_content_edit_key(
                &mut app,
                event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            ),
            TestKeyAction::Continue(true)
        ));
        assert_eq!(
            app.prep.as_ref().and_then(|prep| prep.pr_id.clone()),
            pr_before_edit
        );
        app.create_recovery = CreateRecoveryState::Available;
        app.dialog = Some(TestDialog::CreateRecovery(0));
        assert!(matches!(
            handle_create_recovery_key(
                &mut app,
                &mpsc::unbounded_channel().0,
                0,
                event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            ),
            TestKeyAction::Continue(true)
        ));
        assert_eq!(
            app.prep.as_ref().and_then(|prep| prep.pr_id.clone()),
            pr_before_edit
        );
        let retry_prep = app.prep.clone().expect("prep para recovery");
        let retry_settings = app.build_settings().expect("settings para recovery");
        let (failure_tx, mut failure_rx) = mpsc::unbounded_channel();
        backend_create_with(
            retry_prep,
            retry_settings,
            app.title.clone(),
            app.body.clone(),
            failure_tx,
            |_, _, _, _| async {
                Err(crate::error::AppError::Azure {
                    status: 504,
                    message: "gateway timeout".to_owned(),
                })
            },
        )
        .await;
        while let Ok(event) = failure_rx.try_recv() {
            app.on_event(event);
        }
        assert_eq!(app.phase, TestPhase::Revisao);
        assert_eq!(app.dialog, Some(TestDialog::CreateRecovery(0)));
        assert_eq!(
            app.prep.as_ref().and_then(|prep| prep.pr_id.clone()),
            pr_before_edit
        );
        assert_eq!(app.create_recovery, CreateRecoveryState::Available);
        app.create_recovery = CreateRecoveryState::Unavailable;
        app.panel = Panel::Preview;
        assert!(matches!(
            handle_review_preview_key(
                &mut app,
                event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            ),
            TestKeyAction::Continue(true)
        ));
        assert_eq!(app.dialog, Some(TestDialog::ConfirmCreate(false)));
        assert!(app.created.is_none());
        assert!(!app.parent_updated);

        let settings = app.build_settings().expect("settings para criação");
        let prep = app.prep.clone().expect("prep para criação");
        let (tx, mut rx) = mpsc::unbounded_channel();
        backend_create_with(
            prep,
            settings,
            app.title.clone(),
            app.body.clone(),
            tx,
            |_, _, _, _| async {
                Ok(WorkItem {
                    id: 123,
                    fields: std::collections::HashMap::new(),
                    relations: Vec::new(),
                })
            },
        )
        .await;
        while let Ok(event) = rx.try_recv() {
            app.on_event(event);
        }
        assert_eq!(app.dialog, Some(TestDialog::ConfirmTestQa(false)));
        assert!(app.created.is_some());
        assert!(!app.parent_updated);
        assert_eq!(
            app.prep.as_ref().and_then(|prep| prep.pr_id.as_deref()),
            Some("99")
        );

        assert!(matches!(
            handle_confirm_qa_key(
                &mut app,
                false,
                event::KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            ),
            TestKeyAction::Continue(true)
        ));
        assert_eq!(app.dialog, Some(TestDialog::QaEfforts));
        assert!(!app.parent_updated);

        run_qa_update_with(
            &mut app,
            "1".to_owned(),
            "1".to_owned(),
            |prep, effort, real| async move {
                assert_eq!(prep.parent.id, 11763);
                assert_eq!(effort.as_deref(), Some("1"));
                assert_eq!(real.as_deref(), Some("1"));
                Ok(())
            },
        )
        .await
        .expect("update QA");
        assert!(app.parent_updated);
        assert_eq!(
            app.parent_msg.as_deref(),
            Some("pai atualizado p/ Test QA ✓")
        );
    }

    #[test]
    fn editing_test_should_open_content_editor_without_touching_settings() {
        let mut app = review_app();
        let values = ["area", "qa@example.com", "sprint", "2", "team", "program"];
        for (field, value) in app.fields.iter_mut().zip(values) {
            *field = LineEditor::new(value.to_owned());
        }
        assert!(app.open_content_edit());
        let editor = app.content_edit.as_ref().expect("editor aberto");
        assert_eq!(editor.title.value(), "Card exemplo");
        assert_eq!(editor.body.value(), "## Objetivo\nX");
        for (field, value) in app.fields.iter().zip(values) {
            assert_eq!(field.value, value);
        }
    }

    #[test]
    fn saving_valid_content_should_update_test_preview_exactly() {
        let mut app = review_app();
        assert!(app.open_content_edit());
        let editor = app.content_edit.as_mut().expect("editor aberto");
        editor.title = crate::tui::content_editor::TextEditor::new("Título ✅", true);
        editor.body = crate::tui::content_editor::TextEditor::new(
            "  ação concluída\n- [ ] validar\nlinha final  ",
            false,
        );
        assert!(matches!(
            handle_content_edit_key(
                &mut app,
                event::KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            ),
            TestKeyAction::Continue(true)
        ));
        assert!(app.content_edit.is_none());
        assert_eq!(app.title, "Título ✅");
        assert_eq!(app.body, "  ação concluída\n- [ ] validar\nlinha final  ");
        assert_eq!(
            app.copy_body(),
            "  ação concluída\n- [ ] validar\nlinha final  "
        );
    }

    #[test]
    fn invalid_test_content_should_stay_in_editor_without_remote_start() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut empty_title = review_app();
        empty_title.title = " \n".to_owned();
        start_create(&mut empty_title, &tx);
        assert!(empty_title.content_edit.is_some());
        assert_eq!(empty_title.error.as_deref(), Some("título é obrigatório"));
        assert!(rx.try_recv().is_err());

        let mut empty_body = review_app();
        empty_body.body = " \n\t".to_owned();
        start_create(&mut empty_body, &tx);
        assert!(empty_body.content_edit.is_some());
        assert_eq!(
            empty_body.error.as_deref(),
            Some("corpo é obrigatório para criar o Test Case")
        );
    }

    #[test]
    fn canceling_content_edit_should_discard_test_draft() {
        let mut app = review_app();
        assert!(app.open_content_edit());
        app.content_edit
            .as_mut()
            .expect("editor aberto")
            .body
            .insert_text("\ntexto descartado");
        assert!(matches!(
            handle_content_edit_key(
                &mut app,
                event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            ),
            TestKeyAction::Continue(true)
        ));
        assert!(app.content_edit.is_none());
        assert_eq!(app.title, "Card exemplo");
        assert_eq!(app.body, "## Objetivo\nX");
        assert!(app.open_content_edit());
        assert_eq!(
            app.content_edit.as_ref().unwrap().body.value(),
            "## Objetivo\nX"
        );
    }

    #[test]
    fn edited_test_content_should_use_validate_card_without_pr_body_limit() {
        assert!(test_card::validate_card(" ", "body").is_err());
        assert!(test_card::validate_card("Título", " \n\t").is_err());
        assert!(test_card::validate_card("Título", &"x".repeat(4000)).is_ok());
    }

    #[test]
    fn copy_should_use_approved_body_only_and_editor_should_consume_c() {
        let mut app = review_app();
        app.frozen_create_content = Some(PrDescription {
            title: "Título aprovado".to_owned(),
            body: "body aprovado\n- [ ] exato".to_owned(),
        });
        app.body = "body mutável não aprovado".to_owned();
        assert_eq!(app.copy_body(), "body aprovado\n- [ ] exato");

        app.frozen_create_content = None;
        assert!(app.open_content_edit());
        let before = app.content_edit.as_ref().unwrap().title.value().to_owned();
        assert!(matches!(
            handle_content_edit_key(
                &mut app,
                event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
            ),
            TestKeyAction::Continue(true)
        ));
        assert_eq!(
            app.content_edit.as_ref().unwrap().title.value(),
            format!("{before}c")
        );
    }

    #[test]
    fn create_should_freeze_approved_content_and_use_existing_builders() {
        let mut app = review_app();
        let approved = PrDescription {
            title: "Título aprovado ✅".to_owned(),
            body: "## Objetivo\nação concluída\n- [ ] validar".to_owned(),
        };
        app.frozen_create_content = Some(approved.clone());
        app.title = "título mutável".to_owned();
        app.body = "body mutável".to_owned();
        assert_eq!(create_content_for_attempt(&app), approved);
        app.frozen_create_content = None;
        app.phase = TestPhase::Criando;
        assert!(!app.open_content_edit());
        app.phase = TestPhase::Revisao;
        app.create_recovery = CreateRecoveryState::Available;
        assert!(!app.open_content_edit());
        app.create_recovery = CreateRecoveryState::Unavailable;
        app.frozen_create_content = Some(approved.clone());

        let settings = TestSettings {
            area_path: "Proj\\Time".to_owned(),
            assigned_to: String::new(),
            iteration_path: String::new(),
            priority: 2.0,
            team: "QA".to_owned(),
            program: "Agrotrace".to_owned(),
        };
        let input =
            test_card::build_test_case_input(&settings, "org", 7, &approved.title, &approved.body);
        let patch = crate::azure::work_items::build_create_patch(
            &input,
            Some("https://dev.azure.com/org/_apis/wit/workitems/7"),
        );
        assert_eq!(
            patch[0]["value"],
            serde_json::Value::String("Título aprovado ✅".to_owned())
        );
        assert!(
            input
                .description_html
                .as_deref()
                .is_some_and(|html| html.contains("ação concluída"))
        );
        assert!(
            input
                .steps_xml
                .as_deref()
                .is_some_and(|xml| xml.contains("validar"))
        );
    }

    #[test]
    fn publish_and_create_retry_should_reuse_frozen_content_and_exact_title() {
        let mut app = review_app();
        let approved = PrDescription {
            title: "Título da tentativa".to_owned(),
            body: "body da tentativa".to_owned(),
        };
        app.frozen_create_content = Some(approved.clone());
        app.title = "mutação indevida".to_owned();
        app.body = "outra mutação".to_owned();
        assert_eq!(create_content_for_attempt(&app), approved);
        assert_eq!(create_candidate_title(&app), "Título da tentativa");
    }

    #[test]
    fn test_content_editor_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.open_content_edit();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_content_editor_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn test_content_editor_80x24() -> anyhow::Result<()> {
        let mut app = review_app();
        app.open_content_edit();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_content_editor_80x24", terminal.backend());
        Ok(())
    }

    #[test]
    fn qa_efforts_should_reject_empty_and_negative() {
        let app = TestApp::new();
        assert!(app.validate_qa_efforts().is_err());
        let mut app = TestApp::new();
        app.qa_effort = LineEditor::new("-1".to_owned());
        app.qa_real = LineEditor::new("abc".to_owned());
        assert!(app.validate_qa_efforts().is_err());
    }

    #[test]
    fn qa_efforts_should_accept_non_negative_decimals() {
        let mut app = TestApp::new();
        app.qa_effort = LineEditor::new("1,5".to_owned());
        app.qa_real = LineEditor::new("0".to_owned());
        let (effort, real) = app.validate_qa_efforts().expect("esforços válidos");
        assert_eq!(effort, "1,5");
        assert_eq!(real, "0");
    }

    #[test]
    fn dialogs_should_open_and_close() {
        let mut app = review_app();
        assert!(app.dialog.is_none());
        app.dialog = Some(TestDialog::ConfirmCreate(app.create_initial));
        assert_eq!(app.dialog, Some(TestDialog::ConfirmCreate(true)));
        app.dialog = None;
        app.open_qa_efforts();
        assert_eq!(app.dialog, Some(TestDialog::QaEfforts));
        assert_eq!(app.qa_effort.trimmed(), "1");
    }

    #[test]
    fn created_item_should_offer_parent_test_qa_update() {
        let mut app = TestApp::new();
        app.on_event(TestEvent::CreatedItem(WorkItem {
            id: 99,
            fields: std::collections::HashMap::new(),
            relations: Vec::new(),
        }));

        assert_eq!(app.phase, TestPhase::Pronto);
        assert_eq!(app.created.as_ref().map(|(id, _)| *id), Some(99));
        assert_eq!(app.dialog, Some(TestDialog::ConfirmTestQa(false)));
    }

    #[test]
    fn create_failure_should_keep_review_and_focus_settings_field() {
        let mut app = review_app();
        app.fields[4] = LineEditor::new("QA".to_owned());
        app.fields[5] = LineEditor::new("Agrotrace".to_owned());
        app.on_event(TestEvent::CreateFailed(CreateFailure {
            message: "campo Custom.Team: valor inválido".to_owned(),
            kind: CreateFailureKind::Confirmed,
            field: Some(TestSettingsField::Team),
        }));

        assert_eq!(app.phase, TestPhase::Revisao);
        assert_eq!(app.panel, Panel::Settings);
        assert_eq!(app.field_focus, TestSettingsField::Team.index());
        assert_eq!(app.fields[4].value, "QA");
        assert!(app.dialog.is_none());
    }

    #[test]
    fn unknown_create_failure_should_open_recovery_actions() {
        let mut app = review_app();
        app.on_event(TestEvent::CreateFailed(CreateFailure {
            message: "não foi possível confirmar a criação".to_owned(),
            kind: CreateFailureKind::OutcomeUnknown,
            field: None,
        }));

        assert_eq!(app.phase, TestPhase::Revisao);
        assert_eq!(app.dialog, Some(TestDialog::CreateRecovery(0)));
        assert_eq!(app.panel, Panel::Settings);
    }

    #[test]
    fn local_settings_validation_should_focus_matching_field() {
        assert_eq!(
            settings_field_for_error("team é obrigatório"),
            Some(TestSettingsField::Team)
        );
        assert_eq!(
            settings_field_for_error("programa é obrigatório"),
            Some(TestSettingsField::Program)
        );
        assert_eq!(
            settings_field_for_error("responsável: informe um email válido"),
            Some(TestSettingsField::AssignedTo)
        );
        assert_eq!(settings_field_for_error("erro desconhecido"), None);
    }

    #[test]
    fn delete_candidate_requires_explicit_selection_and_confirmation() {
        let mut app = review_app();
        app.dialog = Some(TestDialog::CandidateList { selected: 0 });
        app.candidates.push(TestCaseCandidate {
            id: 77,
            url: "https://dev.azure.com/org/proj/_workitems/edit/77".to_owned(),
            title: "Card exemplo".to_owned(),
            created_at: "2026-09-11T12:00:00Z".to_owned(),
            parent_matches: true,
            matching_fields: 6,
            comparable_fields: 6,
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        let action = handle_candidate_list_key(
            &mut app,
            &tx,
            0,
            event::KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );
        assert!(matches!(action, TestKeyAction::Continue(true)));
        assert_eq!(
            app.dialog,
            Some(TestDialog::DeleteCandidate {
                selected: 0,
                yes: false,
            })
        );

        let action = handle_delete_candidate_key(
            &mut app,
            &tx,
            0,
            false,
            event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(matches!(action, TestKeyAction::Continue(true)));
        assert_eq!(app.dialog, Some(TestDialog::CandidateList { selected: 0 }));
        assert_eq!(app.candidates.len(), 1);
    }

    #[test]
    fn selected_candidate_should_be_adopted_without_recreating_it() {
        let mut app = review_app();
        app.dialog = Some(TestDialog::CandidateList { selected: 0 });
        app.candidates.push(TestCaseCandidate {
            id: 88,
            url: "https://dev.azure.com/org/proj/_workitems/edit/88".to_owned(),
            title: "Card exemplo".to_owned(),
            created_at: String::new(),
            parent_matches: true,
            matching_fields: 6,
            comparable_fields: 6,
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        let action = handle_candidate_list_key(
            &mut app,
            &tx,
            0,
            event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert!(matches!(action, TestKeyAction::Continue(true)));
        assert_eq!(app.phase, TestPhase::Pronto);
        assert_eq!(app.created.as_ref().map(|(id, _)| *id), Some(88));
        assert_eq!(app.dialog, Some(TestDialog::ConfirmTestQa(false)));
        assert!(app.candidates.is_empty());
    }

    #[test]
    fn test_confirm_create_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.dialog = Some(TestDialog::ConfirmCreate(true));
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_confirm_create_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn test_generating_100x30() -> anyhow::Result<()> {
        let mut app = TestApp::new();
        app.phase = TestPhase::Gerando;
        app.phase_label = "gerando card via IA…".to_owned();
        app.progress = 0.42;
        app.streamed_raw = "# Card de teste\n\n## Objetivo\nTexto parcial".to_owned();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_generating_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn test_qa_efforts_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.open_qa_efforts();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_qa_efforts_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn test_create_recovery_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.error = Some(
            "não foi possível confirmar se o Azure criou o Test Case; verifique antes de reenviar"
                .to_owned(),
        );
        app.dialog = Some(TestDialog::CreateRecovery(0));
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_create_recovery_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn test_candidate_list_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.candidates = vec![
            TestCaseCandidate {
                id: 77,
                url: "https://dev.azure.com/org/proj/_workitems/edit/77".to_owned(),
                title: "Card exemplo".to_owned(),
                created_at: "2026-09-11T12:00:00Z".to_owned(),
                parent_matches: true,
                matching_fields: 6,
                comparable_fields: 6,
            },
            TestCaseCandidate {
                id: 78,
                url: "https://dev.azure.com/org/proj/_workitems/edit/78".to_owned(),
                title: "Card exemplo".to_owned(),
                created_at: "2026-09-11T11:00:00Z".to_owned(),
                parent_matches: false,
                matching_fields: 2,
                comparable_fields: 6,
            },
        ];
        app.dialog = Some(TestDialog::CandidateList { selected: 0 });
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_candidate_list_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn test_candidate_lookup_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.candidate_activity = CandidateActivity::Loading;
        app.dialog = Some(TestDialog::CandidateList { selected: 0 });
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_candidate_lookup_100x30", terminal.backend());
        Ok(())
    }
}
