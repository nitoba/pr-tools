//! TUI inline da atualização do binário (`prt update`).
//!
//! O comando é uma operação curta, não uma sessão exploratória: a tela ocupa
//! poucas linhas, mostra o progresso real do download e devolve o terminal ao
//! shell assim que a instalação termina. O caminho sem TTY continua na camada
//! de feature e não recebe caracteres de controle.

use std::io::IsTerminal as _;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal, TerminalOptions, Viewport,
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};
use tokio::sync::mpsc;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::shimmer::shimmer_text;
use super::{StatusHeader, ascii_only, status_header, status_layout, theme};
use crate::error::AppError;
use crate::features::update::{self, UpdateProgress};

const VIEWPORT_HEIGHT: u16 = 8;
const MIN_WIDTH: u16 = 48;
const MIN_HEIGHT: u16 = 6;

/// Fase visual do instalador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdatePhase {
    /// Descobrindo o asset da plataforma.
    #[default]
    Checking,
    /// Recebendo o arquivo da release.
    Downloading,
    /// Executando `prt --version` no arquivo temporário.
    Validating,
    /// Substituindo o binário atual.
    Installing,
    /// Instalação concluída.
    Done,
    /// Instalação interrompida por uma falha.
    Error,
}

#[derive(Debug)]
enum BackendEvent {
    Progress(UpdateProgress),
    Finished(Result<String, String>),
}

/// Estado renderizável e testável da tela de atualização.
#[derive(Debug, Default)]
pub struct UpdateApp {
    /// Caminho do binário atual.
    pub target: String,
    /// Nome do asset selecionado.
    pub asset: String,
    /// Fase atual.
    pub phase: UpdatePhase,
    /// Bytes recebidos até o momento.
    pub downloaded: u64,
    /// Total de bytes, se o servidor informou `Content-Length`.
    pub total: Option<u64>,
    /// Versão identificada ou instalada.
    pub version: Option<String>,
    /// Mensagem da falha, quando houver.
    pub error: Option<String>,
    /// Fase que falhou, usada para manter o checklist honesto.
    failed_phase: Option<UpdatePhase>,
    /// Frame da animação shimmer.
    pub tick: u64,
}

impl UpdateApp {
    /// Recebe um evento da camada de atualização.
    pub fn on_progress(&mut self, progress: UpdateProgress) {
        match progress {
            UpdateProgress::Checking { target, asset } => {
                self.target = target;
                self.asset = asset;
                self.phase = UpdatePhase::Checking;
                self.downloaded = 0;
                self.total = None;
                self.version = None;
                self.error = None;
                self.failed_phase = None;
            }
            UpdateProgress::Downloading { downloaded, total } => {
                self.phase = UpdatePhase::Downloading;
                self.downloaded = downloaded;
                self.total = total;
            }
            UpdateProgress::Validating => self.phase = UpdatePhase::Validating,
            UpdateProgress::Validated { version } => self.version = Some(version),
            UpdateProgress::Installing => self.phase = UpdatePhase::Installing,
            UpdateProgress::Completed { version } => {
                self.phase = UpdatePhase::Done;
                self.version = Some(version);
                self.downloaded = self.total.unwrap_or(self.downloaded);
                self.error = None;
                self.failed_phase = None;
            }
        }
    }

    /// Marca a operação como falha sem perder a etapa em que ela parou.
    pub fn on_error(&mut self, message: String) {
        self.failed_phase = Some(self.phase);
        self.phase = UpdatePhase::Error;
        self.error = Some(message);
    }

    /// Avança a animação do shimmer.
    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    fn progress(&self) -> Option<f64> {
        if self.phase == UpdatePhase::Done {
            return Some(1.0);
        }
        let total = self.total.filter(|total| *total > 0)?;
        let downloaded = u32::try_from(self.downloaded).unwrap_or(u32::MAX);
        let total = u32::try_from(total).unwrap_or(u32::MAX);
        Some((f64::from(downloaded) / f64::from(total)).clamp(0.0, 1.0))
    }

    fn active(&self) -> bool {
        matches!(
            self.phase,
            UpdatePhase::Checking
                | UpdatePhase::Downloading
                | UpdatePhase::Validating
                | UpdatePhase::Installing
        )
    }

