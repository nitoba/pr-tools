//! Loop vivo da TUI `desc` — cada evento do backend redesenha a tela.
//!
//! Arquitetura: tarefa tokio gera (stream real `aisdk` quando provider é
//! `openai-compatible`, senão fallback com typing simulado) e envia
//! [`BackendEvent`] por `mpsc`; a thread principal consome crossterm +
//! backend a 30fps e desenha [`DescribeApp`] via `Widget for &App`.

use std::io::{IsTerminal, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Tabs, Widget, Wrap,
    },
};
use tokio::sync::mpsc;

use super::describe_app::{DescribeApp, Phase, PublishDialog, PublishSetup};
use super::events::{BackendEvent, LiveOutcome};
use super::shimmer::{f64_from_usize, percent_u16};
use super::{app_layout, border_type, centered_buttons, modal_frame, theme};
use crate::ai;
use crate::azure::AzureClient;
use crate::azure::pull_requests::publish_pull_requests;
use crate::config::Config;
use crate::features::describe::DescribePrep;
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
    /// Targets.
    targets: Vec<String>,
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
    tx: mpsc::UnboundedSender<BackendEvent>,
) {
    let _ = tx.send(BackendEvent::Phase("publicando…".to_owned()));
    let Some(remote) = &base.remote else {
        let _ = tx.send(BackendEvent::Failed(
            "remote Azure DevOps não encontrado.".to_owned(),
        ));
        return;
    };
    let client = AzureClient::new(&remote.organization, &base.pat);
    let _ = tx.send(BackendEvent::Progress(
        0.1,
        "resolvendo repositório…".to_owned(),
    ));
    let work_items: Vec<String> = if base.work_item_id.trim().is_empty() {
        Vec::new()
    } else {
        vec![base.work_item_id.trim().to_owned()]
    };
    let targets = base.targets.clone();
    let total = f64_from_usize(targets.len().max(1));
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
    };
    let result = publish_pull_requests(&client, &input).await;
    match result {
        Ok(published) => {
            for (i, item) in published.iter().enumerate() {
                let _ = tx.send(BackendEvent::Progress(
                    0.1 + 0.9 * (f64_from_usize(i + 1) / total),
                    format!("PR {} criado", item.target),
                ));
                let _ = tx.send(BackendEvent::Log(format!(
                    "PR {} criado: {}",
                    item.target, item.url
                )));
            }
            let _ = tx.send(BackendEvent::Published(published));
        }
        Err(e) => {
            let _ = tx.send(BackendEvent::Failed(format!("falha ao publicar PRs: {e}")));
        }
    }
}

/// Inicia a publicação: fecha o diálogo, marca a fase e dispara a task.
fn start_publish(
    app: &mut DescribeApp,
    base: &PublishBase,
    tx: &mpsc::UnboundedSender<BackendEvent>,
    desc: &crate::ai::PrDescription,
) {
    app.commit_reviewer();
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
    app.publish_dialog = None;
    app.phase = Phase::Publishing;
    "publicando…".clone_into(&mut app.phase_label);
    app.progress = 0.0;
    "criando PRs…".clone_into(&mut app.progress_label);
    let base = base.clone();
    let title = desc.title.clone();
    let body = desc.body.clone();
    let tx = tx.clone();
    tokio::spawn(publish_task(base, title, body, reviewers, tx));
}

