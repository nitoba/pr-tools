//! Editor de título e Markdown compartilhado pelos fluxos `desc` e `test`.
//!
//! O estado é deliberadamente draft-only: o app hospedeiro decide quando
//! aplicar um [`ContentEditAction::Saved`] à sua representação canônica.

use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use unicode_width::UnicodeWidthChar;

use super::{ascii_only, modal_frame, theme};
use crate::ai::PrDescription;

/// Campo de conteúdo que recebe texto e navegação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentField {
    /// Título de uma linha.
    Title,
    /// Corpo Markdown multilinha.
    Body,
}

impl ContentField {
    fn toggle(self) -> Self {
        match self {
            Self::Title => Self::Body,
            Self::Body => Self::Title,
        }
    }
}

/// Regra de validação usada pelo fluxo que abriu o editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentValidation {
    /// Título obrigatório e body do PR com menos de 4000 caracteres.
    PullRequest,
    /// Título e body não vazios após trim, sem limite de tamanho do PR.
    TestCase,
}

/// Erro de conteúdo exibido no campo que precisa de correção.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentValidationError {
    /// Título vazio após trim.
    EmptyTitle,
    /// Body do Test Case vazio após trim.
    EmptyBody,
    /// Body do PR atingiu o limite exclusivo de 4000 caracteres.
    BodyTooLong {
        /// Quantidade de caracteres recebida.
        length: usize,
    },
}

impl ContentValidationError {
    /// Campo ao qual o erro pertence.
    #[must_use]
    pub const fn field(&self) -> ContentField {
        match self {
            Self::EmptyTitle => ContentField::Title,
            Self::EmptyBody | Self::BodyTooLong { .. } => ContentField::Body,
        }
    }
}

impl fmt::Display for ContentValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTitle => write!(formatter, "título é obrigatório"),
            Self::EmptyBody => write!(formatter, "corpo é obrigatório para criar o Test Case"),
            Self::BodyTooLong { length } => write!(
                formatter,
                "corpo do PR deve ter menos de 4000 caracteres (atual: {length})"
            ),
        }
    }
}

/// Resultado de uma tecla consumida pelo editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentEditAction {
    /// A tecla alterou ou navegou no draft.
    Consumed,
    /// O draft passou na validação e pode substituir o conteúdo aprovado.
    Saved(PrDescription),
    /// O draft foi cancelado sem alterar o conteúdo aprovado.
    Cancelled,
    /// A tecla pertence a uma camada externa do TUI.
    Ignored,
}

/// Editor de texto Unicode-safe com cursor em índice de caractere.
#[derive(Debug, Clone)]
pub struct TextEditor {
    /// Conteúdo editável.
    value: String,
    /// Cursor em índice de `char`, nunca em byte.
    cursor: usize,
    /// Se quebras de linha devem ser bloqueadas.
    single_line: bool,
    /// Primeira linha lógica visível.
    scroll_row: usize,
    /// Primeira coluna visual visível.
    scroll_col: usize,
    /// Coluna desejada durante movimento vertical.
    preferred_column: Option<usize>,
}

