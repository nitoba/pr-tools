//! TUI compartilhada para criar/importar o perfil de um remote Azure.

use std::io::IsTerminal;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{StatusHeader, ascii_only, border_type, checkbox, status_header, status_layout, theme};
use crate::config::{Config, ProcessProfile};
use crate::features::onboarding::{OnboardingDraft, ProfileDecision};
use crate::features::process_profiles::ProfileSelection;
use crate::git::RepositoryRemote;

/// Resultado da tela de onboarding.
#[derive(Debug)]
pub enum OnboardingOutcome {
    /// Perfil e binding foram salvos; a seleção já está congelada.
    Saved(Box<ProfileSelection>),
    /// Usuário optou por continuar com o fallback.
    Skipped,
    /// Usuário cancelou sem salvar.
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Action,
    Import,
    Edit,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    ProgramReference,
    AreaPath,
    AssignedTo,
    InheritIterationPath,
    ParentTransition,
    Priority,
    Program,
    ReviewerDev,
    ReviewerSprint,
    Team,
}

enum SaveState {
    Idle,
    Saving {
        receiver: Receiver<Result<ProfileSelection, String>>,
        worker: Option<JoinHandle<()>>,
    },
}

impl Drop for SaveState {
    fn drop(&mut self) {
        let SaveState::Saving { worker, .. } = self else {
            return;
        };
        if let Some(worker) = worker.take() {
            let _ = worker.join();
        }
    }
}

const FIELDS: &[Field] = &[
    Field::Name,
    Field::ProgramReference,
    Field::AreaPath,
    Field::AssignedTo,
    Field::InheritIterationPath,
    Field::ParentTransition,
    Field::Priority,
    Field::Program,
    Field::ReviewerDev,
    Field::ReviewerSprint,
    Field::Team,
];

impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::ProgramReference => "campo Azure do programa",
            Self::AreaPath => "areaPath",
            Self::AssignedTo => "testCard.assignedTo",
            Self::InheritIterationPath => "inheritIterationPath",
            Self::ParentTransition => "parentTransition",
            Self::Priority => "priority",
            Self::Program => "program",
            Self::ReviewerDev => "reviewers.development",
            Self::ReviewerSprint => "reviewers.sprint",
            Self::Team => "testCard.team",
        }
    }

    fn is_toggle(self) -> bool {
        matches!(self, Self::InheritIterationPath)
    }
}

struct ProfileOnboarding {
    config: Config,
    remote: RepositoryRemote,
    draft: OnboardingDraft,
    screen: Screen,
    action: usize,
    imported: usize,
    field: usize,
    edit_value: String,
    cursor: usize,
    error: Option<String>,
    save_state: SaveState,
}

impl ProfileOnboarding {
    fn new(config: Config, remote: RepositoryRemote) -> Self {
        Self {
            config,
            remote,
            draft: OnboardingDraft::default(),
            screen: Screen::Action,
            action: 0,
            imported: 0,
            field: 0,
            edit_value: String::new(),
            cursor: 0,
            error: None,
            save_state: SaveState::Idle,
        }
    }

    fn profiles(&self) -> Vec<ProcessProfile> {
        self.config.profiles.clone()
    }

    fn current_field(&self) -> Field {
        FIELDS[self.field.min(FIELDS.len() - 1)]
    }

    fn get(&self, field: Field) -> String {
        match field {
            Field::Name => self.draft.name.clone(),
            Field::ProgramReference => self.draft.program_field.clone(),
            Field::AreaPath => self.draft.area_path.clone(),
            Field::AssignedTo => self.draft.assigned_to.clone(),
            Field::InheritIterationPath => self.draft.inherit_iteration_path.to_string(),
            Field::ParentTransition => self.draft.parent_transition.clone(),
            Field::Priority => self.draft.priority.to_string(),
            Field::Program => self.draft.program.clone(),
            Field::ReviewerDev => self.draft.reviewer_dev.clone(),
            Field::ReviewerSprint => self.draft.reviewer_sprint.clone(),
            Field::Team => self.draft.team.clone(),
        }
    }

