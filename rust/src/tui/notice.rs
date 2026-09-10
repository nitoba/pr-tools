//! Tela de aviso/resultado em Ratatui — fim dos `println!` interativos.
//!
//! Toda interação que mostra texto em tela passa por aqui ou pelos loops
//! vivos (`live`, `init_wizard`): painel tela-cheia com borda, conteúdo
//! rolável (`j/k`, `PgUp/PgDn`) e saída com `q`/`enter`/`esc`.
//! `println!` fica restrito ao protocolo CLI (`--help`, `--version`) e a
//! saídas não-tela (`--raw`, stdout em pipe).

use std::io::IsTerminal;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget, Wrap,
    },
};

use super::shimmer::{i16_from_u16_saturated, u16_from_i32_clamped, u16_from_usize_saturated};
use super::{app_layout, border_type, theme};

/// Tom da tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// Informação neutra (borda ciano).
    Info,
    /// Sucesso (borda verde).
    Success,
    /// Aviso/erro (borda amarela/vermelha).
    Warning,
}

impl NoticeKind {
    fn border(self) -> Style {
        match self {
            Self::Info => theme().border,
            Self::Success => theme().success,
            Self::Warning => theme().warning,
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Info => "◆",
            Self::Success => "✔",
            Self::Warning => "!",
        }
    }
}

/// Estado da tela de aviso.
pub struct Notice<'a> {
    title: &'a str,
    text: Text<'a>,
    kind: NoticeKind,
    scroll: u16,
}

impl<'a> Notice<'a> {
    /// Cria aviso rolável.
    #[must_use]
    pub fn new(title: &'a str, text: Text<'a>, kind: NoticeKind) -> Self {
        Self {
            title,
            text,
            kind,
            scroll: 0,
        }
    }

    fn scroll_by(&mut self, delta: i16, height: u16) {
        let max =
            u16_from_usize_saturated(self.text.lines.len().saturating_sub(usize::from(height)));
        let next = i32::from(self.scroll) + i32::from(delta);
        self.scroll = u16_from_i32_clamped(next, max);
    }
}

impl Widget for &Notice<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 60 || area.height < 20 {
            render_too_small(area, buf);
            return;
        }
        let [head, body, foot] = app_layout(area);
        Block::new().style(theme().root).render(area, buf);

        // Header parado: a tela é um aviso estático, sem trabalho em curso.
        Paragraph::new(Line::from(vec![
            Span::styled("● ", theme().accent),
            Span::styled("◆ prt ", theme().app_title),
            Span::styled(crate::cli::VERSION, theme().muted),
        ]))
        .render(head, buf);

        let block = Block::default()
            .title(Span::styled(
                format!(" {} {} ", self.kind.glyph(), self.title),
                self.kind.border().add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(self.kind.border())
            .border_type(border_type())
            .padding(ratatui::widgets::Padding::horizontal(1));
        let inner = block.inner(body);
        block.render(body, buf);
        Paragraph::new(self.text.clone())
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(inner, buf);
        let mut state =
            ScrollbarState::new(self.text.lines.len().max(1)).position(self.scroll as usize);
        <Scrollbar as ratatui::widgets::StatefulWidget>::render(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            inner,
            buf,
            &mut state,
        );

        Paragraph::new(Line::from(Span::styled(
            "j/k rolar · PgUp/PgDn página · q/enter/esc sair",
            theme().muted,
        )))
        .render(foot, buf);
    }
}

