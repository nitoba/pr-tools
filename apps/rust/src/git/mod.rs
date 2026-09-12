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

    // Descobre `sprint/<n>` com maior n.
    let branches = git(&["branch", "--list", "sprint/*", "dev", "main", "master"])?;
    let mut sprint_branch = String::new();
    let mut sprint_max: i64 = -1;
    for b in branches
        .lines()
        .map(|l| l.trim().trim_start_matches("* ").trim())
    {
        if let Some(n) = b.strip_prefix("sprint/") {
            if let Ok(v) = n.parse::<i64>() {
                if v > sprint_max {
                    sprint_max = v;
                    b.clone_into(&mut sprint_branch);
                }
            }
        }
    }
    let has = |name: &str| {
        branches
            .lines()
            .any(|l| l.trim().trim_start_matches("* ").trim() == name)
    };
    let base_branch = if !sprint_branch.is_empty() {
        sprint_branch.clone()
    } else if has("dev") {
        "dev".to_owned()
    } else if has("main") {
        "main".to_owned()
    } else if has("master") {
        "master".to_owned()
    } else {
        return Err(AppError::Git {
            message: "nenhuma branch base encontrada (sprint/dev/main/master)".to_owned(),
        });
    };

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
    let remote_url = git(&["remote", "get-url", "origin"]).ok();
    let remote = remote_url.as_deref().and_then(parse_azure_remote);
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
    fn remote_should_parse_modern_https() {
        let r = parse_azure_remote("https://dev.azure.com/minhaorg/meuproj/_git/meurepo").unwrap();
        assert_eq!(r.organization, "minhaorg");
        assert_eq!(r.repository, "meurepo");
    }
}
