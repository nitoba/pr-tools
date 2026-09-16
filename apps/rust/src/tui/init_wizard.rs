//! Wizard `prt init` — formulário multi-etapas em Ratatui, tudo reativo.
//!
//! Nada aqui é estático: cada frame (~10fps) redesenha a borda do campo focado,
//! validação de email ao vivo (✓/✘) a cada tecla e o botão salvar pulsante.
//! Texto via editor próprio de linha única (sem dependência extra); segredos
//! com máscara `•`; selects com ←/→. A lógica pura (draft/validação/
//! persistência) vive em [`crate::features::init`].

use std::collections::HashSet;
use std::io::IsTerminal;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
};
use unicode_width::UnicodeWidthChar;

use super::{StatusHeader, border_type, status_header, status_layout, theme};
use crate::features::init::{
    InitDraft, InitResult, PROVIDERS, REASONING_LEVELS, save_draft_for_profile_with_transition,
    validate_optional_email,
};

/// Resultado do wizard.
#[derive(Debug)]
pub enum InitOutcome {
    /// Config salva (fora da TUI o `main` imprime o resumo final).
    Saved(InitResult),
    /// Usuário abortou (q/Esc na primeira etapa).
    Aborted,
}

/// Etapas do wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Azure,
    Reviewers,
    Provider,
    Model,
    TestDefaults,
    Review,
}

const STEPS: &[Step] = &[
    Step::Azure,
    Step::Reviewers,
    Step::Provider,
    Step::Model,
    Step::TestDefaults,
    Step::Review,
];

const PROFILE_OPTIONS: &[(&str, &str)] = &[
    ("Agrotrace", "Agrotrace · Custom.ProgramasAgrotrace"),
    ("CheckMilk", "CheckMilk · Custom.ProgramasCheckmilk"),
];

impl Step {
    fn title(self) -> &'static str {
        match self {
            Self::Azure => "Azure DevOps",
            Self::Reviewers => "Reviewers",
            Self::Provider => "Provider",
            Self::Model => "Modelo",
            Self::TestDefaults => "Test defaults",
            Self::Review => "Revisão",
        }
    }
}

/// Campos do formulário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Field {
    Pat,
    Sprint,
    Dev,
    Assigned,
    Provider,
    CodexPath,
    CodexModel,
    CodexReasoning,
    OpencodePath,
    OpencodeModel,
    OpencodeReasoning,
    BaseUrl,
    CompatModel,
    CompatReasoning,
    ApiKey,
    Profile,
    Transition,
    AreaPath,
    Team,
    Program,
}

impl Field {
    /// Rótulo exibido.
    fn label(self) -> &'static str {
        match self {
            Self::Pat => "Azure DevOps PAT",
            Self::Sprint => "Email de review da sprint",
            Self::Dev => "Email de review de dev",
            Self::Assigned => "Responsável do card de teste",
            Self::Provider => "Provider padrão",
            Self::CodexPath => "Caminho do executável do Codex",
            Self::CodexModel => "Modelo do Codex",
            Self::CodexReasoning => "Thinking level do Codex",
            Self::OpencodePath => "Caminho do executável do OpenCode",
            Self::OpencodeModel => "Modelo do OpenCode",
            Self::OpencodeReasoning => "Thinking level do OpenCode",
            Self::BaseUrl => "Base URL OpenAI-compatible",
            Self::CompatModel => "Modelo OpenAI-compatible",
            Self::CompatReasoning => "Thinking level",
            Self::ApiKey => "API key",
            Self::Profile => "Perfil de processo",
            Self::Transition => "Transição do Work Item pai",
            Self::AreaPath => "AreaPath padrão",
            Self::Team => "Team padrão",
            Self::Program => "Program padrão",
        }
    }

    /// Dica exibida ao lado do rótulo.
    fn hint(self, draft: &InitDraft) -> String {
        match self {
            Self::Pat => {
                if draft.has_existing_pat {
                    "Enter mantém o atual · code + work items (leitura/escrita)".to_owned()
                } else {
                    "dev.azure.com → User settings → Personal access tokens".to_owned()
                }
            }
            Self::Sprint | Self::Dev | Self::Assigned => {
                "opcional · validado enquanto digita".to_owned()
            }
            Self::Provider => "codex/opencode rodam na máquina; compatible usa base URL".to_owned(),
            Self::CodexPath | Self::OpencodePath => {
                "opcional · vazio usa o PATH do sistema".to_owned()
            }
            Self::CodexModel => format!("padrão: {}", crate::config::CODEX_MODEL),
            Self::OpencodeModel => format!("padrão: {}", crate::config::OPENCODE_MODEL),
            Self::CompatModel => format!("padrão: {}", crate::config::DEFAULT_COMPATIBLE_MODEL),
            Self::BaseUrl => format!("padrão: {}", crate::config::DEFAULT_BASE_URL),
            Self::ApiKey => {
                if draft.has_existing_api_key {
                    "Enter mantém a atual".to_owned()
                } else {
                    "opcional · só p/ endpoints autenticados".to_owned()
                }
            }
            Self::Profile => "Agrotrace ou CheckMilk · schema fixo".to_owned(),
            Self::Transition => "opcional · vazio não executa PATCH de estado".to_owned(),
            Self::CodexReasoning | Self::OpencodeReasoning | Self::CompatReasoning => {
                "←/→ para alternar".to_owned()
            }
            Self::AreaPath => r"opcional · ex.: MeuProjeto\Time".to_owned(),
            Self::Team => "padrão: DevOps".to_owned(),
            Self::Program => "padrão: Agrotrace".to_owned(),
        }
    }

    /// Placeholder quando vazio.
    fn placeholder(self) -> &'static str {
        match self {
            Self::Pat => "ya01.…",
            Self::Sprint | Self::Dev | Self::Assigned => "dev@empresa.com",
            Self::CodexPath | Self::OpencodePath => "C:/.../tool.cmd",
            Self::CodexModel => crate::config::CODEX_MODEL,
            Self::OpencodeModel => crate::config::OPENCODE_MODEL,
            Self::CompatModel => crate::config::DEFAULT_COMPATIBLE_MODEL,
            Self::BaseUrl => crate::config::DEFAULT_BASE_URL,
            Self::AreaPath => r"MeuProjeto\Time",
            Self::Profile | Self::Program => "Agrotrace",
            Self::Transition => "Test QA",
            Self::Team => "DevOps",
            _ => "",
        }
    }

    /// É campo secreto (máscara `•`)?
    fn is_secret(self) -> bool {
        matches!(self, Self::Pat | Self::ApiKey)
    }

    /// É select (←/→) em vez de texto?
    fn is_select(self) -> bool {
        matches!(
            self,
            Self::Provider
                | Self::Profile
                | Self::CodexReasoning
                | Self::OpencodeReasoning
                | Self::CompatReasoning
        )
    }

    /// É email com validação ao vivo?
    fn is_email(self) -> bool {
        matches!(self, Self::Sprint | Self::Dev | Self::Assigned)
    }
}