    fn set(&mut self, field: Field, value: String) {
        match field {
            Field::Name => self.draft.name = value,
            Field::ProgramReference => self.draft.program_field = value,
            Field::AreaPath => self.draft.area_path = value,
            Field::AssignedTo => self.draft.assigned_to = value,
            Field::InheritIterationPath => {
                self.draft.inherit_iteration_path = value == "true";
            }
            Field::ParentTransition => self.draft.parent_transition = value,
            Field::Priority => {
                self.draft.priority = value.replace(',', ".").parse().unwrap_or(f64::NAN);
            }
            Field::Program => self.draft.program = value,
            Field::ReviewerDev => self.draft.reviewer_dev = value,
            Field::ReviewerSprint => self.draft.reviewer_sprint = value,
            Field::Team => self.draft.team = value,
        }
    }

    fn bind_editor(&mut self) {
        let field = self.current_field();
        self.edit_value = self.get(field);
        self.cursor = self.edit_value.chars().count();
    }

    fn choose_action(&mut self) -> Option<OnboardingOutcome> {
        match self.action {
            0 => {
                self.draft = OnboardingDraft::default();
                self.screen = Screen::Edit;
                self.field = 0;
                self.bind_editor();
                None
            }
            1 => {
                let profiles = self.profiles();
                if profiles.is_empty() {
                    self.error = Some("nenhum perfil persistido para importar".to_owned());
                } else {
                    self.imported = 0;
                    self.screen = Screen::Import;
                    self.error = None;
                }
                None
            }
            _ => Some(OnboardingOutcome::Skipped),
        }
    }

    fn import_selected(&mut self) {
        if let Some(profile) = self.profiles().get(self.imported).cloned() {
            self.draft = OnboardingDraft::from_profile(&profile);
            // A origem é apenas um preenchimento inicial. O nome identifica o
            // novo perfil e precisa ser editado para passar a validação.
            self.draft.name = profile.name;
            self.screen = Screen::Edit;
            self.field = 0;
            self.bind_editor();
            self.error = None;
        }
    }

    fn commit(&mut self) {
        let field = self.current_field();
        if !field.is_toggle() {
            self.set(field, self.edit_value.clone());
        }
    }

    fn next_field(&mut self) {
        self.commit();
        if self.field + 1 < FIELDS.len() {
            self.field += 1;
            self.bind_editor();
        } else {
            match crate::features::onboarding::validate_draft(&self.config, &self.draft) {
                Ok(()) => {
                    self.screen = Screen::Review;
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn previous_field(&mut self) {
        self.commit();
        if self.field > 0 {
            self.field -= 1;
            self.bind_editor();
        } else {
            self.screen = Screen::Action;
            self.error = None;
        }
    }

    fn toggle(&mut self) {
        self.draft.inherit_iteration_path = !self.draft.inherit_iteration_path;
        self.error = None;
    }

    fn edit_input(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            || key.modifiers.contains(KeyModifiers::ALT)
        {
            return;
        }
        match key.code {
            KeyCode::Char(ch) => self.insert_text(&ch.to_string()),
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let end = char_byte_index(&self.edit_value, self.cursor);
                    let start = char_byte_index(&self.edit_value, self.cursor - 1);
                    self.edit_value.drain(start..end);
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.edit_value.chars().count() {
                    let start = char_byte_index(&self.edit_value, self.cursor);
                    let end = char_byte_index(&self.edit_value, self.cursor + 1);
                    self.edit_value.drain(start..end);
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.edit_value.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.edit_value.chars().count(),
            _ => {}
        }
        self.error = None;
    }

    fn insert_text(&mut self, text: &str) {
        let text: String = text
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        if text.is_empty() {
            return;
        }
        let index = char_byte_index(&self.edit_value, self.cursor);
        self.edit_value.insert_str(index, &text);
        self.cursor += text.chars().count();
        self.error = None;
    }

    fn is_saving(&self) -> bool {
        matches!(&self.save_state, SaveState::Saving { .. })
    }

    fn begin_save(&mut self) {
        if self.is_saving() {
            return;
        }
        self.commit();
        self.error = None;
        let config = self.config.clone();
        let remote = self.remote.clone();
        let draft = self.draft.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = crate::features::onboarding::save(&config, &remote, &draft)
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.save_state = SaveState::Saving {
            receiver,
            worker: Some(worker),
        };
    }

    fn take_save_result(&mut self) -> Option<Result<ProfileSelection, String>> {
        let SaveState::Saving { receiver, .. } = &self.save_state else {
            return None;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.save_state = SaveState::Idle;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.save_state = SaveState::Idle;
                Some(Err("a gravação do perfil foi interrompida".to_owned()))
            }
        }
    }

    /// Aguarda uma gravação já confirmada antes de sair, evitando que Ctrl+C
    /// deixe uma operação local continuar depois que a tela foi encerrada.
    fn wait_for_save(&mut self) -> Option<Result<ProfileSelection, String>> {
        let result = match &self.save_state {
            SaveState::Idle => return None,
            SaveState::Saving { receiver, .. } => receiver
                .recv()
                .unwrap_or_else(|_| Err("a gravação do perfil foi interrompida".to_owned())),
        };
        self.save_state = SaveState::Idle;
        Some(result)
    }
}

impl Widget for &ProfileOnboarding {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 60 || area.height < 20 {
            Block::new().style(theme().root).render(area, buf);
            let message = format!(
                "terminal muito pequeno {} mínimo 60{}20 (atual {}{}{})",
                dash(),
                dimension_separator(),
                area.width,
                dimension_separator(),
                area.height
            );
            Paragraph::new(truncate_cells(&message, usize::from(area.width)))
                .alignment(ratatui::layout::Alignment::Center)
                .render(area, buf);
            return;
        }
        let [head, body, foot] = status_layout(area);
        Block::new().style(theme().root).render(area, buf);
        let message = match self.screen {
            Screen::Action => "escolha como configurar este remote",
            Screen::Import => "selecione um perfil para copiar",
            Screen::Edit => "edite os valores do novo perfil",
            Screen::Review if self.is_saving() => "salvando perfil localmente",
            Screen::Review => "confirme antes de salvar",
        };
        status_header(
            head,
            buf,
            StatusHeader {
                command: "desc/test",
                phase: "Perfil do repositório Azure",
                message,
                progress: None,
                tick: 0,
                active: false,
                style: theme().accent,
            },
        );
        render_body(self, body, buf);
        Paragraph::new(Line::from(Span::styled(
            footer_hint(
                self.screen,
                self.current_field(),
                self.is_saving(),
                area.width,
            ),
            theme().muted,
        )))
        .render(foot, buf);
    }
}

fn selection_marker(selected: bool) -> &'static str {
    if selected {
        if ascii_only() { "> " } else { "▸ " }
    } else {
        "  "
    }
}

fn error_marker() -> &'static str {
    if ascii_only() { "x " } else { "✘ " }
}

fn caret_glyph() -> &'static str {
    if ascii_only() { "_" } else { "▌" }
}

