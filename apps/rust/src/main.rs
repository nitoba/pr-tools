//! Binário `prt` — `main.rs` mínimo, lógica no lib (`proj-lib-main-split`).
//!
//! Usa `tokio` + `tracing` + `anyhow::Context` no binário
//! (`err-anyhow-app`), `thiserror` no lib (`err-thiserror-lib`).

use anyhow::Context;
use clap::CommandFactory as _;
use prt::features::test_card::TestCardRequest;
use prt::tui::test_flow::{TestFlowOutcome, run_test_flow_request};
use std::future::Future;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    // Parse compatível com o Dart: default `desc`, `--help`/`--version` manuais.
    let raw: Vec<String> = std::env::args().collect();
    if raw.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", prt::cli::help_text());
        return;
    }
    if raw.iter().any(|a| a == "--version" || a == "-v") {
        let commit = option_env!("PRT_COMMIT").unwrap_or("unknown");
        println!("prt v{} ({commit})", prt::cli::VERSION);
        return;
    }

    let options = match prt::cli::parse_cli(raw).map_err(anyhow::Error::new) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("Error: {err:?}");
            std::process::exit(
                err.downcast_ref::<prt::error::AppError>()
                    .map_or(1, prt::error::AppError::exit_code),
            );
        }
    };

    // `completions` sai antes do dispatch: só escreve o script, sem Git/config.
    if let prt::cli::Command::Completions = options.command {
        if let Some(shell) = options.completion_shell {
            clap_complete::generate(
                shell,
                &mut prt::cli::Cli::command(),
                "prt",
                &mut std::io::stdout(),
            );
        }
        return;
    }

    let result = match options.command {
        prt::cli::Command::Desc => run_desc(&options).await,
        prt::cli::Command::Test => run_test(&options).await,
        prt::cli::Command::Init => run_init(&options).await,
        prt::cli::Command::Doctor => run_doctor(&options).await,
        prt::cli::Command::Update => prt::features::update::run()
            .await
            .map_err(anyhow::Error::new),
        // Inalcançável: `completions` retorna mais acima.
        prt::cli::Command::Completions => Ok(()),
    };
    if let Err(err) = result {
        eprintln!("Error: {err:?}");
        std::process::exit(
            err.downcast_ref::<prt::error::AppError>()
                .map_or(1, prt::error::AppError::exit_code),
        );
    }
}

async fn handle_desc_dry_run(
    options: &prt::cli::CliOptions,
    prep: &prt::features::describe::DescribePrep,
) -> anyhow::Result<bool> {
    use prt::tui::notice::{NoticeKind, section, show_notice};
    use ratatui::text::{Line, Span, Text};
    if !options.output.dry_run {
        return Ok(false);
    }
    let provider = prep
        .config
        .providers
        .first()
        .map_or("codex", String::as_str);
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    if !tty {
        println!(
            "PR · dry run\nprovider/model: {provider}\n\n--- sistema ---\n{}\n\n--- usuário ---\n{}",
            prep.config.template, prep.prompt
        );
        return Ok(true);
    }
    let mut lines = vec![
        Line::from(Span::styled(
            format!("provider/model: {provider}"),
            ratatui::style::Style::new(),
        )),
        Line::from(""),
    ];
    lines.extend(section("── sistema ──", &prep.config.template));
    lines.extend(section("── usuário ──", &prep.prompt));
    show_notice("PR · dry run", Text::from(lines), NoticeKind::Info).await?;
    Ok(true)
}

/// Caminho não-interativo de `desc` (`--raw` ou sem tty).
fn format_desc_plain_output(
    desc: &prt::ai::PrDescription,
    targets: &[String],
    work_item_id: &str,
    functional_context: &prt::features::describe::FunctionalContextStatus,
    raw: bool,
) -> String {
    if raw {
        return desc.body.clone();
    }
    format!(
        "Pull Request\n\nTítulo: {}\nTargets: {}\nWork Item: #{}\nContexto funcional: {}\n\n{}\n",
        desc.title,
        targets.join(", "),
        work_item_id,
        functional_context.display_label(),
        desc.body
    )
}

