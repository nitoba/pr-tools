//! TUI da jornada `prt desc --pr <id>`.
//!
//! O fluxo de update tem estado próprio para não compartilhar a confirmação
//! nem o publisher multi-target do fluxo de criação.

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use tokio::sync::mpsc;

use super::content_editor::{
    ContentEditAction, ContentEditState, ContentField, render_content_editor,
};
use super::shimmer::u16_from_i32_clamped;
use super::{
    StatusHeader, ascii_only, border_type, modal_frame, status_header, status_layout, theme,
};
use crate::ai::PrDescription;
use crate::azure::pull_requests::PullRequest;
use crate::features::update_pull_request::{
    self, AzureUpdateGateway, UpdateOutcome, UpdatePrep, execute_update,
};

/// Fase visual da atualização de PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdatePhase {
    /// Preparando a leitura inicial.
    Reading,
    /// Gerando a proposta via IA.
    #[default]
    Generating,
    /// Proposta pronta para revisão.
    Review,
    /// Conteúdo aprovado, aguardando a releitura pré-escrita.
    Confirming,
    /// PATCH/reconciliação em andamento.
    Updating,
    /// Snapshot mudou e exige nova revisão.
    Conflict,
    /// Resultado remoto ainda não confirmado.
    Unknown,
    /// Atualização confirmada ou no-op.
    Done,
    /// Falha terminal.
    Error,
}

/// Resultado que o `main` apresenta depois que a TUI termina.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateTuiOutcome {
    /// Título e descrição foram confirmados por GET.
    Updated {
        /// ID do PR atualizado.
        id: i64,
    },
    /// O PR já tinha exatamente o conteúdo aprovado.
    NoOp {
        /// ID do PR que não precisou de PATCH.
        id: i64,
    },
    /// Usuário saiu antes da confirmação.
    Aborted,
    /// Falha exibida na tela.
    Failed(String),
}

#[derive(Debug)]
enum UpdateBackendEvent {
    Proposal(Result<PrDescription, String>),
    Outcome(Result<UpdateOutcome, String>),
}

/// Estado testável da revisão de update.
#[derive(Debug)]
pub struct UpdateApp {
    /// ID do PR.
    pub pr_id: i64,
    /// Snapshot remoto atual; durante conflito vira o novo estado remoto.
    pub current: PullRequest,
    /// Proposta gerada/editada.
    pub proposal: Option<PrDescription>,
    /// Draft temporário que edita somente a proposta.
    pub content_edit: Option<ContentEditState>,
    /// Conteúdo congelado antes da primeira chamada remota de escrita.
    pub frozen_content: Option<PrDescription>,
    /// Fase atual.
    pub phase: UpdatePhase,
    /// Texto da fase.
    pub phase_label: String,
    /// Progresso visual.
    pub progress: f64,
    /// Frame do status.
    pub tick: u64,
    /// Scroll da revisão.
    pub scroll: u16,
    /// Mensagem de erro/conflito.
    pub error: Option<String>,
    /// Resultado remoto final.
    pub outcome: Option<UpdateOutcome>,
    /// Ajuda modal.
    pub show_help: bool,
}

impl UpdateApp {
    /// Cria o app usando o snapshot remoto inicial.
    #[must_use]
    pub fn new(pr_id: i64, current: &PullRequest) -> Self {
        Self {
            pr_id,
            current: current.clone(),
            proposal: None,
            content_edit: None,
            frozen_content: None,
            phase: UpdatePhase::Generating,
            phase_label: "gerando proposta…".to_owned(),
            progress: 0.1,
            tick: 0,
            scroll: 0,
            error: None,
            outcome: None,
            show_help: false,
        }
    }

    /// Avança o frame da animação.
    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Recebe a proposta gerada ou a falha do provider.
    pub fn on_proposal(&mut self, result: Result<PrDescription, String>) {
        match result {
            Ok(proposal) => {
                self.proposal = Some(proposal);
                self.phase = UpdatePhase::Review;
                self.set_phase_label("revisão");
                self.progress = 1.0;
                self.error = None;
            }
            Err(error) => self.fail(error),
        }
    }