fn save_marker() -> &'static str {
    if ascii_only() { "> " } else { "▸ " }
}

fn dash() -> &'static str {
    if ascii_only() { "-" } else { "—" }
}

fn dimension_separator() -> &'static str {
    if ascii_only() { "x" } else { "×" }
}

fn list_separator() -> &'static str {
    if ascii_only() { "  -  " } else { "  ·  " }
}

fn vertical_keys() -> &'static str {
    if ascii_only() { "up/down" } else { "↑/↓" }
}

fn footer_hint(screen: Screen, field: Field, saving: bool, width: u16) -> String {
    let separator = if ascii_only() { " | " } else { " · " };
    let full = match screen {
        Screen::Action => format!(
            "{} escolher{separator}enter confirmar{separator}esc/q cancelar",
            vertical_keys()
        ),
        Screen::Import => format!(
            "{} escolher{separator}enter importar{separator}esc voltar",
            vertical_keys()
        ),
        Screen::Edit if field.is_toggle() => format!(
            "space alterna{separator}tab/{} prox{separator}shift+tab/{} ant{separator}enter avança{separator}esc volta",
            if ascii_only() { "down" } else { "↓" },
            if ascii_only() { "up" } else { "↑" },
        ),
        Screen::Edit => format!(
            "tab/{} prox{separator}shift+tab/{} ant{separator}enter avança{separator}esc volta",
            if ascii_only() { "down" } else { "↓" },
            if ascii_only() { "up" } else { "↑" },
        ),
        Screen::Review if saving => format!("salvando...{separator}aguarde"),
        Screen::Review => format!("esc editar{separator}q cancelar"),
    };
    if UnicodeWidthStr::width(full.as_str()) <= usize::from(width) {
        return full;
    }

    let compact = match screen {
        Screen::Action => format!(
            "{} escolher{separator}enter confirmar{separator}esc/q sair",
            vertical_keys()
        ),
        Screen::Import => format!(
            "{} escolher{separator}enter importar{separator}esc voltar",
            vertical_keys()
        ),
        Screen::Edit if field.is_toggle() => {
            format!(
                "space alterna{separator}tab/enter prox{separator}shift+tab ant{separator}esc volta"
            )
        }
        Screen::Edit => format!("tab/enter prox{separator}shift+tab ant{separator}esc volta"),
        Screen::Review if saving => format!("salvando...{separator}aguarde"),
        Screen::Review => format!("esc editar{separator}q cancelar"),
    };
    if UnicodeWidthStr::width(compact.as_str()) <= usize::from(width) {
        compact
    } else {
        truncate_cells(&compact, usize::from(width))
    }
}

