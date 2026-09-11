//! Estado do app `desc` — máquina de fases puramente testável (sem terminal).
//!
//! O loop vivo em `live.rs` alimenta este estado via [`BackendEvent`];
//! o `Widget for &App` desenha tudo a cada frame, então cada token,
//! log ou progresso reage na tela em ~33ms.

use std::collections::VecDeque;
use std::time::Instant;

use super::events::BackendEvent;
use super::shimmer::{tick_frame_index, u16_from_i32_clamped};
use super::spin_frames;
use crate::ai::PrDescription;
use crate::azure::pull_requests::{PublishedPr, PullRequestCandidate};
use crate::features::describe::{PublishFailure, PublishFailureKind};

/// Frames do spinner (efeito de atividade).
///
/// Mantido por compat; o estado usa [`super::spin_frames()`], que respeita
/// `PRT_ASCII`/`TERM=dumb`.
pub const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Fase do fluxo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Inicializando / coletando contexto.
    #[default]
    Boot,
    /// Gerando via IA (streaming).
    Generating,
    /// Revisão do resultado (scroll, copy, publish).
    Review,
    /// Publicando PRs (animado).
    Publishing,
    /// Concluído.
    Done,
    /// Erro.
    Error,
}

/// Config de publicação (espelha os defaults do comando Dart).
#[derive(Debug, Clone)]
pub struct PublishSetup {
    /// Email de review para targets `sprint*` (pode ser vazio).
    pub reviewer_sprint: String,
    /// Email de review para os demais targets.
    pub reviewer_dev: String,
}

impl PublishSetup {
    /// Default por target: `sprint*` usa sprint (ou dev de fallback), resto dev.
    #[must_use]
    pub fn default_for(&self, target: &str) -> String {
        if target.contains("sprint") && !self.reviewer_sprint.trim().is_empty() {
            self.reviewer_sprint.trim().to_owned()
        } else {
            self.reviewer_dev.trim().to_owned()
        }
    }
}

/// Diálogo modal do fluxo de publicação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishDialog {
    /// "Criar PR(s)?" — bool = Sim selecionado?
    ConfirmCreate(bool),
    /// Um campo de reviewer por target.
    Reviewers,
    /// "Criar com estes reviewers?" — bool = Sim selecionado?
    ConfirmPublish(bool),
    /// Ações após uma falha de publicação: retry, busca ou voltar.
    PublishRecovery(usize),
    /// Possíveis PRs retornados pela busca de duplicidade.
    CandidateList {
        /// Índice do candidato focado.
        selected: usize,
    },
}

/// Atividade da consulta de possíveis PRs já criados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateActivity {
    /// Nenhuma consulta remota em andamento.
    Idle,
    /// Consultando PRs recentes.
    Loading,
}

