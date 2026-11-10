//! Contexto Git — espelha `domain/change_context.dart` + `change_context_reader_live.dart`.
//!
//! Coleta branch, base (`sprint` > `dev` > `main` > `master`), diff truncado
//! em 8000 linhas, log (50 oneline) e remote Azure DevOps. A sprint usada como
//! base é a do target `sprint/<n>` pedido explicitamente ou, na ausência dele,
//! a sprint mais recente; quando a branch existe localmente e em `origin`, a
//! ref mais atualizada é a escolhida.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::process::Command as ProcCommand;
use std::sync::OnceLock;

use crate::error::{AppError, Result};

/// Limite de linhas do diff (espelha o Dart).
pub const MAX_DIFF_LINES: usize = 8000;

/// Remote Azure DevOps parseado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRemote {
    /// Organização (`dev.azure.com/{org}`).
    pub organization: String,
    /// Projeto.
    pub project: String,
    /// Repositório.
    pub repository: String,
}

/// Snapshot mínimo do checkout usado para detectar divergência antes de uma
/// geração iniciada a partir de uma receipt publicada.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitContextFingerprint {
    /// Identidade absoluta do repositório local.
    pub repository: String,
    /// Branch efetivamente selecionada para a origem.
    pub source_branch: String,
    /// OID observado na ref da origem.
    pub source_oid: String,
    /// OID observado por target, na ordem lexical dos nomes.
    pub target_oids: BTreeMap<String, String>,
}

/// Resultado da comparação de um fingerprint salvo com o checkout atual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FingerprintStatus {
    /// Repositório, branch e todos os OIDs coincidem.
    Exact,
    /// O repositório local ou a branch de origem mudou.
    RepositoryOrBranchChanged,
    /// Um OID de origem/target mudou ou deixou de existir.
    ObjectChanged,
    /// O checkout atual não pôde ser identificado.
    Unavailable,
}

impl GitContextFingerprint {
    /// Captura o checkout atual e os OIDs das refs source/target.
    ///
    /// Refs ausentes são representadas por texto vazio. Em checkout detached,
    /// a origem usa o OID de `HEAD`, mantendo a comparação útil mesmo sem
    /// nome de branch.
    ///
    /// # Errors
    ///
    /// Retorna erro quando o Git não consegue identificar o repositório ou a
    /// branch atualmente checked out.
    pub fn capture(source_branch: &str, targets: &[String]) -> Result<Self> {
        let repository = git(&["rev-parse", "--show-toplevel"])?;
        let current_branch = git(&["branch", "--show-current"])?;
        let source_branch = if source_branch.trim().is_empty() {
            current_branch
        } else {
            source_branch.trim().to_owned()
        };
        let source_oid = ref_oid(&source_branch);
        let target_oids = targets
            .iter()
            .map(|target| (target.clone(), ref_oid(target)))
            .collect();
        Ok(Self {
            repository,
            source_branch,
            source_oid,
            target_oids,
        })
    }

    /// Compara este snapshot com o checkout atual.
    #[must_use]
    pub fn matches_current(&self) -> bool {
        self.compare_current() == FingerprintStatus::Exact
    }

    /// Compara o fingerprint salvo com o checkout atual, preservando o motivo.
    #[must_use]
    pub fn compare_current(&self) -> FingerprintStatus {
        let Ok(repository) = git(&["rev-parse", "--show-toplevel"]) else {
            return FingerprintStatus::Unavailable;
        };
        let Ok(current_branch) = git(&["branch", "--show-current"]) else {
            return FingerprintStatus::Unavailable;
        };
        if repository != self.repository || current_branch != self.source_branch {
            return FingerprintStatus::RepositoryOrBranchChanged;
        }
        let source_oid = ref_oid(&self.source_branch);
        if source_oid.is_empty() || self.source_oid.is_empty() || source_oid != self.source_oid {
            return FingerprintStatus::ObjectChanged;
        }
        if self.target_oids.iter().any(|(target, expected)| {
            let actual = ref_oid(target);
            actual.is_empty() || expected.is_empty() || actual != *expected
        }) {
            return FingerprintStatus::ObjectChanged;
        }
        FingerprintStatus::Exact
    }
}

/// OID da ref local, de `HEAD` em detached checkout, ou de `origin/<branch>`.
fn ref_oid(branch: &str) -> String {
    let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch).trim();
    if branch.is_empty() {
        return git(&["rev-parse", "--verify", "HEAD"]).unwrap_or_default();
    }
    let local = format!("refs/heads/{branch}");
    git(&["rev-parse", "--verify", &local])
        .or_else(|_| {
            let origin = format!("origin/{branch}");
            git(&["rev-parse", "--verify", &origin])
        })
        .unwrap_or_default()
}