/// Campos de cada etapa (Modelo varia por provider).
fn fields_for(step: Step, provider: &str) -> Vec<Field> {
    match step {
        Step::Azure => vec![Field::Pat],
        Step::Reviewers => vec![Field::Sprint, Field::Dev, Field::Assigned],
        Step::Provider => vec![Field::Provider],
        Step::Model => match provider {
            "opencode" => vec![
                Field::OpencodePath,
                Field::OpencodeModel,
                Field::OpencodeReasoning,
            ],
            "openai-compatible" => vec![
                Field::BaseUrl,
                Field::CompatModel,
                Field::CompatReasoning,
                Field::ApiKey,
            ],
            _ => vec![Field::CodexPath, Field::CodexModel, Field::CodexReasoning],
        },
        Step::TestDefaults => vec![
            Field::Profile,
            Field::AreaPath,
            Field::Team,
            Field::Program,
            Field::Transition,
        ],
        Step::Review => vec![],
    }
}

/// Estado do wizard. O `edit_*` é o editor de linha única do campo focado.
pub struct InitWizard {
    draft: InitDraft,
    profile_name: String,
    parent_transition: String,
    step: usize,
    field: usize,
    edit_value: String,
    /// Cursor em índice de char.
    edit_cursor: usize,
    bound: Option<Field>,
    error: Option<String>,
    done: Option<InitResult>,
    tick: u64,
    /// Campos de email que já falharam numa validação de avanço.
    /// Controla o ✘ tardio do `live_status` (sem ✘ enquanto digita).
    flagged: HashSet<Field>,
}

impl InitWizard {
    /// Cria wizard com rascunho pré-preenchido do disco.
    #[must_use]
    pub fn new() -> Self {
        let existing_profile = crate::config::load_config().ok().map_or_else(
            || ("Agrotrace".to_owned(), "Test QA".to_owned()),
            |config| {
                let name = if !config.default_profile.trim().is_empty()
                    && config
                        .effective_process_profiles()
                        .iter()
                        .any(|profile| profile.name == config.default_profile)
                {
                    config.default_profile.clone()
                } else {
                    "Agrotrace".to_owned()
                };
                let transition = config
                    .profiles
                    .iter()
                    .find(|profile| profile.name == name)
                    .and_then(|profile| profile.parent_transition.clone())
                    .unwrap_or_else(|| "Test QA".to_owned());
                (name, transition)
            },
        );
        let mut w = Self {
            draft: InitDraft::load_existing(),
            profile_name: existing_profile.0,
            parent_transition: existing_profile.1,
            step: 0,
            field: 0,
            edit_value: String::new(),
            edit_cursor: 0,
            bound: None,
            error: None,
            done: None,
            tick: 0,
            flagged: HashSet::new(),
        };
        w.rebind();
        w
    }

    fn step(&self) -> Step {
        STEPS[self.step]
    }

    fn fields(&self) -> Vec<Field> {
        fields_for(self.step(), &self.draft.provider)
    }

    fn current_field(&self) -> Option<Field> {
        self.fields().get(self.field).copied()
    }

    /// Valor do campo no rascunho.
    fn get(&self, field: Field) -> String {
        let d = &self.draft;
        match field {
            Field::Pat => d.pat_input.clone(),
            Field::Sprint => d.reviewer_sprint.clone(),
            Field::Dev => d.reviewer_dev.clone(),
            Field::Assigned => d.test_assigned_to.clone(),
            Field::Provider => d.provider.clone(),
            Field::CodexPath => d.codex_path.clone(),
            Field::CodexModel => d.codex_model.clone(),
            Field::CodexReasoning => d.codex_reasoning.clone(),
            Field::OpencodePath => d.opencode_path.clone(),
            Field::OpencodeModel => d.opencode_model.clone(),
            Field::OpencodeReasoning => d.opencode_reasoning.clone(),
            Field::BaseUrl => d.base_url.clone(),
            Field::CompatModel => d.compatible_model.clone(),
            Field::CompatReasoning => d.compatible_reasoning.clone(),
            Field::ApiKey => d.api_key_input.clone(),
            Field::Profile => self.profile_name.clone(),
            Field::Transition => self.parent_transition.clone(),
            Field::AreaPath => d.test_area_path.clone(),
            Field::Team => d.test_team.clone(),
            Field::Program => d.test_program.clone(),
        }
    }