/// Estado completo da tela `desc`.
#[derive(Debug)]
pub struct DescribeApp {
    /// Branch de origem.
    pub branch: String,
    /// Targets (abas).
    pub targets: Vec<String>,
    /// Aba selecionada.
    pub selected_target: usize,
    /// Work Item.
    pub work_item_id: String,
    /// Fase atual.
    pub phase: Phase,
    /// Rótulo da fase (ex.: "tentando codex…").
    pub phase_label: String,
    /// Início (para elapsed).
    pub started_at: Instant,
    /// Frame do spinner (incrementado a cada tick).
    pub tick: u64,
    /// Tokens brutos recebidos (efeito typing).
    pub streamed_raw: String,
    /// Descrição final normalizada.
    pub desc: Option<PrDescription>,
    /// Raw final (para debug).
    pub raw_final: String,
    /// Logs recentes (cap 200).
    pub logs: VecDeque<String>,
    /// Progresso 0.0–1.0.
    pub progress: f64,
    /// Rótulo do progresso.
    pub progress_label: String,
    /// Scroll vertical do preview.
    pub scroll: u16,
    /// Mensagem de erro.
    pub error: Option<String>,
    /// Etapa em que ocorreu o erro (para manter o stepper contextual).
    pub error_step: usize,
    /// Mostra popup de ajuda?
    pub show_help: bool,
    /// Copiado agora? (feedback visual temporário).
    pub copied_flash_until_tick: u64,
    /// URLs publicadas (uma por target, na ordem).
    pub published_urls: Vec<String>,
    /// Valor inicial do "Criar PR(s)?" (vem de `--create`).
    pub create_initial: bool,
    /// Setup de publicação (`None` = publicar indisponível + motivo).
    pub publish_setup: Option<PublishSetup>,
    /// Motivo quando `publish_setup` é `None`.
    pub publish_blocked: Option<String>,
    /// Diálogo modal aberto (publicação).
    pub publish_dialog: Option<PublishDialog>,
    /// Reviewers por target (paralelo a `targets`).
    pub reviewers: Vec<String>,
    /// Índice do campo de reviewer focado.
    pub reviewer_idx: usize,
    /// Buffer de edição do campo focado.
    pub reviewer_edit: String,
    /// Cursor (índice de char) no buffer.
    pub reviewer_cursor: usize,
    /// PRs publicados (target, id, url).
    pub published: Vec<PublishedPr>,
    /// Target atualmente em processamento.
    pub current_publish_target: Option<String>,
    /// Falha de publicação recuperável exibida na revisão.
    pub publish_failure: Option<PublishFailure>,
    /// Possíveis PRs encontrados após resultado incerto.
    pub candidates: Vec<PullRequestCandidate>,
    /// Atividade da busca de candidatos.
    pub(crate) candidate_activity: CandidateActivity,
    /// Mensagem da busca de candidatos.
    pub candidate_message: Option<String>,
}

impl DescribeApp {
    /// Cria estado inicial.
    #[must_use]
    pub fn new(
        branch: &str,
        targets: &[String],
        work_item_id: &str,
        create_initial: bool,
        publish_setup: Option<PublishSetup>,
        publish_blocked: Option<String>,
    ) -> Self {
        Self {
            branch: branch.to_owned(),
            targets: targets.to_vec(),
            selected_target: 0,
            work_item_id: work_item_id.to_owned(),
            phase: Phase::Boot,
            phase_label: "inicializando…".to_owned(),
            started_at: Instant::now(),
            tick: 0,
            streamed_raw: String::new(),
            desc: None,
            raw_final: String::new(),
            logs: VecDeque::with_capacity(200),
            progress: 0.0,
            progress_label: "preparando".to_owned(),
            scroll: 0,
            error: None,
            error_step: 0,
            show_help: false,
            copied_flash_until_tick: 0,
            published_urls: Vec::new(),
            create_initial,
            publish_setup,
            publish_blocked,
            publish_dialog: None,
            reviewers: Vec::new(),
            reviewer_idx: 0,
            reviewer_edit: String::new(),
            reviewer_cursor: 0,
            published: Vec::new(),
            current_publish_target: None,
            publish_failure: None,
            candidates: Vec::new(),
            candidate_activity: CandidateActivity::Idle,
            candidate_message: None,
        }
    }

