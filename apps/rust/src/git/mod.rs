//! Contexto Git — espelha `domain/change_context.dart` + `change_context_reader_live.dart`.
//!
//! Coleta branch, base (`sprint` > `dev` > `main` > `master`), diff truncado
//! em 8000 linhas, log (50 oneline) e remote Azure DevOps.

use regex::Regex;
use std::process::Command as ProcCommand;
use std::sync::OnceLock;

use crate::error::{AppError, Result};

/// Limite de linhas do diff (espelha o Dart).
pub const MAX_DIFF_LINES: usize = 8000;

/// Remote Azure DevOps parseado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRemote {
    /// Organização (`dev.azure.com/{org}`).
    pub organization: String,
    /// Projeto.
    pub project: String,
    /// Repositório.
    pub repository: String,
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

/// Extrai Work Item da branch (`(?:^|[/_-])(\d+)(?:$|[/_-])`).
///
/// # Panics
///
/// Entra em pânico na inicialização do `OnceLock` se a regex fixa for
/// inválida (inacessível em uso normal — o padrão é constante válida).
#[must_use]
pub fn work_item_from_branch(branch: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?:^|[/_-])(\d+)(?:$|[/_-])").expect("regex válida"));
    re.captures(branch)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        .unwrap_or_default()
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
                .trim_start_matches("* ")
                .trim_start_matches("remotes/origin/")
                .trim_start_matches("origin/");
            let captures = re.captures(branch)?;
            let sprint_number = captures.get(1)?.as_str().parse::<i64>().ok()?;
            Some((sprint_number, branch.to_owned()))
        })
        .max_by_key(|(sprint_number, _)| *sprint_number)
        .map(|(_, branch)| branch)
}

/// Resolve uma branch local ou sua referência de acompanhamento em `origin`.
fn resolve_ref(branch: &str) -> Option<String> {
    if branch.is_empty() {
        return None;
    }
    if git(&["rev-parse", "--verify", branch]).is_ok() {
        return Some(branch.to_owned());
    }
    let remote_branch = format!("origin/{branch}");
    git(&["rev-parse", "--verify", &remote_branch])
        .is_ok()
        .then_some(remote_branch)
}

fn resolve_pull_request_ref(remote_ref: &str) -> Option<String> {
    let branch = remote_ref.strip_prefix("refs/heads/")?;
    if branch.is_empty() {
        return None;
    }
    if git(&["rev-parse", "--verify", remote_ref]).is_ok() {
        return Some(remote_ref.to_owned());
    }
    let origin_ref = format!("origin/{branch}");
    git(&["rev-parse", "--verify", &origin_ref])
        .is_ok()
        .then_some(origin_ref)
}

fn pull_request_ranges(source: &str, target: &str) -> (String, String) {
    (
        format!("{target}...{source}"),
        format!("{target}..{source}"),
    )
}