fn char_width(character: char) -> usize {
    UnicodeWidthChar::width(character).unwrap_or(0)
}

fn take_prefix_cells(value: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut width: usize = 0;
    for character in value.chars() {
        let character_width = char_width(character);
        if width.saturating_add(character_width) > max_width {
            break;
        }
        result.push(character);
        width += character_width;
    }
    result
}

fn take_suffix_cells(value: &str, max_width: usize) -> String {
    let mut result = Vec::new();
    let mut width: usize = 0;
    for character in value.chars().rev() {
        let character_width = char_width(character);
        if width.saturating_add(character_width) > max_width {
            break;
        }
        result.push(character);
        width += character_width;
    }
    result.into_iter().rev().collect()
}

fn truncate_cells(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_owned();
    }
    let ellipsis = if ascii_only() { "..." } else { "…" };
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    if max_width <= ellipsis_width {
        return take_prefix_cells(ellipsis, max_width);
    }
    let mut result = take_prefix_cells(value, max_width - ellipsis_width);
    result.push_str(ellipsis);
    result
}

fn truncate_middle_cells(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_owned();
    }
    let ellipsis = if ascii_only() { "..." } else { "…" };
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    if max_width <= ellipsis_width {
        return take_prefix_cells(ellipsis, max_width);
    }
    let available = max_width - ellipsis_width;
    let left_width = available / 2;
    let right_width = available - left_width;
    format!(
        "{}{}{}",
        take_prefix_cells(value, left_width),
        ellipsis,
        take_suffix_cells(value, right_width)
    )
}

fn split_visible_width(
    before_width: usize,
    after_width: usize,
    available: usize,
) -> (usize, usize) {
    let mut left = available / 2;
    let mut right = available - left;
    if before_width < left {
        right += left - before_width;
        left = before_width;
    }
    if after_width < right {
        left += right - after_width;
        right = after_width;
    }
    (left.min(before_width), right.min(after_width))
}

fn editable_value(value: &str, cursor: usize, max_width: usize) -> String {
    let caret = caret_glyph();
    let caret_width = UnicodeWidthStr::width(caret).max(1);
    if max_width <= caret_width {
        return take_prefix_cells(caret, max_width);
    }
    let cursor = cursor.min(value.chars().count());
    let split = char_byte_index(value, cursor);
    let (before, after) = value.split_at(split);
    if UnicodeWidthStr::width(value).saturating_add(caret_width) <= max_width {
        return format!("{before}{caret}{after}");
    }

    let content_width = max_width - caret_width;
    let full_ellipsis = if ascii_only() { "..." } else { "…" };
    let full_ellipsis_width = UnicodeWidthStr::width(full_ellipsis);
    let (ellipsis, ellipsis_width) = if content_width >= full_ellipsis_width {
        (full_ellipsis, full_ellipsis_width)
    } else if content_width > 0 {
        (".", 1)
    } else {
        ("", 0)
    };
    let before_width = UnicodeWidthStr::width(before);
    let after_width = UnicodeWidthStr::width(after);
    let mut left_marker = 0;
    let mut right_marker = 0;
    for _ in 0..3 {
        let available = content_width.saturating_sub(left_marker + right_marker);
        let (visible_before, visible_after) =
            split_visible_width(before_width, after_width, available);
        let wants_left_marker = usize::from(visible_before < before_width) * ellipsis_width;
        let wants_right_marker = usize::from(visible_after < after_width) * ellipsis_width;
        let (next_left_marker, next_right_marker) =
            if wants_left_marker + wants_right_marker <= content_width {
                (wants_left_marker, wants_right_marker)
            } else if wants_left_marker > 0 {
                (wants_left_marker, 0)
            } else {
                (0, wants_right_marker)
            };
        if (next_left_marker, next_right_marker) == (left_marker, right_marker) {
            break;
        }
        left_marker = next_left_marker;
        right_marker = next_right_marker;
    }
    let available = content_width.saturating_sub(left_marker + right_marker);
    let (visible_before, visible_after) = split_visible_width(before_width, after_width, available);
    format!(
        "{}{}{}{}{}",
        if left_marker > 0 { ellipsis } else { "" },
        take_suffix_cells(before, visible_before),
        caret,
        take_prefix_cells(after, visible_after),
        if right_marker > 0 { ellipsis } else { "" },
    )
}

