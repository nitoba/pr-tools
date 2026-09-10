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

use super::markdown::{markdown_text, title_line};
use super::shimmer::{shimmer_bar, shimmer_text};
use super::{app_layout, border_type, centered_buttons, modal_frame, spin_frames, theme};
use crate::azure::WorkItem;
use crate::cli::CliOptions;
use crate::features::test_card::{self, TestCardPrep, TestSettings};

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
    /// Falha terminal (prepare/generate/create).
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
    /// Criado (mostra id/URL; `u` atualiza o pai).
    Pronto,
    /// Erro (mostra mensagem até sair).
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

    /// Fase ocupada (spinner + shimmer ativos)?
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
    /// Início (p/ elapsed).
    started_at: Instant,
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
}

impl TestApp {
    /// Estado inicial (antes do primeiro evento).
    fn new() -> Self {
        Self {
            phase: TestPhase::Preparando,
            phase_label: "preparando contexto…".to_owned(),
            started_at: Instant::now(),
            tick: 0,
            scroll: 0,
            progress: 0.0,
            progress_label: "preparando".to_owned(),
            logs: VecDeque::with_capacity(200),
            streamed_raw: String::new(),
            title: String::new(),
            body: String::new(),
            prep: None,
            fields: std::array::from_fn(|_| LineEditor::new(String::new())),
            field_focus: 4,
            panel: Panel::Preview,
            error: None,
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
        }
    }