    fn message(&self) -> String {
        match self.phase {
            UpdatePhase::Checking => "verificando a versão mais recente".to_owned(),
            UpdatePhase::Downloading => match self.total {
                Some(total) if total > 0 => format!(
                    "baixando {} · {} / {}",
                    self.asset,
                    format_bytes(self.downloaded),
                    format_bytes(total)
                ),
                _ => format!(
                    "baixando {} · {} recebidos",
                    self.asset,
                    format_bytes(self.downloaded)
                ),
            },
            UpdatePhase::Validating => "validando o binário baixado".to_owned(),
            UpdatePhase::Installing => "instalando a nova versão".to_owned(),
            UpdatePhase::Done => self.version.as_deref().map_or_else(
                || "atualização concluída".to_owned(),
                |v| format!("prt pronto · {v}"),
            ),
            UpdatePhase::Error => "a atualização não foi concluída".to_owned(),
        }
    }
}

impl Widget for &UpdateApp {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Block::new().style(theme().root).render(area, buf);
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            render_too_small(area, buf);
            return;
        }

        let [head, body, foot] = status_layout(area);
        status_header(
            head,
            buf,
            StatusHeader {
                command: "update",
                phase: phase_label(self.phase),
                message: &self.message(),
                progress: self.progress(),
                tick: self.tick,
                active: self.active(),
                style: phase_style(self.phase),
            },
        );
        render_steps(self, body, buf);
        Paragraph::new(Line::from(Span::styled(
            footer_text(self.phase),
            theme().muted,
        )))
        .render(foot, buf);
    }
}

fn render_too_small(area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let separator = if ascii_only() { "x" } else { "×" };
    let message = format!(
        "terminal muito pequeno - mínimo {}{}{} (atual {}{}{})",
        MIN_WIDTH, separator, MIN_HEIGHT, area.width, separator, area.height
    );
    Paragraph::new(Line::from(Span::styled(message, theme().warning))).render(area, buf);
}

fn render_steps(app: &UpdateApp, area: Rect, buf: &mut Buffer) {
    let stages = [
        (UpdatePhase::Checking, "verificar disponibilidade"),
        (UpdatePhase::Downloading, "baixar novo binário"),
        (UpdatePhase::Validating, "validar binário"),
        (UpdatePhase::Installing, "instalar atualização"),
    ];
    let mut lines = Vec::with_capacity(5);
    for (phase, label) in stages {
        lines.push(stage_line(app, phase, label));
    }

    let detail = match app.phase {
        UpdatePhase::Error => app.error.as_deref().map_or_else(
            || "falha sem mensagem".to_owned(),
            |error| format!("erro: {error}"),
        ),
        UpdatePhase::Done => {
            if app.target.is_empty() {
                "instalação concluída".to_owned()
            } else {
                format!("destino: {}", app.target)
            }
        }
        _ if !app.target.is_empty() => format!("destino: {}", app.target),
        _ => "aguardando resposta do GitHub".to_owned(),
    };
    let detail_style = if app.phase == UpdatePhase::Error {
        theme().error
    } else {
        theme().muted
    };
    lines.push(Line::from(Span::styled(
        fit_cells(&detail, usize::from(area.width)),
        detail_style,
    )));
    Paragraph::new(lines).render(area, buf);
}

fn stage_line(app: &UpdateApp, stage: UpdatePhase, label: &str) -> Line<'static> {
    let stage_status = stage_state(app, stage);
    let marker = match stage_status {
        StageState::Complete => {
            if ascii_only() {
                "[x]"
            } else {
                "✓"
            }
        }
        StageState::Active => {
            if ascii_only() {
                "[>]"
            } else {
                "●"
            }
        }
        StageState::Failed => {
            if ascii_only() {
                "[!]"
            } else {
                "✘"
            }
        }
        StageState::Pending => {
            if ascii_only() {
                "[ ]"
            } else {
                "○"
            }
        }
    };
    let style = match stage_status {
        StageState::Complete => theme().success,
        StageState::Active => theme().accent,
        StageState::Failed => theme().error,
        StageState::Pending => theme().muted,
    };
    let label = if stage_status == StageState::Active {
        shimmer_text(label, style, app.tick).spans
    } else {
        vec![Span::styled(label.to_owned(), style)]
    };
    let mut spans = vec![Span::styled(format!("{marker} "), style)];
    spans.extend(label);
    if stage == UpdatePhase::Downloading && app.phase == UpdatePhase::Downloading {
        let detail = match app.total {
            Some(total) if total > 0 => format!(
                "  {} / {}",
                format_bytes(app.downloaded),
                format_bytes(total)
            ),
            _ => format!("  {}", format_bytes(app.downloaded)),
        };
        spans.push(Span::styled(detail, theme().muted));
    }
    Line::from(spans)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageState {
    Complete,
    Active,
    Failed,
    Pending,
}

