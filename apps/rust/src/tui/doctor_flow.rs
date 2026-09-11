//! Fluxo interativo do `prt doctor` — verifica o ambiente com lista rolável.
//!
//! Roda [`inspect`](crate::features::doctor::inspect) em background enquanto o
//! header mostra o status indeterminado; quando o relatório chega, exibe a
//! lista de checks (navegável com `j`/`k`) com painel de correção.
//!
//! Retorna o exit code do relatório ([`DoctorReport::exit_code`](crate::features::doctor::DoctorReport::exit_code));
//! se o usuário sair antes do resultado, retorna `130` (abortado).

use std::io::IsTerminal;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget, Wrap},
};
use tokio::sync::oneshot::error::TryRecvError;

use super::{StatusHeader, border_type, status_header, status_layout, theme};
use crate::features::doctor::{Check, DoctorReport, inspect};

/// Tick do status indeterminado durante a verificação.
const TICK_CHECKING: Duration = Duration::from_millis(100);
/// Poll do input (mantém a navegação responsiva entre ticks).
const INPUT_POLL: Duration = Duration::from_millis(10);
/// Altura do painel de detalhe (bordas + 3 linhas de `fix`).
const DETAIL_HEIGHT: u16 = 5;
/// Exit code quando o usuário aborta antes do relatório chegar.
const EXIT_ABORTED: i32 = 130;
/// Mensagem quando o check não exige ação.
const NO_FIX_MSG: &str = "Tudo certo — sem ação necessária.";

/// Glifo do status do check: `✔` ok, `!` aviso, `✘` falha.
fn check_glyph(check: &Check) -> &'static str {
    if check.ok {
        "✔"
    } else if check.warning {
        "!"
    } else {
        "✘"
    }
}

/// Estilo do status do check (par de [`check_glyph`]).
fn check_style(check: &Check) -> Style {
    if check.ok {
        theme().success
    } else if check.warning {
        theme().warning
    } else {
        theme().error
    }
}

/// Linha textual do check (núcleo puro do que a lista renderiza).
///
/// Helper exclusivo de teste (o render usa spans estilizados).
/// Formato: `<glifo> <componente> — <detalhe>`.
#[cfg(test)]
fn format_check_line(check: &Check) -> String {
    let glyph = check_glyph(check);
    let component = check.component;
    let detail = check.detail.as_str();
    format!("{glyph} {component} — {detail}")
}

/// Resumo do relatório p/ header: `todos prontos` ou `N falha(s) e M aviso(s)`.
fn summary_text(report: &DoctorReport) -> String {
    let (failures, warnings) = report.summary();
    if failures == 0 && warnings == 0 {
        "todos prontos ✓".to_owned()
    } else {
        format!("{failures} falha(s) e {warnings} aviso(s)")
    }
}

/// `fix` do check selecionado, ou mensagem padrão quando não há ação.
///
/// Nunca falha: índice fora da faixa ou `fix` vazio devolve [`NO_FIX_MSG`].
fn selected_fix(report: &DoctorReport, selected: usize) -> &str {
    report.checks.get(selected).map_or(NO_FIX_MSG, |check| {
        let fix = check.fix.trim();
        if fix.is_empty() { NO_FIX_MSG } else { fix }
    })
}

/// Relatório de contingência se a tarefa de inspeção morrer antes de enviar.
fn closed_report() -> DoctorReport {
    DoctorReport {
        checks: vec![Check {
            component: "Fluxo doctor",
            ok: false,
            warning: false,
            detail: "A tarefa de inspeção foi abortada antes de concluir.".to_owned(),
            fix: "Repita `prt doctor`.".to_owned(),
        }],
    }
}

/// Estado do fluxo (puro — sem terminal; testável via `#[cfg(test)]`).
struct DoctorFlowApp {
    /// Frame da animação do status.
    tick: u64,
    /// Índice do check selecionado.
    selected: usize,
    /// Deslocamento da lista (mantém o selecionado visível).
    offset: usize,
    /// Relatório pronto (`None` = ainda verificando).
    report: Option<DoctorReport>,
}

impl DoctorFlowApp {
    /// Estado inicial: verificando, sem seleção.
    fn new() -> Self {
        Self {
            tick: 0,
            selected: 0,
            offset: 0,
            report: None,
        }
    }

    /// Avança 1 tick do status.
    fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Guarda o relatório e zera a seleção.
    fn set_report(&mut self, report: DoctorReport) {
        self.report = Some(report);
        self.selected = 0;
        self.offset = 0;
    }

