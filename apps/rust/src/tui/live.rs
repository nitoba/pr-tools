//! Loop vivo da TUI `desc` — cada evento do backend redesenha a tela.
//!
//! Arquitetura: tarefa tokio gera (stream real `aisdk` quando provider é
//! `openai-compatible`, senão fallback com typing simulado) e envia
//! [`BackendEvent`] por `mpsc`; a thread principal consome crossterm +
//! backend a 30fps e desenha [`DescribeApp`] via `Widget for &App`.

use std::io::{IsTerminal, Stdout};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget, Wrap,
    },
};
use tokio::sync::mpsc;

use super::content_editor::{
    ContentEditAction, ContentEditState, ContentField, render_content_editor,
};
use super::describe_app::{CandidateActivity, DescribeApp, Phase, PublishDialog, PublishSetup};
use super::events::{BackendEvent, LiveOutcome};
use super::shimmer::f64_from_usize;
use super::{
    StatusHeader, ascii_only, border_type, centered_buttons, modal_frame, status_header,
    status_layout, theme,
};
use crate::ai;
use crate::azure::AzureClient;
use crate::azure::pull_requests::{PublishedPr, publish_pull_requests};
use crate::config::{self, Config};
use crate::features::describe::{self, DescribePrep};
use crate::git::RepositoryRemote;

/// Resultado do backend antes de normalizar (raw acumulado).
async fn stream_compatible_live(
    config: &Config,
    system: &str,
    prompt: &str,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) -> Result<String, String> {
    use aisdk::core::{DynamicModel, LanguageModelRequest, LanguageModelStreamChunkType};
    use aisdk::providers::OpenAICompatible;

    let base = config.base_url.trim_end_matches('/').to_owned();
    let api_key = if config.api_key.is_empty() {
        "unused"
    } else {
        config.api_key.as_str()
    };
    let provider = OpenAICompatible::<DynamicModel>::builder()
        .base_url(base)
        .api_key(api_key)
        .model_name(config.compatible_model.clone())
        .build()
        .map_err(|e| e.to_string())?;

    let system_owned = if config.compatible_reasoning == "provider-default" {
        system.to_owned()
    } else {
        format!(
            "{system}\nUse nível de reasoning {}.",
            config.compatible_reasoning
        )
    };
    let mut req = LanguageModelRequest::builder()
        .model(provider)
        .system(system_owned.as_str())
        .prompt(prompt)
        .build();
    let mut resp = req.stream_text().await.map_err(|e| e.to_string())?;
    let mut acc = String::new();
    while let Some(chunk) = resp.stream.next().await {
        match chunk {
            LanguageModelStreamChunkType::Text(t) => {
                acc.push_str(&t);
                let _ = tx.send(BackendEvent::Token(t));
            }
            LanguageModelStreamChunkType::Failed(msg)
            | LanguageModelStreamChunkType::Incomplete(msg)
            | LanguageModelStreamChunkType::NotSupported(msg) => return Err(msg),
            LanguageModelStreamChunkType::End(_) => break,
            _ => {}
        }
    }
    if acc.trim().is_empty() {
        Err("openai-compatible: saída vazia".to_owned())
    } else {
        Ok(acc)
    }
}

/// Typing simulado: quebra o texto em pedaços para animar a tela mesmo
/// quando o provider não faz stream (codex/opencode via subprocesso).
async fn emit_typing(text: &str, tx: &mpsc::UnboundedSender<BackendEvent>) {
    let mut buf = String::with_capacity(8);
    for ch in text.chars() {
        buf.push(ch);
        if buf.len() >= 6 || ch == '\n' {
            let _ = tx.send(BackendEvent::Token(std::mem::take(&mut buf)));
            tokio::time::sleep(Duration::from_millis(8)).await;
        }
    }
    if !buf.is_empty() {
        let _ = tx.send(BackendEvent::Token(buf));
    }
}

/// Tarefa de geração: tenta stream real no compatible, senão fallback.
async fn backend_task(prep: DescribePrep, tx: mpsc::UnboundedSender<BackendEvent>) {
    let _ = tx.send(BackendEvent::Phase("coletando contexto git…".to_owned()));
    let _ = tx.send(BackendEvent::Progress(0.05, "contexto pronto".to_owned()));
    let _ = tx.send(BackendEvent::Log(format!(
        "branch {} → {}",
        prep.context.branch,
        prep.targets.join(", ")
    )));
    let _ = tx.send(BackendEvent::Log(format!(
        "contexto funcional: {}",
        prep.functional_context.display_label()
    )));
    let _ = tx.send(BackendEvent::Log(format!(
        "diff: {} linhas · log com {}",
        prep.context.diff_original_lines,
        if prep.context.log.is_empty() {
            "0 commits"
        } else {
            "commits"
        }
    )));

    // Caminho 1: provider único compatible → stream real token a token.
    let single_compatible = prep.config.providers.len() == 1
        && prep
            .config
            .providers
            .first()
            .is_some_and(|p| p == "openai-compatible");
    if single_compatible {
        let _ = tx.send(BackendEvent::Phase(format!(
            "streaming {}…",
            prep.config.compatible_model
        )));
        match stream_compatible_live(&prep.config, &prep.config.template, &prep.prompt, &tx).await {
            Ok(raw) => {
                finish_raw(&prep, raw, &tx).await;
                return;
            }
            Err(e) => {
                let _ = tx.send(BackendEvent::Log(format!(
                    "stream falhou ({e}), caindo p/ fallback"
                )));
            }
        }
    }

    // Caminho 2: fallback clássico (codex → opencode → compatible), com typing.
    for provider in &prep.config.providers {
        let model = match provider.as_str() {
            "codex" => prep.config.codex_model.as_str(),
            "opencode" => prep.config.opencode_model.as_str(),
            _ => prep.config.compatible_model.as_str(),
        };
        let _ = tx.send(BackendEvent::Phase(format!(
            "tentando {provider} ({model})…"
        )));
        let _ = tx.send(BackendEvent::Log(format!("tentando {provider} ({model})")));
        let out = match provider.as_str() {
            "codex" => {
                ai::generate_via_codex(&prep.config, &prep.config.template, &prep.prompt).await
            }
            "opencode" => {
                ai::generate_via_opencode(&prep.config, &prep.config.template, &prep.prompt).await
            }
            _ => {
                ai::generate_via_compatible(&prep.config, &prep.config.template, &prep.prompt).await
            }
        };
        match out {
            Ok(raw) => {
                let _ = tx.send(BackendEvent::Log(format!(
                    "{provider} respondeu, renderizando…"
                )));
                emit_typing(&raw, &tx).await;
                finish_raw(&prep, raw, &tx).await;
                return;
            }
            Err(e) => {
                let _ = tx.send(BackendEvent::Log(format!("{provider} falhou: {e}")));
            }
        }
    }
    let _ = tx.send(BackendEvent::Failed(
        "todos os providers falharam".to_owned(),
    ));
}

/// Normaliza, reescreve se > 3999 e emite `Finished`.
async fn finish_raw(prep: &DescribePrep, raw: String, tx: &mpsc::UnboundedSender<BackendEvent>) {
    let mut desc = ai::normalize_description(&raw, &prep.context.branch);
    if !ai::is_within_limit(&desc.body) {
        let _ = tx.send(BackendEvent::Phase("reescrevendo p/ < 4000…".to_owned()));
        let rewrite_system = format!("{}\n\n{}", prep.config.template, ai::REWRITE_INSTRUCTIONS);
        let rewrite_prompt = format!("## Descrição original\n\n# {}\n\n{}", desc.title, desc.body);
        // Rewrite sem stream (rápido); anima via typing também.
        let report = |_p: &str, _m: &str| {};
        if let Ok(raw2) =
            ai::generate_with_fallback(&prep.config, &rewrite_system, &rewrite_prompt, report).await
        {
            emit_typing(&raw2, tx).await;
            desc = ai::normalize_description(&raw2, &prep.context.branch);
        }
    }
    if let Err(e) = ai::validate_description(&desc) {
        let _ = tx.send(BackendEvent::Failed(e.to_string()));
    } else {
        let _ = tx.send(BackendEvent::Progress(1.0, "pronto".to_owned()));
        let _ = tx.send(BackendEvent::Finished(desc, raw));
    }
}

/// Dados próprios para publicar (extraídos do `prep` antes do backend).
#[derive(Debug, Clone)]
struct PublishBase {
    /// PAT do Azure.
    pat: String,
    /// Remote Azure (sempre `Some` quando publicável).
    remote: Option<RepositoryRemote>,
    /// Branch de origem.
    branch: String,
    /// Work Item (vazio = sem vínculo).
    work_item_id: String,
}

/// Publica a descrição em todos os targets, com logs e progresso por etapa.
///
/// Espelha o fluxo do comando Dart: valida, resolve o repo, cria um PR por
/// target e devolve a lista para o evento `Published`.
async fn publish_task(
    base: PublishBase,
    title: String,
    body: String,
    reviewers: Vec<String>,
    targets: Vec<String>,
    recovery: bool,
    tx: mpsc::UnboundedSender<BackendEvent>,
) {
    let _ = tx.send(BackendEvent::Phase("publicando…".to_owned()));
    let Some(remote) = &base.remote else {
        let _ = tx.send(BackendEvent::Failed(
            "remote Azure DevOps não encontrado.".to_owned(),
        ));
        return;
    };
    let (pat, timeout) = if recovery {
        match config::load_config() {
            Ok(config) => (config.azure_pat, Some(describe::TUI_AZURE_TIMEOUT)),
            Err(error) => {
                let failure = describe::classify_publish_error(&error, None);
                let _ = tx.send(BackendEvent::PublishFailed(failure));
                return;
            }
        }
    } else {
        (base.pat.clone(), None)
    };
    let client = match timeout {
        Some(timeout) => AzureClient::new_with_timeout(&remote.organization, &pat, timeout),
        None => AzureClient::new(&remote.organization, &pat),
    };
    let _ = tx.send(BackendEvent::Progress(
        0.1,
        "resolvendo repositório…".to_owned(),
    ));
    let work_items: Vec<String> = if base.work_item_id.trim().is_empty() {
        Vec::new()
    } else {
        vec![base.work_item_id.trim().to_owned()]
    };
    let total = f64_from_usize(targets.len().max(1));
    let tx_target = tx.clone();
    let tx_started = tx.clone();
    let active_target = Arc::new(Mutex::new(None::<String>));
    let active_target_callback = Arc::clone(&active_target);
    let on_published = |item: &PublishedPr| {
        let completed = targets
            .iter()
            .position(|target| target == &item.target)
            .map_or(1, |index| index + 1);
        let _ = tx_target.send(BackendEvent::Progress(
            0.1 + 0.9 * (f64_from_usize(completed) / total),
            format!("PR {} criado", item.target),
        ));
        let _ = tx_target.send(BackendEvent::PublishedOne(item.clone()));
    };
    let on_target_started = |target: &str| {
        if let Ok(mut active) = active_target_callback.lock() {
            *active = Some(target.to_owned());
        }
        let _ = tx_started.send(BackendEvent::PublishingTarget(target.to_owned()));
    };
    // Resolve reviewers uma vez aqui (o publisher também cacheia; o log
    // mostra o que está acontecendo por target).
    let input = crate::azure::pull_requests::PublishInput {
        remote,
        branch: &base.branch,
        targets: &targets,
        title: &title,
        body: &body,
        work_item_ids: &work_items,
        reviewer_for: &|target| {
            reviewers
                .get(
                    targets
                        .iter()
                        .position(|t| t == target)
                        .unwrap_or(usize::MAX),
                )
                .cloned()
                .unwrap_or_default()
        },
        on_published: Some(&on_published),
        on_target_started: Some(&on_target_started),
    };
    let result = publish_pull_requests(&client, &input).await;
    match result {
        Ok(published) => {
            let _ = tx.send(BackendEvent::Published(published));
        }
        Err(e) => {
            let target = active_target.lock().ok().and_then(|active| active.clone());
            let failure = describe::classify_publish_error(&e, target.as_deref());
            let _ = tx.send(BackendEvent::PublishFailed(failure));
        }
    }
}