    /// Avança 1 tick (~33ms): move spinner e shimmer.
    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Aplica evento do backend — cada chamada muda a tela no próximo frame.
    pub fn on_backend(&mut self, ev: BackendEvent) {
        match ev {
            BackendEvent::Log(line) => {
                if self.logs.len() >= 200 {
                    self.logs.pop_front();
                }
                self.logs.push_back(line);
            }
            BackendEvent::Token(chunk) => {
                self.streamed_raw.push_str(&chunk);
                if self.phase == Phase::Boot {
                    self.phase = Phase::Generating;
                }
            }
            BackendEvent::Progress(ratio, label) => {
                self.progress = ratio.clamp(0.0, 1.0);
                self.progress_label = label;
            }
            BackendEvent::Phase(label) => {
                self.phase_label.clone_from(&label);
                self.push_log(label);
                // A coleta de contexto é a própria etapa de Boot. Só entra
                // em Generating quando o backend sinaliza o provider ou
                // começa a entregar tokens; assim a primeira etapa permanece
                // visível em vez de ser pulada no primeiro evento.
                if self.phase == Phase::Boot && !self.phase_label.starts_with("coletando contexto")
                {
                    self.phase = Phase::Generating;
                }
            }
            BackendEvent::Finished(desc, raw) => {
                self.desc = Some(desc);
                self.raw_final = raw;
                self.phase = Phase::Review;
                "revisão".clone_into(&mut self.phase_label);
                self.progress = 1.0;
                "pronto".clone_into(&mut self.progress_label);
                self.push_log(
                    "descrição pronta — revise, copie (c) ou publique (enter)".to_owned(),
                );
            }
            BackendEvent::Published(published) => {
                self.phase = Phase::Done;
                "publicado".clone_into(&mut self.phase_label);
                self.progress = 1.0;
                "pronto".clone_into(&mut self.progress_label);
                for item in published {
                    if !self.published.iter().any(|current| current.id == item.id) {
                        self.push_log(format!("PR {} criado: {}", item.target, item.url));
                        self.published.push(item);
                    }
                }
                self.published_urls = self.published.iter().map(|p| p.url.clone()).collect();
                self.current_publish_target = None;
                self.publish_failure = None;
                self.candidate_activity = CandidateActivity::Idle;
                self.candidates.clear();
                self.candidate_message = None;
            }
            BackendEvent::PublishedOne(item) => {
                if !self.published.iter().any(|current| current.id == item.id) {
                    self.push_log(format!("PR {} criado: {}", item.target, item.url));
                    self.published_urls.push(item.url.clone());
                    self.published.push(item);
                }
                self.current_publish_target = None;
            }
            BackendEvent::PublishingTarget(target) => {
                self.current_publish_target = Some(target.clone());
                self.phase = Phase::Publishing;
                self.phase_label = format!("publicando {target}…");
                self.progress_label = format!("criando PR {target}");
                self.push_log(format!("criando PR {target}"));
            }
            BackendEvent::PublishFailed(failure) => {
                self.on_publish_failed(&failure);
            }
            BackendEvent::CandidatesLoaded { candidates, error } => {
                self.on_candidates_loaded(candidates, error);
            }
            BackendEvent::Failed(msg) => {
                self.error_step = self.step_index();
                self.phase = Phase::Error;
                self.error = Some(msg.clone());
                "erro".clone_into(&mut self.phase_label);
                self.push_log(format!("erro: {msg}"));
            }
        }
    }

    /// Segundos decorridos.
    #[must_use]
    pub fn elapsed_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Símbolo de atividade: gira enquanto há trabalho, parado no ocioso.
    ///
    /// Ocioso mostra estado final (`●` revisando, `✓` pronto, `✘` erro) em
    /// vez de girar à toa — spinner que nunca para sinaliza trabalho
    /// inexistente.
    #[must_use]
    pub fn spinner(&self) -> &str {
        match self.phase {
            Phase::Boot | Phase::Generating | Phase::Publishing => {
                let frames = spin_frames();
                frames[tick_frame_index(self.tick, frames.len())]
            }
            Phase::Review => {
                if super::ascii_only() {
                    "*"
                } else {
                    "●"
                }
            }
            Phase::Done => {
                if super::ascii_only() {
                    "+"
                } else {
                    "✓"
                }
            }
            Phase::Error => {
                if super::ascii_only() {
                    "x"
                } else {
                    "✘"
                }
            }
        }
    }

    /// Texto do preview: final se pronto, senão stream parcial + cursor.
    #[must_use]
    pub fn preview_text(&self) -> String {
        if let Some(d) = &self.desc {
            format!("# {}\n\n{}", d.title, d.body)
        } else if self.streamed_raw.is_empty() {
            if super::ascii_only() {
                "aguardando primeiro token...".to_owned()
            } else {
                "aguardando primeiro token…".to_owned()
            }
        } else {
            let cursor = if super::ascii_only() { "_" } else { "▊" };
            format!("{}{cursor}", self.streamed_raw)
        }
    }

    /// Índice da etapa exibida no stepper (0 = contexto, 3 = publicação).
    #[must_use]
    pub fn step_index(&self) -> usize {
        match self.phase {
            Phase::Boot => 0,
            Phase::Generating => 1,
            Phase::Review => 2,
            Phase::Publishing => 3,
            Phase::Done => 4,
            Phase::Error => self.error_step.min(3),
        }
    }

