//! Binário `prt` — `main.rs` mínimo, lógica no lib (`proj-lib-main-split`).
//!
//! Usa `tokio` + `tracing` + `anyhow::Context` no binário
//! (`err-anyhow-app`), `thiserror` no lib (`err-thiserror-lib`).

use anyhow::Context;
use clap::CommandFactory as _;
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
    if options.output.raw {
        println!("{}", desc.body);
    } else {
        println!(
            "Pull Request\n\nTítulo: {}\nTargets: {}\nWork Item: #{}\n\n{}\n",
            desc.title,
            prep.targets.join(", "),
            prep.work_item_id,
            desc.body
        );
    }
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
                    format!("PR {} criado: {}", item.target, item.url),
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
        let _ = write!(receipt, "\n✓ PR {} criado: {}", item.target, item.url);
    }
    receipt
}

async fn run_desc(options: &prt::cli::CliOptions) -> anyhow::Result<()> {
    use prt::features::describe;
    use prt::tui::{
        events::LiveOutcome,
        live::run_describe_tui,
        notice::{NoticeKind, show_notice},
    };
    use ratatui::text::Text;

    let prep = describe::prepare(options).context("falha ao preparar contexto")?;
    if handle_desc_dry_run(options, &prep).await? {
        return Ok(());
    }
    // `--raw` ou saída não-tty: texto puro (script-friendly, sem tela).
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    if !tty || options.output.raw {
        return run_desc_plain(options, &prep).await;
    }
    // TUI viva: streaming token a token, logs auto-scroll e barra com shimmer.
    // Clona antes: `prep` é movido para `run_describe_tui`.
    let targets = prep.targets.clone();
    match run_describe_tui(prep, options.create).await? {
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
        LiveOutcome::Failed(msg) => anyhow::bail!("falha na tui: {msg}"),
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
    const MSG: &str =
        "Diagnóstico interativo em implementação.\n\nUse `prt desc` ou `prt init`, já em Ratatui.";
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        println!("Doctor\n\n{MSG}");
        return Ok(());
    }
    let _ = options;
    let code = run_doctor_flow(options.source.as_deref()).await?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}