/// Executa a consulta de possíveis PRs fora do loop de renderização.
async fn backend_find_candidates(
    base: PublishBase,
    title: String,
    target: String,
    tx: mpsc::UnboundedSender<BackendEvent>,
) {
    let Some(remote) = base.remote.as_ref() else {
        let _ = tx.send(BackendEvent::CandidatesLoaded {
            candidates: Vec::new(),
            error: Some("remote Azure DevOps não encontrado".to_owned()),
        });
        return;
    };
    match describe::find_publish_candidates(
        remote,
        &base.branch,
        &base.work_item_id,
        &title,
        &target,
    )
    .await
    {
        Ok(candidates) => {
            let _ = tx.send(BackendEvent::CandidatesLoaded {
                candidates,
                error: None,
            });
        }
        Err(error) => {
            let _ = tx.send(BackendEvent::CandidatesLoaded {
                candidates: Vec::new(),
                error: Some(format!("não foi possível consultar PRs recentes: {error}")),
            });
        }
    }
}

/// Inicia a publicação: fecha o diálogo, marca a fase e dispara a task.
fn start_publish(
    app: &mut DescribeApp,
    base: &PublishBase,
    tx: &mpsc::UnboundedSender<BackendEvent>,
    desc: &crate::ai::PrDescription,
    recovery: bool,
) {
    let approved = publish_content_for_attempt(app, desc);
    let mut validation = ContentEditState::for_pr(&approved);
    if let Err(error) = validation.validate() {
        validation.set_error(error.clone());
        app.content_edit = Some(validation);
        app.publish_dialog = None;
        app.error = Some(error.to_string());
        app.phase = Phase::Review;
        return;
    }
    app.commit_reviewer();
    if recovery {
        // A recuperação deve refletir alterações feitas no `prt init` desde a
        // tentativa anterior, sem alterar o snapshot usado no primeiro envio.
        if let Ok(config) = config::load_config() {
            app.publish_setup = Some(PublishSetup {
                reviewer_sprint: config.reviewer_sprint,
                reviewer_dev: config.reviewer_dev,
            });
        }
    }
    // Vazio volta ao default (como no Dart: vazio = padrão).
    if let Some(setup) = app.publish_setup.clone() {
        for (i, target) in app.targets.iter().enumerate() {
            if app.reviewers.get(i).is_some_and(|v| v.trim().is_empty()) {
                if let Some(slot) = app.reviewers.get_mut(i) {
                    *slot = setup.default_for(target);
                }
            }
        }
    }
    let reviewers = app.reviewers.clone();
    let targets = app.remaining_publish_targets();
    if targets.is_empty() {
        app.phase = Phase::Done;
        "publicado".clone_into(&mut app.phase_label);
        app.progress = 1.0;
        "todos os targets concluídos".clone_into(&mut app.progress_label);
        app.publish_failure = None;
        app.publish_dialog = None;
        return;
    }
    app.publish_dialog = None;
    app.phase = Phase::Publishing;
    "publicando…".clone_into(&mut app.phase_label);
    app.progress = 0.0;
    "criando PRs…".clone_into(&mut app.progress_label);
    // Congela a versão aprovada antes de iniciar a primeira task remota. Em
    // recovery, a mesma versão já existente vence qualquer conteúdo mutável.
    let content = app
        .frozen_publish_content
        .get_or_insert_with(|| approved.clone())
        .clone();
    let base = base.clone();
    let title = content.title;
    let body = content.body;
    let tx = tx.clone();
    tokio::spawn(publish_task(
        base, title, body, reviewers, targets, recovery, tx,
    ));
}

/// Desenha um frame completo a partir do estado — reage a cada token/log.
impl Widget for &DescribeApp {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Piso honesto: terminal miúdo não tenta layout normal.
        if area.width < 60 || area.height < 20 {
            render_too_small(area, buf);
            return;
        }
        // O status global ocupa duas linhas: identidade/fase e progresso.
        let [head, body, foot] = status_layout(area);
        Block::new().style(theme().root).render(area, buf);
        render_header(self, head, buf);
        render_body(self, body, buf);
        render_footer(self, foot, buf);
        if self.show_help {
            render_help(area, buf);
        }
        if let Some(dialog) = &self.publish_dialog {
            render_publish_dialog(self, *dialog, area, buf);
        }
        if let Some(editor) = &self.content_edit {
            render_content_editor(editor, area, buf);
        }
    }
}

fn render_too_small(area: Rect, buf: &mut Buffer) {
    // Estado honesto p/ terminal < 60×20: painel centralizado, sem layout normal.
    if area.width == 0 || area.height == 0 {
        return;
    }
    Block::new().style(theme().root).render(area, buf);
    let msg = if ascii_only() {
        format!(
            "terminal muito pequeno - mínimo 60x20 (atual {}x{})",
            area.width, area.height
        )
    } else {
        format!(
            "terminal muito pequeno — mínimo 60×20 (atual {}×{})",
            area.width, area.height
        )
    };
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
        .title(Span::styled(
            if ascii_only() { " prt " } else { " ◆ prt " },
            theme().warning,
        ))
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

fn render_header(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let kicker = phase_copy(app.phase);
    let candidate_lookup = matches!(app.candidate_activity, CandidateActivity::Loading);
    let active = matches!(
        app.phase,
        Phase::Boot | Phase::Generating | Phase::Publishing
    ) || candidate_lookup;
    let message = if candidate_lookup {
        "consultando PRs recentes…".to_owned()
    } else if app.phase == Phase::Error {
        "consulte os detalhes abaixo".to_owned()
    } else if app.phase == Phase::Review {
        "descrição pronta".to_owned()
    } else if app.phase == Phase::Publishing {
        app.current_publish_target.as_ref().map_or_else(
            || "enviando descrição".to_owned(),
            |target| format!("target {target} em andamento"),
        )
    } else if app.phase == Phase::Done {
        "links disponíveis".to_owned()
    } else {
        terminal_text(&app.phase_label)
    };
    status_header(
        area,
        buf,
        StatusHeader {
            command: "desc",
            phase: kicker,
            message: &message,
            progress: if candidate_lookup {
                None
            } else {
                Some(app.progress)
            },
            tick: app.tick,
            active,
            style: phase_style(app.phase),
        },
    );
}

fn render_body(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let show_context = app.phase == Phase::Review;
    let context_height = if show_context { 4 } else { 0 };
    let [primary, context] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(context_height)]).areas(area);
    render_primary(app, primary, buf);
    if show_context {
        render_context(app, context, buf);
    }
}

fn render_primary(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    match app.phase {
        Phase::Boot => {}
        Phase::Generating => render_stream(app, area, buf),
        Phase::Review => render_description(app, area, buf),
        Phase::Publishing => render_publishing(app, area, buf),
        Phase::Done => render_done(app, area, buf),
        Phase::Error => render_error(app, area, buf),
    }
}

fn render_stream(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    if app.streamed_raw.is_empty() {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme().warning)
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let cursor = if ascii_only() { "_" } else { "▊" };
    let text = format!("{}{cursor}", app.streamed_raw);
    let lines = text.lines().count();
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0))
        .render(inner, buf);
    let mut state = ScrollbarState::new(lines.max(1)).position(app.scroll as usize);
    <Scrollbar as ratatui::widgets::StatefulWidget>::render(
        Scrollbar::new(ScrollbarOrientation::VerticalRight),
        inner,
        buf,
        &mut state,
    );
}

fn render_description(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let chars = app.content_chars();
    let limit_mark = if app
        .desc
        .as_ref()
        .is_some_and(|description| ai::is_within_limit(&description.body))
    {
        if ascii_only() { "ok" } else { "✓" }
    } else if ascii_only() {
        "over"
    } else {
        "✘"
    };
    let title_separator = if ascii_only() { " - " } else { " · " };
    let title = terminal_text(&format!(
        " Body do PR{title_separator}{chars}/4000 {limit_mark} "
    ));
    let block = primary_block(&title, theme().success);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if let Some(description) = &app.desc {
        use super::markdown::{markdown_text, title_line};
        use ratatui::text::Text;
        let mut lines: Vec<Line> = Vec::new();
        if let Some(error) = &app.error {
            lines.push(Line::from(Span::styled(
                format!("{} {error}", if ascii_only() { "x" } else { "✘" }),
                theme().error,
            )));
            lines.push(Line::from(Span::styled(
                if app.publish_failure.is_some() {
                    "A descrição foi preservada; ajuste os reviewers ou recupere o PR abaixo."
                } else {
                    ""
                },
                theme().muted,
            )));
            lines.push(Line::from(""));
        }
        lines.extend([title_line(&description.title), Line::from("")]);
        if ascii_only() {
            // O renderer Markdown usa molduras/checkboxes Unicode; em
            // TERM=dumb mantemos o texto literal e a navegação intacta.
            lines.extend(description.body.lines().map(Line::from));
        } else {
            lines.extend(markdown_text(&description.body).lines);
        }
        let total = lines.len();
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0))
            .render(inner, buf);
        let mut state = ScrollbarState::new(total.max(1)).position(app.scroll as usize);
        <Scrollbar as ratatui::widgets::StatefulWidget>::render(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            inner,
            buf,
            &mut state,
        );
    } else {
        render_stream(app, area, buf);
    }
}

fn render_publishing(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    if app.published.is_empty() {
        return;
    }
    let block = primary_block(" PRs criados ", theme().accent);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let lines = app.published.iter().map(|item| {
        Line::from(vec![
            Span::styled(format!("  {}  ", item.target), branch_style()),
            Span::styled(item.url.clone(), link_style()),
        ])
    });
    Paragraph::new(lines.collect::<Vec<_>>())
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_done(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    if app.published.is_empty() {
        return;
    }
    let block = primary_block(" PRs publicados ", theme().success);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let lines: Vec<Line> = app
        .published
        .iter()
        .map(|item| {
            Line::from(vec![
                Span::styled(format!("  PR #{}  ", item.id), branch_style()),
                Span::styled(format!("{}  ", item.target), branch_style()),
                Span::styled(item.url.clone(), link_style()),
            ])
        })
        .collect();
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_error(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let title = terminal_text(" Detalhes da operação ");
    let block = primary_block(&title, theme().error);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let error = app.error.as_deref().unwrap_or("falha sem mensagem");
    let partial = if app.published.is_empty() {
        "Nenhum PR foi criado antes da falha."
    } else {
        "Alguns PRs foram criados antes da falha; confira os links abaixo."
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(if ascii_only() { "x " } else { "✘ " }, theme().error),
            Span::styled(error, theme().error),
        ]),
        Line::from(""),
        Line::from(Span::styled(partial, theme().muted)),
        Line::from(Span::styled(
            if ascii_only() {
                "r retorna o erro ao comando - q sai - ? ajuda"
            } else {
                "r retorna o erro ao comando · q sai · ? ajuda"
            },
            theme().muted,
        )),
    ];
    if !app.published.is_empty() {
        lines.push(Line::from(Span::styled(
            "PRs criados:",
            theme().accent.add_modifier(Modifier::BOLD),
        )));
        lines.extend(app.published.iter().map(|item| {
            Line::from(vec![
                Span::styled(format!("  PR #{}  ", item.id), branch_style()),
                Span::styled(format!("{}  ", item.target), branch_style()),
                Span::styled(item.url.clone(), link_style()),
            ])
        }));
    }
    lines.truncate(inner.height as usize);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_context(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .title(Span::styled(" Contexto ", theme().muted))
        .borders(Borders::ALL)
        .border_style(theme().muted)
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let work = if app.work_item_id.is_empty() {
        if ascii_only() { "-" } else { "—" }.to_owned()
    } else {
        format!("#{}", app.work_item_id)
    };
    let separator = if ascii_only() { "  -  " } else { "  ·  " };
    let mut context = vec![Span::styled(
        format!(
            "branch {}{separator}work {}{separator}targets ",
            app.branch, work
        ),
        theme().muted,
    )];
    if app.targets.is_empty() {
        context.push(Span::styled(
            if ascii_only() { "-" } else { "—" },
            theme().muted,
        ));
    } else {
        for (idx, target) in app.targets.iter().enumerate() {
            if idx > 0 {
                context.push(Span::styled(", ", theme().muted));
            }
            let style = if idx == app.selected_target {
                theme().accent.add_modifier(Modifier::BOLD)
            } else {
                theme().muted
            };
            context.push(Span::styled(target.clone(), style));
        }
    }
    let functional = format!(
        "Contexto funcional: {}",
        app.functional_context_status.display_label()
    );
    Paragraph::new(vec![
        Line::from(context),
        Line::from(Span::styled(functional, theme().muted)),
    ])
    .wrap(Wrap { trim: false })
    .render(inner, buf);
}

fn primary_block(title: &str, style: Style) -> Block<'static> {
    Block::default()
        .title(Span::styled(title.to_owned(), style))
        .borders(Borders::ALL)
        .border_style(style)
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1))
}