async fn run_desc_plain(
    options: &prt::cli::CliOptions,
    prep: &prt::features::describe::DescribePrep,
) -> anyhow::Result<()> {
    use prt::features::describe;
    if options.create {
        return Err(anyhow::Error::new(prt::error::AppError::cli(
            "criação requer terminal interativo: use sem --create ou em terminal",
        )));
    }
    eprintln!("Gerando descrição via IA…");
    let desc = describe::generate(prep).await.map_err(anyhow::Error::new)?;
    println!(
        "{}",
        format_desc_plain_output(
            &desc,
            &prep.targets,
            &prep.work_item_id,
            &prep.functional_context,
            options.output.raw,
        )
    );
    if options.output.copy && describe::copy_to_clipboard(&desc.body) {
        eprintln!("Descrição copiada para o clipboard.");
    }
    eprintln!("Concluído.");
    Ok(())
}

/// Linhas do resumo final com o mesmo highlight do preview.
fn build_done_lines(
    desc: &prt::ai::PrDescription,
    published: &[prt::azure::pull_requests::PublishedPr],
) -> Vec<ratatui::text::Line<'static>> {
    use prt::tui::markdown::{markdown_text, title_line};
    use ratatui::text::{Line, Span};
    let mut lines = vec![title_line(&desc.title), Line::from("")];
    lines.extend(markdown_text(&desc.body).lines);
    if !published.is_empty() {
        lines.push(Line::from(""));
        for item in published {
            lines.push(Line::from(vec![
                Span::styled("✓ ", ratatui::style::Style::new()),
                Span::styled(
                    format!("PR #{} · {} · {}", item.id, item.target, item.url),
                    ratatui::style::Style::new(),
                ),
            ]));
        }
    }
    lines
}

/// Receipt no scrollback após a notice fullscreen.
fn build_done_receipt(
    desc: &prt::ai::PrDescription,
    targets: &[String],
    published: &[prt::azure::pull_requests::PublishedPr],
) -> String {
    use std::fmt::Write as _;
    let title: String = desc.title.chars().take(100).collect();
    let n_chars = desc.body.chars().count();
    let mut receipt = format!(
        "✓ PR pronta: {title} · targets: {} · {n_chars} chars",
        targets.join(", ")
    );
    for item in published {
        let _ = write!(
            receipt,
            "\n✓ PR #{} · {} · {}",
            item.id, item.target, item.url
        );
    }
    receipt
}

async fn run_desc_handoff<F, Fut>(
    desc: prt::ai::PrDescription,
    targets: Vec<String>,
    receipt: String,
    launch_context: prt::features::test_card::TestCardLaunchContext,
    published: Vec<prt::azure::pull_requests::PublishedPr>,
    run_flow: F,
) -> (String, anyhow::Result<TestFlowOutcome>)
where
    F: FnOnce(TestCardRequest) -> Fut,
    Fut: Future<Output = anyhow::Result<TestFlowOutcome>>,
{
    let expected_receipt = build_done_receipt(&desc, &targets, &published);
    debug_assert_eq!(receipt, expected_receipt);
    let result = run_flow(TestCardRequest::PublishedPr(launch_context)).await;
    (receipt, result)
}

async fn run_desc(options: &prt::cli::CliOptions) -> anyhow::Result<()> {
    use prt::features::describe;
    use prt::tui::live::run_describe_tui;
    if options.pr.is_some() {
        return run_update(options).await;
    }
    if options.resume || options.session.is_some() {
        return run_desc_resume(options).await;
    }

    let prep = describe::prepare(options)
        .await
        .context("falha ao preparar contexto")?;
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    if options.output.dry_run || options.output.raw || !tty {
        if let Some(error) =
            describe::non_interactive_functional_context_error(&prep.functional_context)
        {
            return Err(anyhow::Error::new(error));
        }
    }
    if handle_desc_dry_run(options, &prep).await? {
        return Ok(());
    }
    // `--raw` ou saída não-tty: texto puro (script-friendly, sem tela).
    if !tty || options.output.raw {
        return run_desc_plain(options, &prep).await;
    }
    // TUI viva: streaming token a token, logs auto-scroll e barra com shimmer.
    // Clona antes: `prep` é movido para `run_describe_tui`.
    let targets = prep.targets.clone();
    let outcome = run_describe_tui(prep, options.create).await?;
    finish_desc_tui(options, targets, outcome).await
}