    /// Quantidade de caracteres exibida para a descrição/stream.
    #[must_use]
    pub fn content_chars(&self) -> usize {
        self.desc
            .as_ref()
            .map_or(self.streamed_raw.len(), |description| {
                description.body.len()
            })
    }

    /// Número de tokens (aprox. por chars/4).
    #[must_use]
    pub fn token_count(&self) -> usize {
        self.streamed_raw.len() / 4
    }

    /// Scroll para cima/baixo com clamp simples.
    pub fn scroll_by(&mut self, delta: i16) {
        let next = i32::from(self.scroll) + i32::from(delta);
        self.scroll = u16_from_i32_clamped(next, 5000);
    }

    /// Alterna aba de target.
    pub fn next_target(&mut self) {
        if !self.targets.is_empty() {
            self.selected_target = (self.selected_target + 1) % self.targets.len();
        }
    }

    /// Abre o diálogo "Criar PR(s)?" (só com descrição pronta e sem diálogo).
    pub fn open_confirm_create(&mut self) {
        if self.desc.is_some() && self.publish_dialog.is_none() {
            self.publish_dialog = Some(PublishDialog::ConfirmCreate(self.create_initial));
        }
    }

    /// Abre a edição de reviewers (valores = defaults por target).
    pub fn open_reviewers(&mut self) {
        if self.reviewers.len() != self.targets.len() {
            let setup = self.publish_setup.clone().unwrap_or(PublishSetup {
                reviewer_sprint: String::new(),
                reviewer_dev: String::new(),
            });
            self.reviewers = self.targets.iter().map(|t| setup.default_for(t)).collect();
        }
        self.reviewer_idx = 0;
        self.publish_dialog = Some(PublishDialog::Reviewers);
        self.rebind_reviewer();
    }

    /// Resumo `target: reviewer` (vazio vira `nenhum`), como no Dart.
    #[must_use]
    pub fn reviewer_summary(&self) -> String {
        self.targets
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let reviewer = self.reviewers.get(i).map_or("", String::as_str);
                let shown = if reviewer.trim().is_empty() {
                    "nenhum"
                } else {
                    reviewer.trim()
                };
                format!("{target}: {shown}")
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Persiste o buffer no campo focado e foca outro campo.
    pub fn focus_reviewer(&mut self, idx: usize) {
        self.commit_reviewer();
        self.reviewer_idx = idx.min(self.reviewers.len().saturating_sub(1));
        self.rebind_reviewer();
    }

    /// Persiste o buffer de edição no campo focado.
    pub fn commit_reviewer(&mut self) {
        if let Some(slot) = self.reviewers.get_mut(self.reviewer_idx) {
            slot.clone_from(&self.reviewer_edit);
        }
    }

    /// Carrega o campo focado no buffer de edição (cursor no fim).
    pub fn rebind_reviewer(&mut self) {
        self.reviewer_edit = self
            .reviewers
            .get(self.reviewer_idx)
            .cloned()
            .unwrap_or_default();
        self.reviewer_cursor = self.reviewer_edit.chars().count();
    }

    /// Digitação no campo de reviewer (retorna `true` se consumiu a tecla).
    pub fn reviewer_edit_input(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char(c) => {
                let byte = char_byte_index(&self.reviewer_edit, self.reviewer_cursor);
                self.reviewer_edit.insert(byte, c);
                self.reviewer_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.reviewer_cursor > 0 {
                    let byte = char_byte_index(&self.reviewer_edit, self.reviewer_cursor);
                    let prev = char_byte_index(&self.reviewer_edit, self.reviewer_cursor - 1);
                    self.reviewer_edit.drain(prev..byte);
                    self.reviewer_cursor -= 1;
                }
            }
            KeyCode::Delete => {
                let len = self.reviewer_edit.chars().count();
                if self.reviewer_cursor < len {
                    let byte = char_byte_index(&self.reviewer_edit, self.reviewer_cursor);
                    let next = char_byte_index(&self.reviewer_edit, self.reviewer_cursor + 1);
                    self.reviewer_edit.drain(byte..next);
                }
            }
            KeyCode::Left => self.reviewer_cursor = self.reviewer_cursor.saturating_sub(1),
            KeyCode::Right => {
                self.reviewer_cursor =
                    (self.reviewer_cursor + 1).min(self.reviewer_edit.chars().count());
            }
            KeyCode::Home => self.reviewer_cursor = 0,
            KeyCode::End => self.reviewer_cursor = self.reviewer_edit.chars().count(),
            _ => return false,
        }
        true
    }