    /// Recebe o resultado da operação remota.
    pub fn on_outcome(&mut self, result: Result<UpdateOutcome, String>) {
        match result {
            Ok(outcome @ UpdateOutcome::NoOp) => {
                self.outcome = Some(outcome);
                self.phase = UpdatePhase::Done;
                self.set_phase_label("no-op confirmado");
                self.progress = 1.0;
                self.content_edit = None;
                self.error = None;
            }
            Ok(outcome @ UpdateOutcome::Updated { .. }) => {
                if let UpdateOutcome::Updated { remote } = &outcome {
                    self.current = remote.clone();
                }
                self.outcome = Some(outcome);
                self.phase = UpdatePhase::Done;
                self.set_phase_label("atualizado e confirmado");
                self.progress = 1.0;
                self.content_edit = None;
                self.error = None;
            }
            Ok(UpdateOutcome::Conflict { remote, reason }) => {
                self.current = remote;
                self.outcome = None;
                self.frozen_content = None;
                self.phase = UpdatePhase::Conflict;
                self.set_phase_label("conflito remoto");
                self.error = Some(reason);
                self.content_edit = None;
            }
            Ok(UpdateOutcome::Unknown { reason }) => {
                self.outcome = None;
                self.phase = UpdatePhase::Unknown;
                self.set_phase_label("resultado incerto");
                self.error = Some(reason);
                self.content_edit = None;
            }
            Err(error) => self.fail(error),
        }
    }

    /// Abre edição somente quando a fase é revisão.
    pub fn open_content_edit(&mut self) -> bool {
        if self.phase != UpdatePhase::Review
            || self.content_edit.is_some()
            || self.frozen_content.is_some()
        {
            return false;
        }
        let Some(proposal) = self.proposal.as_ref() else {
            return false;
        };
        self.content_edit = Some(ContentEditState::for_pr(proposal));
        true
    }

    /// Salva o draft exato na proposta ou cancela sem modificar o snapshot.
    pub fn handle_content_key(&mut self, key: KeyEvent) -> bool {
        let Some(editor) = self.content_edit.as_mut() else {
            return false;
        };
        match editor.handle_key(key) {
            ContentEditAction::Saved(content) => {
                self.proposal = Some(content);
                self.content_edit = None;
                self.error = None;
                self.scroll = 0;
                true
            }
            ContentEditAction::Cancelled => {
                self.content_edit = None;
                self.error = None;
                true
            }
            ContentEditAction::Consumed => true,
            ContentEditAction::Ignored => false,
        }
    }

    /// Congela a proposta aprovada e entra na fase de confirmação.
    ///
    /// Em conteúdo inválido, mantém o editor aberto e devolve `None`.
    pub fn confirm(&mut self) -> Option<PrDescription> {
        if self.phase != UpdatePhase::Review || self.content_edit.is_some() {
            return None;
        }
        let proposal = self.proposal.clone()?;
        let mut validation = ContentEditState::for_pr(&proposal);
        if let Err(error) = validation.validate() {
            validation.set_error(error.clone());
            self.content_edit = Some(validation);
            self.error = Some(error.to_string());
            return None;
        }
        self.frozen_content = Some(proposal.clone());
        self.phase = UpdatePhase::Confirming;
        self.set_phase_label("confirmando snapshot remoto…");
        self.error = None;
        Some(proposal)
    }

    /// Congela a proposta antes do início da operação remota.
    ///
    /// A chamada remota só deve ser agendada pelo chamador depois que este
    /// método devolver o conteúdo congelado e a fase `Confirming` tiver sido
    /// observada.
    pub fn begin_update(&mut self) -> Option<PrDescription> {
        self.confirm()
    }

    /// Marca a proposta já congelada como pronta para a operação remota.
    pub fn mark_updating(&mut self) {
        if self.phase != UpdatePhase::Confirming || self.frozen_content.is_none() {
            return;
        }
        self.phase = UpdatePhase::Updating;
        self.set_phase_label("relendo e atualizando…");
    }

    /// Retorna do conflito para uma nova revisão do snapshot atualizado.
    pub fn revisit_conflict(&mut self) -> bool {
        if self.phase != UpdatePhase::Conflict {
            return false;
        }
        self.phase = UpdatePhase::Review;
        self.set_phase_label("revisão após conflito");
        self.error = None;
        true
    }

