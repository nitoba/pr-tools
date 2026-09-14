//! TUI com Ratatui — visual moderno que substitui `terminice` do Dart.
//!
//! Dois níveis:
//! - helpers estáticos (`header`, `card`, …) para `doctor`/`init`;
//! - loop vivo reativo (`live::run_describe_tui`) para `desc`: backend em
//!   tokio empurra `Token`/`Log`/`Progress` por `mpsc` e cada frame (~30fps)
//!   redesenha o preview com cursor e o status global.

pub mod content_editor;
pub mod describe_app;
pub mod doctor_flow;
pub mod events;
pub mod init_wizard;
pub mod live;
pub mod markdown;
pub mod notice;
pub mod shimmer;
pub mod suspend;
pub mod test_flow;
pub mod update_flow;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph, Tabs, Widget, Wrap,
    },
};

use self::shimmer::{filled_cells, percent_u16, shimmer_bar, shimmer_text};

#[cfg(test)]
/// Formats a TUI snapshot with the package version redacted.
pub fn snapshot_value(value: &impl std::fmt::Display) -> String {
    value.to_string().replace(crate::cli::VERSION, "<version>")
}

#[cfg(test)]
/// Asserts a TUI snapshot while ignoring the release-specific package version.
#[macro_export]
macro_rules! assert_tui_snapshot {
    ($name:expr, $value:expr) => {
        insta::assert_snapshot!($name, $crate::tui::snapshot_value($value));
    };
}

/// Tema global da TUI.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Fundo raiz.
    pub root: Style,
    /// Título do app.
    pub app_title: Style,
    /// Abas.
    pub tabs: Style,
    /// Aba selecionada.
    pub tabs_selected: Style,
    /// Bordas dos cards.
    pub border: Style,
    /// Texto de sucesso.
    pub success: Style,
    /// Texto de aviso.
    pub warning: Style,
    /// Texto de erro.
    pub error: Style,
    /// Texto suave (hints).
    pub muted: Style,
    /// Destaque (URLs, IDs).
    pub accent: Style,
}

/// Tema padrão — roxo/ciano sobre fundo escuro.
pub const THEME: Theme = Theme {
    root: Style::new().bg(Color::Rgb(13, 17, 23)),
    app_title: Style::new()
        .fg(Color::Rgb(167, 139, 250))
        .add_modifier(Modifier::BOLD),
    tabs: Style::new().fg(Color::Rgb(139, 148, 158)),
    tabs_selected: Style::new()
        .fg(Color::Rgb(34, 211, 238))
        .add_modifier(Modifier::BOLD),
    border: Style::new().fg(Color::Rgb(88, 101, 242)),
    success: Style::new()
        .fg(Color::Rgb(63, 185, 80))
        .add_modifier(Modifier::BOLD),
    warning: Style::new()
        .fg(Color::Rgb(210, 153, 34))
        .add_modifier(Modifier::BOLD),
    error: Style::new()
        .fg(Color::Rgb(248, 81, 73))
        .add_modifier(Modifier::BOLD),
    muted: Style::new().fg(Color::Rgb(139, 148, 158)),
    accent: Style::new().fg(Color::Rgb(34, 211, 238)),
};

/// Diz se a TUI pode usar cores.
///
/// Falso quando `NO_COLOR` está presente e não-vazia (convenção
/// `no-color.org`) ou quando `TERM == "dumb"`; verdadeiro caso
/// contrário. Lido a cada chamada para respeitar o ambiente.
#[must_use]
pub fn colors_enabled() -> bool {
    if std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()) {
        return false;
    }
    !std::env::var("TERM").is_ok_and(|v| v == "dumb")
}

/// Diz se a TUI deve usar só ASCII (sem borda arredondada nem Braille).
///
/// Verdadeiro quando `PRT_ASCII == "1"` ou quando um terminal interativo
/// anuncia `TERM == "dumb"`. Backends de teste não são tratados como um
/// terminal dumb, para que snapshots não dependam do ambiente do runner.
#[must_use]
pub fn ascii_only() -> bool {
    if std::env::var("PRT_ASCII").is_ok_and(|v| v == "1") {
        return true;
    }

    #[cfg(not(test))]
    {
        std::env::var("TERM").is_ok_and(|v| v == "dumb")
            && std::io::IsTerminal::is_terminal(&std::io::stdout())
    }

    #[cfg(test)]
    false
}