    /// Grava valor no rascunho.
    fn set(&mut self, field: Field, value: String) {
        if field == Field::Profile {
            let previous_default = if self.profile_name == "CheckMilk" {
                "Checkmilk"
            } else {
                "Agrotrace"
            };
            if self.draft.test_program.trim().is_empty()
                || self.draft.test_program.trim() == previous_default
            {
                self.draft.test_program = if value == "CheckMilk" {
                    "Checkmilk".to_owned()
                } else {
                    "Agrotrace".to_owned()
                };
            }
            self.profile_name = value;
            return;
        }
        if field == Field::Transition {
            self.parent_transition = value;
            return;
        }
        let d = &mut self.draft;
        match field {
            Field::Pat => d.pat_input = value,
            Field::Sprint => d.reviewer_sprint = value,
            Field::Dev => d.reviewer_dev = value,
            Field::Assigned => d.test_assigned_to = value,
            Field::Provider => d.provider = value,
            Field::CodexPath => d.codex_path = value,
            Field::CodexModel => d.codex_model = value,
            Field::CodexReasoning => d.codex_reasoning = value,
            Field::OpencodePath => d.opencode_path = value,
            Field::OpencodeModel => d.opencode_model = value,
            Field::OpencodeReasoning => d.opencode_reasoning = value,
            Field::BaseUrl => d.base_url = value,
            Field::CompatModel => d.compatible_model = value,
            Field::CompatReasoning => d.compatible_reasoning = value,
            Field::ApiKey => d.api_key_input = value,
            Field::Profile => unreachable!("perfil tratado antes do draft"),
            Field::Transition => unreachable!("transição tratada antes do draft"),
            Field::AreaPath => d.test_area_path = value,
            Field::Team => d.test_team = value,
            Field::Program => d.test_program = value,
        }
    }

    /// Opções do select atual + índice selecionado.
    fn select_state(&self, field: Field) -> (Vec<(&'static str, &'static str)>, usize) {
        let (options, current) = match field {
            Field::Provider => (PROVIDERS.to_vec(), self.draft.provider.as_str()),
            Field::Profile => (PROFILE_OPTIONS.to_vec(), self.profile_name.as_str()),
            _ => (
                REASONING_LEVELS.to_vec(),
                match field {
                    Field::CodexReasoning => self.draft.codex_reasoning.as_str(),
                    Field::OpencodeReasoning => self.draft.opencode_reasoning.as_str(),
                    _ => self.draft.compatible_reasoning.as_str(),
                },
            ),
        };
        let idx = options.iter().position(|(v, _)| *v == current).unwrap_or(0);
        (options, idx)
    }

    /// Alterna o select atual (+1/-1, circular).
    fn cycle_select(&mut self, field: Field, delta: i32) {
        let (options, idx) = self.select_state(field);
        let count = options.len();
        if count == 0 {
            self.error = None;
            return;
        }
        // `delta` normalizado p/ `[0, count)`: equivale a `(idx + delta) % count`
        // sem nenhum `as` (valores reais têm 2–4 opções).
        let size = i32::try_from(count).unwrap_or(i32::MAX);
        let shift = usize::try_from(delta.rem_euclid(size)).unwrap_or(0);
        let next = options[(idx + shift) % count].0.to_owned();
        self.set(field, next);
        self.error = None;
    }

    /// Persiste o editor no rascunho (quando era campo de texto).
    fn commit(&mut self) {
        if let Some(prev) = self.bound {
            if !prev.is_select() {
                let value = self.edit_value.clone();
                self.set(prev, value);
            }
        }
    }

    /// Sincroniza o editor com o campo focado (commit do anterior).
    fn rebind(&mut self) {
        self.commit();
        let current = self.current_field();
        self.bound = current;
        if let Some(field) = current {
            if field.is_select() {
                return;
            }
            self.edit_value = self.get(field);
            self.edit_cursor = self.edit_value.chars().count();
        }
    }

    /// Digitação no campo focado (retorna `true` se consumiu a tecla).
    fn edit_input(&mut self, key: event::KeyEvent) -> bool {
        if key
            .modifiers
            .contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        match key.code {
            KeyCode::Char(c) => {
                let byte = char_byte_index(&self.edit_value, self.edit_cursor);
                self.edit_value.insert(byte, c);
                self.edit_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.edit_cursor > 0 {
                    let byte = char_byte_index(&self.edit_value, self.edit_cursor);
                    let prev = char_byte_index(&self.edit_value, self.edit_cursor - 1);
                    self.edit_value.drain(prev..byte);
                    self.edit_cursor -= 1;
                }
            }
            KeyCode::Delete => {
                let len = self.edit_value.chars().count();
                if self.edit_cursor < len {
                    let byte = char_byte_index(&self.edit_value, self.edit_cursor);
                    let next = char_byte_index(&self.edit_value, self.edit_cursor + 1);
                    self.edit_value.drain(byte..next);
                }
            }
            KeyCode::Left => self.edit_cursor = self.edit_cursor.saturating_sub(1),
            KeyCode::Right => {
                self.edit_cursor = (self.edit_cursor + 1).min(self.edit_value.chars().count());
            }
            KeyCode::Home => self.edit_cursor = 0,
            KeyCode::End => self.edit_cursor = self.edit_value.chars().count(),
            _ => return false,
        }
        // Validação ao vivo: limpa o erro global; o ✓/✘ do rótulo reage sozinho.
        self.error = None;
        true
    }