async fn run_desc_resume(options: &prt::cli::CliOptions) -> anyhow::Result<()> {
    use prt::config::config_paths;
    use prt::features::session::SessionStore;
    use uuid::Uuid;

    let paths = config_paths();
    let id = if options.resume {
        let sessions = SessionStore::list(&paths).map_err(anyhow::Error::new)?;
        if sessions.is_empty() {
            println!("Nenhuma sessão incompleta encontrada.");
            return Ok(());
        }
        if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            for session in sessions {
                let targets = session
                    .targets
                    .iter()
                    .map(|(target, state)| format!("{target}={state}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "{} · {} · {} · {}",
                    session.session_id, session.repository, session.source_branch, targets
                );
            }
            return Ok(());
        }
        let Some(id) = prt::tui::live::select_session_tui(&sessions)? else {
            return Ok(());
        };
        id
    } else {
        let value = options
            .session
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("UUID de sessão ausente"))?;
        Uuid::parse_str(value).map_err(|error| anyhow::anyhow!("UUID inválido: {error}"))?
    };
    let (store, snapshot) = SessionStore::open(&paths, id).map_err(anyhow::Error::new)?;
    if snapshot.is_complete() {
        anyhow::bail!("a sessão já está completa e não pode ser retomada");
    }
    let targets = snapshot
        .targets
        .iter()
        .map(|target| target.target.clone())
        .collect();
    let outcome = prt::tui::live::run_resumed_tui(store, snapshot).await?;
    finish_desc_tui(options, targets, outcome).await
}

async fn finish_desc_tui(
    options: &prt::cli::CliOptions,
    targets: Vec<String>,
    outcome: prt::tui::events::LiveOutcome,
) -> anyhow::Result<()> {
    use prt::features::describe;
    use prt::tui::events::LiveOutcome;
    use prt::tui::notice::{NoticeKind, show_notice};
    use ratatui::text::Text;
    match outcome {
        LiveOutcome::Done { desc, published } => {
            if options.output.copy {
                let _ = describe::copy_to_clipboard(&desc.body);
            }
            let lines = build_done_lines(&desc, &published);
            show_notice("Pull Request", Text::from(lines), NoticeKind::Success).await?;
            // Receipt no scrollback: a notice é fullscreen e o scrollback
            // fica vazio depois de `ratatui::restore()`. Não remove a notice.
            println!("{}", build_done_receipt(&desc, &targets, &published));
            Ok(())
        }
        LiveOutcome::PrepareTestCase {
            desc,
            launch_context,
            published,
        } => {
            let receipt = build_done_receipt(&desc, &targets, &published);
            let (receipt, test_result) = run_desc_handoff(
                desc,
                targets,
                receipt,
                launch_context,
                published,
                run_test_flow_request,
            )
            .await;
            match test_result {
                Ok(TestFlowOutcome::Created { id, url }) => {
                    println!("{receipt}");
                    println!("✓ Test Case #{id} criado: {url}");
                    Ok(())
                }
                Ok(TestFlowOutcome::ReviewedNoCreate | TestFlowOutcome::Reviewed) => {
                    println!("{receipt}");
                    println!("✓ Test Case revisado; nenhum PR publicado foi alterado.");
                    Ok(())
                }
                Ok(TestFlowOutcome::Aborted) => {
                    show_notice(
                        "PRs publicados preservados",
                        Text::from(receipt.clone()),
                        NoticeKind::Warning,
                    )
                    .await?;
                    println!("{receipt}");
                    std::process::exit(130);
                }
                Err(error) => {
                    show_notice(
                        "Preparação do Test Case falhou",
                        Text::from(format!("{error}\n\n{receipt}")),
                        NoticeKind::Warning,
                    )
                    .await?;
                    println!("{receipt}");
                    Err(error)
                }
            }
        }
        LiveOutcome::Aborted => {
            show_notice(
                "Cancelado",
                Text::from("Operação cancelada."),
                NoticeKind::Warning,
            )
            .await?;
            // `run_describe_tui` + `show_notice` já chamaram `ratatui::restore()`.
            std::process::exit(130);
        }
        LiveOutcome::Discarded => {
            println!("Sessão descartada.");
            Ok(())
        }
        LiveOutcome::Failed(msg) => anyhow::bail!("falha na tui: {msg}"),
    }
}