impl TextEditor {
    /// Cria um editor com o cursor no final do valor.
    #[must_use]
    pub fn new(value: impl Into<String>, single_line: bool) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self {
            value,
            cursor,
            single_line,
            scroll_row: 0,
            scroll_col: 0,
            preferred_column: None,
        }
    }

    /// Retorna o texto atual sem normalização.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Retorna o cursor em índice de caractere.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Insere texto como foi recebido, respeitando a restrição single-line.
    pub fn insert_text(&mut self, text: &str) {
        for character in text.chars() {
            if self.single_line && matches!(character, '\n' | '\r') {
                continue;
            }
            self.insert_char(character);
        }
    }

    /// Trata uma tecla de edição; retorna `true` quando a tecla foi consumida.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release
            || key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        match key.code {
            KeyCode::Char(character) => {
                self.insert_char(character);
                true
            }
            KeyCode::Backspace => {
                self.delete_before_cursor();
                true
            }
            KeyCode::Delete => {
                self.delete_at_cursor();
                true
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                self.reset_vertical_navigation();
                true
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.value.chars().count());
                self.reset_vertical_navigation();
                true
            }
            KeyCode::Up => {
                self.move_vertical(-1);
                true
            }
            KeyCode::Down => {
                self.move_vertical(1);
                true
            }
            KeyCode::Enter => {
                self.handle_enter();
                true
            }
            KeyCode::Home => {
                let (line, _) = self.current_line_and_column();
                self.cursor = self.line_starts()[line];
                self.reset_vertical_navigation();
                true
            }
            KeyCode::End => {
                let (line, _) = self.current_line_and_column();
                self.cursor = self.line_end(line);
                self.reset_vertical_navigation();
                true
            }
            KeyCode::PageUp => {
                self.scroll_row = self.scroll_row.saturating_sub(5);
                true
            }
            KeyCode::PageDown => {
                self.scroll_row = self.scroll_row.saturating_add(5);
                true
            }
            _ => false,
        }
    }

    /// Insere uma quebra no body ou consome `Enter` no título.
    fn handle_enter(&mut self) {
        if !self.single_line {
            self.insert_char('\n');
        }
    }

    fn insert_char(&mut self, character: char) {
        if self.single_line && matches!(character, '\n' | '\r') {
            return;
        }
        let byte = char_byte_index(&self.value, self.cursor);
        self.value.insert(byte, character);
        self.cursor += 1;
        self.reset_vertical_navigation();
    }

    fn delete_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let byte = char_byte_index(&self.value, self.cursor);
        let previous = char_byte_index(&self.value, self.cursor - 1);
        self.value.drain(previous..byte);
        self.cursor -= 1;
        self.reset_vertical_navigation();
    }

    fn delete_at_cursor(&mut self) {
        let length = self.value.chars().count();
        if self.cursor >= length {
            return;
        }
        let byte = char_byte_index(&self.value, self.cursor);
        let next = char_byte_index(&self.value, self.cursor + 1);
        self.value.drain(byte..next);
        self.reset_vertical_navigation();
    }

    fn reset_vertical_navigation(&mut self) {
        self.preferred_column = None;
    }

    fn line_starts(&self) -> Vec<usize> {
        let mut starts = vec![0];
        for (index, character) in self.value.chars().enumerate() {
            if character == '\n' {
                starts.push(index + 1);
            }
        }
        starts
    }

    fn line_end(&self, line: usize) -> usize {
        let starts = self.line_starts();
        let start = starts.get(line).copied().unwrap_or(0);
        starts
            .get(line + 1)
            .copied()
            .map_or_else(|| self.value.chars().count(), |next| next.saturating_sub(1))
            .max(start)
    }

    fn current_line_and_column(&self) -> (usize, usize) {
        let starts = self.line_starts();
        let cursor = self.cursor.min(self.value.chars().count());
        let line = starts
            .iter()
            .enumerate()
            .rfind(|(_, start)| **start <= cursor)
            .map_or(0, |(line, _)| line);
        let column = cursor.saturating_sub(starts[line]);
        (
            line,
            column.min(self.line_end(line).saturating_sub(starts[line])),
        )
    }

    fn move_vertical(&mut self, delta: isize) {
        let (line, column) = self.current_line_and_column();
        let desired = self.preferred_column.unwrap_or(column);
        let target = if delta.is_negative() {
            line.saturating_sub(delta.unsigned_abs())
        } else {
            line.saturating_add(delta.unsigned_abs())
        };
        let starts = self.line_starts();
        let target = target.min(starts.len().saturating_sub(1));
        let target_start = starts[target];
        let target_column = desired.min(self.line_end(target).saturating_sub(target_start));
        self.cursor = target_start + target_column;
        self.preferred_column = Some(desired);
    }

    fn line_text(&self, line: usize) -> Vec<char> {
        let starts = self.line_starts();
        let Some(&start) = starts.get(line) else {
            return Vec::new();
        };
        let end = self.line_end(line);
        self.value
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect()
    }

    fn line_cell_width(chars: &[char]) -> usize {
        chars
            .iter()
            .map(|character| UnicodeWidthChar::width(*character).unwrap_or(0))
            .sum()
    }

    fn cursor_cell_column(&self, line: usize) -> usize {
        let starts = self.line_starts();
        let column = self
            .cursor
            .saturating_sub(starts.get(line).copied().unwrap_or_default());
        Self::line_cell_width(&self.line_text(line)[..column.min(self.line_text(line).len())])
    }

    fn visible_start_row(&self, height: usize) -> usize {
        let (cursor_line, _) = self.current_line_and_column();
        let mut start = self.scroll_row;
        if cursor_line < start {
            start = cursor_line;
        } else if cursor_line >= start.saturating_add(height) {
            start = cursor_line.saturating_sub(height.saturating_sub(1));
        }
        start
    }

    fn render_line(&self, line: usize, horizontal_start: usize, width: usize) -> Line<'static> {
        let chars = self.line_text(line);
        let starts = self.line_starts();
        let cursor_line = self.current_line_and_column().0;
        let cursor_column = self
            .cursor
            .saturating_sub(starts.get(line).copied().unwrap_or_default());
        let mut spans = Vec::new();
        let mut cell = 0usize;
        for (column, character) in chars.iter().copied().enumerate() {
            let char_width = UnicodeWidthChar::width(character).unwrap_or(0);
            let end = cell.saturating_add(char_width.max(1));
            if end > horizontal_start && cell < horizontal_start.saturating_add(width) {
                let style = if line == cursor_line && column == cursor_column {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                spans.push(Span::styled(character.to_string(), style));
            }
            cell = cell.saturating_add(char_width);
        }
        if line == cursor_line && cursor_column >= chars.len() {
            let cursor_cell = self.cursor_cell_column(line);
            if cursor_cell >= horizontal_start
                && cursor_cell < horizontal_start.saturating_add(width)
            {
                spans.push(Span::styled(
                    " ",
                    Style::new().add_modifier(Modifier::REVERSED),
                ));
            }
        }
        if spans.is_empty() && line == cursor_line {
            spans.push(Span::styled(
                " ",
                Style::new().add_modifier(Modifier::REVERSED),
            ));
        }
        Line::from(spans)
    }

    /// Renderiza linhas visíveis, seguindo cursor e viewport sem alterar o texto.
    #[must_use]
    pub fn visible_lines(&self, width: usize, height: usize) -> Vec<Line<'static>> {
        if width == 0 || height == 0 {
            return Vec::new();
        }
        let start_row = self.visible_start_row(height);
        let (cursor_line, _) = self.current_line_and_column();
        let cursor_cell = self.cursor_cell_column(cursor_line);
        let mut horizontal_start = self.scroll_col;
        if cursor_cell < horizontal_start {
            horizontal_start = cursor_cell;
        } else if cursor_cell >= horizontal_start.saturating_add(width) {
            horizontal_start = cursor_cell.saturating_sub(width.saturating_sub(1));
        }
        (start_row..start_row.saturating_add(height))
            .map(|line| self.render_line(line, horizontal_start, width))
            .collect()
    }
}