/// Tema sem cor — mesmo [`Theme`] com tudo zerado.
///
/// Usado via [`theme()`] quando [`colors_enabled()`] é falso.
/// É `const` como o [`THEME`] (`Style::new()` é `const`).
pub const PLAIN: Theme = Theme {
    root: Style::new(),
    app_title: Style::new(),
    tabs: Style::new(),
    tabs_selected: Style::new(),
    border: Style::new(),
    success: Style::new(),
    warning: Style::new(),
    error: Style::new(),
    muted: Style::new(),
    accent: Style::new(),
};

/// Tema efetivo: [`THEME`] com cor, [`PLAIN`] sem cor.
///
/// Prefira este aos acessos diretos a `THEME` nos renders para
/// respeitar `NO_COLOR`/`TERM=dumb`.
#[must_use]
pub fn theme() -> Theme {
    if colors_enabled() { THEME } else { PLAIN }
}

/// Tipo de borda efetivo: arredondada normal, simples em ASCII.
///
/// Prefira este a `BorderType::Rounded` nos renders para
/// respeitar `PRT_ASCII=1`/`TERM=dumb`.
#[must_use]
pub fn border_type() -> BorderType {
    if ascii_only() {
        BorderType::Plain
    } else {
        BorderType::Rounded
    }
}

/// Glifo de checkbox efetivo: `☐`/`☑` normal, `[ ]`/`[x]` em ASCII.
///
/// Prefira este aos literais `☐`/`☑` nos renders para
/// respeitar `PRT_ASCII=1`/`TERM=dumb`.
#[must_use]
pub fn checkbox(checked: bool) -> &'static str {
    if ascii_only() {
        if checked { "[x]" } else { "[ ]" }
    } else if checked {
        "☑"
    } else {
        "☐"
    }
}

/// Renderiza header `◆ prt vX — subtítulo`.
pub fn header(area: Rect, buf: &mut Buffer, subtitle: &str) {
    let brand = if ascii_only() { "prt " } else { "◆ prt " };
    let separator = if ascii_only() { " - " } else { "  ·  " };
    let line = Line::from(vec![
        Span::styled(brand, theme().app_title),
        Span::styled(crate::cli::VERSION, theme().muted),
        Span::styled(format!("{separator}{subtitle}"), theme().muted),
    ]);
    Paragraph::new(line).render(area, buf);
}

/// Dados do header compartilhado das telas com estado.
#[derive(Debug, Clone, Copy)]
pub struct StatusHeader<'a> {
    /// Comando em execução, como `desc` ou `doctor`.
    pub command: &'a str,
    /// Fase principal atual.
    pub phase: &'a str,
    /// Mensagem contextual da fase.
    pub message: &'a str,
    /// Progresso conhecido; `None` representa uma operação indeterminada.
    pub progress: Option<f64>,
    /// Frame usado para animar o progresso indeterminado ou ativo.
    pub tick: u64,
    /// Se a operação ainda está em andamento.
    pub active: bool,
    /// Cor da fase principal.
    pub style: Style,
}

/// Renderiza o único status visual de uma tela.
///
/// A primeira linha identifica comando e fase. A segunda combina mensagem
/// contextual com uma única barra de progresso; operações sem percentual usam
/// uma barra indeterminada em vez de inventar um valor.
pub fn status_header(area: Rect, buf: &mut Buffer, status: StatusHeader<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let [identity, progress] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let separator = if ascii_only() { "  -  " } else { "  ·  " };
    let brand = if ascii_only() { "prt" } else { "◆ prt" };
    Paragraph::new(Line::from(vec![
        Span::styled(format!("{brand} "), theme().app_title),
        Span::styled(crate::cli::VERSION, theme().muted),
        Span::styled(format!("{separator}{}", status.command), theme().muted),
        Span::styled(
            format!("{separator}{}", status_text(status.phase)),
            status.style,
        ),
    ]))
    .render(identity, buf);

    if progress.height == 0 {
        return;
    }
    let message =
        deduplicate_status_message(&status_text(status.phase), &status_text(status.message));
    let percent = status.progress.map(percent_u16);
    let percent_text = percent.map_or_else(String::new, |value| format!("{value}%"));
    let available = usize::from(progress.width);
    let separator_width = separator.chars().count();
    let separator_count = if percent.is_some() { 2 } else { 1 };
    let max_message_width = available.saturating_sub(
        percent_text
            .chars()
            .count()
            .saturating_add(separator_width.saturating_mul(separator_count))
            .saturating_add(8),
    );
    let message = truncate_status(&message, max_message_width);
    let fixed_width = message
        .chars()
        .count()
        .saturating_add(percent_text.chars().count())
        .saturating_add(separator_width.saturating_mul(separator_count));
    let bar_width = available.saturating_sub(fixed_width).max(8);
    let mut spans = if status.active {
        shimmer_text(&message, theme().muted, status.tick).spans
    } else {
        vec![Span::styled(message, theme().muted)]
    };
    spans.push(Span::raw(separator));
    spans.extend(
        status_bar(
            status.progress.unwrap_or(0.0),
            bar_width,
            status.tick,
            status.active,
        )
        .spans,
    );
    if let Some(value) = percent {
        spans.push(Span::styled(format!("{separator}{value}%"), status.style));
    }
    Paragraph::new(Line::from(spans)).render(progress, buf);
}