async fn run_update(options: &prt::cli::CliOptions) -> anyhow::Result<()> {
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    run_update_with_tty(options, tty).await
}

async fn run_update_with_tty(options: &prt::cli::CliOptions, tty: bool) -> anyhow::Result<()> {
    use prt::features::update_pull_request;

    prt::cli::ensure_update_execution_mode(tty, options.output.dry_run)
        .map_err(anyhow::Error::new)?;
    let prep = update_pull_request::prepare(options)
        .await
        .context("falha ao preparar atualização do PR")?;
    run_update_prepared(options, tty, prep).await
}

fn update_dry_run_text(prompt: &str) -> String {
    format!("PR existente · dry run\n\n{prompt}")
}

async fn run_update_prepared(
    options: &prt::cli::CliOptions,
    tty: bool,
    prep: prt::features::update_pull_request::UpdatePrep,
) -> anyhow::Result<()> {
    run_update_prepared_with(options, tty, prep, |text| println!("{text}")).await
}

async fn run_update_prepared_with<F>(
    options: &prt::cli::CliOptions,
    tty: bool,
    prep: prt::features::update_pull_request::UpdatePrep,
    emit: F,
) -> anyhow::Result<()>
where
    F: FnOnce(String),
{
    use prt::tui::notice::{NoticeKind, show_notice};
    use prt::tui::update_flow::{UpdateTuiOutcome, run_update_tui};
    use ratatui::text::Text;

    if options.output.dry_run {
        if tty {
            show_notice(
                "PR existente · dry run",
                Text::from(prep.prompt),
                NoticeKind::Info,
            )
            .await?;
        } else {
            emit(update_dry_run_text(&prep.prompt));
        }
        return Ok(());
    }

    match run_update_tui(&prep)? {
        UpdateTuiOutcome::Updated { id } => {
            show_notice(
                "PR atualizado",
                Text::from(format!(
                    "Título e descrição do PR #{id} foram confirmados pelo Azure DevOps."
                )),
                NoticeKind::Success,
            )
            .await?;
            println!("✓ PR #{id} atualizado e confirmado");
            Ok(())
        }
        UpdateTuiOutcome::NoOp { id } => {
            show_notice(
                "PR sem alterações",
                Text::from(format!(
                    "O PR #{id} já contém exatamente a proposta aprovada."
                )),
                NoticeKind::Info,
            )
            .await?;
            println!("✓ PR #{id} já estava atualizado");
            Ok(())
        }
        UpdateTuiOutcome::Aborted => {
            show_notice(
                "Cancelado",
                Text::from("Operação cancelada; o PR não foi alterado."),
                NoticeKind::Warning,
            )
            .await?;
            std::process::exit(130);
        }
        UpdateTuiOutcome::Failed(message) => anyhow::bail!("falha na atualização: {message}"),
    }
}

async fn run_test(options: &prt::cli::CliOptions) -> anyhow::Result<()> {
    use prt::tui::notice::{NoticeKind, show_notice};
    use prt::tui::test_flow::{TestFlowOutcome, run_test_flow};
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    if options.output.dry_run {
        let prep = prt::features::test_card::prepare(options)
            .await
            .map_err(anyhow::Error::new)?;
        if !tty {
            println!("Test Case · dry run\n\n--- prompt ---\n{}", prep.prompt);
            return Ok(());
        }
        show_notice(
            "Test Case · dry run",
            ratatui::text::Text::from(prep.prompt),
            NoticeKind::Info,
        )
        .await?;
        return Ok(());
    }
    // Sem tty: gera e imprime, sem criar (script-friendly).
    if !tty {
        if options.create {
            return Err(anyhow::Error::new(prt::error::AppError::cli(
                "criação requer terminal interativo: use sem --create ou em terminal",
            )));
        }
        eprintln!("Gerando card de teste via IA…");
        let prep = prt::features::test_card::prepare(options)
            .await
            .map_err(anyhow::Error::new)?;
        let desc = prt::features::test_card::generate(&prep)
            .await
            .map_err(anyhow::Error::new)?;
        println!(
            "Test Case\n\nTítulo: {}\nPai: #{}\n\n{}\n",
            desc.title, prep.parent.id, desc.body
        );
        if options.output.copy && prt::features::describe::copy_to_clipboard(&desc.body) {
            eprintln!("Card copiado para o clipboard.");
        }
        eprintln!("Concluído (sem criação fora de terminal).");
        return Ok(());
    }
    // TUI viva: prepara + gera em spawn, revisa settings e cria.
    match run_test_flow(options).await? {
        TestFlowOutcome::Created { id, url } => {
            println!("✓ Test Case #{id} criado: {url}");
            Ok(())
        }
        TestFlowOutcome::ReviewedNoCreate => {
            println!("✓ Card revisado (sem criação).");
            Ok(())
        }
        TestFlowOutcome::Reviewed | TestFlowOutcome::Aborted => {
            show_notice(
                "Cancelado",
                ratatui::text::Text::from("Operação cancelada."),
                NoticeKind::Warning,
            )
            .await?;
            // `run_test_flow` + `show_notice` já chamaram `ratatui::restore()`.
            std::process::exit(130);
        }
    }
}