    /// Há relatório pronto?
    fn is_ready(&self) -> bool {
        self.report.is_some()
    }

    /// Quantidade de checks (0 enquanto verifica).
    fn len(&self) -> usize {
        self.report.as_ref().map_or(0, |r| r.checks.len())
    }

    /// Seleciona o próximo check (trava no fim).
    fn select_next(&mut self) {
        let len = self.len();
        if len == 0 {
            return;
        }
        let last = len.saturating_sub(1);
        if self.selected < last {
            self.selected = self.selected.saturating_add(1);
        }
    }

    /// Seleciona o check anterior (trava no início).
    fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        }
    }

    /// Ajusta `offset` p/ manter o selecionado dentro de `visible` linhas.
    fn ensure_visible(&mut self, visible: usize) {
        let len = self.len();
        if len == 0 || visible == 0 {
            return;
        }
        self.selected = self.selected.min(len.saturating_sub(1));
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset.saturating_add(visible) {
            self.offset = self.selected.saturating_add(1).saturating_sub(visible);
        }
    }

    /// Exit code atual: o do relatório, ou 130 se ainda verificando.
    fn current_exit_code(&self) -> i32 {
        self.report.as_ref().map_or(
            EXIT_ABORTED,
            super::super::features::doctor::DoctorReport::exit_code,
        )
    }
}

/// Roda o fluxo interativo do `doctor` até o usuário sair.
///
/// `source` é a branch de origem (`--source`); `None` usa a branch atual.
/// Retorna o exit code do relatório (0 sem falhas, 1 com falhas; 130 se o
/// usuário sair antes da verificação concluir).
///
/// # Errors
///
/// Retorna erro se o terminal não puder ser inicializado (sem TTY) ou se o
/// desenho falhar.
pub async fn run_doctor_flow(source: Option<&str>) -> anyhow::Result<i32> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("tui requer terminal interativo");
    }
    let mut terminal: DefaultTerminal = ratatui::init();
    let res = run_loop(&mut terminal, source).await;
    ratatui::restore();
    res
}

/// Loop principal: inspeção em background + render sob dirty-flag.
// Mantido `async` de propósito: o wrapper público `run_doctor_flow`
// (com `.await` em `main.rs`, fora do escopo permitido) aguarda este loop,
// e o fluxo irmão `test_flow::run_loop` é `async` de verdade. Remover o
// `async` aqui só deslocaria o `unused_async` para a API pública.
#[allow(clippy::unused_async)]
async fn run_loop(terminal: &mut DefaultTerminal, source: Option<&str>) -> anyhow::Result<i32> {
    let owned = source.map(str::to_owned);
    let (tx, mut rx) = tokio::sync::oneshot::channel::<DoctorReport>();
    tokio::spawn(async move {
        let report = inspect(owned.as_deref()).await;
        let _ = tx.send(report);
    });

    let mut app = DoctorFlowApp::new();
    let mut last_tick = std::time::Instant::now();
    let mut needs_draw = true;

    loop {
        // 1. Drena o relatório do background sem bloquear.
        if app.report.is_none() {
            match rx.try_recv() {
                Ok(report) => {
                    app.set_report(report);
                    needs_draw = true;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Closed) => {
                    app.set_report(closed_report());
                    needs_draw = true;
                }
            }
        }
        // 2. Tick do status (100ms) suja a tela.
        if last_tick.elapsed() >= TICK_CHECKING {
            app.on_tick();
            last_tick = std::time::Instant::now();
            needs_draw = true;
        }
        // 3. Desenha só se sujo.
        if needs_draw {
            terminal.draw(|f| render_app(&mut app, f.area(), f.buffer_mut()))?;
            needs_draw = false;
        }

        // 4. Input não-bloqueante.
        if event::poll(INPUT_POLL)? {
            if let Event::Key(key) = event::read()? {
                // Filtro de kind: Release sempre ignorado; Repeat só p/ scroll.
                let eh_scroll = matches!(
                    key.code,
                    KeyCode::Char('j' | 'k') | KeyCode::Up | KeyCode::Down
                );
                if key.kind == KeyEventKind::Release
                    || (key.kind == KeyEventKind::Repeat && !eh_scroll)
                {
                    // Ignora sem sujar a tela.
                } else {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                            return Ok(app.current_exit_code());
                        }
                        (KeyCode::Char('z'), m) if m.contains(KeyModifiers::CONTROL) => {
                            #[cfg(unix)]
                            {
                                super::suspend::suspend_to_shell(&mut *terminal)?;
                                needs_draw = true;
                            }
                        }
                        (KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter, _) => {
                            return Ok(app.current_exit_code());
                        }
                        (KeyCode::Char('j') | KeyCode::Down, _) if app.is_ready() => {
                            app.select_next();
                            needs_draw = true;
                        }
                        (KeyCode::Char('k') | KeyCode::Up, _) if app.is_ready() => {
                            app.select_prev();
                            needs_draw = true;
                        }
                        _ => {}
                    }
                }
            }
        }

        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}