    /// Marca flash de "copiado".
    pub fn flash_copied(&mut self) {
        self.copied_flash_until_tick = self.tick + 60; // ~2s a 30fps
    }

    /// Está no flash de copiado?
    #[must_use]
    pub fn is_copied_flash(&self) -> bool {
        self.tick < self.copied_flash_until_tick
    }

    fn push_log(&mut self, line: String) {
        if self.logs.len() >= 200 {
            self.logs.pop_front();
        }
        self.logs.push_back(line);
    }

    fn on_publish_failed(&mut self, failure: &PublishFailure) {
        self.phase = Phase::Review;
        "revisão — falha na publicação".clone_into(&mut self.phase_label);
        "publicação falhou — escolha uma ação".clone_into(&mut self.progress_label);
        self.error = Some(failure.message.clone());
        self.publish_failure = Some(failure.clone());
        self.current_publish_target = None;
        self.candidates.clear();
        self.candidate_activity = CandidateActivity::Idle;
        self.candidate_message = None;
        self.publish_dialog = Some(PublishDialog::PublishRecovery(usize::from(matches!(
            failure.kind,
            PublishFailureKind::OutcomeUnknown
        ))));
        self.push_log(format!("falha ao publicar: {}", failure.message));
    }

    /// Targets que ainda não têm PR confirmado/adotado nesta sessão.
    #[must_use]
    pub fn remaining_publish_targets(&self) -> Vec<String> {
        self.targets
            .iter()
            .filter(|target| !self.published.iter().any(|item| &item.target == *target))
            .cloned()
            .collect()
    }

    /// Recebe o resultado da busca de possíveis PRs.
    pub fn on_candidates_loaded(
        &mut self,
        candidates: Vec<PullRequestCandidate>,
        error: Option<String>,
    ) {
        self.candidate_activity = CandidateActivity::Idle;
        self.candidate_message = error;
        if self.candidate_message.is_none() {
            self.candidates = candidates;
            self.candidates.truncate(8);
        }
        if let Some(PublishDialog::CandidateList { selected }) = self.publish_dialog {
            self.publish_dialog = Some(PublishDialog::CandidateList {
                selected: selected.min(self.candidates.len().saturating_sub(1)),
            });
        }
    }

    /// Adota explicitamente um PR encontrado para o target selecionado.
    pub fn adopt_candidate(&mut self, selected: usize) -> bool {
        let Some(candidate) = self.candidates.get(selected).cloned() else {
            return false;
        };
        if !self
            .published
            .iter()
            .any(|item| item.id == candidate.id || item.target == candidate.target)
        {
            self.published.push(PublishedPr {
                target: candidate.target.clone(),
                id: candidate.id,
                url: candidate.url.clone(),
            });
        }
        self.published_urls = self.published.iter().map(|item| item.url.clone()).collect();
        self.candidates.clear();
        self.candidate_message = None;
        self.candidate_activity = CandidateActivity::Idle;
        self.publish_failure = None;
        self.error = None;
        self.current_publish_target = None;
        self.push_log(format!(
            "PR #{} do target {} adotado",
            candidate.id, candidate.target
        ));
        if self.remaining_publish_targets().is_empty() {
            self.phase = Phase::Done;
            "publicado".clone_into(&mut self.phase_label);
            self.progress = 1.0;
            "todos os targets concluídos".clone_into(&mut self.progress_label);
            self.publish_dialog = None;
        } else {
            self.phase = Phase::Review;
            "revisão — PR adotado".clone_into(&mut self.phase_label);
            "selecione o próximo target".clone_into(&mut self.progress_label);
            self.publish_dialog = Some(PublishDialog::PublishRecovery(0));
        }
        true
    }
}