/// Desenha um frame completo a partir do estado — reage a cada token/log.
impl Widget for &DescribeApp {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Piso honesto: terminal miúdo não tenta layout normal.
        if area.width < 60 || area.height < 20 {
            render_too_small(area, buf);
            return;
        }
        let [head, body, foot] = app_layout(area);
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
    let msg = format!(
        "terminal muito pequeno — mínimo 60×20 (atual {}×{})",
        area.width, area.height
    );
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
        .title(Span::styled(" ◆ prt ", theme().warning))
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
    let phase_txt = match app.phase {
        Phase::Boot => "boot",
        Phase::Generating => "gerando",
        Phase::Review => "revisão",
        Phase::Publishing => "publicando",
        Phase::Done => "ok",
        Phase::Error => "erro",
    };
    // Cor do spinner: pulsa com trabalho, fixa no ocioso.
    let active = matches!(
        app.phase,
        Phase::Generating | Phase::Publishing | Phase::Boot
    );
    let pulse = if active {
        if app.tick % 2 == 0 {
            theme().accent
        } else {
            theme().app_title
        }
    } else {
        theme().accent
    };
    let phase_line = if active {
        super::shimmer::shimmer_text(&app.phase_label, app.tick, 32)
    } else {
        Line::from(Span::styled(app.phase_label.clone(), theme().accent))
    };
    let mut spans = vec![
        Span::styled(format!("{} ", app.spinner()), pulse),
        Span::styled("◆ prt ", theme().app_title),
        Span::styled(crate::cli::VERSION, theme().muted),
        Span::styled(
            format!(
                "  ·  desc  ·  {}  ·  {}s  ·  ~{} tok",
                phase_txt,
                app.elapsed_secs(),
                app.token_count()
            ),
            theme().muted,
        ),
        Span::styled("  ·  ", theme().muted),
    ];
    spans.extend(phase_line.spans);
    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_body(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    // Linha de abas (targets) + colunas com respiro de 1 célula entre painéis.
    // Tabs intactas (outro dono); só as colunas colapsam no modo estreito.
    let [tabs_area, cols] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    if !app.targets.is_empty() {
        let titles: Vec<&str> = app.targets.iter().map(String::as_str).collect();
        Tabs::new(titles)
            .style(theme().tabs)
            .highlight_style(theme().tabs_selected)
            .select(app.selected_target)
            .divider(" │ ")
            .render(tabs_area, buf);
    }
    // Fallback estreito 60 <= w < 100: 1 coluna (preview Min + strip 3 linhas).
    if cols.width < 100 {
        let strip_h = 3.min(cols.height);
        let preview_h = cols.height.saturating_sub(strip_h);
        let preview_area = Rect {
            x: cols.x,
            y: cols.y,
            width: cols.width,
            height: preview_h,
        };
        let strip_area = Rect {
            x: cols.x,
            y: cols.y.saturating_add(preview_h),
            width: cols.width,
            height: strip_h,
        };
        if preview_area.height > 0 && preview_area.width > 0 {
            render_preview(app, preview_area, buf);
        }
        render_narrow_strip(app, strip_area, buf);
        return;
    }
    let [left, _gap, right] = Layout::horizontal([
        Constraint::Percentage(58),
        Constraint::Length(1),
        Constraint::Percentage(42),
    ])
    .areas(cols);
    render_preview(app, left, buf);
    render_side(app, right, buf);
}

fn render_narrow_strip(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    // Strip de 3 linhas p/ largura estreita: barra shimmer + phase + resumo ctx.
    if area.width == 0 || area.height == 0 {
        return;
    }
    let generating = matches!(
        app.phase,
        Phase::Boot | Phase::Generating | Phase::Publishing
    );
    let bar_width = area.width as usize;
    let bar = super::shimmer::shimmer_bar(app.progress, bar_width, app.tick, generating);
    let pct = percent_u16(app.progress);
    let phase_line = if generating {
        super::shimmer::shimmer_text(&app.phase_label, app.tick, 24)
    } else {
        Line::from(Span::styled(app.phase_label.clone(), theme().success))
    };
    let mut phase_spans = phase_line.spans.clone();
    phase_spans.push(Span::styled(format!("  {pct}%"), theme().muted));
    let targets = if app.targets.is_empty() {
        "—".to_owned()
    } else {
        app.targets.join(", ")
    };
    let nchars = app
        .desc
        .as_ref()
        .map_or(app.streamed_raw.len(), |d| d.body.len());
    let flag = if app
        .desc
        .as_ref()
        .is_some_and(|d| ai::is_within_limit(&d.body))
    {
        "✓ <4000"
    } else if app.desc.is_some() {
        "✘ ≥4000"
    } else {
        ""
    };
    let resumo = format!("{} · {} · {nchars} chars {flag}", app.branch, targets);
    if area.height >= 1 {
        let r0 = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        Paragraph::new(bar).render(r0, buf);
    }
    if area.height >= 2 {
        let r1 = Rect {
            x: area.x,
            y: area.y.saturating_add(1),
            width: area.width,
            height: 1,
        };
        Paragraph::new(Line::from(phase_spans)).render(r1, buf);
    }
    if area.height >= 3 {
        let r2 = Rect {
            x: area.x,
            y: area.y.saturating_add(2),
            width: area.width,
            height: 1,
        };
        Paragraph::new(resumo).style(theme().muted).render(r2, buf);
    }
}

fn render_preview(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let (title, is_live) = if app.desc.is_some() {
        (" ◉ Descrição do PR ", false)
    } else {
        (" ◌ Streaming… ", true)
    };
    let block = Block::default()
        .title(Span::styled(
            title,
            if is_live {
                theme().warning
            } else {
                theme().success
            },
        ))
        .borders(Borders::ALL)
        .border_style(if is_live {
            theme().warning
        } else {
            theme().border
        })
        .border_type(border_type())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    // Texto final com highlight Markdown; streaming mostra raw + cursor.
    if let Some(d) = &app.desc {
        use super::markdown::{markdown_text, title_line};
        use ratatui::text::Text;
        let mut lines: Vec<Line> = vec![title_line(&d.title), Line::from("")];
        lines.extend(markdown_text(&d.body).lines);
        let total = lines.len();
        let text = Text::from(lines);
        Paragraph::new(text)
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
        let text = if app.streamed_raw.is_empty() {
            "aguardando primeiro token…".to_owned()
        } else {
            format!("{}▊", app.streamed_raw)
        };
        let lines = text.lines().count();
        Paragraph::new(text)
            .style(Style::new())
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0))
            .render(inner, buf);
        // Scrollbar reage ao scroll (j/k).
        let mut state = ScrollbarState::new(lines.max(1)).position(app.scroll as usize);
        <Scrollbar as ratatui::widgets::StatefulWidget>::render(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            inner,
            buf,
            &mut state,
        );
    }
}