/// Draft compartilhado dos dois campos editáveis.
#[derive(Debug, Clone)]
pub struct ContentEditState {
    /// Editor do título single-line.
    pub title: TextEditor,
    /// Editor do body multiline.
    pub body: TextEditor,
    /// Campo focado.
    pub field: ContentField,
    /// Regra de validação do fluxo hospedeiro.
    pub validation: ContentValidation,
    /// Erro atual, se o último save falhou.
    pub error: Option<ContentValidationError>,
}

impl ContentEditState {
    /// Cria um draft para uma descrição PR.
    #[must_use]
    pub fn for_pr(content: &PrDescription) -> Self {
        Self::new(
            content.title.as_str(),
            content.body.as_str(),
            ContentValidation::PullRequest,
        )
    }

    /// Cria um draft para um Test Case.
    #[must_use]
    pub fn for_test(title: &str, body: &str) -> Self {
        Self::new(title, body, ContentValidation::TestCase)
    }

    /// Cria um draft com a regra de validação informada.
    #[must_use]
    pub fn new(title: &str, body: &str, validation: ContentValidation) -> Self {
        Self {
            title: TextEditor::new(title, true),
            body: TextEditor::new(body, false),
            field: ContentField::Title,
            validation,
            error: None,
        }
    }

    /// Retorna o texto do campo atualmente focado.
    #[must_use]
    pub fn focused_value(&self) -> &str {
        match self.field {
            ContentField::Title => self.title.value(),
            ContentField::Body => self.body.value(),
        }
    }