    /// Move o scroll do preview.
    pub fn scroll_by(&mut self, delta: i16) {
        self.scroll = u16_from_i32_clamped(i32::from(self.scroll) + i32::from(delta), 5000);
    }

    fn set_phase_label(&mut self, label: &str) {
        label.clone_into(&mut self.phase_label);
    }

    /// Texto usado no painel remoto atual.
    #[must_use]
    pub fn current_text(&self) -> String {
        format!("# {}\n\n{}", self.current.title, self.current.description)
    }

    /// Texto usado no painel da proposta.
    #[must_use]
    pub fn proposal_text(&self) -> String {
        self.proposal.as_ref().map_or_else(
            || "aguardando proposta…".to_owned(),
            |proposal| format!("# {}\n\n{}", proposal.title, proposal.body),
        )
    }

    fn fail(&mut self, error: String) {
        self.phase = UpdatePhase::Error;
        self.set_phase_label("erro");
        self.error = Some(error);
        self.content_edit = None;
    }
}

impl Widget for &UpdateApp {
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
        if self.show_help {
            render_help(area, buf);
        }
        if let Some(editor) = &self.content_edit {
            render_content_editor(editor, area, buf);
        }
    }
}

fn render_too_small(area: Rect, buf: &mut Buffer) {
    Block::new().style(theme().root).render(area, buf);
    if area.width == 0 || area.height == 0 {
        return;
    }
    let message = if ascii_only() {
        format!(
            "terminal muito pequeno - mínimo 60x20 (atual {}x{})",
            area.width, area.height
        )
    } else {
        format!(
            "terminal muito pequeno — mínimo 60×20 (atual {}×{})",
            area.width, area.height
        )
    };
    let width = area.width.saturating_sub(2).min(56);
    let height = area.height.min(5);
    let popup = Rect {
        x: area.x.saturating_add(area.width.saturating_sub(width) / 2),
        y: area
            .y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    };
    let block = Block::default()
        .title(Span::styled(" Update ", theme().warning))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(theme().warning);
    let inner = block.inner(popup);
    block.render(popup, buf);
    Paragraph::new(message)
        .style(theme().muted)
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_header(app: &UpdateApp, area: Rect, buf: &mut Buffer) {
    let active = matches!(
        app.phase,
        UpdatePhase::Reading
            | UpdatePhase::Generating
            | UpdatePhase::Confirming
            | UpdatePhase::Updating
    );
    let message = match app.phase {
        UpdatePhase::Review => "proposta pronta — conteúdo atual preservado",
        UpdatePhase::Conflict => "o PR mudou; nova revisão necessária",
        UpdatePhase::Unknown => "verifique o estado remoto antes de repetir",
        UpdatePhase::Done => "GET confirmatório concluído",
        UpdatePhase::Error => "consulte os detalhes abaixo",
        _ => app.phase_label.as_str(),
    };
    status_header(
        area,
        buf,
        StatusHeader {
            command: "desc --pr",
            phase: phase_copy(app.phase),
            message,
            progress: Some(app.progress),
            tick: app.tick,
            active,
            style: phase_style(app.phase),
        },
    );
}

fn render_body(app: &UpdateApp, area: Rect, buf: &mut Buffer) {
    match app.phase {
        UpdatePhase::Review
        | UpdatePhase::Conflict
        | UpdatePhase::Confirming
        | UpdatePhase::Updating => {
            render_review(app, area, buf);
        }
        UpdatePhase::Generating | UpdatePhase::Reading => {
            render_message(
                area,
                "Gerando proposta a partir das refs exatas do PR…",
                theme().accent,
                buf,
            );
        }
        UpdatePhase::Done => {
            render_message(
                area,
                if matches!(app.outcome, Some(UpdateOutcome::NoOp)) {
                    "Nenhuma escrita foi necessária: o PR já estava atualizado."
                } else {
                    "Título e descrição foram atualizados e confirmados por GET."
                },
                theme().success,
                buf,
            );
        }
        UpdatePhase::Unknown | UpdatePhase::Error => render_error(app, area, buf),
    }
}

fn render_review(app: &UpdateApp, area: Rect, buf: &mut Buffer) {
    let [current, proposal] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
    render_panel(
        current,
        " Atual ",
        &app.current_text(),
        theme().muted,
        app.scroll,
        buf,
    );
    let proposal_title = if app.content_edit.is_some() {
        " Proposta · editor ".to_owned()
    } else {
        " Proposta ".to_owned()
    };
    render_panel(
        proposal,
        &proposal_title,
        &app.proposal_text(),
        theme().accent,
        app.scroll,
        buf,
    );
    if let Some(error) = &app.error {
        let error_area = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(area.height.saturating_sub(2)),
            width: area.width.saturating_sub(2),
            height: 2.min(area.height),
        };
        Paragraph::new(Line::from(Span::styled(
            format!("✘ {error}"),
            theme().error,
        )))
        .wrap(Wrap { trim: false })
        .render(error_area, buf);
    }
}

