//! Render de Markdown para Ratatui com highlight.
//!
//! Converte o `body` gerado (`## Descrição`, checklist, code fences) em
//! [`Text`] estilizado: headings coloridos, checkboxes ☐/☑, bullets •,
//! `inline code` com fundo, blocos de código com prefixo │, links sublinhados.
//! Tolera Markdown parcial (streaming) — fences não fechados ainda rendem.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

use super::{checkbox, colors_enabled, theme};

/// Estilos locais (derivados do tema).
fn h1() -> Style {
    theme()
        .accent
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::UNDERLINED)
}
fn h2() -> Style {
    theme().app_title.add_modifier(Modifier::BOLD)
}
fn h3() -> Style {
    theme().muted.add_modifier(Modifier::BOLD)
}
fn code_block_style() -> Style {
    if !colors_enabled() {
        return Style::new();
    }
    Style::new()
        .fg(Color::Rgb(165, 214, 255))
        .bg(Color::Rgb(22, 27, 34))
}
fn inline_code_style() -> Style {
    if !colors_enabled() {
        return Style::new();
    }
    Style::new()
        .fg(Color::Rgb(34, 211, 238))
        .bg(Color::Rgb(22, 27, 34))
        .add_modifier(Modifier::BOLD)
}
fn quote_style() -> Style {
    if !colors_enabled() {
        return Style::new();
    }
    Style::new()
        .fg(Color::Rgb(139, 148, 158))
        .add_modifier(Modifier::ITALIC)
}

/// Linha de título do PR (H1 fora do body).
#[must_use]
pub fn title_line(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("# ", theme().accent),
        Span::styled(title.to_owned(), h1()),
    ])
}

/// Converte Markdown em [`Text`] com highlight.
///
/// # Exemplos
///
/// ```rust
/// use prt::tui::markdown::markdown_text;
/// let t = markdown_text("## Descrição\n\n- [ ] Bug fix\n");
/// assert!(!t.lines.is_empty());
/// ```
#[must_use]
pub fn markdown_text(body: &str) -> Text<'static> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(body, opts);
    let mut st = MdState::new();
    for ev in parser {
        match ev {
            Event::Start(tag) => st.on_start(tag),
            Event::End(tag) => st.on_end(tag),
            Event::Text(text) => st.on_text(text.as_ref()),
            Event::Code(code) => st.on_code(code.as_ref()),
            Event::TaskListMarker(checked) => st.on_task_marker(checked),
            Event::SoftBreak | Event::HardBreak | Event::Rule => {
                let is_rule = matches!(ev, Event::Rule);
                st.on_break(is_rule);
            }
            // HTML/math/footnotes sem representação na TUI — ignora.
            Event::Html(_)
            | Event::FootnoteReference(_)
            | Event::InlineHtml(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => {}
        }
    }
    st.finish()
}

/// Estado acumulado do render Markdown (extraído p/ `too_many_lines`).
struct MdState {
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    in_code_block: bool,
    in_heading: Option<HeadingLevel>,
    list_stack: Vec<Option<u64>>,
    in_quote: bool,
    strong: u8,
    em: u8,
    code: u8,
    link_dest: Option<String>,
}