    /// Status ao vivo do campo (✓/✘ tardio ou segredo configurado).
    ///
    /// Emails inválidos só ganham ✘ depois de falharem numa validação
    /// de avanço (`flagged`); antes disso ficam neutros (só a dica).
    fn live_status(&self, field: Field) -> Option<(String, Style)> {
        if field.is_email() {
            let value = if self.bound == Some(field) {
                self.edit_value.clone()
            } else {
                self.get(field)
            };
            if value.trim().is_empty() {
                return None;
            }
            return Some(if validate_optional_email(&value).is_none() {
                ("✓ ".to_owned(), theme().success)
            } else if self.flagged.contains(&field) {
                ("✘ ".to_owned(), theme().error)
            } else {
                return None;
            });
        }
        if field.is_secret() {
            let configured = match field {
                Field::Pat => {
                    !self.edit_value.trim().is_empty() && self.bound == Some(field)
                        || self.draft.has_existing_pat
                }
                _ => {
                    !self.edit_value.trim().is_empty() && self.bound == Some(field)
                        || self.draft.has_existing_api_key
                }
            };
            if configured {
                return Some(("● ".to_owned(), theme().success));
            }
        }
        None
    }

    /// Valida o campo atual (emails); retorna `true` se ok.
    ///
    /// Emails inválidos entram em `flagged` (✘ tardio); ao voltar
    /// a ficar válido o campo sai de `flagged`.
    fn validate_current(&mut self) -> bool {
        let Some(field) = self.current_field() else {
            return true;
        };
        self.commit();
        let err = match field {
            Field::Sprint | Field::Dev | Field::Assigned => {
                validate_optional_email(&self.get(field)).map(|e| format!("{}: {e}", field.label()))
            }
            _ => None,
        };
        self.error = err;
        if self.error.is_none() {
            self.flagged.remove(&field);
        } else {
            self.flagged.insert(field);
        }
        self.error.is_none()
    }

    /// Avança campo/etapa (Enter). Na revisão, salva.
    fn advance(&mut self) {
        if self.step() == Step::Review {
            self.save();
            return;
        }
        if !self.validate_current() {
            return;
        }
        let fields = self.fields();
        if self.field + 1 < fields.len() {
            self.field += 1;
            self.rebind();
        } else {
            self.step = (self.step + 1).min(STEPS.len() - 1);
            self.field = 0;
            self.rebind();
        }
    }

    /// Volta campo/etapa (Esc). Na primeira, aborta.
    fn back(&mut self) -> Option<InitOutcome> {
        self.commit(); // silencioso, sem validar, para não perder digitação
        self.error = None;
        if self.field > 0 {
            self.field -= 1;
            self.rebind();
            None
        } else if self.step > 0 {
            self.step -= 1;
            let len = fields_for(self.step(), &self.draft.provider).len();
            self.field = len.saturating_sub(1);
            self.rebind();
            None
        } else {
            Some(InitOutcome::Aborted)
        }
    }

    /// Salva e vai para a tela de sucesso.
    fn save(&mut self) {
        self.commit();
        if let Some(err) = self.draft.validate_all() {
            self.error = Some(err);
            return;
        }
        match save_draft_for_profile_with_transition(
            &self.draft,
            &self.profile_name,
            Some(self.parent_transition.as_str()),
        ) {
            Ok(res) => {
                self.done = Some(res);
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.to_string());
            }
        }
    }
}

impl Default for InitWizard {
    fn default() -> Self {
        Self::new()
    }
}

/// Índice de byte do n-ésimo char (saturado no fim).
fn char_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b)
}

/// Desenha um frame — status global + formulário.
impl Widget for &InitWizard {
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

fn render_header(wiz: &InitWizard, area: Rect, buf: &mut Buffer) {
    let total = STEPS.len().saturating_sub(1).max(1);
    let ratio = f64::from(u32::try_from(wiz.step).unwrap_or(u32::MAX))
        / f64::from(u32::try_from(total).unwrap_or(u32::MAX));
    let message = if wiz.done.is_some() {
        "arquivo pronto"
    } else if wiz.step() == Step::Review {
        "resumo final"
    } else {
        wiz.step().title()
    };
    status_header(
        area,
        buf,
        StatusHeader {
            command: "init",
            phase: if wiz.done.is_some() {
                "concluído"
            } else {
                "configuração"
            },
            message,
            progress: Some(if wiz.done.is_some() { 1.0 } else { ratio }),
            tick: wiz.tick,
            active: false,
            style: if wiz.done.is_some() {
                theme().success
            } else {
                theme().accent
            },
        },
    );
}

fn render_body(wiz: &InitWizard, area: Rect, buf: &mut Buffer) {
    let form = area;
    if wiz.done.is_some() {
        render_success(wiz, form, buf);
    } else if wiz.step() == Step::Review {
        render_review(wiz, form, buf);
    } else {
        render_form(wiz, form, buf);
    }
}

/// Cartão do formulário sem repetir a etapa já exibida no header.
fn render_form(wiz: &InitWizard, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme().border)
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);

    let fields_area = inner;

    let fields = wiz.fields();
    let mut y = 0u16;
    for (i, field) in fields.iter().enumerate() {
        let focused = i == wiz.field;
        if field.is_select() {
            let (options, selected) = wiz.select_state(*field);
            let height = u16::try_from(options.len() + 2).unwrap_or(u16::MAX);
            if y + height > fields_area.height {
                break;
            }
            let rect = Rect::new(fields_area.x, fields_area.y + y, fields_area.width, height);
            render_select(*field, &wiz.draft, &options, selected, focused, rect, buf);
            y += height + 1;
        } else {
            if y + 4 > fields_area.height {
                break;
            }
            let rect = Rect::new(fields_area.x, fields_area.y + y, fields_area.width, 4);
            render_text_field(wiz, *field, focused, rect, buf);
            y += 5;
        }
    }
    if let Some(err) = &wiz.error {
        if y + 1 < fields_area.height {
            Paragraph::new(Line::from(vec![
                Span::styled("✘ ", theme().error),
                Span::styled(err.clone(), theme().error),
            ]))
            .render(
                Rect::new(fields_area.x, fields_area.y + y, fields_area.width, 1),
                buf,
            );
        }
    }
}