/// Piso mínimo honesto: terminal pequeno vira mingau, então avisa.
fn render_too_small(area: Rect, buf: &mut Buffer) {
    Block::new().style(theme().root).render(area, buf);
    let msg = format!(
        "terminal muito pequeno — mínimo 60×20 (atual {}x{})",
        area.width, area.height
    );
    let width = area.width.saturating_sub(2).min(56);
    let height = 5.min(area.height);
    if width < 10 || height < 3 {
        return;
    }
    let x = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height) / 2);
    let popup = Rect::new(x, y, width, height);
    Paragraph::new(vec![
        Line::from(Span::styled(msg, theme().warning)),
        Line::from(Span::styled(
            "aumente o terminal e tente de novo",
            theme().muted,
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .title(Span::styled(" Aviso ", theme().warning))
            .borders(Borders::ALL)
            .border_style(theme().warning)
            .border_type(border_type()),
    )
    .render(popup, buf);
}

/// Exibe a tela até o usuário sair.
///
/// # Errors
///
/// Retorna erro se o terminal não puder ser inicializado.
pub async fn show_notice(title: &str, text: Text<'static>, kind: NoticeKind) -> anyhow::Result<()> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("aviso requer terminal interativo");
    }
    let mut terminal: DefaultTerminal = ratatui::init();
    // `run_loop` é síncrono (sem `.await` interno); o `async move`
    // mantém `show_notice` aguardável pelos chamadores sem mudar comportamento.
    let res = async move { run_loop(&mut terminal, title, text, kind) }.await;
    ratatui::restore();
    res
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    title: &str,
    text: Text<'static>,
    kind: NoticeKind,
) -> anyhow::Result<()> {
    let mut notice = Notice::new(title, text, kind);
    // Tela 100% estática: desenha no primeiro frame e a cada input
    // (inclui resize, que chega como evento). Sem tick — nada se move.
    // Altura visível aproximada (recalculada de verdade no render).
    let mut view_height: u16 = 20;
    let mut dirty = true;

    loop {
        if dirty {
            terminal.draw(|f| {
                view_height = f.area().height.saturating_sub(4);
                f.render_widget(&notice, f.area());
            })?;
            dirty = false;
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let ev = event::read()?;
        let Event::Key(key) = ev else {
            // Resize e outros eventos: redesenha sob demanda.
            dirty = true;
            continue;
        };
        // No Windows, Release dispara junto — ignora para não processar 2×.
        if key.kind == KeyEventKind::Release {
            continue;
        }
        // Repeat só faz sentido p/ rolagem; saída exige Press.
        if key.kind == KeyEventKind::Repeat
            && !matches!(
                key.code,
                KeyCode::Char('j' | 'k')
                    | KeyCode::Down
                    | KeyCode::Up
                    | KeyCode::PageDown
                    | KeyCode::PageUp
            )
        {
            continue;
        }
        match key.code {
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                #[cfg(unix)]
                {
                    super::suspend::suspend_to_shell(&mut *terminal)?;
                }
            }
            KeyCode::Char('q' | ' ') | KeyCode::Enter | KeyCode::Esc => {
                return Ok(());
            }
            KeyCode::Char('j') | KeyCode::Down => notice.scroll_by(3, view_height),
            KeyCode::Char('k') | KeyCode::Up => notice.scroll_by(-3, view_height),
            KeyCode::PageDown => {
                notice.scroll_by(i16_from_u16_saturated(view_height), view_height);
            }
            KeyCode::PageUp => {
                notice.scroll_by(-i16_from_u16_saturated(view_height), view_height);
            }
            KeyCode::Home => notice.scroll = 0,
            KeyCode::End => notice.scroll = u16::MAX,
            _ => {}
        }
        // `End` ajustado no próximo `scroll_by`; corrige direto aqui.
        if notice.scroll == u16::MAX {
            let max = u16_from_usize_saturated(
                notice
                    .text
                    .lines
                    .len()
                    .saturating_sub(usize::from(view_height)),
            );
            notice.scroll = max;
        }
        dirty = true;
    }
}

/// Atalho para aviso informativo estático.
#[must_use]
pub fn info_text(body: &str) -> Text<'static> {
    Text::from(body.to_owned())
}

/// Seção com título destacado, para montar avisos multi-parte.
#[must_use]
pub fn section(title: &str, body: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            title.to_owned(),
            theme().accent.add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(body.to_owned(), Style::new())),
        Line::from(""),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_should_clamp_to_content() {
        let mut n = Notice::new("t", Text::from("a\nb\nc"), NoticeKind::Info);
        n.scroll_by(-5, 10);
        assert_eq!(n.scroll, 0);
        n.scroll_by(100, 1);
        assert_eq!(n.scroll, 2);
    }

    #[test]
    fn section_should_build_titled_block() {
        let lines = section("Sistema", "prompt…");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn notice_info_80x24() -> anyhow::Result<()> {
        use ratatui::{Terminal, backend::TestBackend};
        let notice = Notice::new(
            "Teste",
            Text::from("Operação concluída.\nRevise o resumo acima."),
            NoticeKind::Info,
        );
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&notice, f.area()))?;
        insta::assert_snapshot!("notice_info_80x24", terminal.backend());
        Ok(())
    }
}