fn phase_copy(phase: Phase) -> &'static str {
    match phase {
        Phase::Boot => "Preparação",
        Phase::Generating => "Geração via IA",
        Phase::Review => "Revisão",
        Phase::Publishing => "Publicação",
        Phase::Done => "Concluído",
        Phase::Error => "Erro",
    }
}

fn phase_style(phase: Phase) -> Style {
    match phase {
        Phase::Done | Phase::Review => theme().success,
        Phase::Error => theme().error,
        Phase::Boot | Phase::Generating | Phase::Publishing => theme().accent,
    }
}

/// Estilo do target/branch em resultados de publicação.
fn branch_style() -> Style {
    theme().accent.add_modifier(Modifier::BOLD)
}

/// Estilo separado para links publicados.
fn link_style() -> Style {
    if super::colors_enabled() {
        Style::new().fg(Color::Rgb(96, 165, 250))
    } else {
        Style::new()
    }
}

/// Normaliza glifos decorativos de status quando o terminal exige ASCII.
///
/// Conteúdo gerado pelo usuário permanece intacto; esta função é usada apenas
/// para rótulos e mensagens controlados pela própria TUI.
fn terminal_text(value: &str) -> String {
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

fn render_footer(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let success_mark = if ascii_only() { "+" } else { "✓" };
    let hints = if app.content_edit.is_some() {
        if ascii_only() {
            "tab alternar titulo/corpo - ctrl+s salvar - esc cancelar"
        } else {
            "Tab alterna título/corpo · Ctrl+S salvar · Esc cancelar"
        }
    } else {
        match &app.publish_dialog {
            Some(PublishDialog::FunctionalContextFallback(_)) => {
                if ascii_only() {
                    "left/right alternar - y sim - n nao - enter confirmar - esc sair"
                } else {
                    "←/→ alternar · y sim · n não · enter confirmar · esc sair"
                }
            }
            Some(PublishDialog::ConfirmCreate(_) | PublishDialog::ConfirmPublish(_)) => {
                if ascii_only() {
                    "<-/-> alternar - y sim - n nao - enter confirmar - esc voltar"
                } else {
                    "←/→ alternar · y sim · n não · enter confirmar · esc voltar"
                }
            }
            Some(PublishDialog::Reviewers) => {
                if ascii_only() {
                    "digite o reviewer - tab/up/down trocar campo - enter avancar - esc voltar"
                } else {
                    "digite o reviewer · tab/↓↑ trocar campo · enter avançar · esc voltar"
                }
            }
            Some(PublishDialog::PublishRecovery(_)) => {
                if ascii_only() {
                    "up/down escolher - enter confirmar - esc voltar"
                } else {
                    "↑/↓ escolher · enter confirmar · esc voltar"
                }
            }
            Some(PublishDialog::CandidateList { .. }) => {
                if ascii_only() {
                    "up/down escolher - enter adotar - esc voltar"
                } else {
                    "↑/↓ escolher · enter adotar · esc voltar"
                }
            }
            Some(PublishDialog::PublishedPrPicker { .. }) => {
                if ascii_only() {
                    "up/down escolher - enter preparar - esc voltar"
                } else {
                    "↑/↓ escolher · enter preparar · esc voltar"
                }
            }
            None => match app.phase {
                Phase::Review => {
                    if app.frozen_publish_content.is_none() {
                        if ascii_only() {
                            "e editar - enter publicar - c copiar - tab - j/k - ? ajuda - q sair"
                        } else {
                            "e editar · enter publicar · c copiar · tab · j/k · ? ajuda · q sair"
                        }
                    } else if ascii_only() {
                        "enter publicar - c copiar - tab target - j/k scroll - ? ajuda - q sair"
                    } else {
                        "enter publicar · c copiar · tab target · j/k scroll · ? ajuda · q sair"
                    }
                }
                Phase::Done => {
                    if ascii_only() {
                        "t preparar Test Case - q sair"
                    } else {
                        "t preparar Test Case · q sair"
                    }
                }
                Phase::Error => {
                    if ascii_only() {
                        "r retornar erro ao comando - q sair - ? ajuda"
                    } else {
                        "r retornar erro ao comando · q sair · ? ajuda"
                    }
                }
                _ => {
                    if ascii_only() {
                        "j/k scroll - tab target - ? ajuda - q sair"
                    } else {
                        "j/k scroll · tab target · ? ajuda · q sair"
                    }
                }
            },
        }
    };
    let mut spans = vec![Span::styled(hints, theme().muted)];
    if app.is_copied_flash() {
        spans.push(Span::styled(
            format!("   {success_mark} copiado!"),
            theme().success,
        ));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_help(area: Rect, buf: &mut Buffer) {
    // 15 linhas de conteúdo, altura exata.
    let inner = modal_frame(area, buf, " Ajuda ", theme().accent, 62, 15);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let navigation = if ascii_only() {
        vec![
            Line::from(Span::styled(
                "navegacao",
                theme().accent.add_modifier(Modifier::BOLD),
            )),
            Line::from("j/k ou up/down - rolar preview - tab - trocar target"),
            Line::from("e - editar conteúdo - c - copiar body - ? - ajuda - q/esc - sair"),
            Line::from(""),
            Line::from(Span::styled(
                "publicacao",
                theme().accent.add_modifier(Modifier::BOLD),
            )),
            Line::from("enter - publicar - esc - voltar"),
            Line::from("nos reviewers: digite o email - tab/up/down - trocar campo"),
            Line::from("<- / -> ou y/n - alternar Sim/Nao"),
            Line::from(""),
            Line::from(Span::styled(
                "erros",
                theme().accent.add_modifier(Modifier::BOLD),
            )),
            Line::from("r - retry na falha de publicacao ou retornar erro terminal"),
            Line::from(""),
            Line::from(Span::styled(
                "enter confirma - esc sempre volta um nivel",
                theme().muted,
            )),
            Line::from(""),
            Line::from(Span::styled("? fecha esta ajuda", theme().muted)),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                "navegação",
                theme().accent.add_modifier(Modifier::BOLD),
            )),
            Line::from("j/k ou ↑/↓ — rolar preview · tab — trocar target"),
            Line::from("e — Editar conteúdo · c — copiar body · ? — ajuda · q/esc — sair"),
            Line::from(""),
            Line::from(Span::styled(
                "publicação",
                theme().accent.add_modifier(Modifier::BOLD),
            )),
            Line::from("enter — publicar · esc — voltar"),
            Line::from("nos reviewers: digite o email · tab/↓↑ — trocar campo"),
            Line::from("←/→ ou y/n — alternar Sim/Não"),
            Line::from(""),
            Line::from(Span::styled(
                "erros",
                theme().accent.add_modifier(Modifier::BOLD),
            )),
            Line::from("r — retry na falha de publicação ou retornar erro terminal"),
            Line::from(""),
            Line::from(Span::styled(
                "enter confirma · esc sempre volta um nível",
                theme().muted,
            )),
            Line::from(""),
            Line::from(Span::styled("? fecha esta ajuda", theme().muted)),
        ]
    };
    Paragraph::new(navigation)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// Diálogos modais do fluxo de publicação.
fn render_publish_dialog(app: &DescribeApp, dialog: PublishDialog, area: Rect, buf: &mut Buffer) {
    match dialog {
        PublishDialog::FunctionalContextFallback(yes) => {
            render_functional_context_fallback(app, yes, area, buf);
        }
        PublishDialog::ConfirmCreate(yes) => render_confirm_create(app, yes, area, buf),
        PublishDialog::Reviewers => render_reviewers_dialog(app, area, buf),
        PublishDialog::ConfirmPublish(yes) => render_confirm_publish(app, yes, area, buf),
        PublishDialog::PublishRecovery(selected) => {
            render_publish_recovery(app, selected, area, buf);
        }
        PublishDialog::CandidateList { selected } => {
            render_candidate_list(app, selected, area, buf);
        }
        PublishDialog::PublishedPrPicker { selected } => {
            render_published_pr_picker(app, selected, area, buf);
        }
    }
}

fn render_published_pr_picker(app: &DescribeApp, selected: usize, area: Rect, buf: &mut Buffer) {
    let height = 7 + app.published.len();
    let inner = modal_frame(
        area,
        buf,
        " Preparar Test Case ",
        theme().accent,
        96,
        height,
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = vec![Line::from(Span::styled(
        "Escolha o PR que representa esta mudança funcional:",
        theme().muted,
    ))];
    for (index, item) in app.published.iter().enumerate() {
        let marker = if index == selected { ">" } else { " " };
        let style = if index == selected {
            theme().accent.add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} PR #{}  {}  {}", item.id, item.target, item.url),
            style,
        )));
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            if ascii_only() {
                "enter preparar um PR - up/down escolher - esc/q voltar"
            } else {
                "Enter preparar um PR · ↑/↓ escolher · Esc/q voltar"
            },
            theme().muted,
        )),
    ]);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_functional_context_fallback(app: &DescribeApp, yes: bool, area: Rect, buf: &mut Buffer) {
    let inner = modal_frame(
        area,
        buf,
        " Contexto funcional indisponível ",
        theme().warning,
        82,
        10,
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let detail = match &app.functional_context_status {
        crate::features::describe::FunctionalContextStatus::Unavailable(message) => message,
        _ => "não foi possível carregar o Work Item",
    };
    let choose = if yes { "Sim" } else { "Não" };
    Paragraph::new(vec![
        Line::from(Span::styled(
            "O Work Item não pôde ser carregado antes da geração.",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(detail, theme().error)),
        Line::from(""),
        Line::from("Continuar somente com o contexto Git?"),
        Line::from("A geração ainda não começou."),
        Line::from(""),
        centered_buttons(yes, inner.width),
        Line::from(""),
        Line::from(Span::styled(
            format!("selecionado: {choose} · y confirma Git-only · n/Esc sai"),
            theme().muted,
        )),
    ])
    .wrap(Wrap { trim: false })
    .render(inner, buf);
}

fn render_publish_recovery(app: &DescribeApp, selected: usize, area: Rect, buf: &mut Buffer) {
    let inner = modal_frame(area, buf, " Recuperar publicação ", theme().warning, 76, 11);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let unknown = app.publish_failure.as_ref().is_some_and(|failure| {
        matches!(
            failure.kind,
            crate::features::describe::PublishFailureKind::OutcomeUnknown
        )
    });
    let failed_target = app
        .publish_failure
        .as_ref()
        .and_then(|failure| failure.target.as_deref())
        .unwrap_or("target não identificado");
    let options = [
        "reenviar targets pendentes",
        "buscar PR possivelmente criado",
        "editar reviewers",
        "voltar à revisão",
    ];
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Falha durante a publicação ({failed_target})"),
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            if unknown {
                "O resultado é incerto; confirme se já existe um PR antes de reenviar."
            } else {
                "O Azure recusou a operação; revise os reviewers ou tente novamente."
            },
            theme().muted,
        )),
        Line::from(""),
    ];
    for (index, option) in options.iter().enumerate() {
        let marker = if index == selected { ">" } else { " " };
        let style = if index == selected {
            theme().accent.add_modifier(Modifier::BOLD)
        } else {
            theme().muted
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {option}"),
            style,
        )));
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "PRs preservados nesta sessão: {} · targets pendentes: {}",
                app.published.len(),
                app.remaining_publish_targets().len()
            ),
            theme().muted,
        )),
        Line::from(Span::styled(
            if ascii_only() {
                "up/down escolher - enter confirmar - esc voltar"
            } else {
                "↑/↓ escolher · enter confirmar · esc voltar"
            },
            theme().muted,
        )),
    ]);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_candidate_list(app: &DescribeApp, selected: usize, area: Rect, buf: &mut Buffer) {
    let height = 7 + app.candidates.len().min(8);
    let inner = modal_frame(
        area,
        buf,
        " PRs possivelmente criados ",
        theme().warning,
        90,
        height,
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = Vec::new();
    if app.candidate_activity != CandidateActivity::Loading {
        lines.push(Line::from(Span::styled(
            "Adote um PR somente se confirmar que ele corresponde a esta publicação.",
            theme().muted,
        )));
    }
    if let Some(message) = &app.candidate_message {
        lines.push(Line::from(Span::styled(message.clone(), theme().error)));
    }
    if app.candidates.is_empty() && app.candidate_activity != CandidateActivity::Loading {
        lines.push(Line::from(Span::styled(
            "nenhum candidato compatível foi encontrado",
            theme().muted,
        )));
    }
    for (index, candidate) in app.candidates.iter().take(8).enumerate() {
        let marker = if index == selected { ">" } else { " " };
        let match_mark = if candidate.work_item_matches {
            "✓"
        } else {
            "?"
        };
        let style = if index == selected {
            theme().accent.add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{marker} {}  #{}  [{}] {}",
                candidate.target, candidate.id, match_mark, candidate.url
            ),
            style,
        )));
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            if ascii_only() {
                "enter adotar - up/down escolher - esc voltar"
            } else {
                "enter adotar · ↑/↓ escolher · esc voltar"
            },
            theme().muted,
        )),
    ]);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_confirm_create(app: &DescribeApp, yes: bool, area: Rect, buf: &mut Buffer) {
    // 7 linhas de conteúdo: pergunta, respiro, targets, respiro, botões,
    // respiro, dicas.
    let inner = modal_frame(area, buf, " Publicar ", theme().accent, 62, 7);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let targets = if app.targets.is_empty() {
        if ascii_only() {
            "-".to_owned()
        } else {
            "—".to_owned()
        }
    } else {
        app.targets.join(", ")
    };
    Paragraph::new(vec![
        Line::from(Span::styled(
            "Criar PR(s) no Azure DevOps?",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(format!("targets: {targets}"), theme().muted)),
        Line::from(""),
        centered_buttons(yes, inner.width),
        Line::from(""),
        Line::from(Span::styled(
            if ascii_only() {
                "<-/-> alternar - y sim - n nao - enter confirmar - esc voltar"
            } else {
                "←/→ alternar · y sim · n não · enter confirmar · esc voltar"
            },
            theme().muted,
        )),
    ])
    .wrap(Wrap { trim: false })
    .render(inner, buf);
}

fn render_reviewers_dialog(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    // 1 dica + 1 respiro + 2 linhas por target + 1 respiro + 1 dicas.
    let height = 4 + app.targets.len() * 2;
    let inner = modal_frame(area, buf, " Reviewers ", theme().accent, 70, height);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = vec![
        Line::from(Span::styled(
            if ascii_only() {
                "opcional; vazio mantem o padrao - tab/up/down troca de campo"
            } else {
                "opcional; vazio mantém o padrão · tab/↓↑ troca de campo"
            },
            theme().muted,
        )),
        Line::from(""),
    ];
    for (i, target) in app.targets.iter().enumerate() {
        let focused = i == app.reviewer_idx;
        lines.push(Line::from(vec![
            Span::styled(
                if focused {
                    if ascii_only() { "> " } else { "▸ " }
                } else {
                    "  "
                },
                if focused {
                    theme().accent
                } else {
                    theme().muted
                },
            ),
            Span::styled(
                target.clone(),
                if focused {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    theme().muted
                },
            ),
        ]));
        // Valor com barra lateral (sem caixa aninhada: caixa dentro de
        // caixa é o maior causador de poluição visual).
        let value = app.reviewers.get(i).cloned().unwrap_or_default();
        let mut row = vec![Span::styled(
            if ascii_only() { "| " } else { "│ " },
            theme().muted,
        )];
        if focused {
            row.extend(editor_spans(&app.reviewer_edit, app.reviewer_cursor));
        } else if value.trim().is_empty() {
            row.push(Span::styled(
                if ascii_only() {
                    "(padrao)"
                } else {
                    "(padrão)"
                },
                theme().muted,
            ));
        } else {
            row.push(Span::styled(value, Style::new()));
        }
        lines.push(Line::from(row));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if ascii_only() {
            "enter avancar - esc voltar"
        } else {
            "enter avançar · esc voltar"
        },
        theme().muted,
    )));
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