async fn run_init(_options: &prt::cli::CliOptions) -> anyhow::Result<()> {
    use prt::tui::{
        init_wizard::{InitOutcome, run_init_wizard},
        notice::{NoticeKind, show_notice},
    };
    // Sem tty: garante arquivos mínimos preservando o existente (como no Dart).
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        let res = prt::features::init::ensure_defaults().map_err(anyhow::Error::new)?;
        eprintln!("Config salva em {}", res.config_file);
        return Ok(());
    }
    match run_init_wizard().await {
        // O wizard já mostrou a tela de sucesso — sem texto extra.
        Ok(InitOutcome::Saved(_)) => Ok(()),
        Ok(InitOutcome::Aborted) => {
            show_notice(
                "Cancelado",
                ratatui::text::Text::from("Operação cancelada."),
                NoticeKind::Warning,
            )
            .await?;
            // `run_init_wizard` + `show_notice` já chamaram `ratatui::restore()`.
            std::process::exit(130);
        }
        Err(e) => Err(e),
    }
}

async fn run_doctor(options: &prt::cli::CliOptions) -> anyhow::Result<()> {
    use prt::tui::doctor_flow::run_doctor_flow;
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        let report = prt::features::doctor::inspect(options.source.as_deref()).await;
        println!("Doctor");
        for check in &report.checks {
            let status = if check.ok {
                "OK"
            } else if check.warning {
                "AVISO"
            } else {
                "FALHA"
            };
            println!("[{status}] {}: {}", check.component, check.detail);
            if !check.ok && !check.fix.trim().is_empty() {
                println!("  correção: {}", check.fix);
            }
        }
        let (failures, warnings) = report.summary();
        println!("\nResumo: {failures} falha(s), {warnings} aviso(s).");
        let code = report.exit_code();
        if code != 0 {
            std::process::exit(code);
        }
        return Ok(());
    }
    let code = run_doctor_flow(options.source.as_deref()).await?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published_launch_context() -> prt::features::test_card::TestCardLaunchContext {
        let remote = prt::git::RepositoryRemote {
            organization: "org".to_owned(),
            project: "project".to_owned(),
            repository: "repo".to_owned(),
        };
        let parent: prt::azure::WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11763,
            "fields": {"System.Title": "Mudança", "System.WorkItemType": "Task"}
        }))
        .expect("pai");
        prt::features::test_card::TestCardLaunchContext {
            published_pr: prt::azure::pull_requests::PublishedPr {
                target: "dev".to_owned(),
                id: 42,
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/42".to_owned(),
            },
            remote,
            work_item_id: Some(11763),
            work_item: Some(parent),
            source_ref_name: "refs/heads/feature/11763-mudanca".to_owned(),
            target_ref_name: "refs/heads/dev".to_owned(),
            config: prt::config::Config {
                azure_pat: "pat".to_owned(),
                test_team: "DevOps".to_owned(),
                test_program: "Agrotrace".to_owned(),
                ..prt::config::Config::default()
            },
            settings: prt::features::test_card::TestSettings {
                area_path: String::new(),
                assigned_to: String::new(),
                iteration_path: String::new(),
                priority: 2.0,
                team: "DevOps".to_owned(),
                program: "Agrotrace".to_owned(),
            },
            fingerprint: prt::git::GitContextFingerprint::default(),
        }
    }

    fn update_prep() -> prt::features::update_pull_request::UpdatePrep {
        let remote = prt::git::RepositoryRemote {
            organization: "org".to_owned(),
            project: "project".to_owned(),
            repository: "repo".to_owned(),
        };
        let current = prt::azure::pull_requests::PullRequest {
            pull_request_id: 42,
            title: "Atual".to_owned(),
            description: "Body".to_owned(),
            source_ref_name: "refs/heads/feature/42".to_owned(),
            target_ref_name: "refs/heads/dev".to_owned(),
            status: "active".to_owned(),
            repository: prt::azure::pull_requests::PullRequestRepository {
                id: "repo-id".to_owned(),
                name: "repo".to_owned(),
                project: prt::azure::pull_requests::PullRequestProject {
                    name: "project".to_owned(),
                },
            },
        };
        prt::features::update_pull_request::UpdatePrep {
            config: prt::config::Config::default(),
            remote: remote.clone(),
            pr_id: 42,
            current,
            context: prt::git::ChangeContext {
                branch: "feature/42".to_owned(),
                source_ref: "refs/heads/feature/42".to_owned(),
                base_branch: "origin/dev".to_owned(),
                sprint_branch: String::new(),
                diff: "diff".to_owned(),
                diff_original_lines: 1,
                log: "log".to_owned(),
                work_item_id: String::new(),
                remote: Some(remote),
            },
            prompt: "contexto preservado".to_owned(),
        }
    }

    #[tokio::test]
    async fn update_dry_run_and_non_interactive_combinations_should_not_start_provider_or_writer() {
        let options = prt::cli::parse_cli(["prt", "desc", "--pr", "42"]).unwrap();
        let error = run_update_with_tty(&options, false).await.unwrap_err();
        assert!(error.to_string().contains("terminal interativo"));

        let dry_run = prt::cli::parse_cli(["prt", "desc", "--pr", "42", "--dry-run"]).unwrap();
        assert!(dry_run.output.dry_run);
        let mut output = String::new();
        run_update_prepared_with(&dry_run, false, update_prep(), |text| output = text)
            .await
            .unwrap();
        assert_eq!(output, update_dry_run_text("contexto preservado"));
        for extra in ["--raw", "--create", "--no-create"] {
            let error = prt::cli::parse_cli(["prt", "desc", "--pr", "42", extra]).unwrap_err();
            assert_eq!(error.exit_code(), 2);
        }
    }

    #[test]
    fn desc_output_should_show_functional_context_but_raw_should_be_body_only() {
        let status = prt::features::describe::FunctionalContextStatus::Loaded(
            prt::azure::work_items::FunctionalWorkItemContext {
                id: 11763,
                title: "Enriquecer a descrição".to_owned(),
                work_item_type: "User Story".to_owned(),
                area_path: "Produto\\CLI".to_owned(),
                description: Some("não deve ser impresso".to_owned()),
                acceptance_criteria: Some("não deve ser impresso".to_owned()),
            },
        );
        let desc = prt::ai::PrDescription {
            title: "Título gerado".to_owned(),
            body: "## Descrição\nmudança real".to_owned(),
        };
        let formatted =
            format_desc_plain_output(&desc, &["dev".to_owned()], "11763", &status, false);
        assert!(formatted.contains("Contexto funcional: Work Item #11763"));
        assert!(formatted.contains("Enriquecer a descrição"));

        let raw = format_desc_plain_output(&desc, &["dev".to_owned()], "11763", &status, true);
        assert_eq!(raw, desc.body);
        assert!(!raw.contains("Contexto funcional"));
        assert!(!raw.contains("Enriquecer a descrição"));
    }

    #[tokio::test]
    async fn desc_prepare_test_case_should_start_flow_after_terminal_restoration() {
        let desc = prt::ai::PrDescription {
            title: "Descrição".to_owned(),
            body: "## Objetivo\nValidar".to_owned(),
        };
        let context = published_launch_context();
        let published = vec![context.published_pr.clone()];
        let receipt = build_done_receipt(&desc, &["dev".to_owned()], &published);
        let expected_context = context.clone();
        let (actual_receipt, test_result) = run_desc_handoff(
            desc.clone(),
            vec!["dev".to_owned()],
            receipt.clone(),
            context.clone(),
            published.clone(),
            |request| async move {
                match request {
                    TestCardRequest::PublishedPr(actual) => {
                        assert_eq!(actual.published_pr.id, expected_context.published_pr.id);
                        assert_eq!(
                            actual.published_pr.target,
                            expected_context.published_pr.target
                        );
                        assert_eq!(actual.source_ref_name, expected_context.source_ref_name);
                        assert_eq!(actual.target_ref_name, expected_context.target_ref_name);
                        assert!(!actual.settings.team.is_empty());
                    }
                    TestCardRequest::Cli(_) => panic!("handoff publicado virou request CLI"),
                }
                Ok(TestFlowOutcome::ReviewedNoCreate)
            },
        )
        .await;
        assert_eq!(actual_receipt, receipt);
        assert!(matches!(test_result, Ok(TestFlowOutcome::ReviewedNoCreate)));

        let outcome = prt::tui::events::LiveOutcome::PrepareTestCase {
            desc: desc.clone(),
            launch_context: context,
            published: published.clone(),
        };
        match outcome {
            prt::tui::events::LiveOutcome::PrepareTestCase {
                desc: actual_desc,
                launch_context,
                published: receipt,
            } => {
                assert_eq!(actual_desc.title, desc.title);
                assert_eq!(launch_context.published_pr.id, 42);
                assert_eq!(launch_context.published_pr.target, "dev");
                assert_eq!(
                    launch_context.published_pr.url,
                    "https://dev.azure.com/org/project/_git/repo/pullrequest/42"
                );
                assert_eq!(
                    launch_context.source_ref_name,
                    "refs/heads/feature/11763-mudanca"
                );
                assert_eq!(launch_context.target_ref_name, "refs/heads/dev");
                assert_eq!(launch_context.work_item_id, Some(11763));
                assert_eq!(
                    receipt.iter().map(|item| item.id).collect::<Vec<_>>(),
                    published.iter().map(|item| item.id).collect::<Vec<_>>()
                );
            }
            _ => panic!("handoff não carregou o contexto publicado"),
        }
    }

    #[test]
    fn cancelled_test_case_handoff_should_preserve_published_receipt() {
        let desc = prt::ai::PrDescription {
            title: "Descrição".to_owned(),
            body: "body".to_owned(),
        };
        let published = vec![
            prt::azure::pull_requests::PublishedPr {
                target: "sprint/12".to_owned(),
                id: 41,
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/41".to_owned(),
            },
            prt::azure::pull_requests::PublishedPr {
                target: "dev".to_owned(),
                id: 42,
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/42".to_owned(),
            },
        ];
        let receipt = build_done_receipt(
            &desc,
            &["sprint/12".to_owned(), "dev".to_owned()],
            &published,
        );
        for item in &published {
            assert!(receipt.contains(&format!("PR #{}", item.id)));
            assert!(receipt.contains(&item.target));
            assert!(receipt.contains(&item.url));
        }
        assert!(receipt.contains("PR #41 · sprint/12"));
        assert!(receipt.contains("PR #42 · dev"));
        assert!(!receipt.contains("Test Case #"));
    }

    #[test]
    fn failed_test_case_handoff_should_preserve_published_receipt_and_writes() {
        let desc = prt::ai::PrDescription {
            title: "Descrição".to_owned(),
            body: "body".to_owned(),
        };
        let published = vec![prt::azure::pull_requests::PublishedPr {
            target: "dev".to_owned(),
            id: 42,
            url: "https://dev.azure.com/org/project/_git/repo/pullrequest/42".to_owned(),
        }];
        let receipt = build_done_receipt(&desc, &["dev".to_owned()], &published);
        let error = "Azure DevOps recusou a preparação (HTTP 403)";
        let reported = format!("{error}\n\n{receipt}");
        assert!(reported.contains(error));
        assert!(reported.contains("PR #42 · dev"));
        assert!(reported.contains("pullrequest/42"));
        assert!(!reported.contains("criado: https://"));
    }
}
