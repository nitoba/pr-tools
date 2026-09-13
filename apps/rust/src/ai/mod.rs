//! Geração via IA — `aisdk` (OpenAI-compatible) + subprocessos `codex`/`opencode`.
//!
//! Espelha `ai_description_generator.dart` + `genkit_compatible_generator.dart`
//! + `description_normalizer.dart` + `description_limits.dart`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;

use crate::config::Config;
use crate::error::{AppError, Result};

/// Limite estrito do Azure (< 4000).
pub const AZURE_PR_DESCRIPTION_MAX_LENGTH: usize = 4000;

/// Regras injetadas nos prompts.
pub const AZURE_PR_DESCRIPTION_PROMPT_RULES: &str = r#"REGRAS OBRIGATÓRIAS DO CAMPO "body":
- O body deve ter menos de 4000 caracteres (limite estrito: no máximo 3999).
- Conte todos os caracteres do Markdown, incluindo espaços e quebras de linha.
- Preserve somente informações sustentadas pelo contexto; seja conciso e priorize o que mudou e por quê.
- O Work Item é a intenção/requisito funcional, não evidência de implementação.
- Git Log e Git Diff são a evidência da implementação.
- Só descreva um critério de aceitação como implementado quando houver evidência correspondente no Git Log ou Git Diff; caso contrário, trate-o como não confirmado.
- Não copie o Work Item como corpo do PR; descreva a mudança real da branch.
- Nunca ultrapasse esse limite, não inclua o contexto Git na resposta e não escreva texto fora do JSON."#;

/// Descrição de PR normalizada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrDescription {
    /// Título curto (≤ 80 chars).
    pub title: String,
    /// Body em Markdown (< 4000 chars).
    pub body: String,
}

/// Retorna `true` se o body cabe no limite do Azure.
#[must_use]
pub fn is_within_limit(body: &str) -> bool {
    body.chars().count() < AZURE_PR_DESCRIPTION_MAX_LENGTH
}

/// Valida o limite.
///
/// # Errors
///
/// Retorna [`AppError::DescriptionTooLong`] se exceder.
pub fn validate_description(desc: &PrDescription) -> Result<()> {
    if !is_within_limit(&desc.body) {
        return Err(AppError::DescriptionTooLong {
            length: desc.body.chars().count(),
        });
    }
    Ok(())
}

/// Instruções do rewriter (segunda chamada quando excede 4000).
pub const REWRITE_INSTRUCTIONS: &str = r"Você é um revisor técnico. Reescreva a descrição abaixo para ter menos de 4000 caracteres (máximo 3999), contando tudo, sem inventar nem truncar o sentido. Responda somente com JSON {title, body}.";

/// Normaliza saída bruta do modelo para [`PrDescription`].
///
/// Espelha `normalizeDescription`: strip `<think>`, fences, recuperação
/// JSON por brace-matching, fallback `TÍTULO:` / primeira linha.
#[must_use]
pub fn normalize_description(raw: &str, branch: &str) -> PrDescription {
    let mut text = strip_think(raw);
    text = strip_fences(&text).into_owned();
    if let Some(desc) = parse_json_object(&text) {
        return clean_description(desc, branch);
    }
    fallback_from_text(&text, branch)
}

fn strip_think(s: &str) -> String {
    // Remove blocos <think>...</think> (inclui variações de case).
    let mut out = s.to_owned();
    while let Some(start) = find_ascii_case_insensitive(&out, "<think>") {
        let content_start = start + "<think>".len();
        let Some(relative_end) = find_ascii_case_insensitive(&out[content_start..], "</think>")
        else {
            break;
        };
        let end = content_start + relative_end + "</think>".len();
        out.replace_range(start..end, "");
    }
    out
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn strip_fences(s: &str) -> std::borrow::Cow<'_, str> {
    let t = s.trim();
    if t.starts_with("```") {
        if let Some(first_nl) = t.find('\n') {
            let mut rest = &t[first_nl + 1..];
            if let Some(end) = rest.rfind("```") {
                rest = &rest[..end];
            }
            return std::borrow::Cow::Owned(rest.trim().to_owned());
        }
    }
    std::borrow::Cow::Borrowed(s)
}

/// Tenta extrair `{...}` balanceado respeitando strings/escapes.
fn parse_json_object(text: &str) -> Option<PrDescription> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escape = false;
    let mut end: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let slice = text.get(start..=end)?;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(slice) {
        let title = v
            .get("title")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_owned();
        let body = v
            .get("body")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_owned();
        if !title.is_empty() || !body.is_empty() {
            return Some(PrDescription { title, body });
        }
    }
    None
}