fn stage_state(app: &UpdateApp, stage: UpdatePhase) -> StageState {
    if app.phase == UpdatePhase::Error {
        let failed = app.failed_phase.unwrap_or(UpdatePhase::Checking);
        if stage == failed {
            return StageState::Failed;
        }
        return if stage_rank(stage) < stage_rank(failed) {
            StageState::Complete
        } else {
            StageState::Pending
        };
    }
    if app.phase == stage {
        StageState::Active
    } else if stage_rank(stage) < stage_rank(app.phase) || app.phase == UpdatePhase::Done {
        StageState::Complete
    } else {
        StageState::Pending
    }
}

fn stage_rank(phase: UpdatePhase) -> u8 {
    match phase {
        UpdatePhase::Checking => 1,
        UpdatePhase::Downloading => 2,
        UpdatePhase::Validating => 3,
        UpdatePhase::Installing => 4,
        UpdatePhase::Done => 5,
        UpdatePhase::Error => 0,
    }
}

fn phase_label(phase: UpdatePhase) -> &'static str {
    match phase {
        UpdatePhase::Checking => "Verificando",
        UpdatePhase::Downloading => "Download",
        UpdatePhase::Validating => "Validação",
        UpdatePhase::Installing => "Instalação",
        UpdatePhase::Done => "Concluído",
        UpdatePhase::Error => "Erro",
    }
}

fn phase_style(phase: UpdatePhase) -> Style {
    match phase {
        UpdatePhase::Done => theme().success,
        UpdatePhase::Error => theme().error,
        _ => theme().accent,
    }
}

fn footer_text(phase: UpdatePhase) -> &'static str {
    if phase == UpdatePhase::Error {
        "Ctrl+C cancelar · o erro também foi enviado para stderr"
    } else if phase == UpdatePhase::Done {
        "instalação concluída · retornando ao shell"
    } else {
        "Ctrl+C cancelar · trabalhando em segundo plano"
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    if bytes >= MIB {
        let tenths = bytes.saturating_mul(10) / MIB;
        return format!("{}.{:01} MiB", tenths / 10, tenths % 10);
    }
    if bytes >= KIB {
        let tenths = bytes.saturating_mul(10) / KIB;
        return format!("{}.{:01} KiB", tenths / 10, tenths % 10);
    }
    format!("{bytes} B")
}

fn fit_cells(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_owned();
    }
    let ellipsis = if ascii_only() { "..." } else { "…" };
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    if max_width <= ellipsis_width {
        return ellipsis.chars().take(max_width).collect::<String>();
    }
    let mut suffix = String::new();
    let mut width = 0_usize;
    for ch in value.chars().rev() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width.saturating_add(char_width) > max_width - ellipsis_width {
            break;
        }
        width = width.saturating_add(char_width);
        suffix.push(ch);
    }
    let suffix: String = suffix.chars().rev().collect();
    format!("{ellipsis}{suffix}")
}

/// Executa o fluxo visual do `prt update`.
///
/// Em stdout não-interativo, delega para a saída linear da feature. Em um
/// terminal, desenha um viewport inline e mantém a saída final do comando
/// fora do frame animado.
///
/// # Errors
///
/// Retorna erro quando o download ou a instalação não puderem ser
/// concluídos, ou quando o terminal interativo não puder ser inicializado.
pub async fn run() -> anyhow::Result<()> {
    if !std::io::stdout().is_terminal() {
        return update::run().await.map_err(anyhow::Error::new);
    }

    let version = run_interactive()?;
    #[cfg(windows)]
    {
        let _ = version;
        println!("✓ atualização agendada; o novo binário será aplicado ao sair");
    }
    #[cfg(not(windows))]
    println!("✓ prt atualizado com sucesso: {version}");
    Ok(())
}