fn render_body(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let identity = format!(
        "{}/{}/{}",
        app.remote.organization, app.remote.project, app.remote.repository
    );
    let title_width = usize::from(area.width.saturating_sub(12));
    let block = Block::default()
        .title(Span::styled(
            format!(" Remote {} ", truncate_middle_cells(&identity, title_width)),
            theme().accent.add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(theme().border)
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    match app.screen {
        Screen::Action => render_action(app, inner, buf),
        Screen::Import => render_import(app, inner, buf),
        Screen::Edit => render_edit(app, inner, buf),
        Screen::Review => render_review(app, inner, buf),
    }
}

fn render_action(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let lines = [
        "Não há perfil associado a este remote Azure.",
        "A decisão é local; nenhuma chamada de IA ou escrita remota começou.",
        "",
        "Novo perfil",
        "Importar perfil",
        "Agora não",
    ];
    let mut rendered = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if (3..=5).contains(&index) {
                let selected = index - 3 == app.action;
                Line::from(vec![
                    Span::styled(
                        selection_marker(selected),
                        if selected {
                            theme().accent
                        } else {
                            theme().muted
                        },
                    ),
                    Span::styled(
                        (*line).to_owned(),
                        if selected {
                            Style::new().add_modifier(Modifier::BOLD)
                        } else {
                            theme().muted
                        },
                    ),
                ])
            } else {
                Line::from(*line)
            }
        })
        .collect::<Vec<_>>();
    if let Some(error) = &app.error {
        rendered.push(Line::from(Span::styled(
            format!("{}{error}", error_marker()),
            theme().error,
        )));
    }
    Paragraph::new(rendered)
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

fn render_import(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let profiles = app.profiles();
    let items = profiles
        .into_iter()
        .enumerate()
        .map(|(index, profile)| {
            let selected = index == app.imported;
            let available = usize::from(area.width).saturating_sub(2);
            let summary = format!(
                "{}{}{}",
                profile.name,
                list_separator(),
                profile.program_field().unwrap_or("")
            );
            ListItem::new(Line::from(vec![
                Span::styled(
                    selection_marker(selected),
                    if selected {
                        theme().accent
                    } else {
                        theme().muted
                    },
                ),
                Span::styled(
                    truncate_cells(&summary, available),
                    if selected {
                        Style::new().add_modifier(Modifier::BOLD)
                    } else {
                        Style::new()
                    },
                ),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(
        items
            .len()
            .checked_sub(1)
            .map(|last| app.imported.min(last)),
    );
    StatefulWidget::render(List::new(items).highlight_symbol(""), area, buf, &mut state);
}

fn render_edit(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let [form, error_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(area);
    let marker_width: u16 = 2;
    let label_width: u16 = 25;
    for (index, field) in FIELDS.iter().enumerate() {
        if index >= usize::from(form.height) {
            break;
        }
        let row = Rect {
            x: form.x,
            y: form
                .y
                .saturating_add(u16::try_from(index).unwrap_or(u16::MAX)),
            width: form.width,
            height: 1,
        };
        let selected = index == app.field;
        let marker_area = Rect {
            width: row.width.min(marker_width),
            ..row
        };
        Paragraph::new(selection_marker(selected))
            .style(if selected {
                theme().accent
            } else {
                theme().muted
            })
            .render(marker_area, buf);

        let label_x = row.x.saturating_add(marker_area.width);
        let remaining_width = row.width.saturating_sub(marker_area.width);
        let label_area = Rect {
            x: label_x,
            width: remaining_width.min(label_width),
            ..row
        };
        Paragraph::new(format!("{:<24} ", field.label()))
            .style(if selected {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                theme().muted
            })
            .render(label_area, buf);

        let value_area = Rect {
            x: label_area.x.saturating_add(label_area.width),
            width: remaining_width.saturating_sub(label_area.width),
            ..row
        };
        if value_area.width == 0 {
            continue;
        }
        let value = if field.is_toggle() {
            format!("{} (space)", checkbox(app.draft.inherit_iteration_path))
        } else if selected {
            editable_value(&app.edit_value, app.cursor, usize::from(value_area.width))
        } else {
            truncate_cells(&app.get(*field), usize::from(value_area.width))
        };
        Paragraph::new(value).render(value_area, buf);
    }
    if let Some(error) = &app.error {
        Paragraph::new(Span::styled(
            format!("{}{error}", error_marker()),
            theme().error,
        ))
        .wrap(Wrap { trim: false })
        .render(error_area, buf);
    }
}

fn render_review(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let mut lines = vec![Line::from(Span::styled(
        truncate_cells(
            &format!("origem: {}", app.draft.origin.label()),
            usize::from(area.width),
        ),
        theme().muted,
    ))];
    if app.error.is_none() {
        lines.push(Line::from(""));
    }
    lines.extend(app.draft.review_rows().into_iter().map(|(key, value)| {
        let prefix = format!("{key:>24}  ");
        let available =
            usize::from(area.width).saturating_sub(UnicodeWidthStr::width(prefix.as_str()));
        Line::from(vec![
            Span::styled(prefix, theme().muted),
            Span::styled(
                truncate_cells(&value, available),
                Style::new().add_modifier(Modifier::BOLD),
            ),
        ])
    }));
    if app.error.is_none() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        if app.is_saving() {
            format!("{} salvando perfil e binding...", save_marker())
        } else {
            format!("{}[ enter ] salvar perfil e binding", save_marker())
        },
        if app.is_saving() {
            theme().warning.add_modifier(Modifier::BOLD)
        } else {
            theme().success.add_modifier(Modifier::BOLD)
        },
    )));
    if let Some(error) = &app.error {
        let message = format!("{}{error}", error_marker());
        lines.push(Line::from(Span::styled(
            truncate_cells(&message, usize::from(area.width)),
            theme().error,
        )));
    }
    Paragraph::new(lines).render(area, buf);
}

/// Executa o onboarding compartilhado por `desc` e `test`.
///
/// # Errors
///
/// Retorna um erro quando a decisão não requer onboarding, o terminal não é
/// interativo ou a leitura/escrita do terminal falha.
pub fn run_profile_onboarding(decision: ProfileDecision) -> anyhow::Result<OnboardingOutcome> {
    let ProfileDecision::NeedsOnboarding { config, remote } = decision else {
        anyhow::bail!("onboarding não é necessário para este remote")
    };
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("onboarding de perfil requer terminal interativo")
    }
    let mut terminal: DefaultTerminal = ratatui::init();
    let result = run_loop(&mut terminal, config, remote);
    ratatui::restore();
    result
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    config: Config,
    remote: RepositoryRemote,
) -> anyhow::Result<OnboardingOutcome> {
    let mut app = ProfileOnboarding::new(config, remote);
    let mut dirty = true;
    loop {
        if dirty {
            terminal.draw(|frame| frame.render_widget(&app, frame.area()))?;
            dirty = false;
        }
        if let Some(result) = app.take_save_result() {
            match result {
                Ok(selection) => return Ok(OnboardingOutcome::Saved(Box::new(selection))),
                Err(error) => {
                    app.error = Some(error);
                    dirty = true;
                }
            }
            continue;
        }
        if event::poll(Duration::from_millis(50))? {
            let event = event::read()?;
            match event {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        if app.is_saving() {
                            match app.wait_for_save() {
                                Some(Ok(selection)) => {
                                    return Ok(OnboardingOutcome::Saved(Box::new(selection)));
                                }
                                Some(Err(error)) => {
                                    app.error = Some(error);
                                    dirty = true;
                                    continue;
                                }
                                None => {}
                            }
                        }
                        return Ok(OnboardingOutcome::Aborted);
                    }
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('z')
                    {
                        crate::tui::suspend::suspend_to_shell(&mut *terminal)?;
                        dirty = true;
                        continue;
                    }
                    if let Some(outcome) = handle_key(&mut app, key) {
                        return Ok(outcome);
                    }
                    dirty = true;
                }
                Event::Paste(text) => {
                    if app.screen == Screen::Edit && !app.current_field().is_toggle() {
                        app.insert_text(&text);
                        dirty = true;
                    }
                }
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
    }
}

fn handle_key(app: &mut ProfileOnboarding, key: KeyEvent) -> Option<OnboardingOutcome> {
    match app.screen {
        Screen::Action => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Some(OnboardingOutcome::Aborted),
            KeyCode::Up | KeyCode::Char('k') => {
                app.action = app.action.checked_sub(1).unwrap_or(2);
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.action = (app.action + 1) % 3;
                None
            }
            KeyCode::Enter | KeyCode::Char(' ') => app.choose_action(),
            _ => None,
        },
        Screen::Import => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.screen = Screen::Action;
                app.error = None;
                None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.imported = app
                    .imported
                    .checked_sub(1)
                    .unwrap_or_else(|| app.profiles().len().saturating_sub(1));
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let count = app.profiles().len();
                if count > 0 {
                    app.imported = (app.imported + 1) % count;
                }
                None
            }
            KeyCode::Enter => {
                app.import_selected();
                None
            }
            _ => None,
        },
        Screen::Edit => {
            if key.code == KeyCode::BackTab
                || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
            {
                app.previous_field();
                return None;
            }
            if key.code == KeyCode::Esc {
                app.commit();
                app.screen = Screen::Action;
                app.error = None;
                return None;
            }
            if key.code == KeyCode::Char('q') && app.current_field().is_toggle() {
                return Some(OnboardingOutcome::Aborted);
            }
            if app.current_field().is_toggle() {
                match key.code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => app.toggle(),
                    KeyCode::Enter | KeyCode::Tab | KeyCode::Down => app.next_field(),
                    KeyCode::Up => app.previous_field(),
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Enter | KeyCode::Tab | KeyCode::Down => app.next_field(),
                    KeyCode::Up => app.previous_field(),
                    _ => app.edit_input(key),
                }
            }
            None
        }
        Screen::Review if app.is_saving() => None,
        Screen::Review => match key.code {
            KeyCode::Enter => {
                app.begin_save();
                None
            }
            KeyCode::Esc => {
                app.screen = Screen::Edit;
                app.field = 0;
                app.bind_editor();
                app.error = None;
                None
            }
            KeyCode::Char('q') => Some(OnboardingOutcome::Aborted),
            _ => None,
        },
    }
}