fn fallback_from_text(text: &str, branch: &str) -> PrDescription {
    // `TÍTULO: ...`
    for line in text.lines() {
        let up = line.to_uppercase();
        if up.starts_with("TÍTULO:") || up.starts_with("TITULO:") {
            if let Some((_, rest)) = line.split_once(':') {
                let title = rest.trim().to_owned();
                let body = text.replacen(line, "", 1).trim().to_owned();
                return clean_description(
                    PrDescription {
                        title,
                        body: if body.is_empty() {
                            "Sem descrição gerada.".to_owned()
                        } else {
                            body
                        },
                    },
                    branch,
                );
            }
        }
    }
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim();
    let rest: String = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    clean_description(
        PrDescription {
            title: if first.is_empty() {
                branch.to_owned()
            } else {
                first.to_owned()
            },
            body: rest,
        },
        branch,
    )
}

fn clean_description(mut d: PrDescription, branch: &str) -> PrDescription {
    d.title = d.title.trim().trim_matches('"').trim().to_owned();
    if d.title.is_empty() {
        "Atualiza código".clone_into(&mut d.title);
    }
    // Trunca por largura de display (células de terminal), não por chars:
    // CJK/emoji ocupam 2 células e desalinhariam a TUI se contados como 1.
    let title_width: usize = d
        .title
        .chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    if title_width > 80 {
        let mut acc = 0usize;
        let mut out = String::new();
        for ch in d.title.chars() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if acc + w > 80 {
                break;
            }
            acc += w;
            out.push(ch);
        }
        out.trim().clone_into(&mut d.title);
    }
    d.body.replace("---", "").trim().clone_into(&mut d.body);
    // `\n` literais → reais somente se há header/lista.
    if d.body.contains("\\n") && (d.body.contains('#') || d.body.contains("- [")) {
        d.body = d.body.replace("\\n", "\n");
    }
    // Corta contexto Git vazado.
    if let Some(idx) = d.body.find("## Contexto Git") {
        d.body = d.body[..idx].trim().to_owned();
    }
    if d.body.is_empty() {
        "Sem descrição gerada.".clone_into(&mut d.body);
    }
    if d.title.is_empty() {
        branch.clone_into(&mut d.title);
    }
    d
}

/// Monta o prompt de usuário para `desc` (espelha `describe_prompt.dart`).
#[must_use]
pub fn build_describe_prompt(
    branch: &str,
    targets: &[String],
    work_item_id: &str,
    functional_context: Option<&crate::azure::work_items::FunctionalWorkItemContext>,
    log: &str,
    diff: &str,
) -> String {
    let functional = functional_context.map_or_else(String::new, |context| {
        let mut lines = vec![
            "## Contexto funcional do Work Item".to_owned(),
            String::new(),
            format!("ID: {}", context.id),
            format!("Título: {}", context.title),
            format!("Tipo: {}", context.work_item_type),
        ];
        if !context.area_path.is_empty() {
            lines.push(format!("Área: {}", context.area_path));
        }
        if let Some(description) = &context.description {
            lines.push(format!("Descrição: {description}"));
        }
        if let Some(acceptance_criteria) = &context.acceptance_criteria {
            lines.push(format!("Critérios de aceitação: {acceptance_criteria}"));
        }
        lines.push(String::new());
        lines.join("\n")
    });
    format!(
        "{functional}## Contexto Git\n\n**Branch:** {branch}\n**Base branches alvo:** {}\n{}### Git Log (commits desde a base)\n\n```\n{log}\n```\n\n### Git Diff\n\n```diff\n{diff}\n```\n\n### Instruções de saída\n\nGere somente o objeto JSON solicitado pelo prompt de sistema.\n{AZURE_PR_DESCRIPTION_PROMPT_RULES}\n",
        targets.join(", "),
        if work_item_id.is_empty() || functional_context.is_some() {
            String::new()
        } else {
            format!("**Work Item:** #{work_item_id}\n")
        },
        functional = functional,
    )
}