/// Contexto de mudanças coletado do Git.
#[derive(Debug, Clone)]
pub struct ChangeContext {
    /// Branch atual.
    pub branch: String,
    /// Ref de origem (`refs/heads/...`).
    pub source_ref: String,
    /// Base usada para diff/log.
    pub base_branch: String,
    /// Branch `sprint/<maior-n>` (pode ser vazio).
    pub sprint_branch: String,
    /// Diff truncado.
    pub diff: String,
    /// Linhas originais do diff (antes de truncar).
    pub diff_original_lines: usize,
    /// `git log --oneline -50 base..source`.
    pub log: String,
    /// Work Item extraído da branch.
    pub work_item_id: String,
    /// Remote Azure (se parseável).
    pub remote: Option<RepositoryRemote>,
}

fn git(args: &[&str]) -> Result<String> {
    let out = ProcCommand::new("git")
        .args(args)
        .output()
        .map_err(|e| AppError::Git {
            message: format!("falha ao executar git: {e}"),
        })?;
    if !out.status.success() {
        return Err(AppError::Git {
            message: format!(
                "git {} falhou: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Extrai o Work Item da branch.
///
/// O número que identifica uma branch `sprint/<n>` não é um Work Item. Ele é
/// ignorado para evitar que uma branch de sprint seja usada acidentalmente
/// como o pai do Test Case. Se houver uma branch composta, como
/// `sprint/110/feature/11763-desc`, o primeiro número que não pertence ao
/// segmento `sprint` continua sendo considerado.
#[must_use]
pub fn work_item_from_branch(branch: &str) -> String {
    let branch = branch
        .strip_prefix("refs/heads/")
        .unwrap_or(branch)
        .trim_matches(['/', '_', '-']);
    let mut previous_segment: Option<&str> = None;
    for segment in branch.split(['/', '_', '-']) {
        if segment.is_empty() {
            continue;
        }
        let is_sprint_number =
            previous_segment.is_some_and(|previous| previous.eq_ignore_ascii_case("sprint"));
        if !is_sprint_number && segment.chars().all(|character| character.is_ascii_digit()) {
            return segment.to_owned();
        }
        previous_segment = Some(segment);
    }
    String::new()
}

/// Resolve targets (`sprint` → `sprintBranch`; default `[sprintBranch, dev]`).
#[must_use]
pub fn resolve_targets(context: &ChangeContext, requested: &[String]) -> Vec<String> {
    if !requested.is_empty() {
        let mut resolved = Vec::with_capacity(requested.len());
        for requested_target in requested {
            let target = if requested_target == "sprint" {
                context.sprint_branch.clone()
            } else {
                requested_target.clone()
            };
            if !target.is_empty() && !resolved.contains(&target) {
                resolved.push(target);
            }
        }
        return resolved;
    }
    [context.sprint_branch.clone(), "dev".to_owned()]
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect()
}

/// Encontra a sprint numericamente mais recente em uma saída de branches Git.
///
/// A saída pode misturar branches locais (`sprint/12`) e remotas
/// (`origin/sprint/12`). O nome retornado é sempre a referência de branch que
/// o Azure espera (`sprint/12`), sem o prefixo do remote.
fn latest_sprint_branch(branches: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE
        .get_or_init(|| Regex::new(r"^sprint/(\d+)(?:$|[-/].*)").expect("regex de sprint válida"));

    branches
        .lines()
        .filter_map(|line| {
            let branch = line
                .trim()
                .trim_start_matches(['*', '+'])
                .trim_start()
                .trim_start_matches("remotes/origin/")
                .trim_start_matches("origin/");
            let captures = re.captures(branch)?;
            let sprint_number = captures.get(1)?.as_str().parse::<i64>().ok()?;
            Some((sprint_number, branch.to_owned()))
        })
        .max_by_key(|(sprint_number, _)| *sprint_number)
        .map(|(_, branch)| branch)
}

/// Resolve a ref de uma branch base, preferindo a cópia mais atualizada.
///
/// Branches base (`sprint/<n>`, `dev`, ...) são compartilhadas e a cópia local
/// costuma ficar para trás de `origin`. Quando as duas existem e a local é
/// ancestral da remota, a remota é usada: comparar com a local desatualizada
/// faria o diff incluir todo o trabalho alheio que a branch de origem já
/// incorporou da sprint. A local só prevalece se tiver commits próprios.
fn resolve_base_ref_with<F>(branch: &str, command: &mut F) -> Option<String>
where
    F: FnMut(&[&str]) -> Result<String>,
{
    if branch.is_empty() {
        return None;
    }
    let local_ref = format!("refs/heads/{branch}");
    let remote_ref = format!("refs/remotes/origin/{branch}");
    let remote_branch = format!("origin/{branch}");
    let has_local = command(&["rev-parse", "--verify", "--quiet", &local_ref]).is_ok();
    let has_remote = command(&["rev-parse", "--verify", "--quiet", &remote_ref]).is_ok();
    match (has_local, has_remote) {
        (true, true) => {
            let local_is_behind =
                command(&["merge-base", "--is-ancestor", &local_ref, &remote_ref]).is_ok();
            Some(if local_is_behind {
                remote_branch
            } else {
                branch.to_owned()
            })
        }
        (true, false) => Some(branch.to_owned()),
        (false, true) => Some(remote_branch),
        (false, false) => None,
    }
}

fn resolve_pull_request_ref_with<F>(remote_ref: &str, command: &mut F) -> Option<String>
where
    F: FnMut(&[&str]) -> Result<String>,
{
    let branch = remote_ref.strip_prefix("refs/heads/")?;
    if branch.is_empty() {
        return None;
    }
    if command(&["rev-parse", "--verify", remote_ref]).is_ok() {
        return Some(remote_ref.to_owned());
    }
    let origin_ref = format!("origin/{branch}");
    command(&["rev-parse", "--verify", &origin_ref])
        .is_ok()
        .then_some(origin_ref)
}

fn pull_request_ranges(source: &str, target: &str) -> (String, String) {
    (
        format!("{target}...{source}"),
        format!("{target}..{source}"),
    )
}

/// Separa host e caminho de um remote HTTPS, descartando o `userinfo`
/// opcional que o Git Credential Manager pode persistir no Windows.
fn https_remote_parts(url: &str) -> Option<(&str, &str)> {
    let rest = url.trim().strip_prefix("https://")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    Some((host, path))
}

/// Faz parse do remote `origin` (ssh, modern e legacy) para Azure.
#[must_use]
pub fn parse_azure_remote(url: &str) -> Option<RepositoryRemote> {
    // ssh: git@ssh.dev.azure.com:v3/org/project/repo
    if let Some(rest) = url.strip_prefix("git@ssh.dev.azure.com:v3/") {
        let parts: Vec<&str> = rest.split('/').collect();
        let repository = parts.get(2).copied().unwrap_or("").trim_end_matches(".git");
        if parts.len() >= 3
            && !parts[0].is_empty()
            && !parts[1].is_empty()
            && !repository.is_empty()
        {
            return Some(RepositoryRemote {
                organization: parts[0].to_owned(),
                project: parts[1].to_owned(),
                repository: repository.to_owned(),
            });
        }
    }
    // modern: https://[userinfo@]dev.azure.com/org/project/_git/repo
    if let Some((host, rest)) = https_remote_parts(url)
        && host.eq_ignore_ascii_case("dev.azure.com")
    {
        let rest = rest.trim_end_matches(".git");
        let parts: Vec<&str> = rest.split("/_git/").collect();
        if parts.len() == 2 {
            let left: Vec<&str> = parts[0].split('/').collect();
            if left.len() >= 2 {
                let repository = parts[1].split('/').next_back().unwrap_or("");
                if left[0].is_empty() || left[1..].iter().any(|part| part.is_empty()) {
                    return None;
                }
                if repository.is_empty() {
                    return None;
                }
                return Some(RepositoryRemote {
                    organization: left[0].to_owned(),
                    project: left[1..].join("/"),
                    repository: repository.to_owned(),
                });
            }
        }
    }
    // legacy: https://[userinfo@]org.visualstudio.com/project/_git/repo
    if let Some((host, path)) = https_remote_parts(url) {
        if let Some(org) = host.strip_suffix(".visualstudio.com") {
            let (project, repository) = path.split_once("/_git/")?;
            let repository = repository
                .split('/')
                .next_back()
                .unwrap_or("")
                .trim_end_matches(".git");
            if project.is_empty() || repository.is_empty() {
                return None;
            }
            return Some(RepositoryRemote {
                organization: org.to_owned(),
                project: project.to_owned(),
                repository: repository.to_owned(),
            });
        }
    }
    None
}

/// Lê o remote `origin` sem inferir branch ou base.
///
/// # Errors
///
/// Retorna [`AppError::Git`] somente quando o comando Git falha por uma razão
/// diferente de `origin` ausente; um remote não parseável vira `None`.
pub fn origin_remote() -> Result<Option<RepositoryRemote>> {
    let Ok(url) = git(&["remote", "get-url", "origin"]) else {
        return Ok(None);
    };
    Ok(parse_azure_remote(&url))
}

/// Coleta diff e log usando exclusivamente as refs retornadas pelo PR.
///
/// A resolução aceita a ref local `refs/heads/<branch>` ou a correspondente
/// `origin/<branch>`. Não faz fetch e não escolhe uma base alternativa.
///
/// # Errors
///
/// Retorna [`AppError::Git`] quando source/target estão ausentes, quando uma
/// ref não pode ser resolvida ou quando o Git falha ao coletar o contexto.
pub fn collect_for_refs(source_ref: &str, target_ref: &str) -> Result<ChangeContext> {
    let remote = origin_remote()?;
    collect_for_refs_with(source_ref, target_ref, git, remote)
}

pub(crate) fn collect_for_refs_with<F>(
    source_ref: &str,
    target_ref: &str,
    mut command: F,
    remote: Option<RepositoryRemote>,
) -> Result<ChangeContext>
where
    F: FnMut(&[&str]) -> Result<String>,
{
    let source_branch = source_ref
        .strip_prefix("refs/heads/")
        .filter(|branch| !branch.is_empty())
        .ok_or_else(|| AppError::Git {
            message: "sourceRefName ausente ou inválido; atualize/fetch das refs do PR".to_owned(),
        })?;
    if target_ref
        .strip_prefix("refs/heads/")
        .is_none_or(str::is_empty)
    {
        return Err(AppError::Git {
            message: "targetRefName ausente ou inválido; atualize/fetch das refs do PR".to_owned(),
        });
    }
    let source =
        resolve_pull_request_ref_with(source_ref, &mut command).ok_or_else(|| AppError::Git {
            message: format!(
                "ref source {source_ref} não encontrada localmente; atualize/fetch as refs do PR"
            ),
        })?;
    let target =
        resolve_pull_request_ref_with(target_ref, &mut command).ok_or_else(|| AppError::Git {
            message: format!(
                "ref target {target_ref} não encontrada localmente; atualize/fetch as refs do PR"
            ),
        })?;
    let (diff_range, log_range) = pull_request_ranges(&source, &target);
    let diff_raw = command(&["diff", &diff_range])?;
    let diff_original_lines = diff_raw.lines().count();
    let diff = diff_raw
        .lines()
        .take(MAX_DIFF_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let log = command(&["log", "--oneline", "-50", &log_range])?;
    Ok(ChangeContext {
        branch: source_branch.to_owned(),
        source_ref: source_ref.to_owned(),
        base_branch: target,
        sprint_branch: String::new(),
        diff,
        diff_original_lines,
        log,
        work_item_id: work_item_from_branch(source_branch),
        remote,
    })
}

/// Coleta o contexto Git (branch atual ou `source` explícito).
///
/// `requested_targets` são os targets pedidos na CLI. Um target `sprint/<n>`
/// explícito define a base do diff; sem ele, a base é a sprint mais recente,
/// com `dev`/`main`/`master` apenas como fallback quando não há sprint.
///
/// # Errors
///
/// Retorna [`AppError::Git`] se não for repo, branch protegida, sem base ou
/// se a sprint pedida explicitamente não existir localmente nem em `origin`.
pub fn collect(source: Option<&str>, requested_targets: &[String]) -> Result<ChangeContext> {
    collect_with(source, requested_targets, git)
}

pub(crate) fn collect_with<F>(
    source: Option<&str>,
    requested_targets: &[String],
    mut command: F,
) -> Result<ChangeContext>
where
    F: FnMut(&[&str]) -> Result<String>,
{
    let current = command(&["branch", "--show-current"])?;
    let branch = source.unwrap_or(&current).to_owned();
    if branch.is_empty() {
        return Err(AppError::Git {
            message: "branch atual não encontrada (detached head?)".to_owned(),
        });
    }
    if matches!(branch.as_str(), "dev" | "main" | "master") {
        return Err(AppError::Git {
            message: format!("branch de origem não pode ser {branch}"),
        });
    }
    let source_ref = format!("refs/heads/{branch}");

    // Descobre a sprint mais recente entre branches locais e remotas.
    let local_branches = command(&["branch", "--list", "sprint/*", "dev", "main", "master"])?;
    let remote_branches = command(&["branch", "-r"])?;
    let branches = format!("{local_branches}\n{remote_branches}");
    let sprint_branch = latest_sprint_branch(&branches).unwrap_or_default();

    // A sprint pedida explicitamente é a base; nunca cai em outra branch em
    // silêncio, pois isso geraria uma descrição com mudanças alheias.
    let requested_sprint = requested_targets
        .iter()
        .find(|target| target.starts_with("sprint/"));
    let base_branch = if let Some(sprint) = requested_sprint {
        resolve_base_ref_with(sprint, &mut command).ok_or_else(|| AppError::Git {
            message: format!(
                "branch base {sprint} não encontrada localmente nem em origin; \
                 execute `git fetch origin` e tente novamente"
            ),
        })?
    } else {
        [sprint_branch.as_str(), "dev", "main", "master"]
            .into_iter()
            .find_map(|candidate| resolve_base_ref_with(candidate, &mut command))
            .ok_or_else(|| AppError::Git {
                message: "nenhuma branch base encontrada (sprint/dev/main/master)".to_owned(),
            })?
    };

    // Diff: `diff base...source`, fallback `diff base source`.
    let diff_raw = match command(&["diff", &format!("{base_branch}...{branch}")]) {
        Ok(diff) => diff,
        Err(_) => command(&["diff", &base_branch, &branch])?,
    };
    let diff_original_lines = diff_raw.lines().count();
    let diff = diff_raw
        .lines()
        .take(MAX_DIFF_LINES)
        .collect::<Vec<_>>()
        .join("\n");

    let log = command(&[
        "log",
        "--oneline",
        "-50",
        &format!("{base_branch}..{branch}"),
    ])?;
    let remote = command(&["remote", "get-url", "origin"])
        .ok()
        .and_then(|url| parse_azure_remote(&url));
    let work_item_id = work_item_from_branch(&branch);

    Ok(ChangeContext {
        branch,
        source_ref,
        base_branch,
        sprint_branch,
        diff,
        diff_original_lines,
        log,
        work_item_id,
        remote,
    })
}

/// Restringe o contexto às mudanças feitas depois de PRs já concluídos.
///
/// `merged_commits` são os commits da origem entregues por cada PR concluído
/// (um por target). O mais antigo vira a nova base, de modo que diff e log
/// cubram tudo que ainda falta em algum target. Retorna a base usada.
///
/// O contexto fica intacto e `None` é retornado quando o recorte não é
/// confiável: commit ausente no clone, fora do histórico da branch (rebase ou
/// force-push) ou igual à ponta da branch (nada novo desde o PR anterior).
pub fn narrow_to_follow_up(
    context: &mut ChangeContext,
    merged_commits: &[String],
) -> Option<String> {
    narrow_to_follow_up_with(context, merged_commits, git)
}

pub(crate) fn narrow_to_follow_up_with<F>(
    context: &mut ChangeContext,
    merged_commits: &[String],
    mut command: F,
) -> Option<String>
where
    F: FnMut(&[&str]) -> Result<String>,
{
    let branch = context.branch.clone();
    let mut base: Option<&str> = None;
    for commit in merged_commits {
        let commit = commit.trim();
        if commit.is_empty() || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        command(&["merge-base", "--is-ancestor", commit, &branch]).ok()?;
        base = match base {
            Some(current) if command(&["merge-base", "--is-ancestor", current, commit]).is_ok() => {
                Some(current)
            }
            _ => Some(commit),
        };
    }
    let base = base?;
    let base_oid = command(&["rev-parse", "--verify", &format!("{base}^{{commit}}")]).ok()?;
    let tip_oid = command(&["rev-parse", "--verify", &format!("{branch}^{{commit}}")]).ok()?;
    if base_oid == tip_oid {
        return None;
    }
    let diff_raw = command(&["diff", base, &branch]).ok()?;
    let log = command(&["log", "--oneline", "-50", &format!("{base}..{branch}")]).ok()?;
    context.diff_original_lines = diff_raw.lines().count();
    context.diff = diff_raw
        .lines()
        .take(MAX_DIFF_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    context.log = log;
    base.clone_into(&mut context.base_branch);
    Some(base.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_item_should_extract_from_branch() {
        assert_eq!(work_item_from_branch("feature/11763-desc"), "11763");
        assert_eq!(work_item_from_branch("main"), "");
    }

    #[test]
    fn work_item_should_ignore_sprint_branch_number() {
        assert_eq!(work_item_from_branch("sprint/110"), "");
        assert_eq!(work_item_from_branch("refs/heads/sprint/110"), "");
        assert_eq!(
            work_item_from_branch("sprint/110/feature/11763-desc"),
            "11763"
        );
    }

    #[test]
    fn targets_should_default_to_sprint_and_dev() {
        let ctx = ChangeContext {
            branch: "f/1".to_owned(),
            source_ref: "refs/heads/f/1".to_owned(),
            base_branch: "dev".to_owned(),
            sprint_branch: "sprint/12".to_owned(),
            diff: String::new(),
            diff_original_lines: 0,
            log: String::new(),
            work_item_id: String::new(),
            remote: None,
        };
        assert_eq!(resolve_targets(&ctx, &[]), vec!["sprint/12", "dev"]);
        assert_eq!(resolve_targets(&ctx, &["dev".to_owned()]), vec!["dev"]);
        assert_eq!(
            resolve_targets(&ctx, &["sprint".to_owned(), "sprint/12".to_owned()]),
            vec!["sprint/12"]
        );
        assert_eq!(
            resolve_targets(&ctx, &["sprint".to_owned()]),
            vec!["sprint/12"]
        );
    }

    #[test]
    fn latest_sprint_should_include_remote_tracking_branches() {
        let branches = "origin/HEAD -> origin/main\norigin/sprint/10\norigin/sprint/11-hotfix\norigin/sprint/12";
        assert_eq!(latest_sprint_branch(branches), Some("sprint/12".to_owned()));
    }

    #[test]
    fn latest_sprint_should_ignore_malformed_branch_names() {
        let branches = "origin/sprint/foo\norigin/sprint/12x\norigin/sprint/abc/extra";
        assert_eq!(latest_sprint_branch(branches), None);
    }

    #[test]
    fn latest_sprint_should_accept_current_and_worktree_markers() {
        let branches = "  sprint/10\n* sprint/11\n+ sprint/12";
        assert_eq!(latest_sprint_branch(branches), Some("sprint/12".to_owned()));
    }

    /// Simula um repositório a partir das refs existentes e registra chamadas.
    fn fake_git<'a>(
        refs: &'a [&'a str],
        local_behind: bool,
        branches: &'a str,
        calls: &'a mut Vec<String>,
    ) -> impl FnMut(&[&str]) -> Result<String> + 'a {
        move |args: &[&str]| {
            calls.push(args.join(" "));
            let failure = || {
                Err(AppError::Git {
                    message: format!("falhou: {}", args.join(" ")),
                })
            };
            match args {
                ["branch", "--show-current"] => Ok("feature/42".to_owned()),
                ["branch", "--list", ..] => Ok(String::new()),
                ["branch", "-r"] => Ok(branches.to_owned()),
                ["rev-parse", "--verify", "--quiet", name] => {
                    if refs.contains(name) {
                        Ok("oid".to_owned())
                    } else {
                        failure()
                    }
                }
                ["merge-base", "--is-ancestor", ..] => {
                    if local_behind {
                        Ok(String::new())
                    } else {
                        failure()
                    }
                }
                ["diff", range] => Ok(format!("diff {range}")),
                ["log", ..] => Ok("log".to_owned()),
                _ => failure(),
            }
        }
    }

    #[test]
    fn collect_should_diff_against_latest_sprint_instead_of_master() {
        let mut calls = Vec::new();
        let refs = [
            "refs/remotes/origin/sprint/12",
            "refs/remotes/origin/master",
            "refs/heads/master",
        ];
        let branches = "origin/master\norigin/sprint/11\norigin/sprint/12";
        let context = collect_with(None, &[], fake_git(&refs, true, branches, &mut calls)).unwrap();

        assert_eq!(context.sprint_branch, "sprint/12");
        assert_eq!(context.base_branch, "origin/sprint/12");
        assert_eq!(context.diff, "diff origin/sprint/12...feature/42");
        assert!(calls.contains(&"log --oneline -50 origin/sprint/12..feature/42".to_owned()));
    }

    #[test]
    fn collect_should_prefer_origin_sprint_when_local_copy_is_behind() {
        let refs = ["refs/heads/sprint/12", "refs/remotes/origin/sprint/12"];
        let branches = "origin/sprint/12";

        let mut calls = Vec::new();
        let stale = collect_with(None, &[], fake_git(&refs, true, branches, &mut calls)).unwrap();
        assert_eq!(stale.base_branch, "origin/sprint/12");

        let mut calls = Vec::new();
        let ahead = collect_with(None, &[], fake_git(&refs, false, branches, &mut calls)).unwrap();
        assert_eq!(ahead.base_branch, "sprint/12");
    }

    #[test]
    fn collect_should_use_explicit_sprint_target_as_base() {
        let mut calls = Vec::new();
        let refs = [
            "refs/remotes/origin/sprint/11",
            "refs/remotes/origin/sprint/12",
        ];
        let branches = "origin/sprint/11\norigin/sprint/12";
        let targets = ["dev".to_owned(), "sprint/11".to_owned()];
        let context =
            collect_with(None, &targets, fake_git(&refs, true, branches, &mut calls)).unwrap();

        assert_eq!(context.base_branch, "origin/sprint/11");
        assert_eq!(context.sprint_branch, "sprint/12");
    }

    #[test]
    fn collect_should_fail_when_explicit_sprint_target_is_not_fetched() {
        let mut calls = Vec::new();
        let refs = ["refs/heads/master", "refs/remotes/origin/master"];
        let targets = ["sprint/13".to_owned()];
        let error = collect_with(
            None,
            &targets,
            fake_git(&refs, true, "origin/master", &mut calls),
        )
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("sprint/13"), "{message}");
        assert!(message.contains("git fetch"), "{message}");
        assert!(!calls.iter().any(|call| call.starts_with("diff")));
    }

    fn follow_up_context() -> ChangeContext {
        ChangeContext {
            branch: "feature/42".to_owned(),
            source_ref: "refs/heads/feature/42".to_owned(),
            base_branch: "origin/sprint/12".to_owned(),
            sprint_branch: "sprint/12".to_owned(),
            diff: "diff completo".to_owned(),
            diff_original_lines: 1,
            log: "log completo".to_owned(),
            work_item_id: "42".to_owned(),
            remote: None,
        }
    }

    /// Histórico linear `aaa -> bbb -> ccc` com a branch apontando para `tip`.
    fn follow_up_git(tip: &'static str) -> impl FnMut(&[&str]) -> Result<String> {
        let order = |oid: &str| ["aaa", "bbb", "ccc"].iter().position(|known| *known == oid);
        move |args: &[&str]| {
            let failure = || {
                Err(AppError::Git {
                    message: format!("falhou: {}", args.join(" ")),
                })
            };
            let resolve = |name: &str| {
                let name = name.trim_end_matches("^{commit}");
                if name == "feature/42" { tip } else { name }.to_owned()
            };
            match args {
                ["merge-base", "--is-ancestor", ancestor, descendant] => {
                    match (order(&resolve(ancestor)), order(&resolve(descendant))) {
                        (Some(left), Some(right)) if left <= right => Ok(String::new()),
                        _ => failure(),
                    }
                }
                ["rev-parse", "--verify", name] => Ok(resolve(name)),
                ["diff", base, branch] => Ok(format!("diff {base} {branch}\nlinha")),
                ["log", "--oneline", "-50", range] => Ok(format!("log {range}")),
                _ => failure(),
            }
        }
    }

    #[test]
    fn follow_up_should_diff_only_changes_after_the_merged_commit() {
        let mut context = follow_up_context();
        let base =
            narrow_to_follow_up_with(&mut context, &["bbb".to_owned()], follow_up_git("ccc"));

        assert_eq!(base.as_deref(), Some("bbb"));
        assert_eq!(context.diff, "diff bbb feature/42\nlinha");
        assert_eq!(context.diff_original_lines, 2);
        assert_eq!(context.log, "log bbb..feature/42");
        assert_eq!(context.base_branch, "bbb");
    }

    #[test]
    fn follow_up_should_use_oldest_merged_commit_across_targets() {
        for commits in [["bbb", "aaa"], ["aaa", "bbb"]] {
            let mut context = follow_up_context();
            let commits = commits.map(str::to_owned);
            let base = narrow_to_follow_up_with(&mut context, &commits, follow_up_git("ccc"));
            assert_eq!(base.as_deref(), Some("aaa"));
        }
    }

    #[test]
    fn follow_up_should_keep_full_context_when_narrowing_is_not_reliable() {
        // Commit fora do histórico (rebase), nada novo desde o PR e sem PRs.
        for (commits, tip) in [
            (vec!["zzz".to_owned()], "ccc"),
            (vec!["0ff".to_owned()], "ccc"),
            (vec!["ccc".to_owned()], "ccc"),
            (Vec::new(), "ccc"),
        ] {
            let mut context = follow_up_context();
            let base = narrow_to_follow_up_with(&mut context, &commits, follow_up_git(tip));
            assert_eq!(base, None, "{commits:?}");
            assert_eq!(context.diff, "diff completo");
            assert_eq!(context.log, "log completo");
            assert_eq!(context.base_branch, "origin/sprint/12");
        }
    }

    #[test]
    fn remote_should_parse_modern_https() {
        let r = parse_azure_remote("https://dev.azure.com/minhaorg/meuproj/_git/meurepo").unwrap();
        assert_eq!(r.organization, "minhaorg");
        assert_eq!(r.repository, "meurepo");
    }

    #[test]
    fn remote_should_parse_modern_https_with_embedded_username() {
        let r = parse_azure_remote(
            "https://ibsbiosistemico@dev.azure.com/ibsbiosistemico/AGROTRACE/_git/agrotrace-v3",
        )
        .unwrap();

        assert_eq!(
            r,
            RepositoryRemote {
                organization: "ibsbiosistemico".to_owned(),
                project: "AGROTRACE".to_owned(),
                repository: "agrotrace-v3".to_owned(),
            }
        );
    }

    #[test]
    fn remote_should_parse_https_credentials_and_legacy_host() {
        let modern =
            parse_azure_remote("https://user:password@dev.azure.com/org/project/_git/repo.git")
                .unwrap();
        assert_eq!(modern.repository, "repo");

        let legacy =
            parse_azure_remote("https://user@org.visualstudio.com/project/_git/repo").unwrap();
        assert_eq!(legacy.organization, "org");
        assert_eq!(legacy.project, "project");
        assert_eq!(legacy.repository, "repo");
    }

    #[test]
    fn non_azure_remote_is_not_parseable_for_onboarding() {
        assert!(parse_azure_remote("https://github.com/org/project.git").is_none());
        assert!(parse_azure_remote("not a remote URL").is_none());
        assert!(parse_azure_remote("https://org.visualstudio.com").is_none());
    }

    #[test]
    fn collect_for_refs_should_construct_exact_diff_and_log_ranges() {
        let mut calls = Vec::new();
        let mut command = |args: &[&str]| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>());
            if args == ["rev-parse", "--verify", "refs/heads/feature/42"] {
                return Err(AppError::Git {
                    message: "local ref absent".to_owned(),
                });
            }
            if args == ["rev-parse", "--verify", "refs/heads/dev"] {
                return Err(AppError::Git {
                    message: "local ref absent".to_owned(),
                });
            }
            if args == ["rev-parse", "--verify", "origin/feature/42"]
                || args == ["rev-parse", "--verify", "origin/dev"]
            {
                return Ok("resolved".to_owned());
            }
            if args == ["diff", "origin/dev...origin/feature/42"] {
                return Ok("diff output".to_owned());
            }
            if args == ["log", "--oneline", "-50", "origin/dev..origin/feature/42"] {
                return Ok("log output".to_owned());
            }
            Err(AppError::Git {
                message: format!("comando não roteado: {}", args.join(" ")),
            })
        };
        let context = collect_for_refs_with(
            "refs/heads/feature/42",
            "refs/heads/dev",
            &mut command,
            Some(RepositoryRemote {
                organization: "org".to_owned(),
                project: "project".to_owned(),
                repository: "repo".to_owned(),
            }),
        )
        .unwrap();

        assert_eq!(context.source_ref, "refs/heads/feature/42");
        assert_eq!(context.base_branch, "origin/dev");
        assert_eq!(context.diff, "diff output");
        assert_eq!(context.log, "log output");
        assert!(calls.contains(&vec![
            "diff".to_owned(),
            "origin/dev...origin/feature/42".to_owned()
        ]));
        assert!(calls.contains(&vec![
            "log".to_owned(),
            "--oneline".to_owned(),
            "-50".to_owned(),
            "origin/dev..origin/feature/42".to_owned()
        ]));
    }

    #[test]
    fn exact_pr_refs_should_reject_missing_or_unresolvable_refs_without_fallback() {
        let error = collect_for_refs("", "refs/heads/dev").unwrap_err();
        assert!(error.to_string().contains("sourceRefName"));

        let error = collect_for_refs("refs/heads/not-local", "refs/heads/dev").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("not-local"));
        assert!(message.contains("fetch"));
        assert!(!message.contains("fallback"));

        let error = collect_for_refs("refs/heads/not-local", "").unwrap_err();
        assert!(error.to_string().contains("targetRefName"));

        let mut calls = Vec::new();
        let mut command = |args: &[&str]| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>());
            if args == ["rev-parse", "--verify", "refs/heads/feature/42"] {
                return Ok("source sha".to_owned());
            }
            Err(AppError::Git {
                message: "ref ausente".to_owned(),
            })
        };
        let error = collect_for_refs_with(
            "refs/heads/feature/42",
            "refs/heads/not-local-target",
            &mut command,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("not-local-target"));
        assert!(error.to_string().contains("fetch"));
        assert_eq!(calls.len(), 3);
        assert!(!calls.iter().any(|call| {
            call.iter().any(|arg| {
                matches!(arg.as_str(), "dev" | "main" | "master") || arg.starts_with("sprint/")
            })
        }));
    }

    #[test]
    fn fingerprint_should_capture_repository_branch_and_requested_ref_oids() {
        let branch = git(&["branch", "--show-current"]).expect("branch do teste");
        let mut targets = vec!["main".to_owned()];
        if !branch.is_empty() && branch != "main" {
            targets.push(branch.clone());
        }
        let fingerprint =
            GitContextFingerprint::capture("", &targets).expect("fingerprint do checkout");

        assert!(!fingerprint.repository.is_empty());
        assert_eq!(fingerprint.source_branch, branch);
        assert!(!fingerprint.source_oid.is_empty());
        assert_eq!(fingerprint.target_oids.len(), targets.len());
        for target in &targets {
            assert!(fingerprint.target_oids.contains_key(target));
            if !branch.is_empty() {
                assert!(!fingerprint.target_oids[target].is_empty());
            }
        }
    }

    #[test]
    fn session_repo_or_source_branch_mismatch_blocks_publish() {
        let mut fingerprint = GitContextFingerprint::capture("", &[]).expect("fingerprint");
        fingerprint.repository = "outro-checkout".to_owned();
        assert_eq!(
            fingerprint.compare_current(),
            FingerprintStatus::RepositoryOrBranchChanged
        );
        let mut fingerprint = GitContextFingerprint::capture("", &[]).expect("fingerprint");
        fingerprint.source_branch = "outro-branch".to_owned();
        assert_eq!(
            fingerprint.compare_current(),
            FingerprintStatus::RepositoryOrBranchChanged
        );
    }

    #[test]
    fn changed_or_missing_fingerprint_requires_publish_confirmation() {
        let current = GitContextFingerprint::capture("", &[]).expect("fingerprint");
        let mut changed = current.clone();
        changed.source_oid = "changed-oid".to_owned();
        assert_eq!(changed.compare_current(), FingerprintStatus::ObjectChanged);
        changed.source_oid.clear();
        assert_eq!(changed.compare_current(), FingerprintStatus::ObjectChanged);
        let mut missing_target = current;
        missing_target
            .target_oids
            .insert("missing-target".to_owned(), String::new());
        assert_eq!(
            missing_target.compare_current(),
            FingerprintStatus::ObjectChanged
        );
    }

    #[test]
    fn exact_session_fingerprint_has_no_extra_publish_gate() {
        let fingerprint = GitContextFingerprint::capture("", &[]).expect("fingerprint");
        assert_eq!(fingerprint.compare_current(), FingerprintStatus::Exact);
    }

    #[test]
    fn fingerprint_capture_failure_blocks_publish_without_remote_call() {
        let fingerprint = GitContextFingerprint::default();
        assert_ne!(fingerprint.compare_current(), FingerprintStatus::Exact);
        assert!(fingerprint.repository.is_empty());
    }
}