    /// Define o erro de conteúdo e foca o campo que precisa de correção.
    pub fn set_error(&mut self, error: ContentValidationError) {
        self.field = error.field();
        self.error = Some(error);
    }

    /// Valida e devolve o conteúdo exato, sem normalização.
    ///
    /// # Errors
    ///
    /// Retorna o primeiro erro de título/body conforme a regra do fluxo.
    pub fn validate(&self) -> Result<PrDescription, ContentValidationError> {
        if self.title.value().trim().is_empty() {
            return Err(ContentValidationError::EmptyTitle);
        }
        match self.validation {
            ContentValidation::PullRequest => {
                let length = self.body.value().chars().count();
                if length >= 4000 {
                    return Err(ContentValidationError::BodyTooLong { length });
                }
            }
            ContentValidation::TestCase if self.body.value().trim().is_empty() => {
                return Err(ContentValidationError::EmptyBody);
            }
            ContentValidation::TestCase => {}
        }
        Ok(PrDescription {
            title: self.title.value().to_owned(),
            body: self.body.value().to_owned(),
        })
    }

    /// Trata uma tecla e devolve a transição resultante.
    pub fn handle_key(&mut self, key: KeyEvent) -> ContentEditAction {
        if key.kind == KeyEventKind::Release {
            return ContentEditAction::Ignored;
        }
        if key.code == KeyCode::Char('s')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT)
        {
            return match self.validate() {
                Ok(content) => ContentEditAction::Saved(content),
                Err(error) => {
                    self.set_error(error);
                    ContentEditAction::Consumed
                }
            };
        }
        if key.code == KeyCode::Esc && key.modifiers.is_empty() {
            return ContentEditAction::Cancelled;
        }
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            self.field = self.field.toggle();
            return ContentEditAction::Consumed;
        }
        let consumed = match self.field {
            ContentField::Title => {
                if key.code == KeyCode::Enter {
                    true
                } else {
                    self.title.handle_key(key)
                }
            }
            ContentField::Body => {
                if key.code == KeyCode::Enter
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                {
                    self.body.handle_enter();
                    true
                } else {
                    self.body.handle_key(key)
                }
            }
        };
        if consumed {
            self.error = None;
            ContentEditAction::Consumed
        } else {
            ContentEditAction::Ignored
        }
    }
}

/// Renderiza o modal compartilhado de edição de conteúdo.
pub fn render_content_editor(state: &ContentEditState, area: Rect, buf: &mut Buffer) {
    // O editor é um modal de uso prolongado: ocupar a maior parte da tela
    // deixa o Markdown legível sem esconder completamente o contexto.
    let modal_width = area.width.clamp(56, 120);
    let modal_content_height = usize::from(area.height.saturating_sub(4)).clamp(10, 32);
    let inner = modal_frame(
        area,
        buf,
        " Editar conteúdo ",
        theme().accent,
        modal_width,
        modal_content_height,
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let error_height = usize::from(state.error.is_some());
    let [
        header,
        title_label,
        title_area,
        divider,
        body_label,
        body_area,
        footer,
    ] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(u16::try_from(error_height + 1).unwrap_or(u16::MAX)),
    ])
    .areas(inner);

    render_editor_header(state, header, buf);
    let title_error = field_has_error(state, ContentField::Title);
    render_field_label(
        title_label,
        "Título",
        &format!("{} caracteres", state.title.value().chars().count()),
        state.field == ContentField::Title,
        title_error,
        buf,
    );
    render_editor_value(
        &state.title,
        title_area,
        state.field == ContentField::Title,
        title_error,
        buf,
    );
    render_divider(divider, buf);
    let body_detail = match state.validation {
        ContentValidation::PullRequest => {
            format!("{} / 3999 caracteres", state.body.value().chars().count())
        }
        ContentValidation::TestCase => {
            format!("{} caracteres", state.body.value().chars().count())
        }
    };
    render_field_label(
        body_label,
        "Body Markdown",
        body_detail.as_str(),
        state.field == ContentField::Body,
        field_has_error(state, ContentField::Body),
        buf,
    );
    render_editor_value(
        &state.body,
        body_area,
        state.field == ContentField::Body,
        field_has_error(state, ContentField::Body),
        buf,
    );
    render_editor_footer(state, footer, buf);
}