fn render_side(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    // Painéis bem separados: 1 célula de respiro entre cada bloco.
    let [ctx, _g1, logs, _g2, stats] = Layout::vertical([
        Constraint::Length(7),
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
        Constraint::Length(5),
    ])
    .areas(area);
    render_context(app, ctx, buf);
    render_logs(app, logs, buf);
    render_stats(app, stats, buf);
}

fn render_context(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let body = format!(
        "branch  {}\nwork    #{}\ntargets  {}\nbody    {} chars {}",
        app.branch,
        if app.work_item_id.is_empty() {
            "—"
        } else {
            &app.work_item_id
        },
        if app.targets.is_empty() {
            "—".to_owned()
        } else {
            app.targets.join(", ")
        },
        app.desc
            .as_ref()
            .map_or(app.streamed_raw.len(), |d| d.body.len()),
        if app
            .desc
            .as_ref()
            .is_some_and(|d| ai::is_within_limit(&d.body))
        {
            "✓ <4000"
        } else if app.desc.is_some() {
            "✘ ≥4000"
        } else {
            ""
        }
    );
    Paragraph::new(body)
        .block(
            Block::default()
                .title(Span::styled(
                    " ◈ Contexto ",
                    theme().app_title.add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(theme().app_title)
                .border_type(border_type())
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .render(area, buf);
}

fn render_logs(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    // Últimas N linhas cabem na altura — auto-scroll para o fim.
    let height = area.height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = app
        .logs
        .iter()
        .rev()
        .take(height.max(1))
        .rev()
        .map(|l| ListItem::new(Line::from(Span::styled(l.clone(), theme().muted))))
        .collect();
    List::new(items)
        .block(
            Block::default()
                .title(Span::styled(" ≡ Log vivo ", theme().muted))
                .borders(Borders::ALL)
                .border_style(theme().muted)
                .border_type(border_type())
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .render(area, buf);
}

fn render_stats(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    // Barra custom com shimmer enquanto gera; sólida quando pronto.
    let generating = matches!(
        app.phase,
        Phase::Boot | Phase::Generating | Phase::Publishing
    );
    let bar_width = area.width.saturating_sub(4) as usize;
    let bar = super::shimmer::shimmer_bar(app.progress, bar_width, app.tick, generating);
    let label = if generating {
        super::shimmer::shimmer_text(&app.progress_label, app.tick, 24)
    } else {
        Line::from(Span::styled(app.progress_label.clone(), theme().success))
    };
    let pct = percent_u16(app.progress);
    let block = Block::default()
        .title(Span::styled(
            format!(" ⚡ Progresso {pct}% "),
            if generating {
                theme().accent.add_modifier(Modifier::BOLD)
            } else {
                theme().success
            },
        ))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(if generating {
            theme().accent
        } else {
            theme().success
        })
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    let [b, l] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);
    Paragraph::new(bar).render(b, buf);
    Paragraph::new(label).render(l, buf);
}

fn render_footer(app: &DescribeApp, area: Rect, buf: &mut Buffer) {
    let hints = match &app.publish_dialog {
        Some(PublishDialog::ConfirmCreate(_) | PublishDialog::ConfirmPublish(_)) => {
            "←/→ alternar · y sim · n não · enter confirmar · esc voltar"
        }
        Some(PublishDialog::Reviewers) => {
            "digite o reviewer · tab/↓↑ trocar campo · enter avançar · esc voltar"
        }
        None => match app.phase {
            Phase::Review => {
                "enter publicar · c copiar · tab target · j/k scroll · ? ajuda · q sair"
            }
            Phase::Done => "q sair",
            Phase::Error => "r tenta de novo · q sair · ? ajuda",
            _ => "j/k scroll · tab target · ? ajuda · q sair",
        },
    };
    let mut spans = vec![Span::styled(hints, theme().muted)];
    if app.is_copied_flash() {
        spans.push(Span::styled("   ✓ copiado!", theme().success));
    }
    if !app.published_urls.is_empty() {
        spans.push(Span::styled(
            format!("   ✓ {} PR(s) publicado(s)", app.published_urls.len()),
            theme().success,
        ));
    }
    // Barra de erro em destaque quando falha.
    if let Phase::Error = app.phase {
        if let Some(e) = &app.error {
            spans.push(Span::styled(format!("   ✘ {e}"), theme().error));
        }
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_help(area: Rect, buf: &mut Buffer) {
    // 15 linhas de conteúdo, altura exata.
    let inner = modal_frame(area, buf, " Ajuda ", theme().accent, 62, 15);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    Paragraph::new(vec![
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
        Line::from("r — tentar de novo (só na tela de erro)"),
        Line::from(""),
        Line::from(Span::styled(
            "enter confirma · esc sempre volta um nível",
            theme().muted,
        )),
        Line::from(""),
        Line::from(Span::styled("? fecha esta ajuda", theme().muted)),
    ])
    .wrap(Wrap { trim: false })
    .render(inner, buf);
}

/// Diálogos modais do fluxo de publicação.
fn render_publish_dialog(app: &DescribeApp, dialog: PublishDialog, area: Rect, buf: &mut Buffer) {
    match dialog {
        PublishDialog::ConfirmCreate(yes) => render_confirm_create(app, yes, area, buf),
        PublishDialog::Reviewers => render_reviewers_dialog(app, area, buf),
        PublishDialog::ConfirmPublish(yes) => render_confirm_publish(app, yes, area, buf),
    }
}

fn render_confirm_create(app: &DescribeApp, yes: bool, area: Rect, buf: &mut Buffer) {
    // 7 linhas de conteúdo: pergunta, respiro, targets, respiro, botões,
    // respiro, dicas.
    let inner = modal_frame(area, buf, " Publicar ", theme().accent, 62, 7);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let targets = if app.targets.is_empty() {
        "—".to_owned()
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
            "←/→ alternar · y sim · n não · enter confirmar · esc voltar",
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
            "opcional; vazio mantém o padrão · tab/↓↑ troca de campo",
            theme().muted,
        )),
        Line::from(""),
    ];
    for (i, target) in app.targets.iter().enumerate() {
        let focused = i == app.reviewer_idx;
        lines.push(Line::from(vec![
            Span::styled(
                if focused { "▸ " } else { "  " },
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
        let mut row = vec![Span::styled("│ ", theme().muted)];
        if focused {
            row.extend(editor_spans(&app.reviewer_edit, app.reviewer_cursor));
        } else if value.trim().is_empty() {
            row.push(Span::styled("(padrão)", theme().muted));
        } else {
            row.push(Span::styled(value, Style::new()));
        }
        lines.push(Line::from(row));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "enter avançar · esc voltar",
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
        Line::from(Span::styled(app.reviewer_summary(), Style::new())),
        Line::from(""),
        centered_buttons(yes, inner.width),
        Line::from(""),
        Line::from(Span::styled(
            "←/→ alternar · y sim · n não · enter confirmar · esc voltar",
            theme().muted,
        )),
    ];
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
        targets: prep.targets.clone(),
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
                    start_publish(app, publish_base, tx, &desc);
                }
            } else {
                app.publish_dialog = None;
            }
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
                start_publish(app, publish_base, tx, &desc);
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
    match app.desc.clone() {
        Some(desc) if matches!(app.phase, Phase::Review | Phase::Done) => LiveOutcome::Done {
            desc,
            published: app.published.clone(),
        },
        _ => LiveOutcome::Aborted,
    }
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
        (KeyCode::Char('q') | KeyCode::Esc, _) => {
            if app.publish_dialog.is_some() {
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

    let tick_rate = Duration::from_millis(33); // ~30fps p/ spinner/shimmer suaves
    let mut last_tick = std::time::Instant::now();
    // Dirty-flag: desenha só se tick/backend/input sujaram a tela.
    let mut needs_draw = true;

    loop {
        // 1. Drena eventos do backend sem bloquear; suja se houve dado.
        if drain_backend(&mut rx, &mut app) {
            needs_draw = true;
        }
        // 2. Tick de animação (spinner/shimmer continua e suja a tela).
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
}