fn status_bar(ratio: f64, width: usize, tick: u64, active: bool) -> Line<'static> {
    let width = width.max(8);
    if ascii_only() {
        let filled = filled_cells(ratio, width);
        return Line::from(vec![
            Span::styled("#".repeat(filled), theme().accent),
            Span::styled("-".repeat(width.saturating_sub(filled)), theme().muted),
        ]);
    }
    shimmer_bar(ratio, width, tick, active)
}

fn status_text(value: &str) -> String {
    if !ascii_only() {
        return value.to_owned();
    }
    value
        .replace('…', "...")
        .replace('→', "->")
        .replace(['—', '·'], "-")
        .replace('×', "x")
        .replace('✓', "ok")
        .replace('✘', "x")
        .replace('▊', "_")
        .replace(['○', '●'], "o")
}

fn deduplicate_status_message(phase: &str, message: &str) -> String {
    let Some(phase_word) = phase.split_whitespace().next() else {
        return message.to_owned();
    };
    let Some(remainder) = message.strip_prefix(phase_word) else {
        return message.to_owned();
    };
    let remainder = remainder.trim_start();
    if remainder.is_empty() {
        message.to_owned()
    } else {
        remainder.to_owned()
    }
}

fn truncate_status(value: &str, max_width: usize) -> String {
    if value.chars().count() <= max_width {
        return value.to_owned();
    }
    let ellipsis = if ascii_only() { "..." } else { "…" };
    if max_width <= ellipsis.chars().count() {
        return ellipsis.chars().take(max_width).collect();
    }
    let mut truncated: String = value
        .chars()
        .take(max_width.saturating_sub(ellipsis.chars().count()))
        .collect();
    truncated.push_str(ellipsis);
    truncated
}

/// Renderiza card com borda arredondada e título.
pub fn card(area: Rect, buf: &mut Buffer, title: &str, body: &str) {
    let block = Block::default()
        .title(Span::styled(format!(" {title} "), theme().accent))
        .borders(Borders::ALL)
        .border_style(theme().border)
        .border_type(border_type());
    Paragraph::new(body)
        .block(block)
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

/// Renderiza footer com atalhos.
pub fn footer(area: Rect, buf: &mut Buffer, hints: &str) {
    Paragraph::new(Line::from(Span::styled(hints, theme().muted))).render(area, buf);
}

/// Renderiza uma barra de progresso isolada para componentes legados.
pub fn progress(area: Rect, buf: &mut Buffer, label: &str, ratio: f64) {
    Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(border_type())
                .border_style(theme().border),
        )
        .gauge_style(theme().accent)
        .label(label)
        .ratio(ratio.clamp(0.0, 1.0))
        .render(area, buf);
}

/// Renderiza lista de checks do `doctor`.
pub fn checks(area: Rect, buf: &mut Buffer, items: &[(bool, &str, &str)]) {
    let rows: Vec<ListItem> = items
        .iter()
        .map(|(ok, name, detail)| {
            let icon = if *ok { "✔" } else { "✘" };
            let style = if *ok { theme().success } else { theme().error };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{icon} "), style),
                Span::styled(*name, Style::new().add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {detail}"), theme().muted),
            ]))
        })
        .collect();
    List::new(rows)
        .block(
            Block::default()
                .title(Span::styled(" Doctor ", theme().accent))
                .borders(Borders::ALL)
                .border_style(theme().border)
                .border_type(border_type()),
        )
        .render(area, buf);
}

/// Abas do fluxo (`Tabs` widget).
#[must_use]
pub fn tabs_widget(titles: Vec<&str>, selected: usize) -> Tabs<'_> {
    Tabs::new(titles)
        .style(theme().tabs)
        .highlight_style(theme().tabs_selected)
        .select(selected)
        .divider(" │ ")
}