fn field_has_error(state: &ContentEditState, field: ContentField) -> bool {
    state
        .error
        .as_ref()
        .is_some_and(|error| error.field() == field)
}

fn render_editor_header(state: &ContentEditState, area: Rect, buf: &mut Buffer) {
    let context = match state.validation {
        ContentValidation::PullRequest => "Pull Request",
        ContentValidation::TestCase => "Test Case",
    };
    Paragraph::new(vec![
        Line::from(vec![
            Span::styled("RASCUNHO", theme().app_title),
            Span::styled("  ·  ", theme().muted),
            Span::styled(context, theme().accent.add_modifier(Modifier::BOLD)),
        ]),
        Line::from(Span::styled(
            "Tab muda campo · Enter cria linha no body · setas navegam",
            theme().muted,
        )),
    ])
    .style(theme().muted)
    .render(area, buf);
}

fn render_editor_footer(state: &ContentEditState, area: Rect, buf: &mut Buffer) {
    let footer_lines = if let Some(error) = &state.error {
        vec![
            Line::from(Span::styled(format!("✘ {error}"), theme().error)),
            Line::from(Span::styled("Ctrl+S salva · Esc cancela", theme().muted)),
        ]
    } else {
        vec![Line::from(Span::styled(
            "Ctrl+S salva · Esc cancela",
            theme().muted,
        ))]
    };
    Paragraph::new(footer_lines)
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

fn render_field_label(
    area: Rect,
    label: &str,
    detail: &str,
    focused: bool,
    has_error: bool,
    buf: &mut Buffer,
) {
    let marker = if has_error {
        "✘"
    } else if focused {
        "▸"
    } else {
        " "
    };
    let style = if has_error {
        theme().error
    } else if focused {
        theme().accent.add_modifier(Modifier::BOLD)
    } else {
        theme().muted
    };
    let mut spans = vec![
        Span::styled(format!("{marker} {label}"), style),
        Span::styled(format!("  ·  {detail}"), theme().muted),
    ];
    if focused {
        spans.push(Span::styled("  ·  EDITANDO", theme().accent));
    }
    if has_error {
        spans.push(Span::styled("  ·  CORRIGIR", theme().error));
    }
    Paragraph::new(Line::from(spans))
        .style(theme().root)
        .render(area, buf);
}

fn render_editor_value(
    editor: &TextEditor,
    area: Rect,
    focused: bool,
    has_error: bool,
    buf: &mut Buffer,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rail_style = if has_error {
        theme().error
    } else if focused {
        theme().accent
    } else {
        theme().muted
    };
    let rail = if has_error {
        "!"
    } else if focused {
        "▌"
    } else {
        "│"
    };
    for row in area.y..area.y.saturating_add(area.height) {
        buf.set_string(area.x, row, rail, rail_style);
    }
    let content_area = Rect {
        x: area.x.saturating_add(2),
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    };
    Paragraph::new(editor.visible_lines(
        usize::from(content_area.width),
        usize::from(content_area.height),
    ))
    .style(theme().root)
    .wrap(Wrap { trim: false })
    .render(content_area, buf);
}

fn render_divider(area: Rect, buf: &mut Buffer) {
    let glyph = if ascii_only() { "-" } else { "─" };
    Paragraph::new(Line::from(Span::styled(
        glyph.repeat(usize::from(area.width)),
        theme().border,
    )))
    .render(area, buf);
}

/// Índice de byte do n-ésimo `char`, saturado no fim.
fn char_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(byte, _)| byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn content_editor_unicode_and_multiline_navigation_should_preserve_text() {
        let mut editor = TextEditor::new(String::new(), false);
        editor.insert_text("Título — ação ✅");
        editor.handle_key(key(KeyCode::Enter));
        editor.insert_text("ação concluída\n- [ ] validar\nlinha final");

        assert_eq!(
            editor.value(),
            "Título — ação ✅\nação concluída\n- [ ] validar\nlinha final"
        );
        editor.handle_key(key(KeyCode::Home));
        assert_eq!(editor.value().chars().nth(editor.cursor()), Some('l'));
        editor.handle_key(key(KeyCode::Up));
        editor.handle_key(key(KeyCode::End));
        assert_eq!(editor.value().chars().nth(editor.cursor()), Some('\n'));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::End));
        assert_eq!(editor.value().chars().nth(editor.cursor()), None);
        editor.handle_key(key(KeyCode::Left));
        assert_eq!(editor.value().chars().nth(editor.cursor()), Some('l'));
    }

    #[test]
    fn content_editor_tab_should_cycle_only_title_and_body() {
        let mut state = ContentEditState::for_pr(&PrDescription {
            title: "T".to_owned(),
            body: "B".to_owned(),
        });
        assert_eq!(state.field, ContentField::Title);
        assert!(matches!(
            state.handle_key(key(KeyCode::Tab)),
            ContentEditAction::Consumed
        ));
        assert_eq!(state.field, ContentField::Body);
        assert!(matches!(
            state.handle_key(key(KeyCode::BackTab)),
            ContentEditAction::Consumed
        ));
        assert_eq!(state.field, ContentField::Title);
    }

    #[test]
    fn content_editor_enter_should_insert_body_newline_only() {
        let mut state = ContentEditState::for_pr(&PrDescription {
            title: "T".to_owned(),
            body: "ab".to_owned(),
        });
        let title_before = state.title.value().to_owned();
        assert!(matches!(
            state.handle_key(key(KeyCode::Enter)),
            ContentEditAction::Consumed
        ));
        assert_eq!(state.title.value(), title_before);
        state.field = ContentField::Body;
        state.body.handle_key(key(KeyCode::Home));
        state.body.handle_key(key(KeyCode::Right));
        assert!(matches!(
            state.handle_key(key(KeyCode::Enter)),
            ContentEditAction::Consumed
        ));
        assert_eq!(state.body.value(), "a\nb");
    }

    #[test]
    fn content_editor_should_consume_review_shortcuts() {
        let mut state = ContentEditState::for_pr(&PrDescription {
            title: "T".to_owned(),
            body: "B".to_owned(),
        });
        for character in ['q', 'j', 'k', 'c'] {
            assert!(matches!(
                state.handle_key(key(KeyCode::Char(character))),
                ContentEditAction::Consumed
            ));
        }
        assert_eq!(state.title.value(), "Tqjkc");
    }

    #[test]
    fn edited_pr_body_should_use_existing_3999_character_boundary() {
        let mut state = ContentEditState::for_pr(&PrDescription {
            title: "T".to_owned(),
            body: "a".repeat(3999),
        });
        assert!(matches!(
            state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL,)),
            ContentEditAction::Saved(_)
        ));
        state.body = TextEditor::new("a".repeat(4000), false);
        assert!(matches!(
            state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL,)),
            ContentEditAction::Consumed
        ));
        assert_eq!(
            state.error,
            Some(ContentValidationError::BodyTooLong { length: 4000 })
        );
    }

    #[test]
    fn saved_content_should_preserve_exact_whitespace_unicode_and_markdown() {
        let title = "  Título — ✅  ";
        let body = "  ação\n- [ ] validar\nlinha final  ";
        let mut state = ContentEditState::for_pr(&PrDescription {
            title: title.to_owned(),
            body: body.to_owned(),
        });
        let ContentEditAction::Saved(saved) =
            state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
        else {
            panic!("conteúdo válido deveria salvar")
        };
        assert_eq!(saved.title, title);
        assert_eq!(saved.body, body);
    }
}