fn render_panel(
    area: Rect,
    title: &str,
    content: &str,
    style: Style,
    scroll: u16,
    buf: &mut Buffer,
) {
    let block = Block::default()
        .title(Span::styled(title.to_owned(), style))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(style)
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    Paragraph::new(content.to_owned())
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .render(inner, buf);
}

fn render_message(area: Rect, message: &str, style: Style, buf: &mut Buffer) {
    let block = Block::default()
        .title(Span::styled(" PR existente ", style))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(style);
    let inner = block.inner(area);
    block.render(area, buf);
    Paragraph::new(message)
        .style(theme().muted)
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_error(app: &UpdateApp, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .title(Span::styled(" Resultado da operação ", theme().error))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(theme().error);
    let inner = block.inner(area);
    block.render(area, buf);
    let message = app.error.as_deref().unwrap_or("falha sem mensagem");
    Paragraph::new(vec![
        Line::from(Span::styled(format!("✘ {message}"), theme().error)),
        Line::from(Span::styled("q sai · ? ajuda", theme().muted)),
    ])
    .wrap(Wrap { trim: false })
    .render(inner, buf);
}

fn render_footer(app: &UpdateApp, area: Rect, buf: &mut Buffer) {
    let text = if app.content_edit.is_some() {
        "Tab alterna título/corpo · Ctrl+S salva · Esc cancela"
    } else {
        match app.phase {
            UpdatePhase::Review => {
                "e editar · Enter confirmar · q/Esc cancelar · j/k rolar · ? ajuda"
            }
            UpdatePhase::Conflict => "r revisar novamente · q/Esc sair · ? ajuda",
            UpdatePhase::Done | UpdatePhase::Unknown | UpdatePhase::Error => "q/Esc sair · ? ajuda",
            _ => "aguarde · q/Esc sair",
        }
    };
    Paragraph::new(Line::from(Span::styled(text, theme().muted))).render(area, buf);
}

fn render_help(area: Rect, buf: &mut Buffer) {
    let inner = modal_frame(area, buf, " Ajuda · PR existente ", theme().accent, 70, 8);
    Paragraph::new(vec![
        Line::from(Span::styled("Atual", theme().muted)),
        Line::from("snapshot remoto congelado até a confirmação"),
        Line::from(Span::styled("Proposta", theme().accent)),
        Line::from("resultado da IA, editável sem alterar Atual"),
        Line::from("e editar · Enter confirmar · Ctrl+S salvar · Esc cancelar"),
        Line::from("q sair · ? fechar ajuda"),
    ])
    .wrap(Wrap { trim: false })
    .render(inner, buf);
}

fn phase_copy(phase: UpdatePhase) -> &'static str {
    match phase {
        UpdatePhase::Reading => "Leitura",
        UpdatePhase::Generating => "Geração via IA",
        UpdatePhase::Review => "Revisão",
        UpdatePhase::Confirming => "Reconciliação prévia",
        UpdatePhase::Updating => "Atualização",
        UpdatePhase::Conflict => "Conflito",
        UpdatePhase::Unknown => "Resultado incerto",
        UpdatePhase::Done => "Concluído",
        UpdatePhase::Error => "Erro",
    }
}

fn phase_style(phase: UpdatePhase) -> Style {
    match phase {
        UpdatePhase::Done | UpdatePhase::Review => theme().success,
        UpdatePhase::Error | UpdatePhase::Conflict | UpdatePhase::Unknown => theme().error,
        _ => theme().accent,
    }
}

