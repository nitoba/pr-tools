//! TUI com Ratatui — visual moderno que substitui `terminice` do Dart.
//!
//! Dois níveis:
//! - helpers estáticos (`header`, `card`, …) para `doctor`/`init`;
//! - loop vivo reativo (`live::run_describe_tui`) para `desc`: backend em
//!   tokio empurra `Token`/`Log`/`Progress` por `mpsc` e cada frame (~30fps)
//!   redesenha spinner, preview com cursor, logs auto-scroll e barra de
//!   progresso com shimmer.

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

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph, Tabs, Widget, Wrap,
    },
};

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
/// Verdadeiro quando `TERM == "dumb"` ou `PRT_ASCII == "1"`.
/// Lido a cada chamada para respeitar o ambiente.
#[must_use]
pub fn ascii_only() -> bool {
    if std::env::var("TERM").is_ok_and(|v| v == "dumb") {
        return true;
    }
    std::env::var("PRT_ASCII").is_ok_and(|v| v == "1")
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

// Faixas de frames do spinner (detalhe interno de [`spin_frames`]).
const SPIN_BRAILLE: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPIN_ASCII: &[&str] = &["-", "\\", "|", "/"];

/// Frames do spinner efetivo: Braille normal, ASCII simples.
///
/// Prefira este aos arrays locais de frames nos renders para
/// respeitar `PRT_ASCII=1`/`TERM=dumb`.
#[must_use]
pub fn spin_frames() -> &'static [&'static str] {
    if ascii_only() {
        SPIN_ASCII
    } else {
        SPIN_BRAILLE
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
    let line = Line::from(vec![
        Span::styled("◆ prt ", theme().app_title),
        Span::styled(crate::cli::VERSION, theme().muted),
        Span::styled(format!("  ·  {subtitle}"), theme().muted),
    ]);
    Paragraph::new(line).render(area, buf);
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

/// Renderiza barra de progresso (spinner de geração via IA).
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