impl MdState {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            current: Vec::new(),
            in_code_block: false,
            in_heading: None,
            list_stack: Vec::new(),
            in_quote: false,
            strong: 0,
            em: 0,
            // Contador de `code` inline (sempre 0: `Event::Code` tem ramo próprio).
            code: 0,
            link_dest: None,
        }
    }

    fn flush(&mut self) {
        if self.current.is_empty() {
            self.lines.push(Line::from(""));
        } else {
            self.lines
                .push(Line::from(std::mem::take(&mut self.current)));
        }
    }

    fn on_start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.in_heading = Some(level);
                let prefix = match level {
                    HeadingLevel::H1 => "# ",
                    HeadingLevel::H2 => "## ",
                    HeadingLevel::H3 => "### ",
                    _ => "#### ",
                };
                self.current.push(Span::styled(prefix, theme().muted));
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.in_code_block = true;
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => {
                        let l = l.into_string();
                        if l.is_empty() { "code".to_owned() } else { l }
                    }
                    CodeBlockKind::Indented => "code".to_owned(),
                };
                self.lines.push(Line::from(vec![
                    Span::styled("╭─ ", theme().muted),
                    Span::styled(lang, theme().muted.add_modifier(Modifier::ITALIC)),
                ]));
            }
            Tag::List(n) => self.list_stack.push(n),
            Tag::Item => {
                self.flush();
                // Detecta checkbox no próximo texto? pulldown-cmark entrega
                // `TaskListMarker` separado — prefixo provisório aqui.
                let depth = self.list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let bullet = match self.list_stack.last().copied().flatten() {
                    Some(_) => Span::styled("1. ", theme().accent),
                    None => Span::styled("• ", theme().accent),
                };
                self.current.push(Span::raw(indent));
                self.current.push(bullet);
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.in_quote = true;
                self.current.push(Span::styled("▌ ", theme().muted));
            }
            Tag::Emphasis => self.em += 1,
            Tag::Strong => self.strong += 1,
            Tag::Link { dest_url, .. } => {
                self.link_dest = Some(dest_url.into_string());
            }
            // `Paragraph`, tabelas, imagens etc. não precisam de ação —
            // o wildcard cobre todos com o mesmo corpo vazio.
            _ => {}
        }
    }

    fn on_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                // Aplica cor do heading a todos os spans da linha.
                let style = match self.in_heading {
                    Some(HeadingLevel::H1) => h1(),
                    Some(HeadingLevel::H2) => h2(),
                    _ => h3(),
                };
                for sp in &mut self.current {
                    sp.style = sp.style.patch(style);
                }
                self.flush();
                // Linha em branco após heading para separação visual.
                self.lines.push(Line::from(""));
                self.in_heading = None;
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.flush();
                self.lines
                    .push(Line::from(Span::styled("╰──", theme().muted)));
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
            }
            // `Item` e `Paragraph` só descarregam a linha atual.
            TagEnd::Item | TagEnd::Paragraph => {
                self.flush();
            }
            TagEnd::BlockQuote(_) => {
                self.in_quote = false;
                self.flush();
            }
            TagEnd::Emphasis => self.em = self.em.saturating_sub(1),
            TagEnd::Strong => self.strong = self.strong.saturating_sub(1),
            TagEnd::Link => {
                if let Some(dest) = self.link_dest.take() {
                    self.current
                        .push(Span::styled(format!(" ({dest})"), theme().muted));
                }
            }
            _ => {}
        }
    }

    fn on_text(&mut self, s: &str) {
        if self.in_code_block {
            self.push_code_lines(s);
        } else if self.in_heading.is_some() {
            self.current
                .push(styled_inline(s, self.strong.max(1), self.em, self.code));
        } else {
            self.push_prose(s);
        }
    }

    fn push_code_lines(&mut self, s: &str) {
        for (i, line) in s.lines().enumerate() {
            if i > 0 {
                self.flush();
            }
            self.current.push(Span::styled("│ ", theme().muted));
            self.current
                .push(Span::styled(line.to_owned(), code_block_style()));
        }
        // pulldown-cmark pode entregar code com \n final — garante flush.
        if s.ends_with('\n') {
            self.flush();
        }
    }

    fn push_prose(&mut self, s: &str) {
        // Checklist inline: "- [ ] texto" quando o parser não emitiu TaskListMarker
        // (ex.: dentro de parágrafo). Trata prefixos manualmente.
        let mut rest = s;
        // Se a linha atual só tem indent+bullet, verifica marcador.
        let line_is_fresh = self.current.len() <= 2;
        if line_is_fresh {
            if let Some(after) = rest.strip_prefix("[ ] ") {
                self.current.pop(); // remove bullet genérico
                self.current.push(Span::styled(
                    format!("{} ", checkbox(false)),
                    theme().warning,
                ));
                rest = after;
            } else if let Some(after) = rest
                .strip_prefix("[x] ")
                .or_else(|| rest.strip_prefix("[X] "))
            {
                self.current.pop();
                self.current.push(Span::styled(
                    format!("{} ", checkbox(true)),
                    theme().success,
                ));
                rest = after;
            }
        }
        // Quebra por \n preservando estilos.
        let mut parts = rest.split('\n').peekable();
        while let Some(part) = parts.next() {
            if !part.is_empty() {
                if self.in_quote {
                    self.current.push(Span::styled(
                        part.to_owned(),
                        quote_style().patch(if self.strong > 0 {
                            Style::new().add_modifier(Modifier::BOLD)
                        } else {
                            Style::new()
                        }),
                    ));
                } else {
                    self.current
                        .push(styled_inline(part, self.strong, self.em, self.code));
                }
            }
            if parts.peek().is_some() {
                self.flush();
                // Reaplica prefixo de continuação (lista/quote).
                if self.in_quote {
                    self.current.push(Span::styled("▌ ", theme().muted));
                }
            }
        }
    }

    fn on_code(&mut self, code: &str) {
        self.current
            .push(Span::styled(format!(" `{code}` "), inline_code_style()));
    }

    fn on_task_marker(&mut self, checked: bool) {
        // Substitui o bullet genérico pelo checkbox colorido.
        self.current.pop();
        if checked {
            self.current.push(Span::styled(
                format!("{} ", checkbox(true)),
                theme().success,
            ));
        } else {
            self.current.push(Span::styled(
                format!("{} ", checkbox(false)),
                theme().warning,
            ));
        }
    }

    fn on_break(&mut self, is_rule: bool) {
        self.flush();
        if is_rule {
            self.lines
                .push(Line::from(Span::styled("─".repeat(40), theme().muted)));
        }
    }

    fn finish(mut self) -> Text<'static> {
        if !self.current.is_empty() {
            self.lines
                .push(Line::from(std::mem::take(&mut self.current)));
        }
        // Sublinha links: pulldown já anexou destino; aplica underline ao texto
        // anterior quando há destino? (simplificado: mantém como está).
        let _ = self.link_dest;
        Text::from(self.lines)
    }
}

/// Aplica estilos inline ao texto puro.
fn styled_inline(text: &str, strong: u8, em: u8, code: u8) -> Span<'static> {
    if code > 0 {
        return Span::styled(format!(" `{text}` "), inline_code_style());
    }
    let mut style = Style::new();
    if strong > 0 {
        style = style.add_modifier(Modifier::BOLD);
    }
    if em > 0 {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if style == Style::new() {
        Span::raw(text.to_owned())
    } else {
        Span::styled(text.to_owned(), style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_should_produce_lines() {
        let t = markdown_text("## Descrição\n\nResumo aqui\n");
        assert!(t.lines.len() >= 3);
    }

    #[test]
    fn checklist_should_render_boxes() {
        let t = markdown_text("- [ ] Bug fix\n- [x] Nova feature\n");
        let flat: String = t
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.clone()))
            .collect();
        assert!(flat.contains('☐'));
        assert!(flat.contains('☑'));
    }

    #[test]
    fn code_block_should_have_border() {
        let t = markdown_text("```diff\n+ linha nova\n```\n");
        let flat: String = t
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.clone()))
            .collect();
        assert!(flat.contains('│'));
    }

    #[test]
    fn partial_markdown_should_not_panic() {
        let t = markdown_text("## Descr\n\n- [ ] item sem fim\n```diff\n+ parcial");
        assert!(!t.lines.is_empty());
    }
}
