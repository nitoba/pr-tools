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
    Ok(acc)
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
    if !prep.work_item_id.is_empty() {
        let _ = tx.send(BackendEvent::Log(format!(
            "work item #{}",
            prep.work_item_id
        )));
    }
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
    let base = base.clone();
    let title = desc.title.clone();
    let body = desc.body.clone();
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
    let context_height = if show_context { 3 } else { 0 };
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
                Span::styled(format!("  {}  ", item.target), branch_style()),
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
                Span::styled(format!("  {}  ", item.target), branch_style()),
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
    Paragraph::new(Line::from(context))
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
    let hints = match &app.publish_dialog {
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
        None => match app.phase {
            Phase::Review => {
                if ascii_only() {
                    "enter publicar - c copiar - tab target - j/k scroll - ? ajuda - q sair"
                } else {
                    "enter publicar · c copiar · tab target · j/k scroll · ? ajuda · q sair"
                }
            }
            Phase::Done => "q sair",
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
            Line::from("c - copiar body - ? - alternar ajuda - q/esc - sair"),
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
            Line::from("c — copiar body · ? — alternar ajuda · q/esc — sair"),
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
        PublishDialog::ConfirmCreate(yes) => render_confirm_create(app, yes, area, buf),
        PublishDialog::Reviewers => render_reviewers_dialog(app, area, buf),
        PublishDialog::ConfirmPublish(yes) => render_confirm_publish(app, yes, area, buf),
        PublishDialog::PublishRecovery(selected) => {
            render_publish_recovery(app, selected, area, buf);
        }
        PublishDialog::CandidateList { selected } => {
            render_candidate_list(app, selected, area, buf);
        }
    }
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

/// Navegação/scroll (`j/k`, setas, `tab`) — retorna `true` se consumiu a tecla.
fn on_nav_key(app: &mut DescribeApp, key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::KeyCode;
    match (key.code, app.publish_dialog) {
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

/// Cópia do body (`c`) — fora do diálogo ou digitando no editor.
fn on_copy_key(app: &mut DescribeApp, key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::KeyCode;
    if key.code != KeyCode::Char('c') {
        return false;
    }
    if app.publish_dialog.is_none() {
        if let Some(d) = &app.desc {
            if crate::features::describe::copy_to_clipboard(&d.body) {
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
    let Some(desc) = app.desc.clone() else {
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
    tokio::spawn(backend_find_candidates(base, desc.title, target, tx));
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

fn run_loop(
    terminal: &mut DefaultTerminal,
    prep: DescribePrep,
    create_initial: bool,
) -> anyhow::Result<LiveOutcome> {
    let (tx, mut rx) = mpsc::unbounded_channel::<BackendEvent>();
    let (publish_setup, publish_blocked, publish_base) = make_publish_parts(&prep);
    let mut app = DescribeApp::new(
        &prep.context.branch,
        &prep.targets,
        &prep.work_item_id,
        create_initial,
        publish_setup,
        publish_blocked,
    );
    // Backend roda em paralelo e empurra tokens/logs (`tx` fica no loop
    // para a task de publicação criada sob demanda).
    tokio::spawn(backend_task(prep, tx.clone()));

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
            if let Event::Key(key) = event::read()? {
                // Filtro de kind: Release sempre ignorado; Repeat só p/ scroll.
                let eh_scroll = matches!(
                    key.code,
                    KeyCode::Char('j' | 'k') | KeyCode::Up | KeyCode::Down
                );
                if key.kind == KeyEventKind::Release
                    || (key.kind == KeyEventKind::Repeat && !eh_scroll)
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
    use crate::tui::describe_app::DescribeApp;
    use crate::tui::events::BackendEvent;
    use ratatui::{Terminal, backend::TestBackend};

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
            LiveOutcome::Aborted | LiveOutcome::Failed(_) => {
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
            LiveOutcome::Done { .. } | LiveOutcome::Aborted => {
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