/// Layout padrão: header / corpo / footer.
#[must_use]
pub fn app_layout(area: Rect) -> [Rect; 3] {
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    [head, body, foot]
}

/// Layout padrão das telas com status global: header / corpo / footer.
#[must_use]
pub fn status_layout(area: Rect) -> [Rect; 3] {
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    [head, body, foot]
}

/// Limpa popup central (para confirms).
///
/// Legado: dimensiona por percentual e gera vazios em telas altas.
/// Prefira [`modal_frame`] (altura fixa acompanhando o conteúdo).
pub fn centered_popup(area: Rect, buf: &mut Buffer, percent_x: u16, percent_y: u16) -> Rect {
    let vertical =
        Layout::vertical([Constraint::Percentage(percent_y)]).flex(ratatui::layout::Flex::Center);
    let horizontal =
        Layout::horizontal([Constraint::Percentage(percent_x)]).flex(ratatui::layout::Flex::Center);
    let [mid] = vertical.areas(area);
    let [popup] = horizontal.areas(mid);
    dim_background(area, buf);
    Clear.render(popup, buf);
    popup
}

/// Popup modal de altura fixa, centralizado, já com título e borda.
///
/// `content_height` é o número de linhas de conteúdo (**sem** as 2 bordas);
/// a altura final é clampada à área. Altura fixa evita o vazio dos popups
/// percentuais em telas altas — passe exatamente as linhas renderizadas.
#[must_use]
pub fn modal_frame(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    style: Style,
    width: u16,
    content_height: usize,
) -> Rect {
    let height = u16::try_from(content_height.saturating_add(2)).unwrap_or(u16::MAX);
    let width = width.clamp(10, area.width.max(10)).min(area.width);
    let height = height.clamp(3, area.height.max(3)).min(area.height);
    if width == 0 || height == 0 {
        return Rect::default();
    }
    let x = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height) / 2);
    let popup = Rect {
        x,
        y,
        width,
        height,
    };
    dim_background(area, buf);
    Clear.render(popup, buf);
    let block = Block::default()
        .title(Span::styled(title.to_owned(), style))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(style)
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(popup);
    block.render(popup, buf);
    inner
}

/// Diminui o conteúdo já desenhado antes de um modal assumir o foco.
///
/// Terminais não oferecem transparência nem blur por pixel. O modificador
/// `DIM` é o equivalente portátil: preserva o contexto e reduz a competição
/// visual, enquanto o `Clear` do popup remove o efeito na área modal antes de
/// renderizar o seu conteúdo.
fn dim_background(area: Rect, buf: &mut Buffer) {
    buf.set_style(area, Style::new().add_modifier(Modifier::DIM));
}

/// Botões Sim/Não com o selecionado em reverso (padrão dos confirms).
#[must_use]
pub fn yes_no_spans(yes_selected: bool) -> Vec<Span<'static>> {
    let (yes_style, no_style) = if yes_selected {
        (
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            Style::new(),
        )
    } else {
        (
            Style::new(),
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
        )
    };
    vec![
        Span::styled(" Sim ", yes_style),
        Span::styled("   ", Style::new()),
        Span::styled(" Não ", no_style),
    ]
}

/// Linha de botões Sim/Não centralizada na largura interna dada.
///
/// Os botões ocupam 13 células (` Sim ` + 3 espaços + ` Não`); o resto vira
/// respiro igual dos dois lados. Sem centralizar, os botões grudam na
/// esquerda e o diálogo parece torto.
#[must_use]
pub fn centered_buttons(yes_selected: bool, inner_width: u16) -> Line<'static> {
    const BUTTONS_WIDTH: u16 = 13;
    let pad = " ".repeat(usize::from(inner_width.saturating_sub(BUTTONS_WIDTH)) / 2);
    let mut spans = vec![Span::styled(pad, Style::new())];
    spans.extend(yes_no_spans(yes_selected));
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_background_should_mark_existing_cells() {
        let area = Rect::new(0, 0, 20, 10);
        let mut buffer = Buffer::empty(area);

        dim_background(area, &mut buffer);

        assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
        assert!(buffer[(19, 9)].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn modal_frame_should_clear_dim_from_popup_area() {
        let area = Rect::new(0, 0, 20, 10);
        let mut buffer = Buffer::empty(area);

        let _ = modal_frame(area, &mut buffer, " Teste ", theme().accent, 10, 3);

        assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
        assert!(!buffer[(5, 2)].modifier.contains(Modifier::DIM));
    }
}