fn handle_key(
    app: &mut UpdateApp,
    key: KeyEvent,
    gateway: &AzureUpdateGateway,
    tx: &mpsc::UnboundedSender<UpdateBackendEvent>,
) -> Option<UpdateTuiOutcome> {
    if app.content_edit.is_some() {
        app.handle_content_key(key);
        return None;
    }
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key.code == KeyCode::Char('?') && key.modifiers.is_empty() {
        app.show_help = !app.show_help;
        return None;
    }
    if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        && !matches!(app.phase, UpdatePhase::Confirming | UpdatePhase::Updating)
    {
        if app.show_help {
            app.show_help = false;
            return None;
        }
        return Some(quit_outcome(app));
    }
    if matches!(app.phase, UpdatePhase::Confirming | UpdatePhase::Updating) {
        return None;
    }
    match key.code {
        KeyCode::Char('e') if app.phase == UpdatePhase::Review => {
            app.open_content_edit();
        }
        KeyCode::Char('r') if app.phase == UpdatePhase::Conflict => {
            app.revisit_conflict();
        }
        KeyCode::Enter if app.phase == UpdatePhase::Review => {
            if let Some(approved) = app.begin_update() {
                app.mark_updating();
                let gateway = gateway.clone();
                let initial = app.current.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let result = execute_update(&gateway, &initial, &approved)
                        .await
                        .map_err(|error| error.to_string());
                    let _ = tx.send(UpdateBackendEvent::Outcome(result));
                });
            }
        }
        KeyCode::Char('j') | KeyCode::Down => app.scroll_by(3),
        KeyCode::Char('k') | KeyCode::Up => app.scroll_by(-3),
        _ => {}
    }
    None
}

fn quit_outcome(app: &UpdateApp) -> UpdateTuiOutcome {
    match &app.outcome {
        Some(UpdateOutcome::NoOp) => UpdateTuiOutcome::NoOp { id: app.pr_id },
        Some(UpdateOutcome::Updated { .. }) => UpdateTuiOutcome::Updated { id: app.pr_id },
        _ if matches!(
            app.phase,
            UpdatePhase::Review | UpdatePhase::Generating | UpdatePhase::Reading
        ) =>
        {
            UpdateTuiOutcome::Aborted
        }
        _ => UpdateTuiOutcome::Failed(
            app.error
                .clone()
                .unwrap_or_else(|| "operação não confirmada".to_owned()),
        ),
    }
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    mut app: UpdateApp,
    gateway: &AzureUpdateGateway,
    mut rx: mpsc::UnboundedReceiver<UpdateBackendEvent>,
    tx: &mpsc::UnboundedSender<UpdateBackendEvent>,
) -> anyhow::Result<UpdateTuiOutcome> {
    let tick_rate = Duration::from_millis(33);
    let mut last_tick = std::time::Instant::now();
    let mut needs_draw = true;
    loop {
        while let Ok(event) = rx.try_recv() {
            match event {
                UpdateBackendEvent::Proposal(result) => app.on_proposal(result),
                UpdateBackendEvent::Outcome(result) => app.on_outcome(result),
            }
            needs_draw = true;
        }
        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = std::time::Instant::now();
            needs_draw = true;
        }
        if needs_draw {
            terminal.draw(|frame| frame.render_widget(&app, frame.area()))?;
            needs_draw = false;
        }
        if event::poll(Duration::from_millis(10))? {
            match event::read()? {
                Event::Paste(text) if app.content_edit.is_some() => {
                    if let Some(editor) = app.content_edit.as_mut() {
                        match editor.field {
                            ContentField::Title => editor.title.insert_text(&text),
                            ContentField::Body => editor.body.insert_text(&text),
                        }
                        editor.error = None;
                        needs_draw = true;
                    }
                }
                Event::Key(key) => {
                    let is_editor_key = app.content_edit.is_some()
                        && matches!(
                            key.code,
                            KeyCode::Char(_)
                                | KeyCode::Backspace
                                | KeyCode::Delete
                                | KeyCode::Left
                                | KeyCode::Right
                                | KeyCode::Up
                                | KeyCode::Down
                                | KeyCode::Home
                                | KeyCode::End
                                | KeyCode::Enter
                                | KeyCode::Tab
                                | KeyCode::BackTab
                                | KeyCode::PageUp
                                | KeyCode::PageDown
                                | KeyCode::Esc
                        );
                    if key.kind != KeyEventKind::Release
                        && (key.kind != KeyEventKind::Repeat || is_editor_key)
                    {
                        if let Some(outcome) = handle_key(&mut app, key, gateway, tx) {
                            return Ok(outcome);
                        }
                        needs_draw = true;
                    }
                }
                Event::Resize(..) => needs_draw = true,
                _ => {}
            }
        }
    }
}

