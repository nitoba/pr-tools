//! TUI compartilhada para criar/importar o perfil de um remote Azure.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
};

use super::{StatusHeader, border_type, status_header, status_layout, theme};
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
            Self::ProgramReference => "programField",
            Self::AreaPath => "areaPath",
            Self::AssignedTo => "assignedTo",
            Self::InheritIterationPath => "inheritIterationPath",
            Self::ParentTransition => "parentTransition",
            Self::Priority => "priority",
            Self::Program => "program",
            Self::ReviewerDev => "reviewerDev",
            Self::ReviewerSprint => "reviewerSprint",
            Self::Team => "team",
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
    tick: u64,
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
            tick: 0,
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
        if key
            .modifiers
            .contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return;
        }
        match key.code {
            KeyCode::Char(ch) => {
                let index = char_byte_index(&self.edit_value, self.cursor);
                self.edit_value.insert(index, ch);
                self.cursor += 1;
            }
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

    fn save(&mut self) -> Option<OnboardingOutcome> {
        self.commit();
        match crate::features::onboarding::save(&self.config, &self.remote, &self.draft) {
            Ok(selection) => Some(OnboardingOutcome::Saved(Box::new(selection))),
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }
}

impl Widget for &ProfileOnboarding {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 60 || area.height < 20 {
            Block::new().style(theme().root).render(area, buf);
            Paragraph::new(format!(
                "terminal muito pequeno — mínimo 60×20 (atual {}×{})",
                area.width, area.height
            ))
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
                tick: self.tick,
                active: false,
                style: theme().accent,
            },
        );
        render_body(self, body, buf);
        Paragraph::new(Line::from(Span::styled(
            match self.screen {
                Screen::Action => "↑/↓ escolher · enter confirmar · esc/q cancelar",
                Screen::Import => "↑/↓ escolher · enter importar · esc voltar",
                Screen::Edit => "tab/↓ próximo · shift+tab/↑ anterior · enter avançar · esc voltar",
                Screen::Review => "enter salvar · esc voltar para editar · q cancelar",
            },
            theme().muted,
        )))
        .render(foot, buf);
    }
}

fn render_body(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .title(Span::styled(
            format!(
                " Remote {}/{}/{} ",
                app.remote.organization, app.remote.project, app.remote.repository
            ),
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
                        if selected { "▸ " } else { "  " },
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
            format!("✘ {error}"),
            theme().error,
        )));
    }
    Paragraph::new(rendered)
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

fn render_import(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let items = app
        .profiles()
        .into_iter()
        .enumerate()
        .map(|(index, profile)| {
            let selected = index == app.imported;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "▸ " } else { "  " },
                    if selected {
                        theme().accent
                    } else {
                        theme().muted
                    },
                ),
                Span::styled(
                    format!(
                        "{}  ·  {}",
                        profile.name,
                        profile.program_field().unwrap_or("")
                    ),
                    if selected {
                        Style::new().add_modifier(Modifier::BOLD)
                    } else {
                        Style::new()
                    },
                ),
            ]))
        })
        .collect::<Vec<_>>();
    List::new(items).render(area, buf);
}

fn render_edit(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let [form, error_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(area);
    let lines = FIELDS
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let selected = index == app.field;
            let value = if selected && !field.is_toggle() {
                format!("{}▌", app.edit_value)
            } else {
                app.get(*field)
            };
            Line::from(vec![
                Span::styled(
                    if selected { "▸ " } else { "  " },
                    if selected {
                        theme().accent
                    } else {
                        theme().muted
                    },
                ),
                Span::styled(
                    format!("{:<24} ", field.label()),
                    if selected {
                        Style::new().add_modifier(Modifier::BOLD)
                    } else {
                        theme().muted
                    },
                ),
                Span::raw(value),
            ])
        })
        .collect::<Vec<_>>();
    Paragraph::new(lines).render(form, buf);
    if let Some(error) = &app.error {
        Paragraph::new(Span::styled(format!("✘ {error}"), theme().error))
            .wrap(Wrap { trim: false })
            .render(error_area, buf);
    }
}

fn render_review(app: &ProfileOnboarding, area: Rect, buf: &mut Buffer) {
    let mut lines = vec![
        Line::from(Span::styled(
            format!("origem: {}", app.draft.origin.label()),
            theme().muted,
        )),
        Line::from(Span::styled(
            format!(
                "binding: {}/{}/{}",
                app.remote.organization, app.remote.project, app.remote.repository
            ),
            theme().accent,
        )),
        Line::from(""),
    ];
    lines.extend(app.draft.review_rows().into_iter().map(|(key, value)| {
        Line::from(vec![
            Span::styled(format!("{key:>24}  "), theme().muted),
            Span::styled(value, Style::new().add_modifier(Modifier::BOLD)),
        ])
    }));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "▸ [ enter ] salvar perfil e binding",
        theme().success.add_modifier(Modifier::BOLD),
    )));
    if let Some(error) = &app.error {
        lines.push(Line::from(Span::styled(
            format!("✘ {error}"),
            theme().error,
        )));
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(area, buf);
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
    let mut last_tick = Instant::now();
    loop {
        if last_tick.elapsed() >= Duration::from_millis(100) {
            app.tick = app.tick.wrapping_add(1);
            last_tick = Instant::now();
        }
        terminal.draw(|frame| frame.render_widget(&app, frame.area()))?;
        if event::poll(Duration::from_millis(50))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(OnboardingOutcome::Aborted);
            }
            if let Some(outcome) = handle_key(&mut app, key) {
                return Ok(outcome);
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
                    KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                        app.previous_field();
                    }
                    KeyCode::Up => app.previous_field(),
                    _ => app.edit_input(key),
                }
            }
            None
        }
        Screen::Review => match key.code {
            KeyCode::Enter => app.save(),
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

    fn remote() -> RepositoryRemote {
        RepositoryRemote {
            organization: "IBSBioSistemico".to_owned(),
            project: "Projeto".to_owned(),
            repository: "repo".to_owned(),
        }
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
}