fn run_interactive() -> anyhow::Result<String> {
    let mut terminal: DefaultTerminal = ratatui::init_with_options(TerminalOptions {
        viewport: Viewport::Inline(VIEWPORT_HEIGHT),
    });
    let (tx, rx) = mpsc::unbounded_channel();
    let operation_tx = tx.clone();
    let operation = tokio::spawn(async move {
        let result = update::run_with_progress(move |progress| {
            let _ = operation_tx.send(BackendEvent::Progress(progress));
        })
        .await
        .map_err(|error| error.to_string());
        let _ = tx.send(BackendEvent::Finished(result));
    });

    let result = run_loop(&mut terminal, rx);
    if result.is_err() {
        operation.abort();
    }
    // `ratatui::restore` handles raw mode, but an inline viewport did not
    // enter the alternate screen; make cursor visibility explicit here.
    let _ = terminal.show_cursor();
    ratatui::restore();
    result
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    mut rx: mpsc::UnboundedReceiver<BackendEvent>,
) -> anyhow::Result<String> {
    let mut app = UpdateApp::default();
    let tick_rate = Duration::from_millis(33);
    let mut last_tick = std::time::Instant::now();
    let mut needs_draw = true;
    let mut result = None;

    loop {
        while let Ok(event) = rx.try_recv() {
            match event {
                BackendEvent::Progress(progress) => app.on_progress(progress),
                BackendEvent::Finished(operation_result) => {
                    if let Err(message) = &operation_result {
                        app.on_error(message.clone());
                    }
                    result = Some(operation_result);
                }
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
        if let Some(result) = result {
            return result.map_err(|message| anyhow::anyhow!(message));
        }
        if event::poll(Duration::from_millis(10))? {
            let Event::Key(key) = event::read()? else {
                needs_draw = true;
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }
            if is_cancel_key(key) {
                return Err(anyhow::Error::new(AppError::Cancelled));
            }
        }
    }
}

fn is_cancel_key(key: KeyEvent) -> bool {
    key.kind != KeyEventKind::Release
        && key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_tui_snapshot;
    use ratatui::{Terminal, backend::TestBackend};

    fn downloading_app() -> UpdateApp {
        let mut app = UpdateApp::default();
        app.on_progress(UpdateProgress::Checking {
            target: "/home/dev/.local/bin/prt".to_owned(),
            asset: "prt-linux-x64".to_owned(),
        });
        app.on_progress(UpdateProgress::Downloading {
            downloaded: 2 * 1024 * 1024 + 256 * 1024,
            total: Some(8 * 1024 * 1024),
        });
        app
    }

    #[test]
    fn download_progress_should_compute_real_ratio() {
        let app = downloading_app();
        assert_eq!(app.progress(), Some(0.28125));
        assert_eq!(format_bytes(app.downloaded), "2.2 MiB");
    }

    #[test]
    fn failed_download_should_preserve_failed_stage() {
        let mut app = downloading_app();
        app.on_error("conexão encerrada".to_owned());
        assert_eq!(app.phase, UpdatePhase::Error);
        assert_eq!(app.failed_phase, Some(UpdatePhase::Downloading));
        assert_eq!(
            stage_state(&app, UpdatePhase::Downloading),
            StageState::Failed
        );
        assert_eq!(
            stage_state(&app, UpdatePhase::Checking),
            StageState::Complete
        );
    }

    #[test]
    fn update_downloading_80x8() -> anyhow::Result<()> {
        let app = downloading_app();
        let mut terminal = Terminal::new(TestBackend::new(80, 8))?;
        terminal.draw(|frame| frame.render_widget(&app, frame.area()))?;
        assert_tui_snapshot!("update_downloading_80x8", terminal.backend());
        Ok(())
    }

    #[test]
    fn update_downloading_60x8() -> anyhow::Result<()> {
        let app = downloading_app();
        let mut terminal = Terminal::new(TestBackend::new(60, 8))?;
        terminal.draw(|frame| frame.render_widget(&app, frame.area()))?;
        assert_tui_snapshot!("update_downloading_60x8", terminal.backend());
        Ok(())
    }

    #[test]
    fn update_done_80x8() -> anyhow::Result<()> {
        let mut app = downloading_app();
        app.on_progress(UpdateProgress::Validating);
        app.on_progress(UpdateProgress::Validated {
            version: "prt v9.1.0".to_owned(),
        });
        app.on_progress(UpdateProgress::Installing);
        app.on_progress(UpdateProgress::Completed {
            version: "prt v9.1.0".to_owned(),
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 8))?;
        terminal.draw(|frame| frame.render_widget(&app, frame.area()))?;
        assert_tui_snapshot!("update_done_80x8", terminal.backend());
        Ok(())
    }
}