/// Gera via `aisdk` contra endpoint OpenAI-compatible.
///
/// Usa `LanguageModelRequest::builder()` (type-state) com `base_url`/`api_key`.
/// Mantém `codex`/`opencode` via subprocesso com fallback — espelha o Dart.
///
/// # Errors
///
/// Retorna [`AppError::Ai`] se o provider falhar ao construir, em timeout,
/// transporte ou resposta inválida.
pub async fn generate_via_compatible(
    config: &Config,
    system: &str,
    prompt: &str,
) -> Result<String> {
    use aisdk::core::{DynamicModel, LanguageModelRequest};
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
        .map_err(|e| AppError::Ai {
            provider: "openai-compatible".to_owned(),
            message: e.to_string(),
        })?;

    let system_owned = if config.compatible_reasoning == "provider-default" {
        system.to_owned()
    } else {
        format!(
            "{system}\nUse nível de reasoning {}.",
            config.compatible_reasoning
        )
    };

    let result = LanguageModelRequest::builder()
        .model(provider)
        .system(system_owned.as_str())
        .prompt(prompt)
        .build()
        .generate_text()
        .await
        .map_err(|e| AppError::Ai {
            provider: "openai-compatible".to_owned(),
            message: e.to_string(),
        })?;
    let text = result.text().unwrap_or_default();
    if text.trim().is_empty() {
        return Err(AppError::Ai {
            provider: "openai-compatible".to_owned(),
            message: "saída vazia".to_owned(),
        });
    }
    Ok(text)
}

/// Gera via `codex exec` (subprocesso, stdin = system+prompt).
///
/// # Errors
///
/// Retorna [`AppError::Ai`] se o binário falhar ou sair diferente de zero.
pub async fn generate_via_codex(config: &Config, system: &str, prompt: &str) -> Result<String> {
    let executable = if config.codex_path.trim().is_empty() {
        "codex"
    } else {
        config.codex_path.trim()
    };
    let mut args = vec![
        "exec".to_owned(),
        "-m".to_owned(),
        config.codex_model.clone(),
        "-c".to_owned(),
        "approval_policy=never".to_owned(),
        "-c".to_owned(),
        "sandbox_mode=read-only".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--color".to_owned(),
        "never".to_owned(),
    ];
    if config.codex_reasoning != "provider-default" {
        args.push("-c".to_owned());
        args.push(format!("model_reasoning_effort={}", config.codex_reasoning));
    }
    args.push("-".to_owned());
    run_subprocess(executable, &args, &format!("{system}\n\n{prompt}")).await
}

/// Gera via `opencode run` (arquivo temporário de prompt).
///
/// # Errors
///
/// Retorna [`AppError::Ai`] se a escrita do prompt temporário falhar, o
/// binário `opencode` sair diferente de zero ou a saída vier vazia.
pub async fn generate_via_opencode(config: &Config, system: &str, prompt: &str) -> Result<String> {
    let executable = if config.opencode_path.trim().is_empty() {
        "opencode"
    } else {
        config.opencode_path.trim()
    };
    let path = std::env::temp_dir().join(format!("prt-opencode-{}.md", std::process::id()));
    tokio::fs::write(&path, format!("{system}\n\n{prompt}"))
        .await
        .map_err(|e| AppError::Ai {
            provider: "opencode".to_owned(),
            message: e.to_string(),
        })?;
    let res = run_subprocess(
        executable,
        &[
            "run".to_owned(),
            "--format".to_owned(),
            "default".to_owned(),
            "--pure".to_owned(),
            "--agent".to_owned(),
            "general".to_owned(),
            "--model".to_owned(),
            config.opencode_model.clone(),
            "--file".to_owned(),
            path.to_string_lossy().into_owned(),
            "Gere o JSON solicitado no arquivo de contexto.".to_owned(),
        ],
        "",
    )
    .await;
    let _ = tokio::fs::remove_file(&path).await;
    res
}

async fn run_subprocess(cmd: &str, args: &[String], stdin_text: &str) -> Result<String> {
    run_subprocess_with_timeout(cmd, args, stdin_text, std::time::Duration::from_secs(300)).await
}