/// Desenha um frame completo a partir do estado.
fn render_app(app: &mut DoctorFlowApp, area: Rect, buf: &mut Buffer) {
    // Piso honesto: terminal miúdo não tenta layout normal.
    if area.width < 60 || area.height < 20 {
        render_too_small(area, buf);
        return;
    }
    let [head, body, foot] = status_layout(area);
    Block::new().style(theme().root).render(area, buf);
    render_header(app, head, buf);
    render_body(app, body, buf);
    render_footer(app, foot, buf);
}

/// Aviso honesto p/ terminal < 60×20.
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
        .title(Span::styled(" ◆ prt doctor ", theme().warning))
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

/// Header com fase e resumo global do diagnóstico.
fn render_header(app: &DoctorFlowApp, area: Rect, buf: &mut Buffer) {
    let (message, style, progress, active) = if let Some(report) = &app.report {
        let (failures, warnings) = report.summary();
        let style = if failures > 0 {
            theme().error
        } else if warnings > 0 {
            theme().warning
        } else {
            theme().success
        };
        (summary_text(report), style, Some(1.0), false)
    } else {
        (
            "verificando ambiente…".to_owned(),
            theme().accent,
            None,
            true,
        )
    };
    status_header(
        area,
        buf,
        StatusHeader {
            command: "doctor",
            phase: "diagnóstico",
            message: &message,
            progress,
            tick: app.tick,
            active,
            style,
        },
    );
}

/// Corpo: lista (Min) + detalhe do `fix` (5 linhas), quando o relatório chega.
fn render_body(app: &mut DoctorFlowApp, area: Rect, buf: &mut Buffer) {
    if app.report.is_none() {
        return;
    }
    if app.report.as_ref().is_some_and(|r| r.checks.is_empty()) {
        Paragraph::new("nenhum check retornado pela inspeção.")
            .style(theme().muted)
            .block(
                Block::default()
                    .title(Span::styled(" Doctor ", theme().accent))
                    .borders(Borders::ALL)
                    .border_style(theme().border)
                    .border_type(border_type())
                    .padding(ratatui::widgets::Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: false })
            .render(area, buf);
        return;
    }
    let detail_h = DETAIL_HEIGHT.min(area.height);
    let [list_area, detail_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(detail_h)]).areas(area);
    render_list(app, list_area, buf);
    let Some(report) = app.report.as_ref() else {
        return;
    };
    let fix = selected_fix(report, app.selected);
    Paragraph::new(fix)
        .style(theme().muted)
        .block(
            Block::default()
                .title(Span::styled(" Como corrigir ", theme().accent))
                .borders(Borders::ALL)
                .border_style(theme().border)
                .border_type(border_type())
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: false })
        .render(detail_area, buf);
}

/// Lista rolável de checks com highlight reverso no selecionado.
fn render_list(app: &mut DoctorFlowApp, area: Rect, buf: &mut Buffer) {
    let visible = usize::from(area.height.saturating_sub(2));
    app.ensure_visible(visible.max(1));
    let Some(report) = app.report.as_ref() else {
        return;
    };
    let items: Vec<ListItem> = report
        .checks
        .iter()
        .map(|check| {
            let style = check_style(check);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", check_glyph(check)), style),
                Span::styled(check.component, Style::new().add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {}", check.detail), theme().muted),
            ]))
        })
        .collect();
    let title = format!(" Checks ({}) ", report.checks.len());
    let list = List::new(items)
        .block(
            Block::default()
                .title(Span::styled(title, theme().accent))
                .borders(Borders::ALL)
                .border_style(theme().border)
                .border_type(border_type())
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ");
    let mut state = ListState::default()
        .with_selected(Some(app.selected))
        .with_offset(app.offset);
    StatefulWidget::render(list, area, buf, &mut state);
}