/// Campo de texto: rótulo com status ao vivo + editor com borda pulsante.
fn render_text_field(wiz: &InitWizard, field: Field, focused: bool, area: Rect, buf: &mut Buffer) {
    let [label_area, input_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(3)]).areas(area);
    let marker = if focused { "▸ " } else { "  " };
    let mut label = vec![
        Span::styled(
            marker,
            if focused {
                theme().accent
            } else {
                theme().muted
            },
        ),
        Span::styled(
            field.label().to_owned(),
            if focused {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                theme().muted
            },
        ),
    ];
    // Status ao vivo: ✓/✘ do email reage a cada tecla.
    if let Some((glyph, style)) = wiz.live_status(field) {
        label.push(Span::styled(format!("  {glyph}"), style));
    }
    label.push(Span::styled(
        format!("  ·  {}", field.hint(&wiz.draft)),
        theme().muted,
    ));
    Paragraph::new(Line::from(label)).render(label_area, buf);

    if focused {
        // Borda pulsante — o foco "respira" com o tick.
        let border = if wiz.tick % 2 == 0 {
            theme().accent
        } else {
            theme().app_title
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type())
            .border_style(border);
        let inner = block.inner(input_area);
        block.render(input_area, buf);
        render_editor(
            &editor_display(field, &wiz.edit_value, true),
            wiz.edit_cursor,
            field.is_secret(),
            inner,
            buf,
        );
    } else {
        let shown = wiz.get(field);
        let (display, style) = if field.is_secret() && !shown.is_empty() {
            ("•".repeat(shown.chars().count().min(24)), Style::new())
        } else if shown.is_empty() {
            (
                field.placeholder().to_owned(),
                theme().muted.add_modifier(Modifier::ITALIC),
            )
        } else {
            (shown, Style::new())
        };
        Paragraph::new(Span::styled(display, style))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(border_type())
                    .border_style(theme().muted),
            )
            .render(input_area, buf);
    }
}

/// Texto exibido no editor (máscara `•` nos segredos).
fn editor_display(field: Field, value: &str, _focused: bool) -> String {
    if field.is_secret() {
        "•".repeat(value.chars().count())
    } else {
        value.to_owned()
    }
}