/// Executa a jornada interativa de update.
///
/// # Errors
///
/// Retorna erro se a TUI, autenticação ou preparação do gateway não puder ser
/// inicializada.
pub fn run_update_tui(prep: &UpdatePrep) -> anyhow::Result<UpdateTuiOutcome> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        anyhow::bail!("atualização de PR requer terminal interativo");
    }
    let gateway = update_pull_request::gateway_for(prep).map_err(anyhow::Error::new)?;
    let (tx, rx) = mpsc::unbounded_channel();
    let generation_tx = tx.clone();
    let generation_prep = prep.clone();
    tokio::spawn(async move {
        let result = update_pull_request::generate(&generation_prep)
            .await
            .map_err(|error| error.to_string());
        let _ = generation_tx.send(UpdateBackendEvent::Proposal(result));
    });
    let mut terminal: DefaultTerminal = ratatui::init();
    let result = run_loop(
        &mut terminal,
        UpdateApp::new(prep.pr_id, &prep.current),
        &gateway,
        rx,
        &tx,
    );
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::azure::pull_requests::{PullRequestProject, PullRequestRepository};
    use ratatui::{Terminal, backend::TestBackend};

    fn pull_request() -> PullRequest {
        PullRequest {
            pull_request_id: 42,
            title: "Título atual".to_owned(),
            description: "Descrição atual\n- nota humana  ".to_owned(),
            source_ref_name: "refs/heads/feature/42".to_owned(),
            target_ref_name: "refs/heads/dev".to_owned(),
            status: "active".to_owned(),
            repository: PullRequestRepository {
                id: "repo-id".to_owned(),
                name: "repo".to_owned(),
                project: PullRequestProject {
                    name: "project".to_owned(),
                },
            },
        }
    }

    fn review_app() -> UpdateApp {
        let mut app = UpdateApp::new(42, &pull_request());
        app.on_proposal(Ok(PrDescription {
            title: "Título proposto".to_owned(),
            body: "Body proposto\n- [ ] validar".to_owned(),
        }));
        app
    }

    #[test]
    fn update_review_should_separate_current_snapshot_from_editable_proposal() {
        let mut app = review_app();
        assert_eq!(app.current.title, "Título atual");
        assert_eq!(app.proposal.as_ref().unwrap().title, "Título proposto");
        assert!(app.open_content_edit());
        let editor = app.content_edit.as_ref().unwrap();
        assert_eq!(editor.title.value(), "Título proposto");
        assert_eq!(app.current.description, "Descrição atual\n- nota humana  ");
    }

    #[test]
    fn update_save_should_preserve_exact_proposal_and_unchanged_current_snapshot() {
        let mut app = review_app();
        let current = app.current.clone();
        assert!(app.open_content_edit());
        let editor = app.content_edit.as_mut().unwrap();
        editor.title = crate::tui::content_editor::TextEditor::new("  Título ✅  ", true);
        editor.body = crate::tui::content_editor::TextEditor::new("  body\n- [ ] ação  ", false);
        assert!(app.handle_content_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL,)));
        assert_eq!(
            app.proposal,
            Some(PrDescription {
                title: "  Título ✅  ".to_owned(),
                body: "  body\n- [ ] ação  ".to_owned(),
            })
        );
        assert_eq!(app.current, current);
    }

    #[test]
    fn update_editor_should_enforce_title_and_body_boundaries() {
        let mut empty_title = review_app();
        assert!(empty_title.open_content_edit());
        empty_title.content_edit.as_mut().unwrap().title =
            crate::tui::content_editor::TextEditor::new("   ", true);
        assert!(
            empty_title
                .handle_content_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL,))
        );
        assert!(empty_title.content_edit.is_some());
        assert_eq!(
            empty_title.content_edit.as_ref().unwrap().error,
            Some(crate::tui::content_editor::ContentValidationError::EmptyTitle)
        );

        let mut body_too_long = review_app();
        assert!(body_too_long.open_content_edit());
        body_too_long.content_edit.as_mut().unwrap().body =
            crate::tui::content_editor::TextEditor::new("a".repeat(4000), false);
        assert!(
            body_too_long
                .handle_content_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL,))
        );
        assert!(body_too_long.content_edit.is_some());
        assert_eq!(
            body_too_long.content_edit.as_ref().unwrap().error,
            Some(crate::tui::content_editor::ContentValidationError::BodyTooLong { length: 4000 })
        );

        let mut body_at_limit = review_app();
        assert!(body_at_limit.open_content_edit());
        body_at_limit.content_edit.as_mut().unwrap().body =
            crate::tui::content_editor::TextEditor::new("a".repeat(3999), false);
        assert!(
            body_at_limit
                .handle_content_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL,))
        );
        assert!(body_at_limit.content_edit.is_none());

        let mut empty_body = review_app();
        assert!(empty_body.open_content_edit());
        empty_body.content_edit.as_mut().unwrap().body =
            crate::tui::content_editor::TextEditor::new("", false);
        assert!(
            empty_body
                .handle_content_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL,))
        );
        assert!(empty_body.content_edit.is_none());
        assert_eq!(empty_body.proposal.unwrap().body, "");

        let mut invalid_confirm = review_app();
        invalid_confirm.proposal = Some(PrDescription {
            title: "   ".to_owned(),
            body: "body".to_owned(),
        });
        assert!(invalid_confirm.confirm().is_none());
        assert_eq!(invalid_confirm.phase, UpdatePhase::Review);
        assert!(invalid_confirm.frozen_content.is_none());
        assert!(invalid_confirm.content_edit.is_some());
    }

    #[test]
    fn update_cancel_should_discard_draft_without_remote_write() {
        let mut app = review_app();
        let original = app.proposal.clone();
        assert!(app.open_content_edit());
        app.content_edit
            .as_mut()
            .unwrap()
            .title
            .insert_text(" alterado");
        assert!(app.handle_content_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(app.content_edit.is_none());
        assert_eq!(app.proposal, original);
        assert_eq!(app.current.title, "Título atual");
        assert!(app.frozen_content.is_none());
        assert_eq!(app.phase, UpdatePhase::Review);

        let gateway = update_pull_request::gateway_for_test();
        let (tx, mut rx) = mpsc::unbounded_channel();
        assert!(matches!(
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                &gateway,
                &tx,
            ),
            Some(UpdateTuiOutcome::Aborted)
        ));
        assert!(rx.try_recv().is_err());
        assert_eq!(app.phase, UpdatePhase::Review);
        assert_eq!(review_app().proposal, original);
    }

    #[test]
    fn update_ui_should_freeze_approved_content_before_remote_operation() {
        let mut app = review_app();
        let approved = app.proposal.clone().unwrap();
        assert_eq!(app.begin_update(), Some(approved.clone()));
        assert_eq!(app.frozen_content, Some(approved));
        assert_eq!(app.phase, UpdatePhase::Confirming);
        app.mark_updating();
        assert_eq!(app.phase, UpdatePhase::Updating);
        assert!(!app.open_content_edit());
        app.on_outcome(Ok(UpdateOutcome::Conflict {
            remote: pull_request(),
            reason: "mudou".to_owned(),
        }));
        assert!(!app.open_content_edit());
        app.on_outcome(Ok(UpdateOutcome::Unknown {
            reason: "incerto".to_owned(),
        }));
        assert!(!app.open_content_edit());
    }

    #[test]
    fn update_review_80x24() -> anyhow::Result<()> {
        let app = review_app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
        terminal.draw(|frame| frame.render_widget(&app, frame.area()))?;
        insta::assert_snapshot!("update_review_80x24", terminal.backend());
        Ok(())
    }

    #[test]
    fn update_editor_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.open_content_edit();
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| frame.render_widget(&app, frame.area()))?;
        insta::assert_snapshot!("update_editor_100x30", terminal.backend());
        Ok(())
    }
}