async fn run_subprocess_with_timeout(
    cmd: &str,
    args: &[String],
    stdin_text: &str,
    wait: std::time::Duration,
) -> Result<String> {
    use tokio::process::Command;
    let mut child = None;
    let mut last_spawn_error = None;
    for candidate in crate::process::command_candidates(cmd) {
        match Command::new(&candidate)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(process) => {
                child = Some(process);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                last_spawn_error = Some(error);
            }
            Err(error) => {
                return Err(AppError::Ai {
                    provider: cmd.to_owned(),
                    message: format!("falha ao executar {cmd}: {error}"),
                });
            }
        }
    }
    let Some(mut child) = child else {
        let error = last_spawn_error.map_or_else(
            || "comando não encontrado".to_owned(),
            |error| error.to_string(),
        );
        return Err(AppError::Ai {
            provider: cmd.to_owned(),
            message: format!("falha ao executar {cmd}: {error}"),
        });
    };
    let operation = async {
        if !stdin_text.is_empty() {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                stdin
                    .write_all(stdin_text.as_bytes())
                    .await
                    .map_err(|e| AppError::Ai {
                        provider: cmd.to_owned(),
                        message: e.to_string(),
                    })?;
            }
        }
        child.wait_with_output().await.map_err(|e| AppError::Ai {
            provider: cmd.to_owned(),
            message: e.to_string(),
        })
    };
    let out = tokio::time::timeout(wait, operation)
        .await
        .map_err(|_| AppError::Ai {
            provider: cmd.to_owned(),
            message: format!("timeout após {}s", wait.as_secs()),
        })??;
    if !out.status.success() {
        return Err(AppError::Ai {
            provider: cmd.to_owned(),
            message: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if text.is_empty() {
        return Err(AppError::Ai {
            provider: cmd.to_owned(),
            message: "saída vazia".to_owned(),
        });
    }
    Ok(text)
}

/// Gera com fallback na ordem de `config.providers`, acumulando erros.
///
/// # Errors
///
/// Retorna [`AppError::Ai`] com provider `"todos"` se todos os providers
/// configurados falharem.
pub async fn generate_with_fallback(
    config: &Config,
    system: &str,
    prompt: &str,
    report: impl Fn(&str, &str),
) -> Result<String> {
    let mut errors = Vec::new();
    for provider in &config.providers {
        let model = match provider.as_str() {
            "codex" => config.codex_model.as_str(),
            "opencode" => config.opencode_model.as_str(),
            _ => config.compatible_model.as_str(),
        };
        report(provider, model);
        let out = match provider.as_str() {
            "codex" => generate_via_codex(config, system, prompt).await,
            "opencode" => generate_via_opencode(config, system, prompt).await,
            _ => generate_via_compatible(config, system, prompt).await,
        };
        match out {
            Ok(text) => return Ok(text),
            Err(e) => errors.push(format!("{provider}: {e}")),
        }
    }
    Err(AppError::Ai {
        provider: "todos".to_owned(),
        message: errors.join("; "),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn functional_context() -> crate::azure::work_items::FunctionalWorkItemContext {
        crate::azure::work_items::FunctionalWorkItemContext {
            id: 11763,
            title: "Enriquecer a descrição".to_owned(),
            work_item_type: "User Story".to_owned(),
            area_path: "Produto\\CLI".to_owned(),
            description: Some("A descrição funcional".to_owned()),
            acceptance_criteria: Some("O critério fica evidenciado no diff".to_owned()),
        }
    }

    #[test]
    fn describe_prompt_should_include_projected_functional_context() {
        let context = functional_context();
        let prompt = build_describe_prompt(
            "feature/11763-contexto",
            &["dev".to_owned()],
            "11763",
            Some(&context),
            "abc123 commit",
            "diff --git a/src/lib.rs b/src/lib.rs",
        );

        assert!(prompt.contains("## Contexto funcional do Work Item"));
        assert!(prompt.contains("ID: 11763"));
        assert!(prompt.contains("Título: Enriquecer a descrição"));
        assert!(prompt.contains("Tipo: User Story"));
        assert!(prompt.contains("Área: Produto\\CLI"));
        assert!(prompt.contains("Descrição: A descrição funcional"));
        assert!(prompt.contains("Critérios de aceitação: O critério fica evidenciado no diff"));
        assert!(prompt.contains("## Contexto Git"));
        assert!(prompt.find("## Contexto funcional do Work Item") < prompt.find("## Contexto Git"));
    }

    #[test]
    fn no_work_item_should_skip_azure_and_functional_prompt() {
        let prompt = build_describe_prompt(
            "feature/sem-work-item",
            &["dev".to_owned()],
            "",
            None,
            "abc123 commit",
            "diff",
        );

        assert!(!prompt.contains("## Contexto funcional do Work Item"));
        assert!(!prompt.contains("Work Item:"));
        assert!(prompt.contains("## Contexto Git"));
    }

    #[test]
    fn describe_prompt_should_separate_intent_from_git_evidence() {
        let prompt = build_describe_prompt(
            "feature/11763-contexto",
            &["dev".to_owned()],
            "11763",
            Some(&functional_context()),
            "abc123 commit",
            "diff",
        );

        assert!(prompt.contains("intenção/requisito funcional"));
        assert!(prompt.contains("Git Log e Git Diff são a evidência da implementação"));
        assert!(prompt.contains("critério de aceitação como implementado"));
        assert!(prompt.contains("Não copie o Work Item como corpo do PR"));
    }

    #[test]
    fn normalizer_should_parse_json_with_fences() {
        let raw = "```json\n{\"title\": \"Atualiza fluxo\", \"body\": \"## Descrição\\nX\"}\n```";
        let d = normalize_description(raw, "feature/1");
        assert_eq!(d.title, "Atualiza fluxo");
        assert!(d.body.contains("## Descrição"));
    }

    #[test]
    fn normalizer_should_strip_think_and_context() {
        let raw = "<think>raciocínio</think>\n{\"title\": \"T\", \"body\": \" Corpo ## Contexto Git vazado\"}";
        let d = normalize_description(raw, "b");
        assert_eq!(d.title, "T");
        assert!(!d.body.contains("Contexto Git"));
    }

    #[test]
    fn normalizer_should_strip_think_after_case_changing_unicode() {
        let raw = "\u{212a}<THINK>segredo</THINK>\n{\"title\":\"T\",\"body\":\"B\"}";
        let d = normalize_description(raw, "b");
        assert_eq!(d.title, "T");
        assert_eq!(d.body, "B");
    }

    #[test]
    fn normalizer_should_fallback_to_first_line() {
        let d = normalize_description("Só um texto livre", "minha-branch");
        assert_eq!(d.title, "Só um texto livre");
    }

    #[test]
    fn limits_should_reject_4000_chars() {
        assert!(is_within_limit(&"a".repeat(3999)));
        assert!(!is_within_limit(&"a".repeat(4000)));
        let d = PrDescription {
            title: "t".to_owned(),
            body: "a".repeat(4000),
        };
        assert!(validate_description(&d).is_err());
    }

    #[test]
    fn limits_should_count_unicode_characters() {
        assert!(is_within_limit(&"é".repeat(3999)));
    }

    #[tokio::test]
    async fn compatible_provider_should_reject_empty_text() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 8192];
            let _ = socket.read(&mut request).await.unwrap();
            let body = r#"{"id":"audit","object":"chat.completion","created":0,"model":"audit","choices":[]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = Config {
            base_url: format!("http://{address}/v1"),
            compatible_model: "audit".to_owned(),
            ..Config::default()
        };
        let error = generate_via_compatible(&config, "system", "prompt")
            .await
            .unwrap_err();
        server.await.unwrap();
        assert!(error.to_string().contains("saída vazia"));
    }

    #[tokio::test]
    async fn cancelling_subprocess_should_terminate_child() {
        let dir = tempfile::tempdir().unwrap();
        let started = dir.path().join("started");
        let completed = dir.path().join("completed");

        #[cfg(windows)]
        let (command, args) = {
            let started = started.to_string_lossy().replace('\'', "''");
            let completed = completed.to_string_lossy().replace('\'', "''");
            let script = format!(
                "Set-Content -LiteralPath '{started}' -Value started; Start-Sleep -Milliseconds 1000; Set-Content -LiteralPath '{completed}' -Value completed; Write-Output done"
            );
            (
                "powershell.exe".to_owned(),
                vec![
                    "-NoProfile".to_owned(),
                    "-NonInteractive".to_owned(),
                    "-Command".to_owned(),
                    script,
                ],
            )
        };

        #[cfg(not(windows))]
        let (command, args) = {
            let script = "printf started > \"$1\"; sleep 1; printf completed > \"$2\"; printf done";
            (
                "sh".to_owned(),
                vec![
                    "-c".to_owned(),
                    script.to_owned(),
                    "prt-test".to_owned(),
                    started.to_string_lossy().into_owned(),
                    completed.to_string_lossy().into_owned(),
                ],
            )
        };

        let task = tokio::spawn(async move { run_subprocess(&command, &args, "").await });
        for _ in 0..100 {
            if started.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(started.exists(), "o subprocesso não chegou a iniciar");

        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        assert!(
            !completed.exists(),
            "o subprocesso continuou após o cancelamento"
        );
    }

    #[tokio::test]
    async fn subprocess_timeout_should_cover_stdin_write() {
        let command = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let args = vec![
            "--ignored".to_owned(),
            "--exact".to_owned(),
            "ai::tests::subprocess_blocking_child".to_owned(),
            "--nocapture".to_owned(),
            "--test-threads=1".to_owned(),
        ];
        let input = "x".repeat(16 * 1024 * 1024);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            run_subprocess_with_timeout(&command, &args, &input, std::time::Duration::from_secs(1)),
        )
        .await
        .expect("o timeout interno não cobriu a escrita no stdin")
        .unwrap_err();
        assert!(result.to_string().contains("timeout após 1s"));
    }

    #[test]
    #[ignore = "processo auxiliar para o teste de timeout"]
    fn subprocess_blocking_child() {
        std::thread::sleep(std::time::Duration::from_secs(5));
        println!("done");
    }
}