fn char_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

/// Converte uma decisão já criada em uma execução de onboarding.
///
/// # Errors
///
/// Propaga os erros da tela de onboarding.
pub fn run_for_decision(decision: ProfileDecision) -> anyhow::Result<OnboardingOutcome> {
    run_profile_onboarding(decision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProcessProfile;
    use ratatui::{Terminal, backend::TestBackend};

    fn remote() -> RepositoryRemote {
        RepositoryRemote {
            organization: "IBSBioSistemico".to_owned(),
            project: "Projeto".to_owned(),
            repository: "repo".to_owned(),
        }
    }

    fn rendered_text(app: &ProfileOnboarding, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget(app, frame.area()))
            .expect("draw");
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn action_screen_should_fit_at_80x24() {
        let app = ProfileOnboarding::new(Config::default(), remote());
        let text = rendered_text(&app, 80, 24);

        assert!(text.contains("Novo perfil"));
        assert!(text.contains("Importar perfil"));
        assert!(text.contains("Agora não"));
        assert!(text.contains("esc/q cancelar"));
    }

    #[test]
    fn review_screen_should_keep_save_action_visible_at_60x20() {
        let mut app = ProfileOnboarding::new(Config::default(), remote());
        app.draft = OnboardingDraft {
            name: "IBS Novo".to_owned(),
            program_field: "Custom.ProgramasNovo".to_owned(),
            ..OnboardingDraft::default()
        };
        app.screen = Screen::Review;
        let text = rendered_text(&app, 60, 20);

        assert!(text.contains("[ enter ] salvar perfil e binding"));
    }

    #[test]
    fn below_minimum_should_show_resize_message_instead_of_controls() {
        let app = ProfileOnboarding::new(Config::default(), remote());
        let text = rendered_text(&app, 59, 20);

        assert!(text.contains("terminal muito pequeno"));
        assert!(!text.contains("Novo perfil"));
    }

    #[test]
    fn long_editable_value_should_keep_caret_visible_with_cell_widths() {
        let mut app = ProfileOnboarding::new(Config::default(), remote());
        app.screen = Screen::Edit;
        app.edit_value = "inicio界界界界界界界界fim".to_owned();
        app.cursor = app.edit_value.chars().count();
        let text = rendered_text(&app, 60, 20);

        assert!(text.contains(caret_glyph()));
    }

    #[test]
    fn pasted_text_should_insert_without_breaking_single_line_fields() {
        let mut app = ProfileOnboarding::new(Config::default(), remote());
        app.screen = Screen::Edit;
        app.edit_value = "ab".to_owned();
        app.cursor = 1;

        app.insert_text("界\ncd");

        assert_eq!(app.edit_value, "a界cdb");
        assert_eq!(app.cursor, 4);
    }

    #[test]
    fn iteration_path_toggle_should_show_its_keyboard_hint() {
        let mut app = ProfileOnboarding::new(Config::default(), remote());
        app.screen = Screen::Edit;
        app.field = FIELDS
            .iter()
            .position(|field| field.is_toggle())
            .expect("toggle field");
        app.bind_editor();
        let text = rendered_text(&app, 60, 20);

        assert!(text.contains("(space)"));
    }

    #[test]
    fn edit_screen_should_identify_test_case_profile_fields() {
        let mut app = ProfileOnboarding::new(Config::default(), remote());
        app.screen = Screen::Edit;
        let text = rendered_text(&app, 100, 30);

        assert!(text.contains("campo Azure do programa"));
        assert!(text.contains("testCard.assignedTo"));
        assert!(text.contains("testCard.team"));
    }

    #[test]
    fn backtab_should_move_to_the_previous_editable_field() {
        let mut app = ProfileOnboarding::new(Config::default(), remote());
        app.screen = Screen::Edit;
        app.field = 1;
        app.bind_editor();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE),
        );

        assert_eq!(app.field, 0);
    }

    #[test]
    fn action_screen_should_show_remote_and_all_three_decisions() {
        let app = ProfileOnboarding::new(Config::default(), remote());
        let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 30));
        app.render(buffer.area, &mut buffer);
        let text = buffer
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(text.contains("Novo perfil"));
        assert!(text.contains("Importar perfil"));
        assert!(text.contains("Agora não"));
    }

    #[test]
    fn review_screen_should_show_all_fields_and_explicit_save() {
        let mut app = ProfileOnboarding::new(Config::default(), remote());
        app.draft = OnboardingDraft {
            name: "IBS Novo".to_owned(),
            program_field: "Custom.ProgramasNovo".to_owned(),
            area_path: "Projeto\\QA".to_owned(),
            assigned_to: "qa@example.com".to_owned(),
            parent_transition: "Test QA".to_owned(),
            priority: 2.0,
            program: "Produto".to_owned(),
            reviewer_dev: "dev@example.com".to_owned(),
            reviewer_sprint: "sprint@example.com".to_owned(),
            team: "QA".to_owned(),
            ..OnboardingDraft::default()
        };
        app.screen = Screen::Review;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 30));
        app.render(buffer.area, &mut buffer);
        let text = buffer
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();

        assert!(text.contains("IBSBioSistemico/Projeto/repo"));
        assert!(text.contains("IBS Novo"));
        assert!(text.contains("Custom.ProgramasNovo"));
        assert!(text.contains("[ enter ] salvar perfil e binding"));
        assert_eq!(text.matches("IBSBioSistemico/Projeto/repo").count(), 1);
    }

    #[test]
    fn import_should_copy_profile_and_keep_name_editable() {
        let mut config = Config::default();
        config.profiles.push(ProcessProfile {
            name: "Origem".to_owned(),
            program_field: "Custom.ProgramasOrigem".to_owned(),
            area_path: "Area".to_owned(),
            assigned_to: "qa@example.com".to_owned(),
            team: "Team".to_owned(),
            program: "Program".to_owned(),
            priority: 3.0,
            inherit_iteration_path: false,
            parent_transition: None,
            reviewer_dev: String::new(),
            reviewer_sprint: String::new(),
        });
        let mut app = ProfileOnboarding::new(config, remote());
        app.action = 1;
        app.choose_action();
        app.import_selected();
        assert_eq!(app.draft.name, "Origem");
        assert_eq!(app.draft.program_field, "Custom.ProgramasOrigem");
        assert_eq!(app.draft.team, "Team");
    }

    #[test]
    fn import_screen_should_scroll_to_a_later_profile() {
        let mut config = Config::default();
        for index in 0..20 {
            config.profiles.push(ProcessProfile {
                name: format!("Perfil {index}"),
                program_field: format!("Custom.Programas{index}"),
                area_path: String::new(),
                assigned_to: String::new(),
                team: String::new(),
                program: String::new(),
                priority: 2.0,
                inherit_iteration_path: true,
                parent_transition: None,
                reviewer_dev: String::new(),
                reviewer_sprint: String::new(),
            });
        }
        let mut app = ProfileOnboarding::new(config, remote());
        app.screen = Screen::Import;
        app.imported = 19;
        let text = rendered_text(&app, 60, 20);

        assert!(text.contains("Perfil 19"));
    }
}