fn render_confirm_publish(app: &DescribeApp, yes: bool, area: Rect, buf: &mut Buffer) {
    // pergunta + respiro + N resumos + respiro + botões + respiro + dicas.
    let inner = modal_frame(
        area,
        buf,
        " Confirmar ",
        theme().accent,
        66,
        6 + app.targets.len(),
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = vec![
        Line::from(Span::styled(
            "Criar PR(s) com estes reviewers?",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for (index, target) in app.targets.iter().enumerate() {
        let reviewer = app.reviewers.get(index).map_or("", String::as_str);
        let shown = if reviewer.trim().is_empty() {
            "nenhum"
        } else {
            reviewer.trim()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{target}: "), branch_style()),
            Span::styled(shown.to_owned(), Style::new()),
        ]));
    }
    lines.extend([
        Line::from(""),
        centered_buttons(yes, inner.width),
        Line::from(""),
        Line::from(Span::styled(
            if ascii_only() {
                "<-/-> alternar - y sim - n nao - enter confirmar - esc voltar"
            } else {
                "←/→ alternar · y sim · n não · enter confirmar · esc voltar"
            },
            theme().muted,
        )),
    ]);
    lines.truncate(inner.height as usize);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// Linha do editor com cursor reverso (espelha o editor do wizard).
fn editor_spans(value: &str, cursor: usize) -> Vec<Span<'static>> {
    let chars: Vec<char> = value.chars().collect();
    let cursor = cursor.min(chars.len());
    let mut spans = Vec::with_capacity(chars.len() + 1);
    for (i, ch) in chars.iter().enumerate() {
        let style = if i == cursor {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        spans.push(Span::styled(ch.to_string(), style));
    }
    if cursor >= chars.len() {
        spans.push(Span::styled(
            " ",
            Style::new().add_modifier(Modifier::REVERSED),
        ));
    }
    spans
}

/// Roda a TUI interativa até sair.
///
/// `create_initial` é o valor inicial do "Criar PR(s)?" (vem de `--create`).
/// Retorna o outcome para o `main` copiar/publicar fora do alternate screen.
///
/// # Errors
///
/// Retorna erro se o terminal não puder ser inicializado.
pub async fn run_describe_tui(
    prep: DescribePrep,
    create_initial: bool,
) -> anyhow::Result<LiveOutcome> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("tui requer terminal interativo");
    }
    let mut terminal: DefaultTerminal = ratatui::init();
    // `run_loop` é síncrono (sem `.await` interno); o `async move`
    // mantém `run_describe_tui` aguardável sem mudar comportamento.
    let res = async move { run_loop(&mut terminal, prep, create_initial) }.await;
    ratatui::restore();
    res
}

/// Partes de publicação extraídas do `prep` antes do backend.
fn make_publish_parts(prep: &DescribePrep) -> (Option<PublishSetup>, Option<String>, PublishBase) {
    // Setup de publicação (espelha `validateCreation` + defaults do Dart).
    let pat = prep.config.azure_pat.clone();
    let (publish_setup, publish_blocked) = match &prep.context.remote {
        None => (None, Some("remote Azure DevOps não encontrado.".to_owned())),
        Some(_) if pat.trim().is_empty() => (
            None,
            Some("PAT não configurado — rode `prt init`.".to_owned()),
        ),
        Some(_) => (
            Some(PublishSetup {
                reviewer_sprint: prep.config.reviewer_sprint.clone(),
                reviewer_dev: prep.config.reviewer_dev.clone(),
            }),
            None,
        ),
    };
    // Base própria para a task de publicação (o `prep` move para o backend).
    let publish_base = PublishBase {
        pat,
        remote: prep.context.remote.clone(),
        branch: prep.context.branch.clone(),
        work_item_id: prep.work_item_id.clone(),
    };
    (publish_setup, publish_blocked, publish_base)
}

/// Drena eventos do backend sem bloquear; retorna `true` se sujou a tela.
fn drain_backend(rx: &mut mpsc::UnboundedReceiver<BackendEvent>, app: &mut DescribeApp) -> bool {
    let mut novo = false;
    while let Ok(ev) = rx.try_recv() {
        app.on_backend(ev);
        novo = true;
    }
    novo
}

fn start_backend(
    pending_prep: &mut Option<DescribePrep>,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) {
    if let Some(prep) = pending_prep.take() {
        tokio::spawn(backend_task(prep, tx.clone()));
    }
}

/// Navegação/scroll (`j/k`, setas, `tab`) — retorna `true` se consumiu a tecla.
fn on_nav_key(app: &mut DescribeApp, key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::KeyCode;
    match (key.code, app.publish_dialog) {
        (KeyCode::Down | KeyCode::Char('j'), Some(PublishDialog::PublishedPrPicker { .. })) => {
            app.move_published_pr_picker(true)
        }
        (KeyCode::Up | KeyCode::Char('k'), Some(PublishDialog::PublishedPrPicker { .. })) => {
            app.move_published_pr_picker(false)
        }
        (KeyCode::Char('j') | KeyCode::Down, Some(PublishDialog::PublishRecovery(selected))) => {
            app.publish_dialog = Some(PublishDialog::PublishRecovery((selected + 1) % 4));
            true
        }
        (KeyCode::Char('k') | KeyCode::Up, Some(PublishDialog::PublishRecovery(selected))) => {
            app.publish_dialog = Some(PublishDialog::PublishRecovery((selected + 3) % 4));
            true
        }
        (KeyCode::Char('j') | KeyCode::Down, Some(PublishDialog::CandidateList { selected })) => {
            if !app.candidates.is_empty() {
                app.publish_dialog = Some(PublishDialog::CandidateList {
                    selected: (selected + 1) % app.candidates.len(),
                });
            }
            true
        }
        (KeyCode::Char('k') | KeyCode::Up, Some(PublishDialog::CandidateList { selected })) => {
            if !app.candidates.is_empty() {
                app.publish_dialog = Some(PublishDialog::CandidateList {
                    selected: (selected + app.candidates.len() - 1) % app.candidates.len(),
                });
            }
            true
        }
        (KeyCode::Char('j'), _) if app.publish_dialog == Some(PublishDialog::Reviewers) => {
            // No editor, `j` digita (emails contêm a letra).
            app.reviewer_edit_input(key);
            true
        }
        (KeyCode::Char('k'), _) if app.publish_dialog == Some(PublishDialog::Reviewers) => {
            app.reviewer_edit_input(key);
            true
        }
        (KeyCode::Char('j'), _) => {
            app.scroll_by(3);
            true
        }
        (KeyCode::Char('k'), _) => {
            app.scroll_by(-3);
            true
        }
        (KeyCode::Down, _) => {
            if app.publish_dialog == Some(PublishDialog::Reviewers) {
                let n = app.reviewers.len().max(1);
                app.focus_reviewer((app.reviewer_idx + 1) % n);
            } else {
                app.scroll_by(3);
            }
            true
        }
        (KeyCode::Up, _) => {
            if app.publish_dialog == Some(PublishDialog::Reviewers) {
                let n = app.reviewers.len().max(1);
                app.focus_reviewer((app.reviewer_idx + n - 1) % n);
            } else {
                app.scroll_by(-3);
            }
            true
        }
        (KeyCode::Tab, _) => {
            if app.publish_dialog == Some(PublishDialog::Reviewers) {
                let n = app.reviewers.len().max(1);
                app.focus_reviewer((app.reviewer_idx + 1) % n);
            } else {
                app.next_target();
            }
            true
        }
        _ => false,
    }
}

/// Trata uma tecla enquanto o editor de conteúdo está aberto.
fn handle_content_edit_key(app: &mut DescribeApp, key: crossterm::event::KeyEvent) -> bool {
    let Some(editor) = app.content_edit.as_mut() else {
        return false;
    };
    let action = editor.handle_key(key);
    match action {
        ContentEditAction::Saved(content) => {
            app.desc = Some(content);
            app.content_edit = None;
            app.error = None;
            app.scroll = 0;
            app.logs
                .push_back("conteúdo salvo — preview atualizado".to_owned());
            true
        }
        ContentEditAction::Cancelled => {
            app.content_edit = None;
            app.error = None;
            true
        }
        ContentEditAction::Consumed => true,
        ContentEditAction::Ignored => false,
    }
}

/// Trata paste do terminal enquanto o editor está aberto.
fn handle_content_paste(app: &mut DescribeApp, text: &str) -> bool {
    let Some(editor) = app.content_edit.as_mut() else {
        return false;
    };
    match editor.field {
        ContentField::Title => editor.title.insert_text(text),
        ContentField::Body => editor.body.insert_text(text),
    }
    editor.error = None;
    true
}

/// Cópia do body (`c`) — fora do diálogo ou digitando no editor.
fn on_copy_key(app: &mut DescribeApp, key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::KeyCode;
    if key.code != KeyCode::Char('c') {
        return false;
    }
    if app.publish_dialog.is_none() {
        if let Some(body) = approved_publish_body(app) {
            if crate::features::describe::copy_to_clipboard(body) {
                app.flash_copied();
                app.logs.push_back("body copiado ✓".to_owned());
            } else {
                app.logs.push_back(
                    "clipboard indisponível (SSH?) — selecione e copie manualmente".to_owned(),
                );
            }
            return true;
        }
        return false;
    }
    if app.publish_dialog == Some(PublishDialog::Reviewers) {
        app.reviewer_edit_input(key);
        return true;
    }
    false
}

/// Target usado na consulta de duplicidade: o target que falhou, ou o
/// primeiro ainda pendente quando a falha ocorreu antes de iniciar um target.
fn candidate_target(app: &DescribeApp) -> Option<String> {
    app.publish_failure
        .as_ref()
        .and_then(|failure| failure.target.clone())
        .or_else(|| app.remaining_publish_targets().into_iter().next())
}

/// Inicia a busca de PRs possivelmente criados antes de uma resposta perdida.
fn start_candidate_search(
    app: &mut DescribeApp,
    base: &PublishBase,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) {
    let Some(target) = candidate_target(app) else {
        app.candidate_message = Some("não há target pendente para consultar".to_owned());
        return;
    };
    let Some(title) = publish_candidate_title(app) else {
        return;
    };
    app.candidate_activity = CandidateActivity::Loading;
    app.candidate_message = None;
    app.candidates.clear();
    app.publish_dialog = Some(PublishDialog::CandidateList { selected: 0 });
    app.phase_label = format!("buscando PRs recentes ({target})…");
    "consultando possível duplicidade".clone_into(&mut app.progress_label);
    app.logs
        .push_back(format!("buscando PRs recentes do target {target}"));
    let base = base.clone();
    let tx = tx.clone();
    tokio::spawn(backend_find_candidates(base, title, target, tx));
}

/// Retorna o conteúdo aprovado para esta tentativa, mantendo o snapshot
/// intacto em retries e nos targets restantes.
fn publish_content_for_attempt(
    app: &DescribeApp,
    generated: &crate::ai::PrDescription,
) -> crate::ai::PrDescription {
    app.frozen_publish_content
        .clone()
        .unwrap_or_else(|| generated.clone())
}

/// Título usado pela busca de duplicidade do PR atual.
fn publish_candidate_title(app: &DescribeApp) -> Option<String> {
    app.frozen_publish_content
        .as_ref()
        .or(app.desc.as_ref())
        .map(|content| content.title.clone())
}

/// Body aprovado exibido ao copiar, inclusive durante uma recuperação.
fn approved_publish_body(app: &DescribeApp) -> Option<&str> {
    app.frozen_publish_content
        .as_ref()
        .or(app.desc.as_ref())
        .map(|content| content.body.as_str())
}

/// `Enter` — avança o fluxo de publicação conforme o diálogo aberto.
fn on_enter_key(
    app: &mut DescribeApp,
    publish_base: &PublishBase,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) -> bool {
    match app.publish_dialog {
        None => {
            // Revisão → inicia o fluxo de publicação.
            if app.phase == Phase::Review && app.desc.is_some() {
                if app.publish_failure.is_some() {
                    let selected = app.publish_failure.as_ref().map_or(0, |failure| {
                        usize::from(matches!(
                            failure.kind,
                            describe::PublishFailureKind::OutcomeUnknown
                        ))
                    });
                    app.publish_dialog = Some(PublishDialog::PublishRecovery(selected));
                    return true;
                }
                match &app.publish_blocked {
                    Some(reason) => {
                        app.logs
                            .push_back(format!("publicação indisponível: {reason}"));
                    }
                    None => app.open_confirm_create(),
                }
                return true;
            }
            false
        }
        Some(
            PublishDialog::FunctionalContextFallback(_) | PublishDialog::PublishedPrPicker { .. },
        ) => true,
        Some(PublishDialog::ConfirmCreate(yes)) => {
            if yes {
                app.open_reviewers();
            } else {
                app.publish_dialog = None;
            }
            true
        }
        Some(PublishDialog::Reviewers) => {
            app.commit_reviewer();
            let n = app.reviewers.len();
            if app.reviewer_idx + 1 < n {
                app.focus_reviewer(app.reviewer_idx + 1);
            } else {
                // Vazio volta ao default (como no Dart).
                if let Some(setup) = app.publish_setup.clone() {
                    for (i, target) in app.targets.iter().enumerate() {
                        if app.reviewers.get(i).is_some_and(|v| v.trim().is_empty()) {
                            if let Some(slot) = app.reviewers.get_mut(i) {
                                *slot = setup.default_for(target);
                            }
                        }
                    }
                }
                app.publish_dialog = Some(PublishDialog::ConfirmPublish(true));
            }
            true
        }
        Some(PublishDialog::ConfirmPublish(yes)) => {
            if yes {
                if let Some(desc) = app.desc.clone() {
                    let recovery = app.publish_failure.is_some();
                    start_publish(app, publish_base, tx, &desc, recovery);
                }
            } else {
                app.publish_dialog = None;
            }
            true
        }
        Some(PublishDialog::PublishRecovery(selected)) => {
            match selected {
                0 => {
                    if let Some(desc) = app.desc.clone() {
                        start_publish(app, publish_base, tx, &desc, true);
                    }
                }
                1 => start_candidate_search(app, publish_base, tx),
                2 => app.open_reviewers(),
                _ => app.publish_dialog = None,
            }
            true
        }
        Some(PublishDialog::CandidateList { selected }) => {
            app.adopt_candidate(selected);
            true
        }
    }
}

/// Confirms (`←/→`, `y/n`) — retorna `true` se consumiu a tecla.
fn on_confirm_key(
    app: &mut DescribeApp,
    key: crossterm::event::KeyEvent,
    publish_base: &PublishBase,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) -> bool {
    use crossterm::event::KeyCode;
    match (key.code, app.publish_dialog) {
        (KeyCode::Left | KeyCode::Right, Some(PublishDialog::ConfirmCreate(yes))) => {
            // Alterna Sim/Não nos confirms.
            app.publish_dialog = Some(PublishDialog::ConfirmCreate(!yes));
            true
        }
        (KeyCode::Left | KeyCode::Right, Some(PublishDialog::ConfirmPublish(yes))) => {
            app.publish_dialog = Some(PublishDialog::ConfirmPublish(!yes));
            true
        }
        (KeyCode::Left | KeyCode::Right, Some(PublishDialog::Reviewers)) => {
            app.reviewer_edit_input(key);
            true
        }
        (KeyCode::Char('y'), Some(PublishDialog::ConfirmCreate(_))) => {
            app.open_reviewers();
            true
        }
        (KeyCode::Char('y'), Some(PublishDialog::ConfirmPublish(_))) => {
            if let Some(desc) = app.desc.clone() {
                let recovery = app.publish_failure.is_some();
                start_publish(app, publish_base, tx, &desc, recovery);
            }
            true
        }
        (KeyCode::Char('a'), Some(PublishDialog::CandidateList { selected })) => {
            app.adopt_candidate(selected);
            true
        }
        (KeyCode::Char('r'), Some(PublishDialog::PublishRecovery(_))) => {
            if let Some(desc) = app.desc.clone() {
                start_publish(app, publish_base, tx, &desc, true);
            }
            true
        }
        (KeyCode::Char('y'), Some(PublishDialog::Reviewers)) => {
            // No editor, `y` digita.
            app.reviewer_edit_input(key);
            true
        }
        (KeyCode::Char('n'), _) => {
            if matches!(
                app.publish_dialog,
                Some(PublishDialog::ConfirmCreate(_) | PublishDialog::ConfirmPublish(_))
            ) {
                app.publish_dialog = None;
                true
            } else if app.publish_dialog == Some(PublishDialog::Reviewers) {
                app.reviewer_edit_input(key);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Resultado ao sair da tela: uma descrição pronta preserva os PRs criados.
fn quit_outcome(app: &DescribeApp) -> LiveOutcome {
    let Some(desc) = app.desc.clone() else {
        return LiveOutcome::Aborted;
    };
    if app.phase == Phase::Done {
        return LiveOutcome::Done {
            desc,
            published: app.published.clone(),
        };
    }
    if app.phase == Phase::Review {
        if let Some(failure) = &app.publish_failure {
            let partial = if app.published.is_empty() {
                "nenhum PR foi confirmado"
            } else {
                "os PRs já confirmados foram preservados"
            };
            return LiveOutcome::Failed(format!(
                "publicação incompleta: {}; {partial}",
                failure.message
            ));
        }
        if !app.published.is_empty() && !app.remaining_publish_targets().is_empty() {
            return LiveOutcome::Failed(
                "publicação incompleta: há targets pendentes; os PRs já confirmados foram preservados"
                    .to_owned(),
            );
        }
        return LiveOutcome::Done {
            desc,
            published: app.published.clone(),
        };
    }
    LiveOutcome::Aborted
}

/// Despacha uma tecla já filtrada por `kind`; retorna `Ok(Some(outcome))`
/// quando o loop deve sair, `Ok(None)` para continuar. Seta `needs_draw`
/// quando a tela sujou.
fn handle_key_event(
    app: &mut DescribeApp,
    key: crossterm::event::KeyEvent,
    publish_base: &PublishBase,
    tx: &mpsc::UnboundedSender<BackendEvent>,
    terminal: &mut DefaultTerminal,
    needs_draw: &mut bool,
) -> anyhow::Result<Option<LiveOutcome>> {
    use crossterm::event::KeyCode;
    if app.content_edit.is_some() && handle_content_edit_key(app, key) {
        *needs_draw = true;
        return Ok(None);
    }
    if app.is_waiting_for_functional_context() {
        return Ok(handle_functional_context_key(app, key, needs_draw));
    }
    if let Some(PublishDialog::PublishedPrPicker { selected }) = app.publish_dialog {
        match key.code {
            KeyCode::Enter => {
                let Some(launch_context) = app.selected_published_pr(selected) else {
                    return Ok(Some(LiveOutcome::Failed(
                        "não foi possível montar o contexto do PR publicado".to_owned(),
                    )));
                };
                let Some(desc) = app.desc.clone() else {
                    return Ok(Some(LiveOutcome::Failed(
                        "descrição ausente para continuar ao Test Case".to_owned(),
                    )));
                };
                return Ok(Some(LiveOutcome::PrepareTestCase {
                    desc,
                    launch_context,
                    published: app.published.clone(),
                }));
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                app.publish_dialog = None;
                *needs_draw = true;
                return Ok(None);
            }
            _ => {}
        }
    }
    if key.modifiers.is_empty()
        && matches!(key.code, KeyCode::Char('t' | 'T'))
        && app.phase == Phase::Done
        && app.publish_dialog.is_none()
    {
        if app.published.len() > 1 {
            app.open_published_pr_picker();
            *needs_draw = true;
            return Ok(None);
        }
        let Some(launch_context) = app.selected_published_pr(0) else {
            return Ok(Some(LiveOutcome::Failed(
                "não foi possível montar o contexto do PR publicado".to_owned(),
            )));
        };
        let Some(desc) = app.desc.clone() else {
            return Ok(Some(LiveOutcome::Failed(
                "descrição ausente para continuar ao Test Case".to_owned(),
            )));
        };
        return Ok(Some(LiveOutcome::PrepareTestCase {
            desc,
            launch_context,
            published: app.published.clone(),
        }));
    }
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
            if app.phase == Phase::Publishing {
                app.logs
                    .push_back("publicação em andamento — aguarde a conclusão".to_owned());
                *needs_draw = true;
                return Ok(None);
            }
            return Ok(Some(LiveOutcome::Aborted));
        }
        (KeyCode::Char('z'), m) if m.contains(KeyModifiers::CONTROL) => {
            #[cfg(unix)]
            {
                super::suspend::suspend_to_shell(&mut *terminal)?;
                *needs_draw = true;
            }
            return Ok(None);
        }
        (KeyCode::Char('q'), _) if app.publish_dialog != Some(PublishDialog::Reviewers) => {
            if app.phase == Phase::Publishing {
                app.logs
                    .push_back("publicação em andamento — aguarde a conclusão".to_owned());
                *needs_draw = true;
            } else if app.publish_dialog.is_some() {
                // Fecha o diálogo e volta à revisão (não sai).
                app.publish_dialog = None;
                *needs_draw = true;
            } else {
                return Ok(Some(quit_outcome(app)));
            }
            return Ok(None);
        }
        (KeyCode::Esc, _) => {
            if app.phase == Phase::Publishing {
                app.logs
                    .push_back("publicação em andamento — aguarde a conclusão".to_owned());
                *needs_draw = true;
            } else if matches!(app.publish_dialog, Some(PublishDialog::ConfirmPublish(_))) {
                // Permite corrigir reviewers antes de confirmar a publicação.
                app.commit_reviewer();
                app.publish_dialog = Some(PublishDialog::Reviewers);
                app.rebind_reviewer();
                *needs_draw = true;
            } else if app.publish_dialog.is_some() {
                // Fecha o diálogo e volta à revisão (não sai).
                app.publish_dialog = None;
                *needs_draw = true;
            } else {
                return Ok(Some(quit_outcome(app)));
            }
            return Ok(None);
        }
        (KeyCode::Char('?'), _) => {
            app.show_help = !app.show_help;
            *needs_draw = true;
            return Ok(None);
        }
        (KeyCode::Char('e'), m)
            if m.is_empty() && app.phase == Phase::Review && app.publish_dialog.is_none() =>
        {
            if app.open_content_edit() {
                *needs_draw = true;
            }
            return Ok(None);
        }
        (KeyCode::Enter, _) => {
            if on_enter_key(app, publish_base, tx) {
                *needs_draw = true;
            }
            return Ok(None);
        }
        _ => {}
    }
    if on_nav_key(app, key) {
        *needs_draw = true;
        return Ok(None);
    }
    if on_copy_key(app, key) {
        *needs_draw = true;
        return Ok(None);
    }
    if on_confirm_key(app, key, publish_base, tx) {
        *needs_draw = true;
        return Ok(None);
    }
    if key.code == KeyCode::Char('r') && app.phase == Phase::Error {
        return Ok(Some(LiveOutcome::Failed(
            app.error.clone().unwrap_or_default(),
        )));
    }
    // No editor de reviewers, todo o resto digita
    // (inclusive `q`, `c`, `j`, `k` — emails contêm).
    if app.publish_dialog == Some(PublishDialog::Reviewers) {
        app.reviewer_edit_input(key);
        *needs_draw = true;
    }
    Ok(None)
}

fn handle_functional_context_key(
    app: &mut DescribeApp,
    key: crossterm::event::KeyEvent,
    needs_draw: &mut bool,
) -> Option<LiveOutcome> {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Left | KeyCode::Right => {
            app.toggle_functional_context_fallback();
            *needs_draw = true;
            None
        }
        KeyCode::Char('y') => {
            let _ = app.confirm_functional_git_only();
            *needs_draw = true;
            None
        }
        KeyCode::Enter => {
            if matches!(
                app.publish_dialog,
                Some(PublishDialog::FunctionalContextFallback(true))
            ) {
                let _ = app.confirm_functional_git_only();
                *needs_draw = true;
                None
            } else {
                Some(LiveOutcome::Aborted)
            }
        }
        KeyCode::Char('n' | 'q') | KeyCode::Esc => Some(LiveOutcome::Aborted),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(LiveOutcome::Aborted)
        }
        _ => None,
    }
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    prep: DescribePrep,
    create_initial: bool,
) -> anyhow::Result<LiveOutcome> {
    let (tx, mut rx) = mpsc::unbounded_channel::<BackendEvent>();
    let (publish_setup, publish_blocked, publish_base) = make_publish_parts(&prep);
    let functional_context_status = prep.functional_context.clone();
    let launch_prep = prep.clone();
    let mut app = DescribeApp::new(
        &prep.context.branch,
        &prep.targets,
        &prep.work_item_id,
        create_initial,
        publish_setup,
        publish_blocked,
    );
    app.set_launch_prep(launch_prep);
    let mut pending_prep = Some(prep);
    // Backend roda em paralelo e empurra tokens/logs (`tx` fica no loop
    // para a task de publicação criada sob demanda).
    app.set_functional_context_status(functional_context_status);
    if !app.is_waiting_for_functional_context() {
        start_backend(&mut pending_prep, &tx);
    }

    let tick_rate = Duration::from_millis(33); // ~30fps p/ barra/status suaves
    let mut last_tick = std::time::Instant::now();
    // Dirty-flag: desenha só se tick/backend/input sujaram a tela.
    let mut needs_draw = true;

    loop {
        // 1. Drena eventos do backend sem bloquear; suja se houve dado.
        if drain_backend(&mut rx, &mut app) {
            needs_draw = true;
        }
        // 2. Tick de animação (barra/status continuam e sujam a tela).
        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = std::time::Instant::now();
            needs_draw = true;
        }
        // 3. Desenha só se sujo.
        if needs_draw {
            terminal.draw(|f| f.render_widget(&app, f.area()))?;
            needs_draw = false;
        }

        // 4. Input não-bloqueante (poll 10ms p/ manter 30fps).
        if event::poll(Duration::from_millis(10))? {
            match event::read()? {
                Event::Paste(text) => {
                    if handle_content_paste(&mut app, text.as_str()) {
                        needs_draw = true;
                    }
                }
                Event::Key(key) => {
                    // Filtro de kind: Release sempre ignorado; Repeat passa
                    // também para a edição do conteúdo ativo.
                    let eh_scroll = matches!(
                        key.code,
                        KeyCode::Char('j' | 'k') | KeyCode::Up | KeyCode::Down
                    );
                    let eh_content_edit = app.content_edit.is_some()
                        && matches!(
                            key.code,
                            KeyCode::Char(_)
                                | KeyCode::Backspace
                                | KeyCode::Delete
                                | KeyCode::Left
                                | KeyCode::Right
                                | KeyCode::Up
                                | KeyCode::Down
                                | KeyCode::Home
                                | KeyCode::End
                                | KeyCode::Enter
                                | KeyCode::Tab
                                | KeyCode::BackTab
                                | KeyCode::PageUp
                                | KeyCode::PageDown
                                | KeyCode::Esc
                        );
                    if key.kind == KeyEventKind::Release
                        || (key.kind == KeyEventKind::Repeat && !eh_scroll && !eh_content_edit)
                    {
                        // Ignora sem sujar a tela.
                    } else if let Some(outcome) = handle_key_event(
                        &mut app,
                        key,
                        &publish_base,
                        &tx,
                        &mut *terminal,
                        &mut needs_draw,
                    )? {
                        return Ok(outcome);
                    }
                    if app.functional_context_fallback_confirmed && pending_prep.is_some() {
                        start_backend(&mut pending_prep, &tx);
                    }
                }
                _ => {}
            }
        }

        // Saída automática? Não — usuário decide (q/enter). Evita fechar no stream.
        let _ = std::io::Write::flush(&mut std::io::stdout() as &mut Stdout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::PrDescription;
    use crate::azure::work_items::FunctionalWorkItemContext;
    use crate::features::describe::{DescribePrep, FunctionalContextStatus};
    use crate::tui::describe_app::DescribeApp;
    use crate::tui::events::BackendEvent;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{DefaultTerminal, Terminal, backend::TestBackend};

    /// Estado Review de exemplo p/ snapshots: heading + checklist + code fence.
    fn review_app() -> DescribeApp {
        let mut app = DescribeApp::new(
            "feature/11763-exemplo",
            &["dev".to_owned()],
            "11763",
            false,
            Some(PublishSetup {
                reviewer_sprint: "sprint@x.com".to_owned(),
                reviewer_dev: "dev@x.com".to_owned(),
            }),
            None,
        );
        app.set_functional_context_status(FunctionalContextStatus::Loaded(
            FunctionalWorkItemContext {
                id: 11763,
                title: "Atualiza fluxo de checkout".to_owned(),
                work_item_type: "User Story".to_owned(),
                area_path: "Produto\\CLI".to_owned(),
                description: None,
                acceptance_criteria: None,
            },
        ));
        let desc = PrDescription {
            title: "Atualiza fluxo de checkout".to_owned(),
            body: "## Descrição\nAtualiza o fluxo de checkout para validar o carrinho.\n\n## Checklist\n- [x] Testes locais\n- [ ] Review\n\n```diff\n+ valida carrinho\n- ignora erro\n```\n"
                .to_owned(),
        };
        app.on_backend(BackendEvent::Finished(
            desc,
            "{\"title\":\"Atualiza fluxo de checkout\"}".to_owned(),
        ));
        app
    }

    fn launch_prep() -> DescribePrep {
        let remote = crate::git::RepositoryRemote {
            organization: "org".to_owned(),
            project: "project".to_owned(),
            repository: "repo".to_owned(),
        };
        let work_item: crate::azure::WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11763,
            "fields": {
                "System.Title": "Mudança funcional",
                "System.WorkItemType": "User Story",
                "System.IterationPath": "project\\Sprint 12"
            }
        }))
        .expect("snapshot do Work Item");
        DescribePrep {
            config: crate::config::Config {
                azure_pat: "pat".to_owned(),
                test_team: "DevOps".to_owned(),
                test_program: "Agrotrace".to_owned(),
                ..crate::config::Config::default()
            },
            context: crate::git::ChangeContext {
                branch: "feature/11763-exemplo".to_owned(),
                source_ref: "refs/heads/feature/11763-exemplo".to_owned(),
                base_branch: "dev".to_owned(),
                sprint_branch: String::new(),
                diff: "diff".to_owned(),
                diff_original_lines: 1,
                log: "log".to_owned(),
                work_item_id: "11763".to_owned(),
                remote: Some(remote),
            },
            targets: vec!["dev".to_owned()],
            work_item_id: "11763".to_owned(),
            functional_context: FunctionalContextStatus::Loaded(FunctionalWorkItemContext {
                id: 11763,
                title: "Mudança funcional".to_owned(),
                work_item_type: "User Story".to_owned(),
                area_path: "Produto\\CLI".to_owned(),
                description: None,
                acceptance_criteria: None,
            }),
            work_item: Some(work_item),
            fingerprint: crate::git::GitContextFingerprint::default(),
            prompt: "prompt".to_owned(),
        }
    }

    fn published(id: i64, target: &str) -> PublishedPr {
        PublishedPr {
            target: target.to_owned(),
            id,
            url: format!("https://dev.azure.com/org/project/_git/repo/pullrequest/{id}"),
        }
    }

    fn test_terminal() -> DefaultTerminal {
        Terminal::new(ratatui::backend::CrosstermBackend::new(std::io::stdout()))
            .expect("terminal de teste")
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn functional_context_failure_should_require_explicit_git_only_confirmation() {
        let mut app = DescribeApp::new(
            "feature/11763-exemplo",
            &["dev".to_owned()],
            "11763",
            false,
            None,
            None,
        );
        app.set_functional_context_status(FunctionalContextStatus::Unavailable(
            "Azure DevOps recusou a leitura do Work Item (HTTP 403)".to_owned(),
        ));
        assert!(app.is_waiting_for_functional_context());
        assert_eq!(
            app.publish_dialog,
            Some(PublishDialog::FunctionalContextFallback(false))
        );
        assert!(!app.functional_context_fallback_confirmed);

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("terminal de teste");
        terminal
            .draw(|f| f.render_widget(&app, f.area()))
            .expect("render");
        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(rendered.contains("Contexto funcional indisponível"));
        assert!(rendered.contains("Continuar somente com o contexto Git?"));
        assert!(rendered.contains("HTTP 403"));

        for key_code in [KeyCode::Char('n'), KeyCode::Esc] {
            let mut declined = DescribeApp::new(
                "feature/11763-exemplo",
                &["dev".to_owned()],
                "11763",
                false,
                None,
                None,
            );
            declined.set_functional_context_status(FunctionalContextStatus::Unavailable(
                "Azure DevOps recusou a leitura do Work Item (HTTP 403)".to_owned(),
            ));
            let mut needs_draw = false;
            let outcome = handle_functional_context_key(
                &mut declined,
                KeyEvent::new(key_code, KeyModifiers::NONE),
                &mut needs_draw,
            );
            assert!(matches!(outcome, Some(LiveOutcome::Aborted)));
            assert!(!declined.functional_context_fallback_confirmed);
            assert!(!needs_draw);
        }

        app.toggle_functional_context_fallback();
        assert_eq!(
            app.publish_dialog,
            Some(PublishDialog::FunctionalContextFallback(true))
        );
        assert!(app.confirm_functional_git_only());
        assert!(!app.is_waiting_for_functional_context());
        assert!(app.functional_context_fallback_confirmed);
        assert_eq!(app.publish_dialog, None);
    }

    #[test]
    fn functional_context_should_not_leak_raw_work_item_data() {
        let mut app = review_app();
        app.set_functional_context_status(FunctionalContextStatus::Loaded(
            FunctionalWorkItemContext {
                id: 11763,
                title: "Enriquecer a descrição".to_owned(),
                work_item_type: "User Story".to_owned(),
                area_path: "Produto\\CLI".to_owned(),
                description: Some("segredo rico que não deve aparecer".to_owned()),
                acceptance_criteria: Some("critério rico".to_owned()),
            },
        ));
        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).expect("terminal de teste");
        terminal
            .draw(|f| f.render_widget(&app, f.area()))
            .expect("render");
        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(rendered.contains("Contexto funcional: Work Item #11763"));
        assert!(rendered.contains("Enriquecer a descrição"));
        assert!(!rendered.contains("segredo rico"));
        assert!(!rendered.contains("critério rico"));
    }

    #[test]
    fn editing_desc_should_open_content_editor_with_generated_content() {
        let mut app = review_app();
        assert!(app.open_content_edit());
        let editor = app.content_edit.as_ref().expect("editor aberto");
        assert_eq!(editor.title.value(), "Atualiza fluxo de checkout");
        assert_eq!(editor.body.value(), app.desc.as_ref().unwrap().body);
    }

    #[test]
    fn saving_valid_content_should_update_desc_preview_exactly() {
        let mut app = review_app();
        assert!(app.open_content_edit());
        let editor = app.content_edit.as_mut().expect("editor aberto");
        editor.title = crate::tui::content_editor::TextEditor::new("Título — ✅", true);
        editor.body = crate::tui::content_editor::TextEditor::new(
            "  ação concluída\n- [ ] validar\nlinha final  ",
            false,
        );
        let saved = handle_content_edit_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
        );
        assert!(saved);
        assert!(app.content_edit.is_none());
        assert_eq!(app.desc.as_ref().unwrap().title, "Título — ✅");
        assert_eq!(
            app.desc.as_ref().unwrap().body,
            "  ação concluída\n- [ ] validar\nlinha final  "
        );
        assert_eq!(
            app.preview_text(),
            "# Título — ✅\n\n  ação concluída\n- [ ] validar\nlinha final  "
        );
    }

    #[test]
    fn invalid_desc_content_should_stay_in_editor_without_remote_start() {
        let mut app = review_app();
        app.desc = Some(PrDescription {
            title: "   ".to_owned(),
            body: "corpo".to_owned(),
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        let base = PublishBase {
            pat: String::new(),
            remote: None,
            branch: "feature/x".to_owned(),
            work_item_id: String::new(),
        };
        let desc = app.desc.clone().expect("descrição");
        start_publish(&mut app, &base, &tx, &desc, false);
        assert!(app.content_edit.is_some());
        assert_eq!(app.phase, Phase::Review);
        assert_eq!(app.error.as_deref(), Some("título é obrigatório"));
    }

    #[test]
    fn canceling_content_edit_should_discard_desc_draft() {
        let mut app = review_app();
        let original = app.desc.clone().expect("descrição");
        assert!(app.open_content_edit());
        app.content_edit
            .as_mut()
            .expect("editor aberto")
            .title
            .insert_text(" alterado");
        assert!(handle_content_edit_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ));
        assert!(app.content_edit.is_none());
        assert_eq!(app.desc, Some(original));
        assert!(app.open_content_edit());
        let editor = app.content_edit.as_ref().expect("editor reaberto");
        assert_eq!(editor.title.value(), "Atualiza fluxo de checkout");
    }

    #[test]
    fn copy_should_use_approved_body_only_and_editor_should_consume_c() {
        let mut app = review_app();
        let approved = PrDescription {
            title: "Título aprovado".to_owned(),
            body: "body aprovado\n- [ ] exato".to_owned(),
        };
        app.frozen_publish_content = Some(approved.clone());
        app.desc = Some(PrDescription {
            title: "Título antigo".to_owned(),
            body: "body antigo".to_owned(),
        });
        assert_eq!(approved_publish_body(&app), Some(approved.body.as_str()));

        app.frozen_publish_content = None;
        assert!(app.open_content_edit());
        let before = app.content_edit.as_ref().unwrap().title.value().to_owned();
        assert!(handle_content_edit_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        ));
        assert_eq!(
            app.content_edit.as_ref().unwrap().title.value(),
            format!("{before}c")
        );
    }

    #[test]
    fn publish_should_freeze_approved_content_before_first_remote_call() {
        let mut app = review_app();
        let generated = app.desc.clone().expect("descrição");
        let approved = PrDescription {
            title: "Título editado".to_owned(),
            body: "Markdown editado\n- [ ] exato ✅".to_owned(),
        };
        app.frozen_publish_content = Some(approved.clone());
        let attempt = publish_content_for_attempt(&app, &generated);
        assert_eq!(attempt, approved);
        app.frozen_publish_content = None;
        app.phase = Phase::Publishing;
        assert!(!app.open_content_edit());
        app.phase = Phase::Review;
        app.publish_dialog = Some(PublishDialog::PublishRecovery(0));
        assert!(!app.open_content_edit());
        app.publish_dialog = None;
        app.frozen_publish_content = Some(attempt.clone());

        let remote = crate::git::RepositoryRemote {
            organization: "org".to_owned(),
            project: "project".to_owned(),
            repository: "repo".to_owned(),
        };
        let targets = vec!["sprint/12".to_owned(), "dev".to_owned()];
        let reviewer_for = |_: &str| String::new();
        let input = crate::azure::pull_requests::PublishInput {
            remote: &remote,
            branch: &app.branch,
            targets: &targets,
            title: &attempt.title,
            body: &attempt.body,
            work_item_ids: &[],
            reviewer_for: &reviewer_for,
            on_published: None,
            on_target_started: None,
        };
        for _target in input.targets {
            assert_eq!(input.title, "Título editado");
            assert_eq!(input.body, "Markdown editado\n- [ ] exato ✅");
        }
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn publish_and_create_retry_should_reuse_frozen_content_and_exact_title() {
        let mut app = review_app();
        let approved = PrDescription {
            title: "Título da tentativa".to_owned(),
            body: "body da tentativa".to_owned(),
        };
        app.frozen_publish_content = Some(approved.clone());
        app.desc.as_mut().unwrap().title = "mutação indevida".to_owned();
        assert_eq!(publish_candidate_title(&app), Some(approved.title.clone()));
        assert_eq!(
            publish_content_for_attempt(&app, &app.desc.clone().unwrap()),
            approved
        );
    }

    #[test]
    fn pending_publish_targets_should_reuse_one_frozen_content_snapshot() {
        let mut app = review_app();
        app.targets = vec!["sprint/12".to_owned(), "dev".to_owned()];
        app.frozen_publish_content = Some(PrDescription {
            title: "Título único".to_owned(),
            body: "body único".to_owned(),
        });
        app.published.push(PublishedPr {
            target: "sprint/12".to_owned(),
            id: 1,
            url: "https://example/pr/1".to_owned(),
        });
        let generated = PrDescription {
            title: "não usar".to_owned(),
            body: "não usar".to_owned(),
        };
        assert_eq!(app.remaining_publish_targets(), vec!["dev"]);
        assert_eq!(
            publish_content_for_attempt(&app, &generated).title,
            "Título único"
        );
        assert_eq!(
            publish_candidate_title(&app),
            Some("Título único".to_owned())
        );
    }

    #[test]
    fn desc_content_editor_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.open_content_edit();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_content_editor_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_content_editor_80x24() -> anyhow::Result<()> {
        let mut app = review_app();
        app.open_content_edit();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_content_editor_80x24", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_review_80x24() -> anyhow::Result<()> {
        let app = review_app();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_review_80x24", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_review_100x30() -> anyhow::Result<()> {
        let app = review_app();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_review_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_boot_100x30() -> anyhow::Result<()> {
        let app = DescribeApp::new(
            "feature/11763-exemplo",
            &["dev".to_owned()],
            "11763",
            false,
            None,
            Some("PAT não configurado".to_owned()),
        );
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_boot_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_generating_100x30() -> anyhow::Result<()> {
        let mut app = DescribeApp::new(
            "feature/11763-exemplo",
            &["dev".to_owned()],
            "11763",
            false,
            None,
            None,
        );
        app.on_backend(BackendEvent::Phase("streaming provider…".to_owned()));
        app.on_backend(BackendEvent::Progress(0.42, "gerando descrição".to_owned()));
        app.on_backend(BackendEvent::Token(
            "# Atualiza checkout\n\n## Descrição\n".to_owned(),
        ));
        app.on_tick();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_generating_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_publishing_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.phase = Phase::Publishing;
        app.phase_label = "publicando…".to_owned();
        app.progress = 0.55;
        app.progress_label = "criando PR dev".to_owned();
        app.logs.push_back("criando PR dev".to_owned());
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_publishing_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_done_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.on_backend(BackendEvent::Published(vec![
            crate::azure::pull_requests::PublishedPr {
                target: "dev".to_owned(),
                id: 42,
                url: "https://dev.azure.com/example/pr/42".to_owned(),
            },
        ]));
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_done_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn done_screen_should_render_published_ids_targets_urls_and_test_action() {
        let mut app = review_app();
        app.set_launch_prep(launch_prep());
        app.on_backend(BackendEvent::Published(vec![
            published(42, "dev"),
            published(43, "sprint/12"),
        ]));
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|f| f.render_widget(&app, f.area()))
            .expect("render");
        let rendered = buffer_text(&terminal);
        assert!(rendered.contains("PR #42"));
        assert!(rendered.contains("dev"));
        assert!(rendered.contains("pullrequest/42"));
        assert!(rendered.contains("PR #43"));
        assert!(rendered.contains("sprint/12"));
        assert!(rendered.contains("pullrequest/43"));
        assert!(rendered.contains("t preparar Test Case"));

        let mut t = test_terminal();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut needs_draw = false;
        let outcome = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('T'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut t,
            &mut needs_draw,
        )
        .expect("tecla T");
        assert!(outcome.is_none());
        assert!(matches!(
            app.publish_dialog,
            Some(PublishDialog::PublishedPrPicker { .. })
        ));

        let mut needs_draw = false;
        let outcome = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut t,
            &mut needs_draw,
        )
        .expect("cancelamento pelo q");
        assert!(outcome.is_none());
        assert!(app.publish_dialog.is_none());

        let outcome = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut t,
            &mut needs_draw,
        )
        .expect("saída do Done");
        assert!(matches!(outcome, Some(LiveOutcome::Done { .. })));
    }

    #[test]
    fn partial_publication_should_not_offer_test_case_handoff() {
        let mut app = review_app();
        app.on_backend(BackendEvent::PublishedOne(published(42, "dev")));
        app.on_backend(BackendEvent::PublishFailed(
            crate::features::describe::PublishFailure {
                message: "target sprint/12: resposta incerta".to_owned(),
                kind: crate::features::describe::PublishFailureKind::OutcomeUnknown,
                target: Some("sprint/12".to_owned()),
            },
        ));
        assert!(app.publish_failure.is_some());
        assert_eq!(app.published[0].id, 42);
        assert_eq!(app.published[0].target, "dev");
        assert_eq!(app.published[0].url, published(42, "dev").url);
        let mut terminal = test_terminal();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut needs_draw = false;
        let outcome = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut terminal,
            &mut needs_draw,
        )
        .expect("tecla t");
        assert!(outcome.is_none());
        assert!(matches!(
            app.publish_dialog,
            Some(PublishDialog::PublishRecovery(_))
        ));

        let mut error = review_app();
        error.on_backend(BackendEvent::PublishedOne(published(42, "dev")));
        error.on_backend(BackendEvent::Failed("falha final".to_owned()));
        assert_eq!(error.phase, Phase::Error);
        assert_eq!(error.published[0].id, 42);
        assert_eq!(error.published[0].target, "dev");
        assert_eq!(error.published[0].url, published(42, "dev").url);
        let mut terminal = test_terminal();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut needs_draw = false;
        let outcome = handle_key_event(
            &mut error,
            KeyEvent::new(KeyCode::Char('T'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut terminal,
            &mut needs_draw,
        )
        .expect("tecla T após erro");
        assert!(outcome.is_none());
        assert!(rx.try_recv().is_err());
        assert_eq!(error.phase, Phase::Error);
        assert!(error.publish_dialog.is_none());
    }

    #[test]
    fn single_published_pr_should_take_test_case_fast_path() {
        let mut app = review_app();
        app.set_launch_prep(launch_prep());
        app.on_backend(BackendEvent::Published(vec![published(42, "dev")]));
        let mut terminal = test_terminal();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut needs_draw = false;
        let outcome = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut terminal,
            &mut needs_draw,
        )
        .expect("tecla t");
        match outcome {
            Some(LiveOutcome::PrepareTestCase {
                launch_context,
                published,
                ..
            }) => {
                assert_eq!(launch_context.published_pr.id, 42);
                assert_eq!(launch_context.published_pr.target, "dev");
                assert_eq!(
                    launch_context.published_pr.url,
                    "https://dev.azure.com/org/project/_git/repo/pullrequest/42"
                );
                assert_eq!(published.len(), 1);
                assert!(app.publish_dialog.is_none());
            }
            _ => panic!("PR único não seguiu o fast path"),
        }
    }

    #[test]
    fn published_pr_picker_should_select_one_pr_in_publication_order() {
        let mut app = review_app();
        app.set_launch_prep(launch_prep());
        app.on_backend(BackendEvent::Published(vec![
            published(42, "sprint/12"),
            published(43, "dev"),
        ]));
        assert!(app.open_published_pr_picker());
        assert_eq!(
            app.publish_dialog,
            Some(PublishDialog::PublishedPrPicker { selected: 0 })
        );
        assert!(app.move_published_pr_picker(true));
        assert_eq!(
            app.publish_dialog,
            Some(PublishDialog::PublishedPrPicker { selected: 1 })
        );

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|f| f.render_widget(&app, f.area()))
            .expect("render picker");
        let rendered = buffer_text(&terminal);
        assert!(rendered.contains("PR #42  sprint/12"));
        assert!(rendered.contains(&published(42, "sprint/12").url));
        assert!(rendered.contains("PR #43  dev"));
        assert!(rendered.contains(&published(43, "dev").url));
        assert_eq!(app.published.len(), 2);
        assert!(app.move_published_pr_picker(true));
        assert_eq!(
            app.publish_dialog,
            Some(PublishDialog::PublishedPrPicker { selected: 0 })
        );
        assert!(app.move_published_pr_picker(false));
        assert_eq!(
            app.publish_dialog,
            Some(PublishDialog::PublishedPrPicker { selected: 1 })
        );

        let mut handler_terminal = test_terminal();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut needs_draw = false;
        let outcome = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut handler_terminal,
            &mut needs_draw,
        )
        .expect("seleção do PR");
        match outcome {
            Some(LiveOutcome::PrepareTestCase {
                launch_context,
                published,
                ..
            }) => {
                assert_eq!(launch_context.published_pr.id, 43);
                assert_eq!(
                    published.iter().map(|item| item.id).collect::<Vec<_>>(),
                    [42, 43]
                );
            }
            _ => panic!("picker não entregou um único PR selecionado"),
        }
    }

    #[test]
    fn one_handoff_activation_should_prepare_one_test_case_for_multiple_targets() {
        let mut app = review_app();
        app.set_launch_prep(launch_prep());
        app.on_backend(BackendEvent::Published(vec![
            published(42, "sprint/12"),
            published(43, "dev"),
        ]));
        let mut terminal = test_terminal();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut needs_draw = false;
        let open = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut terminal,
            &mut needs_draw,
        )
        .expect("ativação do handoff");
        assert!(open.is_none());
        assert!(matches!(
            app.publish_dialog,
            Some(PublishDialog::PublishedPrPicker { selected: 0 })
        ));

        let selected = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut terminal,
            &mut needs_draw,
        )
        .expect("seleção única do handoff");
        let mut preparation_count = 0;
        match selected {
            Some(LiveOutcome::PrepareTestCase {
                launch_context,
                published,
                ..
            }) => {
                preparation_count += 1;
                assert_eq!(launch_context.published_pr.id, 42);
                assert_eq!(published.len(), 2);
            }
            _ => panic!("ativação multi-target não entregou o handoff"),
        }
        assert_eq!(preparation_count, 1);
        assert!(app.publish_dialog.is_none());
    }

    #[test]
    fn published_pr_picker_cancel_should_return_without_handoff() {
        let mut app = review_app();
        app.set_launch_prep(launch_prep());
        app.on_backend(BackendEvent::Published(vec![
            published(42, "sprint/12"),
            published(43, "dev"),
        ]));
        app.open_published_pr_picker();
        let mut terminal = test_terminal();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut needs_draw = false;
        let outcome = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut terminal,
            &mut needs_draw,
        )
        .expect("cancelamento do picker");
        assert!(outcome.is_none());
        assert_eq!(app.phase, Phase::Done);
        assert!(app.publish_dialog.is_none());
        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.published.iter().map(|item| item.id).collect::<Vec<_>>(),
            [42, 43]
        );

        app.open_published_pr_picker();
        let mut no_side_effects = mpsc::unbounded_channel();
        let mut needs_draw = false;
        let outcome = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &no_side_effects.0,
            &mut terminal,
            &mut needs_draw,
        )
        .expect("q no picker");
        assert!(outcome.is_none());
        assert!(no_side_effects.1.try_recv().is_err());
        assert!(app.publish_dialog.is_none());

        let mut needs_draw = false;
        let exit = handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            &PublishBase {
                pat: String::new(),
                remote: None,
                branch: String::new(),
                work_item_id: String::new(),
            },
            &tx,
            &mut terminal,
            &mut needs_draw,
        )
        .expect("saída do Done");
        assert!(matches!(exit, Some(LiveOutcome::Done { .. })));
    }

    #[test]
    fn desc_error_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.phase = Phase::Publishing;
        app.phase_label = "publicando…".to_owned();
        app.progress = 0.35;
        app.progress_label = "criando PR dev".to_owned();
        app.on_backend(BackendEvent::Failed("Azure DevOps indisponível".to_owned()));
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_error_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_too_small_50x15() -> anyhow::Result<()> {
        let app = review_app();
        let backend = TestBackend::new(50, 15);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_too_small_50x15", terminal.backend());
        Ok(())
    }

    #[test]
    fn leaving_done_should_return_published_prs() {
        let mut app = review_app();
        app.on_backend(BackendEvent::Published(vec![
            crate::azure::pull_requests::PublishedPr {
                target: "dev".to_owned(),
                id: 42,
                url: "https://dev.azure.com/example/pr/42".to_owned(),
            },
        ]));
        let outcome = quit_outcome(&app);

        match outcome {
            LiveOutcome::Done { published, .. } => {
                assert_eq!(published.len(), 1);
                assert_eq!(published[0].url, "https://dev.azure.com/example/pr/42");
            }
            LiveOutcome::Aborted | LiveOutcome::Failed(_) | LiveOutcome::PrepareTestCase { .. } => {
                panic!("PR publicado não pode sair como cancelamento")
            }
        }
    }

    #[test]
    fn leaving_after_incomplete_publish_should_return_failure() {
        let mut app = review_app();
        app.on_backend(BackendEvent::PublishedOne(PublishedPr {
            target: "dev".to_owned(),
            id: 42,
            url: "https://dev.azure.com/example/pr/42".to_owned(),
        }));
        app.on_backend(BackendEvent::PublishFailed(
            crate::features::describe::PublishFailure {
                message: "target sprint/12: resposta incerta".to_owned(),
                kind: crate::features::describe::PublishFailureKind::OutcomeUnknown,
                target: Some("sprint/12".to_owned()),
            },
        ));

        match quit_outcome(&app) {
            LiveOutcome::Failed(message) => {
                assert!(message.contains("publicação incompleta"));
                assert!(message.contains("preservados"));
            }
            LiveOutcome::Done { .. }
            | LiveOutcome::Aborted
            | LiveOutcome::PrepareTestCase { .. } => {
                panic!("publicação incompleta não pode sair como sucesso")
            }
        }
    }

    #[test]
    fn desc_reviewers_dialog_100x30() -> anyhow::Result<()> {
        let mut app = review_app();
        app.open_reviewers();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_reviewers_dialog_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_confirm_publish_100x30() -> anyhow::Result<()> {
        use crate::tui::describe_app::PublishDialog;
        let mut app = review_app();
        app.open_reviewers();
        app.publish_dialog = Some(PublishDialog::ConfirmPublish(false));
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!("desc_confirm_publish_100x30", terminal.backend());
        Ok(())
    }

    #[test]
    fn desc_confirm_publish_multiple_targets_should_use_one_line_per_reviewer() -> anyhow::Result<()>
    {
        use crate::tui::describe_app::PublishDialog;
        let mut app = review_app();
        app.targets = vec!["sprint/110".to_owned(), "dev".to_owned()];
        app.reviewers = vec![
            "iohan.hinokuma@ibssystemico.org.br".to_owned(),
            "ronaldo.pereira@ibssystemico.com.br".to_owned(),
        ];
        app.publish_dialog = Some(PublishDialog::ConfirmPublish(true));
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| f.render_widget(&app, f.area()))?;
        insta::assert_snapshot!(
            "desc_confirm_publish_multiple_targets_100x30",
            terminal.backend()
        );
        Ok(())
    }
}