/// Footer honesto: atalhos + exit code que a saída vai retornar.
fn render_footer(app: &DoctorFlowApp, area: Rect, buf: &mut Buffer) {
    let code = app.current_exit_code();
    let hints = if app.is_ready() {
        format!("j/k navegar · q/enter/esc sair (código {code})")
    } else {
        format!("q/esc sai sem resultado (código {code})")
    };
    Paragraph::new(Line::from(Span::styled(hints, theme().muted))).render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    /// Check de sucesso p/ testes.
    fn ok_check() -> Check {
        Check {
            component: "Git",
            ok: true,
            warning: false,
            detail: "git 2.45.0".to_owned(),
            fix: String::new(),
        }
    }

    /// Check de aviso p/ testes.
    fn warn_check() -> Check {
        Check {
            component: "Branch de trabalho",
            ok: false,
            warning: true,
            detail: "detached HEAD".to_owned(),
            fix: "Mude para uma branch de trabalho.".to_owned(),
        }
    }

    /// Check de falha p/ testes.
    fn fail_check() -> Check {
        Check {
            component: "Azure DevOps PAT",
            ok: false,
            warning: false,
            detail: "PAT não configurado".to_owned(),
            fix: "Execute `prt init`.".to_owned(),
        }
    }

    /// Relatório com 1 falha + 1 aviso p/ testes.
    fn mixed_report() -> DoctorReport {
        DoctorReport {
            checks: vec![ok_check(), warn_check(), fail_check()],
        }
    }

    #[test]
    fn glyph_should_match_status() {
        assert_eq!(check_glyph(&ok_check()), "✔");
        assert_eq!(check_glyph(&warn_check()), "!");
        assert_eq!(check_glyph(&fail_check()), "✘");
    }

    #[test]
    fn format_line_should_contain_glyph_component_and_detail() {
        let line = format_check_line(&fail_check());
        assert!(line.contains("✘"), "glifo ausente: {line}");
        assert!(
            line.contains("Azure DevOps PAT"),
            "componente ausente: {line}"
        );
        assert!(
            line.contains("PAT não configurado"),
            "detalhe ausente: {line}"
        );
    }

    #[test]
    fn summary_clean_should_say_ready() {
        let report = DoctorReport {
            checks: vec![ok_check()],
        };
        assert_eq!(summary_text(&report), "todos prontos ✓");
    }

    #[test]
    fn summary_mixed_should_count_failures_and_warnings() {
        assert_eq!(summary_text(&mixed_report()), "1 falha(s) e 1 aviso(s)");
    }

    #[test]
    fn selected_fix_should_return_fix_or_default() {
        let report = mixed_report();
        assert_eq!(
            selected_fix(&report, 1),
            "Mude para uma branch de trabalho."
        );
        assert_eq!(selected_fix(&report, 0), NO_FIX_MSG);
        assert_eq!(selected_fix(&report, 99), NO_FIX_MSG);
    }

    #[test]
    fn selection_should_clamp_at_bounds() {
        let mut app = DoctorFlowApp::new();
        app.set_report(mixed_report());
        assert_eq!(app.selected, 0);
        app.select_prev();
        assert_eq!(app.selected, 0);
        app.select_next();
        app.select_next();
        app.select_next();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn ensure_visible_should_scroll_offset() {
        let mut app = DoctorFlowApp::new();
        app.set_report(mixed_report());
        app.selected = 2;
        app.ensure_visible(1);
        assert_eq!(app.offset, 2);
        app.selected = 0;
        app.ensure_visible(1);
        assert_eq!(app.offset, 0);
    }

    #[test]
    fn doctor_checking_should_keep_status_only_in_header() -> anyhow::Result<()> {
        let mut app = DoctorFlowApp::new();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| render_app(&mut app, f.area(), f.buffer_mut()))?;
        insta::assert_snapshot!("doctor_checking_80x24", terminal.backend());
        Ok(())
    }

    #[test]
    fn doctor_ready_should_keep_checks_and_fixes() -> anyhow::Result<()> {
        let mut app = DoctorFlowApp::new();
        app.set_report(mixed_report());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| render_app(&mut app, f.area(), f.buffer_mut()))?;
        insta::assert_snapshot!("doctor_ready_80x24", terminal.backend());
        Ok(())
    }

    #[test]
    fn exit_code_should_follow_report_or_abort() {
        let mut app = DoctorFlowApp::new();
        assert_eq!(app.current_exit_code(), EXIT_ABORTED);
        app.set_report(mixed_report());
        assert_eq!(app.current_exit_code(), 1);
        app.set_report(DoctorReport {
            checks: vec![ok_check()],
        });
        assert_eq!(app.current_exit_code(), 0);
    }
}