/// Faz parse do remote `origin` (ssh, modern e legacy) para Azure.
#[must_use]
pub fn parse_azure_remote(url: &str) -> Option<RepositoryRemote> {
    // ssh: git@ssh.dev.azure.com:v3/org/project/repo
    if let Some(rest) = url.strip_prefix("git@ssh.dev.azure.com:v3/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 3 {
            return Some(RepositoryRemote {
                organization: parts[0].to_owned(),
                project: parts[1].to_owned(),
                repository: parts[2].trim_end_matches(".git").to_owned(),
            });
        }
    }
    // modern: https://dev.azure.com/org/project/_git/repo
    if let Some(rest) = url.strip_prefix("https://dev.azure.com/") {
        let rest = rest.trim_end_matches(".git");
        let parts: Vec<&str> = rest.split("/_git/").collect();
        if parts.len() == 2 {
            let left: Vec<&str> = parts[0].split('/').collect();
            if left.len() >= 2 {
                return Some(RepositoryRemote {
                    organization: left[0].to_owned(),
                    project: left[1..].join("/"),
                    repository: parts[1].split('/').next_back().unwrap_or("").to_owned(),
                });
            }
        }
    }
    // legacy: https://org.visualstudio.com/project/_git/repo
    if let Some(rest) = url.strip_prefix("https://") {
        if let Some(org) = rest.strip_suffix(".visualstudio.com") {
            return Some(RepositoryRemote {
                organization: org.to_owned(),
                project: String::new(),
                repository: String::new(),
            });
        }
        if rest.contains(".visualstudio.com/") {
            let (org, tail) = rest.split_once(".visualstudio.com/").unwrap_or(("", ""));
            let repo = tail.split("/_git/").last().unwrap_or("").to_owned();
            let project = tail.split("/_git/").next().unwrap_or("").to_owned();
            return Some(RepositoryRemote {
                organization: org.to_owned(),
                project,
                repository: repo,
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
    let source = resolve_pull_request_ref(source_ref).ok_or_else(|| AppError::Git {
        message: format!(
            "ref source {source_ref} não encontrada localmente; atualize/fetch as refs do PR"
        ),
    })?;
    let target = resolve_pull_request_ref(target_ref).ok_or_else(|| AppError::Git {
        message: format!(
            "ref target {target_ref} não encontrada localmente; atualize/fetch as refs do PR"
        ),
    })?;
    let (diff_range, log_range) = pull_request_ranges(&source, &target);
    let diff_raw = git(&["diff", &diff_range])?;
    let diff_original_lines = diff_raw.lines().count();
    let diff = diff_raw
        .lines()
        .take(MAX_DIFF_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let log = git(&["log", "--oneline", "-50", &log_range])?;
    let remote = origin_remote()?;
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
/// # Errors
///
/// Retorna [`AppError::Git`] se não for repo, branch protegida ou sem base.
pub fn collect(source: Option<&str>) -> Result<ChangeContext> {
    let current = git(&["branch", "--show-current"])?;
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
    let local_branches = git(&["branch", "--list", "sprint/*", "dev", "main", "master"])?;
    let remote_branches = git(&["branch", "-r"])?;
    let branches = format!("{local_branches}\n{remote_branches}");
    let sprint_branch = latest_sprint_branch(&branches).unwrap_or_default();
    let base_branch = [sprint_branch.as_str(), "dev", "main", "master"]
        .into_iter()
        .find_map(resolve_ref)
        .ok_or_else(|| AppError::Git {
            message: "nenhuma branch base encontrada (sprint/dev/main/master)".to_owned(),
        })?;

    // Diff: `diff base...source`, fallback `diff base source`.
    let diff_raw = git(&["diff", &format!("{base_branch}...{branch}")])
        .or_else(|_| git(&["diff", &base_branch, &branch]))?;
    let diff_original_lines = diff_raw.lines().count();
    let diff = diff_raw
        .lines()
        .take(MAX_DIFF_LINES)
        .collect::<Vec<_>>()
        .join("\n");

    let log = git(&[
        "log",
        "--oneline",
        "-50",
        &format!("{base_branch}..{branch}"),
    ])?;
    let remote = origin_remote()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_item_should_extract_from_branch() {
        assert_eq!(work_item_from_branch("feature/11763-desc"), "11763");
        assert_eq!(work_item_from_branch("main"), "");
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
    fn remote_should_parse_modern_https() {
        let r = parse_azure_remote("https://dev.azure.com/minhaorg/meuproj/_git/meurepo").unwrap();
        assert_eq!(r.organization, "minhaorg");
        assert_eq!(r.repository, "meurepo");
    }

    #[test]
    fn collect_for_refs_should_construct_exact_diff_and_log_ranges() {
        assert_eq!(
            pull_request_ranges("refs/heads/feature/42", "origin/dev"),
            (
                "origin/dev...refs/heads/feature/42".to_owned(),
                "origin/dev..refs/heads/feature/42".to_owned()
            )
        );
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
    }
}