    /// Avança 1 tick (~33ms).
    fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Segundos decorridos.
    fn elapsed_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Símbolo de atividade: gira só com trabalho; parado no ocioso.
    fn spinner(&self) -> &str {
        if !self.phase.busy() {
            return match self.phase {
                TestPhase::Erro => {
                    if super::ascii_only() {
                        "x"
                    } else {
                        "✘"
                    }
                }
                TestPhase::Pronto => {
                    if super::ascii_only() {
                        "+"
                    } else {
                        "✓"
                    }
                }
                _ => {
                    if super::ascii_only() {
                        "*"
                    } else {
                        "●"
                    }
                }
            };
        }
        let frames = spin_frames();
        let idx = usize::try_from(self.tick).unwrap_or(usize::MAX) % frames.len().max(1);
        match frames.get(idx) {
            Some(f) => f,
            None => "-",
        }
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

    /// Último log (p/ a faixa de progresso).
    fn last_log(&self) -> &str {
        match self.logs.back() {
            Some(l) => l.as_str(),
            None => "",
        }
    }

    /// Corpo atual p/ copiar (final se houver, senão stream parcial).
    fn copy_body(&self) -> String {
        if self.body.is_empty() {
            self.streamed_raw.clone()
        } else {
            self.body.clone()
        }
    }

    /// Aplica evento do backend.
    fn on_event(&mut self, ev: TestEvent) {
        match ev {
            TestEvent::Log(line) => {
                if self.logs.len() >= 200 {
                    let _ = self.logs.pop_front();
                }
                self.logs.push_back(line);
            }
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
            } => {
                for (i, val) in initial.into_iter().enumerate() {
                    if let Some(slot) = self.fields.get_mut(i) {
                        *slot = LineEditor::new(val);
                    }
                }
                self.title = title;
                self.body = body;
                self.prep = Some(*prep);
                self.phase = TestPhase::Revisao;
                "revisão".clone_into(&mut self.phase_label);
                self.progress = 1.0;
                "pronto p/ revisão".clone_into(&mut self.progress_label);
                self.scroll = 0;
                self.panel = Panel::Preview;
                if self.logs.len() >= 200 {
                    let _ = self.logs.pop_front();
                }
                self.logs.push_back(
                    "card pronto — revise, ajuste settings (tab) e crie (enter)".to_owned(),
                );
            }
            TestEvent::CreatedItem(item) => {
                let id = item.id;
                let url = match self.prep.as_ref() {
                    Some(p) => test_case_url(p, id),
                    None => format!("workitem:{id}"),
                };
                self.created = Some((id, url));
                self.phase = TestPhase::Pronto;
                "test case criado".clone_into(&mut self.phase_label);
                self.progress = 1.0;
                "criado".clone_into(&mut self.progress_label);
                if self.logs.len() >= 200 {
                    let _ = self.logs.pop_front();
                }
                self.logs.push_back(format!("criado #{id}"));
            }
            TestEvent::Failed(msg) => {
                self.phase = TestPhase::Erro;
                "erro".clone_into(&mut self.phase_label);
                self.error = Some(msg.clone());
                if self.logs.len() >= 200 {
                    let _ = self.logs.pop_front();
                }
                self.logs.push_back(format!("erro: {msg}"));
            }
        }
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
        let (effort, real) = match self.prep.as_ref() {
            Some(p) => parent_effort_defaults(&p.parent),
            None => ("1".to_owned(), "1".to_owned()),
        };
        self.qa_effort = LineEditor::new(effort);
        self.qa_real = LineEditor::new(real);
        self.qa_focus = 0;
        self.dialog = Some(TestDialog::QaEfforts);
        self.error = None;
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
async fn backend_prepare_generate(options: CliOptions, tx: mpsc::UnboundedSender<TestEvent>) {
    let _ = tx.send(TestEvent::PhaseLabel("preparando contexto…".to_owned()));
    let _ = tx.send(TestEvent::Progress(0.05, "coletando git/pr/pai".to_owned()));
    let _ = tx.send(TestEvent::Log(
        "lendo config, git e work item pai…".to_owned(),
    ));
    let prep = match test_card::prepare(&options).await {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(TestEvent::Failed(e.to_string()));
            return;
        }
    };
    let _ = tx.send(TestEvent::Log(format!("pai #{} resolvido", prep.parent.id)));
    let _ = tx.send(TestEvent::PhaseLabel("gerando card via IA…".to_owned()));
    let _ = tx.send(TestEvent::Progress(0.3, "chamando provider".to_owned()));
    let desc = match test_card::generate(&prep).await {
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
    let initial = initial_field_values(&options, &prep);
    let _ = tx.send(TestEvent::Progress(1.0, "pronto p/ revisão".to_owned()));
    let _ = tx.send(TestEvent::Generated {
        prep: Box::new(prep),
        title: desc.title,
        body: desc.body,
        initial,
    });
}

/// Valores iniciais dos 6 campos (CLI > config; iteração herdada do pai).
fn initial_field_values(options: &CliOptions, prep: &TestCardPrep) -> [String; 6] {
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
    let _ = tx.send(TestEvent::PhaseLabel("criando test case…".to_owned()));
    let _ = tx.send(TestEvent::Progress(0.5, "enviando ao azure".to_owned()));
    match test_card::create(&prep, &settings, title.as_str(), body.as_str()).await {
        Ok(item) => {
            let _ = tx.send(TestEvent::Progress(1.0, "criado".to_owned()));
            let _ = tx.send(TestEvent::CreatedItem(item));
        }
        Err(e) => {
            let _ = tx.send(TestEvent::Failed(e.to_string()));
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
        let [head, body, foot] = app_layout(area);
        Block::new().style(theme().root).render(area, buf);
        render_header(self, head, buf);
        render_body(self, body, buf);
        render_footer(self, foot, buf);
        if let Some(dialog) = &self.dialog {
            render_dialog(self, *dialog, area, buf);
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

/// Header com spinner, fase e elapsed.
fn render_header(app: &TestApp, area: Rect, buf: &mut Buffer) {
    // Parado no ocioso: cor fixa em vez de pulsar à toa.
    let pulse = if app.phase.busy() {
        if app.tick % 2 == 0 {
            theme().accent
        } else {
            theme().app_title
        }
    } else {
        theme().accent
    };
    let phase_line = if app.phase.busy() {
        shimmer_text(app.phase_label.as_str(), app.tick, 28)
    } else {
        Line::from(Span::styled(app.phase_label.clone(), theme().accent))
    };
    let chars = if app.body.is_empty() {
        app.streamed_raw.len()
    } else {
        app.body.len()
    };
    let mut spans = vec![
        Span::styled(format!("{} ", app.spinner()), pulse),
        Span::styled("◆ prt ", theme().app_title),
        Span::styled(crate::cli::VERSION, theme().muted),
        Span::styled(
            format!(
                "  ·  test  ·  {}  ·  {}s  ·  {chars} chars",
                app.phase.short(),
                app.elapsed_secs()
            ),
            theme().muted,
        ),
        Span::styled("  ·  ", theme().muted),
    ];
    spans.extend(phase_line.spans);
    Paragraph::new(Line::from(spans)).render(area, buf);
}

/// Corpo: faixa de progresso + colunas preview/settings.
fn render_body(app: &TestApp, area: Rect, buf: &mut Buffer) {
    let [prog_area, cols] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);
    render_progress(app, prog_area, buf);
    if cols.height == 0 || cols.width == 0 {
        return;
    }
    if cols.width < 100 {
        let half = cols.height / 2;
        let top = Rect {
            x: cols.x,
            y: cols.y,
            width: cols.width,
            height: half,
        };
        let bottom = Rect {
            x: cols.x,
            y: cols.y.saturating_add(half),
            width: cols.width,
            height: cols.height.saturating_sub(half),
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
    .areas(cols);
    render_preview(app, left, buf);
    render_settings(app, right, buf);
}

/// Faixa de progresso com shimmer + último log.
fn render_progress(app: &TestApp, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let bar_width = area.width as usize;
    let bar = shimmer_bar(app.progress, bar_width, app.tick, app.phase.busy());
    // `progress` vive em 0.0–1.0 (com clamp em `on_event`); trunca a casa
    // decimal por ops de float (`as u16` truncava igual) e formata sem `as`.
    let pct = (app.progress * 100.0).trunc().clamp(0.0, 100.0);
    if area.height >= 1 {
        Paragraph::new(bar).render(
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            },
            buf,
        );
    }
    if area.height >= 2 {
        let label = if app.phase.busy() {
            shimmer_text(app.progress_label.as_str(), app.tick, 24)
        } else {
            Line::from(Span::styled(app.progress_label.clone(), theme().success))
        };
        let mut spans = label.spans.clone();
        spans.push(Span::styled(format!("  {pct:.0}%"), theme().muted));
        Paragraph::new(Line::from(spans)).render(
            Rect {
                x: area.x,
                y: area.y.saturating_add(1),
                width: area.width,
                height: 1,
            },
            buf,
        );
    }
    if area.height >= 3 {
        Paragraph::new(app.last_log().to_owned())
            .style(theme().muted)
            .render(
                Rect {
                    x: area.x,
                    y: area.y.saturating_add(2),
                    width: area.width,
                    height: 1,
                },
                buf,
            );
    }
}

/// Preview do card (Markdown final ou stream parcial).
fn render_preview(app: &TestApp, area: Rect, buf: &mut Buffer) {
    let ready = !app.title.is_empty() || !app.body.is_empty();
    let title = if ready {
        " ◉ Card de teste "
    } else {
        " ◌ Gerando… "
    };
    let border = if ready {
        theme().border
    } else {
        theme().warning
    };
    let block = Block::default()
        .title(Span::styled(
            title,
            if ready {
                theme().success
            } else {
                theme().warning
            },
        ))
        .borders(Borders::ALL)
        .border_style(border)
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
            "aguardando a IA…".to_owned()
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
    let mut lines: Vec<Line> = Vec::new();
    for (i, slot) in app.fields.iter().enumerate() {
        let focused = focused_panel && app.field_focus % FIELD_COUNT == i;
        let label = field_label(i);
        let marker = if focused { "▸ " } else { "  " };
        let label_style = if focused {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            theme().muted
        };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if focused {
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
    if let Some(e) = app.error.as_ref() {
        lines.push(Line::from(vec![
            Span::styled("✘ ", theme().error),
            Span::styled(e.clone(), theme().error),
        ]));
    }
    if let Some(m) = app.parent_msg.as_ref() {
        lines.push(Line::from(Span::styled(m.clone(), theme().muted)));
    }
    if app.is_copied_flash() {
        lines.push(Line::from(Span::styled("✓ copiado!", theme().success)));
    }
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((0, 0))
        .render(inner, buf);
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
    }
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
        lines.push(Line::from(vec![
            Span::styled("▸ [ enter ] atualizar pai p/ Test QA", theme().accent),
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
    let hints = match &app.dialog {
        Some(TestDialog::ConfirmCreate(_) | TestDialog::ConfirmTestQa(_)) => {
            "←/→ alternar · y sim · n não · enter confirmar · esc voltar"
        }
        Some(TestDialog::QaEfforts) => {
            "digite o número · tab troca campo · enter confirma · esc volta"
        }
        None => match app.phase {
            TestPhase::Preparando | TestPhase::Gerando => "j/k rolar preview · q/esc abortar",
            TestPhase::Criando => "criando… aguarde · q/esc abortar",
            TestPhase::Revisao => match app.panel {
                Panel::Preview => "tab settings · enter continuar · c copia · j/k rola · q sai",
                Panel::Settings => {
                    "digite p/ editar · ↑/↓ campo · tab preview · enter continuar · esc volta"
                }
            },
            TestPhase::Pronto => "enter Test QA · c copia · outra tecla sai",
            TestPhase::Erro => "qualquer tecla sai",
        },
    };
    let mut spans = vec![Span::styled(hints, theme().muted)];
    if app.is_copied_flash() {
        spans.push(Span::styled("   ✓ copiado!", theme().success));
    }
    if let Some(e) = app.error.as_ref() {
        if app.phase == TestPhase::Revisao {
            spans.push(Span::styled(format!("   ✘ {e}"), theme().error));
        }
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
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("tui requer terminal interativo");
    }
    let mut terminal: DefaultTerminal = ratatui::init();
    let res = run_loop(&mut terminal, options).await;
    ratatui::restore();
    res
}

/// Loop principal: drena backend, ticka a ~30fps e trata teclas.
async fn run_loop(
    terminal: &mut DefaultTerminal,
    options: &CliOptions,
) -> anyhow::Result<TestFlowOutcome> {
    let (tx, mut rx) = mpsc::unbounded_channel::<TestEvent>();
    let mut app = TestApp::new();
    let owned = options.clone();
    app.create_initial = owned.create;
    app.no_create = owned.no_create;
    let first_tx = tx.clone();
    tokio::spawn(backend_prepare_generate(owned, first_tx));

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
        if event::poll(Duration::from_millis(10))? {
            let ev = event::read()?;
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
                continue;
            }
            let action = handle_test_phase_key(&mut app, key);
            if let Some(res) = apply_test_key_action(action, &mut needs_draw) {
                return res;
            }
        }
    }
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
    is_scroll || (app.phase == TestPhase::Revisao && app.panel == Panel::Settings && is_edit)
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
                Ok(TestKeyAction::Continue(true))
            } else {
                Ok(TestKeyAction::Continue(false))
            }
        }
    }
}

/// Despacha a tecla (fora de diálogo) conforme a fase atual.
fn handle_test_phase_key(app: &mut TestApp, key: event::KeyEvent) -> TestKeyAction {
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
                app.panel = Panel::Preview;
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
    match app.build_settings() {
        Ok(settings) => {
            if let Some(prep) = app.prep.clone() {
                let title = app.title.clone();
                let body = app.body.clone();
                app.phase = TestPhase::Criando;
                "criando test case…".clone_into(&mut app.phase_label);
                app.progress = 0.3;
                "enviando…".clone_into(&mut app.progress_label);
                app.error = None;
                let tx2 = tx.clone();
                tokio::spawn(backend_create(prep, settings, title, body, tx2));
            } else {
                app.error = Some("sem contexto p/ criar (prepare falhou)".to_owned());
            }
        }
        Err(msg) => {
            app.error = Some(msg);
        }
    }
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
    match app.prep.clone() {
        Some(prep) => {
            match test_card::update_parent(&prep, Some(effort.as_str()), Some(real.as_str())).await
            {
                Ok(()) => {
                    app.parent_updated = true;
                    app.parent_msg = Some("pai atualizado p/ Test QA ✓".to_owned());
                }
                Err(e) => {
                    app.parent_msg = Some(format!("falha ao atualizar pai: {e}"));
                }
            }
        }
        None => {
            app.parent_msg = Some("sem contexto do pai p/ atualizar".to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn review_app() -> TestApp {
        let mut app = TestApp::new();
        app.title = "Card exemplo".to_owned();
        app.body = "## Objetivo\nX".to_owned();
        app.phase = TestPhase::Revisao;
        app.create_initial = true;
        app
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
    fn test_qa_efforts_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.open_qa_efforts();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("test_qa_efforts_100x30", terminal.backend());
        Ok(())
    }
}