/// Índice de byte do n-ésimo char (saturado no fim).
fn char_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> DescribeApp {
        DescribeApp::new(
            "feature/11763-x",
            &["dev".to_owned(), "sprint/12".to_owned()],
            "11763",
            false,
            Some(PublishSetup {
                reviewer_sprint: "sprint@x.com".to_owned(),
                reviewer_dev: "dev@x.com".to_owned(),
            }),
            None,
        )
    }

    #[test]
    fn token_should_switch_to_generating_and_update_preview() {
        let mut a = app();
        assert_eq!(a.phase, Phase::Boot);
        a.on_backend(BackendEvent::Token("{\"title\":".to_owned()));
        assert_eq!(a.phase, Phase::Generating);
        assert!(a.preview_text().contains("title"));
        assert!(a.streamed_raw.contains("title"));
    }

    #[test]
    fn context_phase_should_keep_boot_until_generation_starts() {
        let mut a = app();
        a.on_backend(BackendEvent::Phase("coletando contexto git…".to_owned()));
        assert_eq!(a.phase, Phase::Boot);
        assert_eq!(a.step_index(), 0);

        a.on_backend(BackendEvent::Phase("streaming gpt…".to_owned()));
        assert_eq!(a.phase, Phase::Generating);
        assert_eq!(a.step_index(), 1);
    }

    #[test]
    fn spinner_should_stop_when_idle() {
        let frames: std::collections::HashSet<&str> =
            crate::tui::spin_frames().iter().copied().collect();
        // Ocioso: símbolo parado por fase, nunca frame animado.
        let mut a = app();
        a.on_backend(BackendEvent::Finished(
            PrDescription {
                title: "T".to_owned(),
                body: "B".to_owned(),
            },
            "raw".to_owned(),
        ));
        assert_eq!(a.spinner(), "●");
        a.on_backend(BackendEvent::Failed("x".to_owned()));
        assert_eq!(a.spinner(), "✘");
        // Ocupado: gira com o tick.
        let mut b = app();
        b.on_backend(BackendEvent::Token("tok".to_owned()));
        let first = b.spinner().to_owned();
        assert!(frames.contains(first.as_str()), "{first}");
        b.on_tick();
        let _ = b.spinner();
    }

    #[test]
    fn finished_should_enter_review() {
        let mut a = app();
        a.on_backend(BackendEvent::Finished(
            PrDescription {
                title: "Atualiza fluxo".to_owned(),
                body: "## Descrição\nX".to_owned(),
            },
            "raw".to_owned(),
        ));
        assert_eq!(a.phase, Phase::Review);
        assert!(a.preview_text().contains("Atualiza fluxo"));
    }

    #[test]
    fn failed_should_enter_error() {
        let mut a = app();
        a.on_backend(BackendEvent::Failed("timeout".to_owned()));
        assert_eq!(a.phase, Phase::Error);
        assert_eq!(a.error.as_deref(), Some("timeout"));
    }

    #[test]
    fn scroll_should_clamp() {
        let mut a = app();
        a.scroll_by(-5);
        assert_eq!(a.scroll, 0);
        a.scroll_by(10);
        assert_eq!(a.scroll, 10);
    }

    #[test]
    fn reviewers_should_default_sprint_and_dev() {
        let mut a = app();
        a.open_reviewers();
        assert_eq!(a.reviewers, vec!["dev@x.com", "sprint@x.com"]);
        assert_eq!(
            a.reviewer_summary(),
            "dev: dev@x.com; sprint/12: sprint@x.com"
        );
    }

    #[test]
    fn reviewer_summary_should_show_nenhum_when_empty() {
        let mut a = app();
        a.open_reviewers();
        a.reviewers = vec![String::new(), "  ".to_owned()];
        assert_eq!(a.reviewer_summary(), "dev: nenhum; sprint/12: nenhum");
    }

    #[test]
    fn reviewer_editor_should_commit_cursor_edits_when_switching_fields() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut a = app();
        a.open_reviewers();
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert!(a.reviewer_edit_input(key(KeyCode::Char('!'))));
        assert!(a.reviewer_edit_input(key(KeyCode::Left)));
        assert!(a.reviewer_edit_input(key(KeyCode::Char('X'))));
        a.focus_reviewer(1);
        assert_eq!(a.reviewers[0], "dev@x.comX!");
        assert_eq!(a.reviewer_edit, "sprint@x.com");

        assert!(a.reviewer_edit_input(key(KeyCode::Home)));
        assert!(a.reviewer_edit_input(key(KeyCode::Char('x'))));
        a.focus_reviewer(0);
        assert_eq!(a.reviewers[1], "xsprint@x.com");
    }

    #[test]
    fn reopening_reviewers_after_failure_should_preserve_current_values() {
        let mut a = app();
        a.open_reviewers();
        a.reviewers[0] = "changed@x.com".to_owned();
        a.open_reviewers();
        assert_eq!(a.reviewers[0], "changed@x.com");
    }

    #[test]
    fn published_should_enter_done_with_urls() {
        use crate::azure::pull_requests::PublishedPr;
        let mut a = app();
        a.on_backend(BackendEvent::Published(vec![PublishedPr {
            target: "dev".to_owned(),
            id: 7,
            url: "https://x/pr/7".to_owned(),
        }]));
        assert_eq!(a.phase, Phase::Done);
        assert_eq!(a.published_urls, vec!["https://x/pr/7"]);
        assert_eq!(a.published.len(), 1);
    }

    #[test]
    fn published_one_should_preserve_partial_success_before_final_event() {
        use crate::azure::pull_requests::PublishedPr;

        let mut a = app();
        a.on_backend(BackendEvent::PublishedOne(PublishedPr {
            target: "dev".to_owned(),
            id: 7,
            url: "https://x/pr/7".to_owned(),
        }));
        a.on_backend(BackendEvent::Failed("sprint indisponível".to_owned()));

        assert_eq!(a.phase, Phase::Error);
        assert_eq!(a.published.len(), 1);
        assert_eq!(a.published_urls, vec!["https://x/pr/7"]);
        assert!(a.logs.iter().any(|line| line.contains("PR dev criado")));
    }

    #[test]
    fn publish_failure_should_preserve_partial_success_and_open_recovery() {
        let mut a = app();
        a.on_backend(BackendEvent::PublishedOne(PublishedPr {
            target: "dev".to_owned(),
            id: 7,
            url: "https://x/pr/7".to_owned(),
        }));
        a.on_backend(BackendEvent::PublishFailed(PublishFailure {
            message: "target sprint/12: resposta incerta".to_owned(),
            kind: PublishFailureKind::OutcomeUnknown,
            target: Some("sprint/12".to_owned()),
        }));

        assert_eq!(a.phase, Phase::Review);
        assert_eq!(a.published.len(), 1);
        assert_eq!(a.remaining_publish_targets(), vec!["sprint/12"]);
        assert_eq!(a.publish_dialog, Some(PublishDialog::PublishRecovery(1)));
    }

    #[test]
    fn adopted_candidate_should_complete_pending_target_without_recreating_it() {
        let mut a = app();
        a.on_backend(BackendEvent::PublishedOne(PublishedPr {
            target: "dev".to_owned(),
            id: 7,
            url: "https://x/pr/7".to_owned(),
        }));
        a.candidates.push(PullRequestCandidate {
            target: "sprint/12".to_owned(),
            id: 8,
            url: "https://x/pr/8".to_owned(),
            title: "T".to_owned(),
            source_ref: "refs/heads/feature/11763-x".to_owned(),
            target_ref: "refs/heads/sprint/12".to_owned(),
            created_at: "2026-09-11T20:00:00Z".to_owned(),
            work_item_matches: true,
        });

        assert!(a.adopt_candidate(0));
        assert_eq!(a.phase, Phase::Done);
        assert_eq!(a.published.len(), 2);
        assert_eq!(a.published[1].id, 8);
        assert!(a.remaining_publish_targets().is_empty());
    }
}