/// Linha do editor com cursor reverso e scroll horizontal que segue o cursor.
///
/// Largura medida em células de terminal (CJK/emoji valem 2), não em chars.
/// A máscara `•` tem largura 1, então segredos não mudam o cálculo.
/// Itera por chars (sem fatiar `&str` por byte) para nunca quebrar UTF-8.
fn render_editor(display: &str, cursor: usize, _secret: bool, area: Rect, buf: &mut Buffer) {
    let width = area.width as usize;
    if width == 0 {
        return;
    }
    let chars: Vec<char> = display.chars().collect();
    let widths: Vec<usize> = chars
        .iter()
        .map(|&ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .collect();
    let cursor = cursor.min(chars.len());
    // Células que o cursor ocupa: o char sob o cursor, ou 1 (bloco) no fim.
    let cursor_cell: usize = if cursor < chars.len() {
        widths[cursor].max(1)
    } else {
        1
    };
    // Janela `[start, end)` de chars cuja soma cabe em `width`,
    // sempre contendo o cursor; expande à esquerda e depois à direita.
    let mut start = cursor;
    let mut end = if cursor < chars.len() {
        cursor + 1
    } else {
        chars.len()
    };
    let mut used = cursor_cell;
    while start > 0 && used.saturating_add(widths[start - 1]) <= width {
        start -= 1;
        used += widths[start];
    }
    while end < chars.len() && used.saturating_add(widths[end]) <= width {
        used += widths[end];
        end += 1;
    }
    let mut spans = Vec::new();
    for (i, ch) in chars[start..end].iter().enumerate() {
        let global = start + i;
        let style = if global == cursor {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        spans.push(Span::styled(ch.to_string(), style));
    }
    if cursor >= chars.len() {
        // Cursor no fim: bloco reverso.
        spans.push(Span::styled(
            " ",
            Style::new().add_modifier(Modifier::REVERSED),
        ));
    }
    if spans.is_empty() {
        spans.push(Span::styled(
            " ",
            Style::new().add_modifier(Modifier::REVERSED),
        ));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

/// Select: opções em lista vertical com destaque no atual.
fn render_select(
    field: Field,
    draft: &InitDraft,
    options: &[(&str, &str)],
    selected: usize,
    focused: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    let [label_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let marker = if focused { "▸ " } else { "  " };
    Paragraph::new(Line::from(vec![
        Span::styled(
            marker,
            if focused {
                theme().accent
            } else {
                theme().muted
            },
        ),
        Span::styled(
            field.label().to_owned(),
            if focused {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                theme().muted
            },
        ),
        Span::styled(format!("  ·  {}", field.hint(draft)), theme().muted),
    ]))
    .render(label_area, buf);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (value, label))| {
            let (glyph, style) = if i == selected {
                ("● ", theme().accent.add_modifier(Modifier::BOLD))
            } else {
                ("○ ", theme().muted)
            };
            ListItem::new(Line::from(vec![
                Span::styled(glyph, style),
                Span::styled((*label).to_owned(), style),
                Span::styled(format!("  ({value})"), theme().muted),
            ]))
        })
        .collect();
    List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(border_type())
                .border_style(if focused {
                    theme().accent
                } else {
                    theme().muted
                }),
        )
        .render(list_area, buf);
}

/// Tela de revisão: resumo + botão salvar pulsante.
fn render_review(wiz: &InitWizard, area: Rect, buf: &mut Buffer) {
    let d = &wiz.draft;
    let model_line = match d.provider.as_str() {
        "opencode" => format!(
            "{} ({})",
            or_empty(&d.opencode_model, crate::config::OPENCODE_MODEL),
            d.opencode_reasoning
        ),
        "openai-compatible" => format!(
            "{} @ {} ({})",
            or_empty(&d.compatible_model, crate::config::DEFAULT_COMPATIBLE_MODEL),
            or_empty(&d.base_url, crate::config::DEFAULT_BASE_URL),
            d.compatible_reasoning
        ),
        _ => format!(
            "{} ({})",
            or_empty(&d.codex_model, crate::config::CODEX_MODEL),
            d.codex_reasoning
        ),
    };
    let executable = match d.provider.as_str() {
        "opencode" => or_dash(&d.opencode_path),
        "codex" => or_dash(&d.codex_path),
        _ => "—".to_owned(),
    };
    let rows = [
        (
            "perfil",
            format!(
                "{} / {}",
                wiz.profile_name,
                if wiz.profile_name == "CheckMilk" {
                    "Custom.ProgramasCheckmilk"
                } else {
                    "Custom.ProgramasAgrotrace"
                }
            ),
        ),
        ("transição pai", or_dash(&wiz.parent_transition)),
        (
            "binding",
            crate::git::collect(None)
                .ok()
                .and_then(|context| context.remote)
                .map_or_else(
                    || "remote Azure não detectado".to_owned(),
                    |remote| {
                        format!(
                            "{}/{}/{}",
                            remote.organization, remote.project, remote.repository
                        )
                    },
                ),
        ),
        ("provider", d.provider.clone()),
        ("modelo", model_line),
        ("executável", executable),
        (
            "azure pat",
            secret_state(!d.pat_input.trim().is_empty() || d.has_existing_pat),
        ),
        ("review sprint", or_dash(&d.reviewer_sprint)),
        ("review dev", or_dash(&d.reviewer_dev)),
        ("card responsável", or_dash(&d.test_assigned_to)),
        ("areapath", or_dash(&d.test_area_path)),
        (
            "team / program",
            format!(
                "{} / {}",
                or_empty(&d.test_team, "DevOps"),
                or_empty(&d.test_program, "Agrotrace")
            ),
        ),
    ];
    let mut text: Vec<Line> = vec![
        Line::from(Span::styled(
            "Confira antes de salvar — esc volta para editar.",
            theme().muted,
        )),
        Line::from(""),
    ];
    for (k, v) in rows {
        text.push(Line::from(vec![
            Span::styled(format!("{k:>16}  "), theme().muted),
            Span::styled(v, Style::new().add_modifier(Modifier::BOLD)),
        ]));
    }
    text.push(Line::from(""));
    // Botão salvar alterna de cor com o tick — impossível não notar.
    let glow = wiz.tick % 2 == 0;
    text.push(Line::from(vec![
        Span::styled(
            "▸ [ enter ] salvar configuração",
            if glow {
                theme().success
            } else {
                theme().accent
            },
        ),
        Span::styled("   ·   esc voltar", theme().muted),
    ]));
    Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title(Span::styled(
                    " ✔ Revisão ",
                    theme().success.add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(theme().success)
                .border_type(border_type())
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .render(area, buf);
    if let Some(err) = &wiz.error {
        // Erro curto: altura fixa justa (1–2 linhas + bordas).
        let inner = super::modal_frame(area, buf, " Erro ", theme().error, 64, 2);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        Paragraph::new(format!("✘ {err}"))
            .style(theme().error)
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }
}

/// Tela de sucesso após salvar.
fn render_success(wiz: &InitWizard, area: Rect, buf: &mut Buffer) {
    let res = wiz.done.as_ref().expect("tela de sucesso exige resultado");
    let text = vec![
        Line::from(Span::styled(
            "✔ Config salva!",
            theme().success.add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("arquivo  ", theme().muted),
            Span::styled(res.config_file.clone(), Style::new()),
        ]),
        Line::from(vec![
            Span::styled(".env     ", theme().muted),
            Span::styled(res.env_file.clone(), Style::new()),
        ]),
        Line::from(vec![
            Span::styled("pat      ", theme().muted),
            Span::styled(secret_state(res.pat_configured), Style::new()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Pronto. Execute `prt desc --dry-run` para testar.",
            theme().muted,
        )),
        Line::from(""),
        Line::from(Span::styled(
            "qualquer tecla sai",
            theme().muted.add_modifier(Modifier::ITALIC),
        )),
    ];
    Paragraph::new(text)
        .block(
            Block::default()
                .title(Span::styled(
                    " ✔ Pronto ",
                    theme().success.add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(theme().success)
                .border_type(border_type())
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .render(area, buf);
}

fn or_dash(value: &str) -> String {
    let t = value.trim();
    if t.is_empty() {
        "—".to_owned()
    } else {
        t.to_owned()
    }
}

fn or_empty(value: &str, default: &str) -> String {
    let t = value.trim();
    if t.is_empty() {
        default.to_owned()
    } else {
        t.to_owned()
    }
}

fn secret_state(configured: bool) -> String {
    if configured {
        "•••••••• (configurado)".to_owned()
    } else {
        "não configurado".to_owned()
    }
}

fn render_footer(wiz: &InitWizard, area: Rect, buf: &mut Buffer) {
    // Ajuda honesta por contexto: `q` só aborta em selects/revisão/sucesso,
    // em campo de texto `q` digita a letra.
    let hints = if wiz.done.is_some() {
        "qualquer tecla sai"
    } else if wiz.step() == Step::Review {
        "enter salvar · esc voltar · q sair"
    } else if matches!(wiz.current_field(), Some(f) if f.is_select()) {
        "←/→ alternar · tab/↓ próximo · shift+tab/↑ anterior · enter avançar · esc voltar · q sair"
    } else {
        "digite normalmente · tab/↓ próximo · shift+tab/↑ anterior · enter avançar · esc voltar"
    };
    Paragraph::new(Line::from(Span::styled(hints, theme().muted))).render(area, buf);
}

/// Roda o wizard até salvar ou abortar.
///
/// # Errors
///
/// Retorna erro se o terminal não puder ser inicializado.
pub async fn run_init_wizard() -> anyhow::Result<InitOutcome> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("wizard requer terminal interativo");
    }
    let mut terminal: DefaultTerminal = ratatui::init();
    let res = run_loop(&mut terminal).await;
    ratatui::restore();
    res
}

// Mantido `async` de propósito: o wrapper público `run_init_wizard`
// (com `.await` em `main.rs`, fora do escopo permitido) aguarda este loop,
// e o fluxo irmão `test_flow::run_loop` é `async` de verdade. Remover o
// `async` aqui só deslocaria o `unused_async` para a API pública.
#[allow(clippy::unused_async)]
async fn run_loop(terminal: &mut DefaultTerminal) -> anyhow::Result<InitOutcome> {
    let mut wiz = InitWizard::new();
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = std::time::Instant::now();

    loop {
        // Tick contínuo: pulsos do formulário animam mesmo parado.
        if last_tick.elapsed() >= tick_rate {
            wiz.tick = wiz.tick.wrapping_add(1);
            last_tick = std::time::Instant::now();
        }
        terminal.draw(|f| f.render_widget(&wiz, f.area()))?;

        // Tela de sucesso: qualquer tecla (Press) encerra entregando o resultado.
        if wiz.done.is_some() {
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        let res = wiz.done.take().expect("resultado presente");
                        return Ok(InitOutcome::Saved(res));
                    }
                }
            }
            continue;
        }

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        // No Windows, Release dispara junto — ignora para não processar 2×.
        // Repeat passa direto: o editor já trata Left/Right/Delete por evento.
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if let Some(outcome) = handle_wizard_key(&mut wiz, &mut *terminal, key)? {
            return Ok(outcome);
        }
    }
}

/// Trata uma tecla do wizard; `Some` encerra o loop com o resultado.
///
/// Extraído de `run_loop` (que estourou `too_many_lines`); comportamento
/// idêntico, só `continue` virou `return None` e `return Ok(x)` virou
/// `return Some(x)`.
fn handle_wizard_key(
    wiz: &mut InitWizard,
    terminal: &mut DefaultTerminal,
    key: event::KeyEvent,
) -> anyhow::Result<Option<InitOutcome>> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Ok(Some(InitOutcome::Aborted));
    }
    if key.code == KeyCode::Char('z') && key.modifiers.contains(KeyModifiers::CONTROL) {
        #[cfg(unix)]
        {
            super::suspend::suspend_to_shell(&mut *terminal)?;
        }
        // No Windows ignora; redesenho vem no próximo tick de todo modo.
        return Ok(None);
    }
    // `q` aborta em selects e na revisão; em campo de texto digita `q`.
    if key.code == KeyCode::Char('q')
        && (matches!(wiz.current_field(), Some(f) if f.is_select()) || wiz.step() == Step::Review)
    {
        return Ok(Some(InitOutcome::Aborted));
    }

    let field = wiz.current_field();
    match key.code {
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                if let Some(outcome) = wiz.back() {
                    return Ok(Some(outcome));
                }
            } else {
                wiz.advance();
            }
        }
        KeyCode::Enter => {
            wiz.advance();
        }
        // `Esc` e `Up` voltam campo/etapa (na primeira, abortam): corpos iguais.
        KeyCode::Esc | KeyCode::Up => {
            if let Some(outcome) = wiz.back() {
                return Ok(Some(outcome));
            }
        }
        KeyCode::Down => {
            if !wiz.validate_current() {
                return Ok(None);
            }
            let fields = wiz.fields();
            if wiz.field + 1 < fields.len() {
                wiz.field += 1;
                wiz.rebind();
            } else {
                wiz.advance();
            }
        }
        KeyCode::Left | KeyCode::Right => {
            if let Some(f) = field {
                if f.is_select() {
                    let delta = if key.code == KeyCode::Right { 1 } else { -1 };
                    wiz.cycle_select(f, delta);
                    return Ok(None);
                }
            }
            wiz.edit_input(key);
        }
        _ => {
            if let Some(f) = field {
                if f.is_select() {
                    match key.code {
                        KeyCode::Char('l' | ' ') => wiz.cycle_select(f, 1),
                        KeyCode::Char('h') => wiz.cycle_select(f, -1),
                        KeyCode::Char('q') => return Ok(Some(InitOutcome::Aborted)),
                        _ => {}
                    }
                    return Ok(None);
                }
            }
            wiz.edit_input(key);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_should_cover_all_fields() {
        for provider in ["codex", "opencode", "openai-compatible"] {
            let fields = fields_for(Step::Model, provider);
            assert!(!fields.is_empty(), "sem campos p/ {provider}");
        }
        assert_eq!(
            fields_for(Step::Model, "codex"),
            vec![Field::CodexPath, Field::CodexModel, Field::CodexReasoning]
        );
        assert_eq!(
            fields_for(Step::Model, "opencode"),
            vec![
                Field::OpencodePath,
                Field::OpencodeModel,
                Field::OpencodeReasoning
            ]
        );
        assert!(fields_for(Step::Review, "codex").is_empty());
    }

    #[test]
    fn wizard_should_advance_and_back() {
        let mut wiz = InitWizard::new();
        assert_eq!(wiz.step(), Step::Azure);
        wiz.advance();
        assert_eq!(wiz.step(), Step::Reviewers);
        let out = wiz.back();
        assert!(out.is_none());
        assert_eq!(wiz.step(), Step::Azure);
    }

    #[test]
    fn wizard_should_block_invalid_email() {
        let mut wiz = InitWizard::new();
        wiz.advance(); // → reviewers
        wiz.edit_value = "email-ruim".to_owned();
        assert!(!wiz.validate_current());
        assert!(wiz.error.is_some());
    }

    #[test]
    fn cycle_should_wrap_around() {
        let mut wiz = InitWizard::new();
        wiz.set(Field::Provider, "codex".to_owned());
        wiz.cycle_select(Field::Provider, -1);
        assert_eq!(wiz.draft.provider, "openai-compatible");
        wiz.cycle_select(Field::Provider, 1);
        assert_eq!(wiz.draft.provider, "codex");
    }

    #[test]
    fn editor_should_insert_and_move_cursor() {
        let mut wiz = InitWizard::new();
        wiz.rebind();
        let key = |c: char| event::KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        wiz.edit_input(key('a'));
        wiz.edit_input(key('b'));
        assert_eq!(wiz.edit_value, "ab");
        wiz.edit_input(event::KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        wiz.edit_input(key('X'));
        assert_eq!(wiz.edit_value, "aXb");
    }

    #[test]
    fn live_status_should_mark_valid_email() {
        let mut wiz = InitWizard::new();
        // Hermético: limpa valor vindo do disco da máquina.
        // (Na etapa Azure o Sprint não está bound, então lê do draft.)
        wiz.draft.reviewer_sprint.clear();
        assert!(wiz.live_status(Field::Sprint).is_none());
        wiz.draft.reviewer_sprint = "dev@empresa.com".to_owned();
        let (glyph, _) = wiz
            .live_status(Field::Sprint)
            .expect("email válido marca ✓");
        assert_eq!(glyph, "✓ ");
        // Inválido ainda sem falha de avanço: neutro (só a dica, sem ✘).
        wiz.draft.reviewer_sprint = "ruim".to_owned();
        assert!(wiz.live_status(Field::Sprint).is_none());
        // Após falhar validação de avanço, marca ✘.
        wiz.advance(); // Azure → Reviewers (Sprint focado, carrega "ruim").
        assert!(!wiz.validate_current());
        let (glyph, _) = wiz
            .live_status(Field::Sprint)
            .expect("email inválido marca ✘ após falha");
        assert_eq!(glyph, "✘ ");
        // Ao voltar a ficar válido, limpa o flag e volta a ✓ (sem expect novo).
        wiz.edit_value = "dev@empresa.com".to_owned();
        assert!(wiz.validate_current());
        assert_eq!(
            wiz.live_status(Field::Sprint).map(|(g, _)| g).as_deref(),
            Some("✓ ")
        );
    }

    /// Wizard hermético p/ snapshots: zera tudo que `load_existing()` lê do disco.
    fn hermetic_wizard() -> InitWizard {
        let mut wiz = InitWizard::new();
        wiz.profile_name = "Agrotrace".to_owned();
        wiz.parent_transition = "Test QA".to_owned();
        wiz.draft.pat_input.clear();
        wiz.draft.has_existing_pat = false;
        wiz.draft.reviewer_sprint.clear();
        wiz.draft.reviewer_dev.clear();
        wiz.draft.test_assigned_to.clear();
        wiz.draft.provider = "codex".to_owned();
        wiz.draft.codex_path.clear();
        wiz.draft.codex_model.clear();
        wiz.draft.codex_reasoning = "provider-default".to_owned();
        wiz.draft.opencode_path.clear();
        wiz.draft.opencode_model.clear();
        wiz.draft.opencode_reasoning = "provider-default".to_owned();
        wiz.draft.base_url.clear();
        wiz.draft.compatible_model.clear();
        wiz.draft.compatible_reasoning = "provider-default".to_owned();
        wiz.draft.api_key_input.clear();
        wiz.draft.has_existing_api_key = false;
        wiz.draft.test_area_path.clear();
        wiz.draft.test_team = "DevOps".to_owned();
        wiz.draft.test_program = "Agrotrace".to_owned();
        wiz.step = 0;
        wiz.field = 0;
        wiz.edit_value.clear();
        wiz.edit_cursor = 0;
        wiz.bound = None;
        wiz.error = None;
        wiz.done = None;
        wiz.tick = 0;
        wiz.flagged.clear();
        wiz.rebind();
        wiz
    }

    #[test]
    fn wizard_review_should_show_profile_binding_and_reviewers() {
        let mut wiz = hermetic_wizard();
        wiz.profile_name = "CheckMilk".to_owned();
        wiz.draft.reviewer_dev = "dev@checkmilk.example".to_owned();
        wiz.draft.reviewer_sprint = "sprint@checkmilk.example".to_owned();
        wiz.step = STEPS.len() - 1;
        let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 100, 30));
        render_review(&wiz, buffer.area, &mut buffer);
        let text = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().to_owned())
            .collect::<String>();
        assert!(text.contains("CheckMilk"));
        assert!(text.contains("dev@checkmilk.example"));
        assert!(text.contains("sprint@checkmilk.example"));
    }

    #[test]
    fn init_azure_80x24() -> anyhow::Result<()> {
        use ratatui::{Terminal, backend::TestBackend};
        let app = hermetic_wizard();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        crate::assert_tui_snapshot!("init_azure_80x24", terminal.backend());
        Ok(())
    }

    #[test]
    fn init_azure_60x22() -> anyhow::Result<()> {
        use ratatui::{Terminal, backend::TestBackend};
        let app = hermetic_wizard();
        let backend = TestBackend::new(60, 22);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        crate::assert_tui_snapshot!("init_azure_60x22", terminal.backend());
        Ok(())
    }
}
