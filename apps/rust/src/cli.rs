//! Parsing da CLI — espelha `cli_parser.dart` + `cli_options.dart`.
//!
//! Comandos: `desc` (default), `test`, `init`, `doctor`, `update`, `completions`.
//! Mantém as mesmas flags, validações e mensagens do Dart.

use clap::{Parser, Subcommand};
use clap_complete::Shell;
use uuid::Uuid;

use crate::error::{AppError, Result};

/// Versão da CLI, mantida em sincronia com o pacote Rust publicado.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Providers de IA suportados.
pub const PROVIDERS: &[&str] = &["codex", "opencode", "openai-compatible"];

/// Newtype para Work Item / PR id — `type-newtype-ids`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItemId(String);

impl WorkItemId {
    /// Cria após validar que é numérico positivo.
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Cli`] se não for ID numérico positivo.
    pub fn parse(label: &str, value: &str) -> Result<Self> {
        let trimmed = value.trim();
        if !trimmed.chars().all(|c| c.is_ascii_digit()) || trimmed.is_empty() {
            return Err(AppError::cli(format!(
                "{label} inválido: use um ID numérico."
            )));
        }
        let n: i64 = trimmed
            .parse()
            .map_err(|_| AppError::cli(format!("{label} inválido: use um ID numérico.")))?;
        if n <= 0 {
            return Err(AppError::cli(format!(
                "{label} inválido: use um ID positivo."
            )));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Retorna o valor como `&str` sem alocar — `own-borrow-over-clone`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Comando top-level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Gera descrição de PR e opcionalmente cria PRs.
    Desc,
    /// Gera card de Test Case e opcionalmente cria.
    Test,
    /// Wizard de configuração inicial.
    Init,
    /// Diagnóstico do ambiente.
    Doctor,
    /// Baixa a versão mais recente e atualiza o binário instalado.
    #[command(alias = "updade")]
    Update,
    /// Gera script de completions para o shell.
    Completions,
}

/// CLI raiz — `prt [comando] [opções]`.
#[derive(Debug, Parser)]
#[command(name = "prt", version = VERSION, about = "Gera descrições de PR e Test Cases a partir do contexto Git.")]
pub struct Cli {
    /// Subcomando (omitido = `desc`, como no Dart).
    #[command(subcommand)]
    pub command: Option<CommandWithOpts>,
}

#[derive(Debug, Subcommand)]
/// Subcomando normalizado da CLI (espelha `Command` do Dart).
pub enum CommandWithOpts {
    /// `prt desc` — gera descrição de PR.
    Desc(DescOpts),
    /// `prt test` — gera card de Test Case.
    Test(TestOpts),
    /// `prt init` — wizard de configuração.
    Init,
    /// `prt doctor` — diagnóstico do ambiente.
    Doctor(DoctorOpts),
    /// `prt update` — atualiza o binário instalado.
    #[command(alias = "updade")]
    Update,
    /// `prt completions <shell>` — script de completions.
    Completions(CompletionsOpts),
}

/// Opções compartilhadas de geração.
#[derive(Debug, Clone, Parser)]
pub struct SharedGen {
    /// Branch de origem.
    #[arg(long)]
    pub source: Option<String>,
    /// Provider: codex, opencode ou openai-compatible.
    #[arg(long)]
    pub provider: Option<String>,
    /// Modelo do provider.
    #[arg(long)]
    pub model: Option<String>,
    /// Endpoint OpenAI-compatible.
    #[arg(long = "base-url")]
    pub base_url: Option<String>,
    /// API key do endpoint.
    #[arg(long = "api-key")]
    pub api_key: Option<String>,
    /// Confirma a criação após gerar o conteúdo.
    #[arg(long, conflicts_with = "no_create")]
    pub create: bool,
    /// Mostra o prompt sem chamar o modelo.
    #[arg(long = "dry-run")]
    pub dry_run: bool,
    /// Imprime somente o Markdown.
    #[arg(long)]
    pub raw: bool,
}

/// Opções de `prt desc`.
#[derive(Debug, Clone, Parser)]
pub struct DescOpts {
    /// Flags compartilhadas (`--source`, `--provider`, `--create`, …).
    #[command(flatten)]
    pub shared: SharedGen,
    /// Target; pode repetir.
    #[arg(long = "target")]
    pub targets: Vec<String>,
    /// Work Item.
    #[arg(long = "work-item")]
    pub work_item: Option<String>,
    /// PR Azure DevOps existente a atualizar.
    #[arg(long, allow_hyphen_values = true)]
    pub pr: Option<String>,
    /// Não copia o conteúdo.
    #[arg(long = "no-copy")]
    pub no_copy: bool,
    /// Apenas gera o Test Case (compat; sem efeito em desc).
    #[arg(long = "no-create")]
    pub no_create: bool,
    /// Lista sessões de `desc` que podem ser retomadas.
    #[arg(long, conflicts_with = "session")]
    pub resume: bool,
    /// Retoma uma sessão específica pelo UUID v4.
    #[arg(long, conflicts_with = "resume")]
    pub session: Option<String>,
}

/// Opções de `prt test`.
#[derive(Debug, Clone, Parser)]
pub struct TestOpts {
    /// Flags compartilhadas (`--source`, `--provider`, `--create`, …).
    #[command(flatten)]
    pub shared: SharedGen,
    /// Work Item pai.
    #[arg(long = "work-item")]
    pub work_item: Option<String>,
    /// PR Azure DevOps para contexto do card.
    #[arg(long, allow_hyphen_values = true)]
    pub pr: Option<String>,
    /// `AreaPath` do Test Case.
    #[arg(long = "area-path")]
    pub area_path: Option<String>,
    /// Responsável do Test Case.
    #[arg(long = "assigned-to")]
    pub assigned_to: Option<String>,
    /// `IterationPath` do Test Case.
    #[arg(long = "iteration-path")]
    pub iteration_path: Option<String>,
    /// Prioridade do Test Case.
    #[arg(long)]
    pub priority: Option<String>,
    /// Campo Custom.Team.
    #[arg(long)]
    pub team: Option<String>,
    /// Campo Custom.ProgramasAgrotrace.
    #[arg(long)]
    pub program: Option<String>,
    /// Exemplos de Test Cases (0-5).
    #[arg(long)]
    pub examples: Option<String>,
    /// Apenas gera, sem criar.
    #[arg(long = "no-create")]
    pub no_create: bool,
    /// Não copia (compat).
    #[arg(long = "no-copy")]
    pub no_copy: bool,
}

/// Opções de `prt doctor`.
#[derive(Debug, Clone, Parser)]
pub struct DoctorOpts {
    /// Branch de origem para coletar contexto.
    #[arg(long)]
    pub source: Option<String>,
}

/// Opções de `prt completions <shell>`.
#[derive(Debug, Clone, Parser)]
pub struct CompletionsOpts {
    /// Shell: bash, zsh, fish, powershell ou elvish.
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Flags de saída/geração (`--dry-run`, `--raw`, `--no-copy`).
///
/// Agrupa os booleanos de apresentação para que [`CliOptions`] não exceda o
/// limite do lint `struct_excessive_bools`. `create`/`no_create` seguem
/// em [`CliOptions`] por compatibilidade com a TUI.
#[derive(Debug, Clone, Default)]
pub struct OutputFlags {
    /// `--dry-run`.
    pub dry_run: bool,
    /// `--raw`.
    pub raw: bool,
    /// Deve copiar para clipboard (`!no-copy`).
    pub copy: bool,
}

/// Opções normalizadas (espelha `CliOptions` do Dart).
#[derive(Debug, Clone)]
pub struct CliOptions {
    /// Comando resolvido.
    pub command: Command,
    /// Branch de origem.
    pub source: Option<String>,
    /// Targets (`desc`).
    pub targets: Vec<String>,
    /// Lista sessões retomáveis (`desc`).
    pub resume: bool,
    /// UUID da sessão a retomar (`desc`).
    pub session: Option<String>,
    /// Work Item.
    pub work_item: Option<WorkItemId>,
    /// Provider override.
    pub provider: Option<String>,
    /// Modelo override.
    pub model: Option<String>,
    /// Base URL override.
    pub base_url: Option<String>,
    /// API key override.
    pub api_key: Option<String>,
    /// Flag `--create`.
    pub create: bool,
    /// Flag `--no-create`.
    pub no_create: bool,
    /// PR id (`desc` update ou contexto de `test`).
    pub pr: Option<WorkItemId>,
    /// Demais campos de `test` (espelham as flags de mesmo nome).
    /// `AreaPath` do Test Case.
    pub area_path: Option<String>,
    /// Responsável do Test Case.
    pub assigned_to: Option<String>,
    /// `IterationPath` do Test Case.
    pub iteration_path: Option<String>,
    /// Prioridade do Test Case.
    pub priority: Option<String>,
    /// Campo Custom.Team.
    pub team: Option<String>,
    /// Campo Custom.ProgramasAgrotrace.
    pub program: Option<String>,
    /// Exemplos de Test Cases (0-5).
    pub examples: Option<String>,
    /// Flags de saída (`--dry-run`, `--raw`, `--no-copy`).
    pub output: OutputFlags,
    /// Shell de `completions` (`None` nos demais comandos).
    pub completion_shell: Option<Shell>,
}

/// Valida provider.
fn validate_provider(value: Option<&str>) -> Result<Option<String>> {
    if let Some(p) = value {
        if !PROVIDERS.contains(&p) {
            return Err(AppError::cli(format!(
                "provider inválido: {p}. use codex, opencode ou openai-compatible."
            )));
        }
        return Ok(Some(p.to_owned()));
    }
    Ok(None)
}

/// Valida targets (`dev`, `sprint`, `sprint/<n>`).
fn validate_targets(targets: &[String]) -> Result<Vec<String>> {
    let mut validated = Vec::with_capacity(targets.len());
    for t in targets {
        let numbered_sprint = t.strip_prefix("sprint/").is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        });
        if t != "dev" && t != "sprint" && !numbered_sprint {
            return Err(AppError::cli(format!(
                "target inválido: {t}. use dev, sprint ou sprint/<número>."
            )));
        }
        if !validated.contains(t) {
            validated.push(t.clone());
        }
    }
    Ok(validated)
}

/// Converte argv estilo Dart (`--opt valor` e `--opt=valor`) via clap,
/// com default `desc` quando nenhum subcomando é informado.
fn normalize_argv<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let collected: Vec<String> = args.into_iter().map(|s| s.as_ref().to_owned()).collect();
    if collected.len() <= 1 {
        return collected;
    }
    // Se o primeiro posicional não é subcomando nem flag, prefixa `desc`.
    let first = collected.get(1).map_or("", String::as_str);
    if !first.starts_with('-')
        && !matches!(
            first,
            "desc" | "test" | "init" | "doctor" | "update" | "updade" | "completions"
        )
    {
        return Vec::new(); // sinaliza comando desconhecido abaixo
    }
    if !first.starts_with('-') {
        return collected;
    }
    // Só flags → default `desc`.
    let mut out = vec![collected[0].clone(), "desc".to_owned()];
    out.extend(collected.into_iter().skip(1));
    out
}

/// Rejeita comando desconhecido explícito (`prt frobnicate`).
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se o primeiro posicional não for subcomando nem flag.
fn reject_unknown_command(raw: &[String]) -> Result<()> {
    if raw.len() > 1 {
        let first = raw[1].as_str();
        if !first.starts_with('-')
            && !matches!(
                first,
                "desc" | "test" | "init" | "doctor" | "update" | "updade" | "completions"
            )
        {
            return Err(AppError::cli(format!("comando desconhecido: {first}")));
        }
    }
    Ok(())
}

/// Opções padrão de `prt` puro (`desc` sem flags).
fn default_cli_options() -> CliOptions {
    CliOptions {
        command: Command::Desc,
        source: None,
        targets: Vec::new(),
        resume: false,
        session: None,
        work_item: None,
        provider: None,
        model: None,
        base_url: None,
        api_key: None,
        create: false,
        no_create: false,
        pr: None,
        area_path: None,
        assigned_to: None,
        iteration_path: None,
        priority: None,
        team: None,
        program: None,
        examples: None,
        output: OutputFlags {
            dry_run: false,
            raw: false,
            copy: true,
        },
        completion_shell: None,
    }
}

/// Opções vazias para comandos sem flags (`init`, `doctor`, `completions`).
fn empty_cli_options(command: Command) -> CliOptions {
    CliOptions {
        command,
        ..default_cli_options()
    }
}

/// Converte `DescOpts` validadas em [`CliOptions`].
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se targets, work-item ou provider forem inválidos.
fn desc_cli_options(o: DescOpts) -> Result<CliOptions> {
    let resume_requested = o.resume || o.session.is_some();
    if resume_requested
        && (o.shared.source.is_some()
            || o.shared.provider.is_some()
            || o.shared.model.is_some()
            || o.shared.base_url.is_some()
            || o.shared.api_key.is_some()
            || o.shared.create
            || o.shared.dry_run
            || o.shared.raw
            || !o.targets.is_empty()
            || o.work_item.is_some()
            || o.pr.is_some()
            || o.no_copy
            || o.no_create)
    {
        return Err(AppError::cli(
            "retomar uma sessão não pode ser combinado com opções de geração ou publicação",
        ));
    }
    if let Some(session) = o.session.as_deref() {
        let id = Uuid::parse_str(session)
            .map_err(|_| AppError::cli("--session requer um UUID v4 válido"))?;
        if id.get_version_num() != 4 {
            return Err(AppError::cli("--session requer um UUID v4 válido"));
        }
    }
    let pr = parse_optional_work_item(o.pr.as_deref(), "--pr")?;
    if pr.is_some() && (o.shared.raw || o.shared.create || o.no_create) {
        return Err(AppError::cli(
            "atualização de PR requer revisão interativa: --pr não pode ser combinado com --raw, --create ou --no-create",
        ));
    }
    Ok(CliOptions {
        command: Command::Desc,
        source: o.shared.source,
        targets: validate_targets(&o.targets)?,
        resume: o.resume,
        session: o.session,
        work_item: parse_optional_work_item(o.work_item.as_deref(), "--work-item")?,
        provider: validate_provider(o.shared.provider.as_deref())?,
        model: o.shared.model,
        base_url: o.shared.base_url,
        api_key: o.shared.api_key,
        create: o.shared.create,
        no_create: o.no_create,
        pr,
        area_path: None,
        assigned_to: None,
        iteration_path: None,
        priority: None,
        team: None,
        program: None,
        examples: None,
        output: OutputFlags {
            dry_run: o.shared.dry_run,
            raw: o.shared.raw,
            copy: !o.no_copy,
        },
        completion_shell: None,
    })
}

/// Converte `TestOpts` validadas em [`CliOptions`].
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se work-item, pr ou provider forem inválidos.
fn test_cli_options(o: TestOpts) -> Result<CliOptions> {
    Ok(CliOptions {
        command: Command::Test,
        source: o.shared.source,
        targets: Vec::new(),
        resume: false,
        session: None,
        work_item: parse_optional_work_item(o.work_item.as_deref(), "--work-item")?,
        provider: validate_provider(o.shared.provider.as_deref())?,
        model: o.shared.model,
        base_url: o.shared.base_url,
        api_key: o.shared.api_key,
        create: o.shared.create,
        no_create: o.no_create,
        pr: parse_optional_work_item(o.pr.as_deref(), "--pr")?,
        area_path: o.area_path,
        assigned_to: o.assigned_to,
        iteration_path: o.iteration_path,
        priority: o.priority,
        team: o.team,
        program: o.program,
        examples: o.examples,
        output: OutputFlags {
            dry_run: o.shared.dry_run,
            raw: o.shared.raw,
            copy: !o.no_copy,
        },
        completion_shell: None,
    })
}

/// Converte ID opcional (`--work-item`/`--pr`) em [`WorkItemId`].
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se o valor não for ID numérico positivo.
fn parse_optional_work_item(value: Option<&str>, label: &str) -> Result<Option<WorkItemId>> {
    value.map(|v| WorkItemId::parse(label, v)).transpose()
}

/// Converte subcomando validado em [`CliOptions`].
///
/// # Errors
///
/// Propaga [`AppError::Cli`] das validações de cada subcomando.
fn build_options(sub: CommandWithOpts) -> Result<CliOptions> {
    match sub {
        CommandWithOpts::Desc(o) => desc_cli_options(o),
        CommandWithOpts::Test(o) => test_cli_options(o),
        CommandWithOpts::Init => Ok(empty_cli_options(Command::Init)),
        CommandWithOpts::Doctor(o) => Ok(CliOptions {
            source: o.source,
            ..empty_cli_options(Command::Doctor)
        }),
        CommandWithOpts::Update => Ok(empty_cli_options(Command::Update)),
        CommandWithOpts::Completions(o) => Ok(CliOptions {
            completion_shell: Some(o.shell),
            ..empty_cli_options(Command::Completions)
        }),
    }
}

/// Faz parse de `argv` completo (inclui nome do binário).
///
/// # Errors
///
/// Retorna [`AppError::Cli`] para comando/opção inválidos.
pub fn parse_cli<I, S>(args: I) -> Result<CliOptions>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let raw: Vec<String> = args.into_iter().map(|s| s.as_ref().to_owned()).collect();
    reject_unknown_command(&raw)?;
    let normalized = normalize_argv(&raw);
    if normalized.is_empty() {
        return Err(AppError::cli(format!("comando desconhecido: {}", raw[1])));
    }
    let cli = Cli::try_parse_from(&normalized).map_err(|e| {
        // clap já trata `--help`/`--version` com exit; aqui só erros reais.
        AppError::cli(e.to_string())
    })?;

    let Some(sub) = cli.command else {
        // `prt` puro → `desc` sem flags.
        return Ok(default_cli_options());
    };
    build_options(sub)
}

/// Verifica se o fluxo de atualização pode iniciar a execução escolhida.
///
/// O update existente só gera/escreve dentro de uma TUI; sem terminal, apenas
/// o dry-run é permitido e ele para antes de qualquer provider ou writer.
///
/// # Errors
///
/// Retorna [`AppError::Cli`] quando a operação pede escrita sem terminal e sem
/// `--dry-run`.
pub fn ensure_update_execution_mode(tty: bool, dry_run: bool) -> Result<()> {
    if !tty && !dry_run {
        return Err(AppError::cli(
            "atualização de PR requer terminal interativo; use --dry-run para apenas visualizar o prompt",
        ));
    }
    Ok(())
}

/// Texto de ajuda (espelha `helpText` do Dart).
#[must_use]
pub fn help_text() -> String {
    format!(
        "prt v{VERSION}\n\nGera descrições de PR e Test Cases a partir do contexto Git.\n\nUso:\n  \
         prt desc [opções]\n  prt test [opções]\n  prt init\n  prt doctor\n  prt update\n  prt completions <shell>\n\nOpções:\n  \
         --source <branch>       Branch de origem\n  --target <branch>       Target; pode repetir\n  \
         --work-item <id>        Work Item\n  --provider <nome>       codex, opencode ou openai-compatible\n  \
         --model <nome>          Modelo do provider\n  --base-url <url>        Endpoint OpenAI-compatible\n  \
         --api-key <key>         API key do endpoint\n  --create                Confirma a criação após gerar o conteúdo\n  \
         --no-create             Apenas gera o Test Case\n  --pr <id>               PR existente (desc) ou contexto do Test Case\n  \
         --area-path <path>      AreaPath do Test Case\n  --assigned-to <valor>   Responsável do Test Case\n  \
         --iteration-path <path> IterationPath do Test Case\n  --priority <n>          Prioridade do Test Case\n  \
         --team <nome>           Campo Custom.Team\n  --program <nome>        Campo Custom.ProgramasAgrotrace\n  \
         --examples <n>          Exemplos de Test Cases (0-5)\n  --dry-run               Mostra o prompt sem chamar o modelo\n  \
         --raw                   Imprime somente o Markdown\n  --no-copy               Não copia o conteúdo\n  --resume                Lista sessões de descrição retomáveis\n  --session <uuid>        Retoma uma sessão específica (UUID v4)\n  \
         --version, -v           Mostra a versão\n  --help, -h              Mostra esta ajuda"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desc_should_default_when_no_subcommand() {
        let opts = parse_cli(["prt"]).unwrap();
        assert_eq!(opts.command, Command::Desc);
        assert!(opts.targets.is_empty());
    }

    #[test]
    fn desc_should_accept_repeated_targets() {
        let opts = parse_cli(["prt", "desc", "--target", "dev", "--target", "sprint"]).unwrap();
        assert_eq!(opts.targets, vec!["dev", "sprint"]);
    }

    #[test]
    fn desc_should_reject_invalid_target() {
        let err = parse_cli(["prt", "desc", "--target", "main"]).unwrap_err();
        assert!(err.to_string().contains("target inválido"));
    }

    #[test]
    fn desc_should_reject_non_numeric_sprint_target() {
        for target in ["sprint/", "sprint/doze", "sprint/12x"] {
            let err = parse_cli(["prt", "desc", "--target", target]).unwrap_err();
            assert!(err.to_string().contains("target inválido"));
        }
    }

    #[test]
    fn desc_should_deduplicate_targets_preserving_order() {
        let opts = parse_cli([
            "prt",
            "desc",
            "--target",
            "dev",
            "--target",
            "sprint/12",
            "--target",
            "dev",
        ])
        .unwrap();
        assert_eq!(opts.targets, vec!["dev", "sprint/12"]);
    }

    #[test]
    fn cli_should_reject_unknown_command() {
        let err = parse_cli(["prt", "frobnicate"]).unwrap_err();
        assert!(err.to_string().contains("comando desconhecido"));
    }

    #[test]
    fn cli_should_parse_update_and_its_typo_alias() {
        assert_eq!(
            parse_cli(["prt", "update"]).unwrap().command,
            Command::Update
        );
        assert_eq!(
            parse_cli(["prt", "updade"]).unwrap().command,
            Command::Update
        );
    }

    #[test]
    fn cli_should_reject_invalid_provider() {
        let err = parse_cli(["prt", "desc", "--provider", "llama"]).unwrap_err();
        assert!(err.to_string().contains("provider inválido"));
    }

    #[test]
    fn work_item_should_reject_non_numeric() {
        let err = parse_cli(["prt", "desc", "--work-item", "abc"]).unwrap_err();
        assert!(err.to_string().contains("--work-item inválido"));
    }

    #[test]
    fn desc_pr_should_select_one_update_journey() {
        let opts = parse_cli(["prt", "desc", "--pr", "42"]).unwrap();
        assert_eq!(opts.command, Command::Desc);
        assert_eq!(opts.pr.as_ref().map(WorkItemId::as_str), Some("42"));
        assert!(opts.targets.is_empty());
    }

    #[test]
    fn update_command_should_remain_binary_update() {
        assert_eq!(
            parse_cli(["prt", "update"]).unwrap().command,
            Command::Update
        );
        assert_eq!(
            parse_cli(["prt", "updade"]).unwrap().command,
            Command::Update
        );
    }

    #[test]
    fn desc_pr_should_reject_invalid_ids_before_external_effects() {
        for value in ["", "abc", "0", "-1"] {
            let error = parse_cli(["prt", "desc", "--pr", value]).unwrap_err();
            assert_eq!(error.exit_code(), 2);
            assert!(error.to_string().contains("--pr inválido"));
        }
    }

    #[test]
    fn cli_should_reject_update_output_combinations() {
        let options = parse_cli(["prt", "desc", "--pr", "42", "--dry-run"]).unwrap();
        assert!(options.output.dry_run);
        assert!(ensure_update_execution_mode(false, options.output.dry_run).is_ok());
        assert!(ensure_update_execution_mode(true, false).is_ok());
        let non_interactive = ensure_update_execution_mode(false, false).unwrap_err();
        assert_eq!(non_interactive.exit_code(), 2);
        assert!(non_interactive.to_string().contains("terminal interativo"));
        for extra in ["--raw", "--create", "--no-create"] {
            let error = parse_cli(["prt", "desc", "--pr", "42", extra]).unwrap_err();
            assert_eq!(error.exit_code(), 2);
        }
    }

    #[test]
    fn session_selector_requires_existing_incomplete_uuid() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let options = parse_cli(["prt", "desc", "--session", id]).unwrap();
        assert_eq!(options.session.as_deref(), Some(id));
        assert!(!options.resume);
        let invalid = parse_cli(["prt", "desc", "--session", "not-a-uuid"]).unwrap_err();
        assert_eq!(invalid.exit_code(), 2);
    }

    #[test]
    fn resume_flags_conflict_with_generation_and_publish_options() {
        for extra in [
            "--source",
            "--provider",
            "--model",
            "--base-url",
            "--api-key",
            "--create",
            "--dry-run",
            "--raw",
            "--target",
            "--work-item",
            "--pr",
            "--no-copy",
            "--no-create",
        ] {
            let mut args = vec!["prt", "desc", "--resume", extra];
            if matches!(
                extra,
                "--source"
                    | "--provider"
                    | "--model"
                    | "--base-url"
                    | "--api-key"
                    | "--target"
                    | "--work-item"
                    | "--pr"
            ) {
                args.push("value");
            }
            let error = parse_cli(args).unwrap_err();
            assert_eq!(error.exit_code(), 2, "{extra}");
        }
    }
}
