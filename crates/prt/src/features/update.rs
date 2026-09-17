//! Atualização automática do binário instalado via GitHub Releases.
//!
//! O comando baixa o artefato Rust correspondente à plataforma atual e o
//! substitui no caminho retornado por [`std::env::current_exe`]. Em Windows,
//! a troca é agendada em um processo auxiliar porque o executável em uso não
//! pode ser removido enquanto o comando está rodando.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use futures::StreamExt as _;
use tokio::io::AsyncWriteExt as _;

use crate::cli::VERSION;
use crate::error::{AppError, Result};

const DEFAULT_REPOSITORY: &str = "nitoba/pr-tools";

/// Eventos publicados durante o download e instalação do binário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateProgress {
    /// Localiza o binário atual e o asset da plataforma.
    Checking {
        /// Caminho do binário que será substituído.
        target: String,
        /// Nome do asset da release.
        asset: String,
    },
    /// Recebe um novo trecho do binário.
    Downloading {
        /// Quantidade já recebida em bytes.
        downloaded: u64,
        /// Tamanho total informado pelo servidor, quando disponível.
        total: Option<u64>,
    },
    /// Confere se o arquivo baixado responde como um `prt` válido.
    Validating,
    /// O arquivo foi validado e a versão foi identificada.
    Validated {
        /// Texto da versão retornado por `prt --version`.
        version: String,
    },
    /// Substitui o binário instalado pelo arquivo validado.
    Installing,
    /// A instalação terminou.
    Completed {
        /// Texto da versão instalada.
        version: String,
    },
}

/// Baixa e instala a versão mais recente do `prt`.
///
/// # Errors
///
/// Retorna [`AppError::Update`] se o repositório, a plataforma, o binário
/// baixado ou a substituição local forem inválidos. Erros de rede e escrita
/// também são propagados.
pub async fn run() -> Result<()> {
    let mut download_started = false;
    let version = run_with_progress(move |event| match event {
        UpdateProgress::Checking { target, asset } => {
            println!("→ verificando atualização do prt");
            println!("  destino: {target}");
            println!("  asset:   {asset}");
        }
        UpdateProgress::Downloading { .. } if !download_started => {
            download_started = true;
            println!("→ baixando a versão mais recente do GitHub");
        }
        UpdateProgress::Validating => println!("→ validando o binário baixado"),
        UpdateProgress::Validated { version } => println!("✓ versão baixada: {version}"),
        UpdateProgress::Installing => println!("→ instalando a atualização"),
        UpdateProgress::Downloading { .. } | UpdateProgress::Completed { .. } => {}
    })
    .await?;

    #[cfg(windows)]
    {
        let _ = version;
        println!("✓ atualização agendada; o novo binário será aplicado ao sair");
    }
    #[cfg(not(windows))]
    println!("✓ prt atualizado com sucesso: {version}");
    Ok(())
}

/// Baixa e instala a versão mais recente, emitindo o estado da operação.
///
/// O callback é síncrono de propósito: ele só deve atualizar um modelo local
/// ou enviar um evento para a UI. Nenhuma operação de rede ou disco deve ser
/// feita pelo callback.
///
/// # Errors
///
/// Retorna [`AppError::Update`] se o repositório, a plataforma, o binário
/// baixado ou a substituição local forem inválidos. Erros de rede e escrita
/// também são propagados.
pub async fn run_with_progress<F>(mut emit: F) -> Result<String>
where
    F: FnMut(UpdateProgress) + Send + 'static,
{
    let target = std::env::current_exe().map_err(|error| {
        update_error(format!(
            "não foi possível localizar o binário atual: {error}"
        ))
    })?;
    let asset = asset_name(&target)?;
    let repository = repository()?;
    let url = format!("https://github.com/{repository}/releases/latest/download/{asset}");
    let temporary = temporary_path(&target)?;

    emit(UpdateProgress::Checking {
        target: target.display().to_string(),
        asset: asset.clone(),
    });

    if let Err(error) = download(&url, &temporary, |downloaded, total| {
        emit(UpdateProgress::Downloading { downloaded, total });
    })
    .await
    {
        remove_quietly(&temporary);
        return Err(error);
    }

    emit(UpdateProgress::Validating);
    let downloaded_version = match validate_binary(&temporary) {
        Ok(version) => version,
        Err(error) => {
            remove_quietly(&temporary);
            return Err(error);
        }
    };
    emit(UpdateProgress::Validated {
        version: downloaded_version.clone(),
    });

    emit(UpdateProgress::Installing);
    if let Err(error) = replace_binary(&temporary, &target) {
        remove_quietly(&temporary);
        return Err(error);
    }

    emit(UpdateProgress::Completed {
        version: downloaded_version.clone(),
    });
    Ok(downloaded_version)
}

fn update_error(message: impl Into<String>) -> AppError {
    AppError::Update {
        message: message.into(),
    }
}

fn repository() -> Result<String> {
    let raw =
        std::env::var("PR_TOOLS_REPOSITORY").unwrap_or_else(|_| DEFAULT_REPOSITORY.to_owned());
    normalize_repository(&raw)
}

fn normalize_repository(raw: &str) -> Result<String> {
    let mut value = raw.trim().trim_end_matches('/').to_owned();
    for prefix in [
        "https://github.com/",
        "http://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest.to_owned();
            break;
        }
    }
    if let Some(rest) = value.strip_suffix(".git") {
        value = rest.to_owned();
    }
    let mut parts = value.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(update_error(format!(
            "repositório GitHub inválido: {value} (use owner/repo)"
        )));
    }
    Ok(format!("{owner}/{name}"))
}

fn platform_suffix() -> Result<&'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Ok("linux-x64");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Ok("linux-arm64");
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Ok("macos-arm64");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Ok("windows-x64");
    #[allow(unreachable_code)]
    Err(update_error(format!(
        "plataforma não suportada para atualização: {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )))
}

fn asset_name(target: &Path) -> Result<String> {
    let suffix = platform_suffix()?;
    let current_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| update_error("não foi possível identificar o nome do binário atual"))?;
    let prefix = if current_name.starts_with("prt-rust-") {
        "prt-rust"
    } else {
        "prt"
    };
    let extension = if cfg!(windows) { ".exe" } else { "" };
    Ok(format!("{prefix}-{suffix}{extension}"))
}

fn temporary_path(target: &Path) -> Result<PathBuf> {
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| update_error("não foi possível identificar a pasta de instalação"))?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| update_error("não foi possível identificar o nome do binário atual"))?;
    let extension = if cfg!(windows) { ".exe" } else { ".tmp" };
    Ok(parent.join(format!(".{name}.update-{}{extension}", std::process::id())))
}

async fn download<F>(url: &str, destination: &Path, mut emit: F) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent(format!("prt/{VERSION}"))
        .build()?;
    let response = client.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(update_error(format!(
            "GitHub retornou HTTP {} ao baixar o asset: {}",
            status.as_u16(),
            truncate(&body, 180)
        )));
    }
    let total = response.content_length();
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(destination).await?;
    let mut downloaded = 0_u64;
    emit(downloaded, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded = downloaded.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        emit(downloaded, total);
    }
    file.flush().await?;
    if downloaded == 0 {
        return Err(update_error("o GitHub retornou um arquivo vazio"));
    }
    set_executable(destination)?;
    Ok(())
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn validate_binary(path: &Path) -> Result<String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|error| update_error(format!("o arquivo baixado não é executável: {error}")))?;
    if !output.status.success() {
        return Err(update_error(format!(
            "o binário baixado falhou ao responder --version (status {})",
            output.status
        )));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !version.starts_with("prt v") {
        return Err(update_error(
            "o arquivo baixado não parece ser um binário válido do prt",
        ));
    }
    Ok(version.lines().next().unwrap_or_default().to_owned())
}

#[cfg(not(windows))]
fn replace_binary(temporary: &Path, target: &Path) -> Result<()> {
    std::fs::rename(temporary, target).map_err(|error| {
        update_error(format!(
            "não foi possível substituir {}: {error}",
            target.display()
        ))
    })
}

#[cfg(windows)]
fn replace_binary(temporary: &Path, target: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt as _;
    use std::process::Stdio;

    let script = target.with_file_name(format!(
        ".{}.update-{}.cmd",
        target
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("prt.exe"),
        std::process::id()
    ));
    let target_text = batch_path(target)?;
    let temporary_text = batch_path(temporary)?;
    let script_text = batch_path(&script)?;
    let content = format!(
        "@echo off\r\nsetlocal\r\n:retry\r\ndel /f /q \"{target_text}\" >nul 2>&1\r\nif exist \"{target_text}\" (\r\n  timeout /t 1 /nobreak >nul\r\n  goto retry\r\n)\r\nmove /y \"{temporary_text}\" \"{target_text}\" >nul 2>&1\r\nif errorlevel 1 (\r\n  timeout /t 1 /nobreak >nul\r\n  goto retry\r\n)\r\ndel /f /q \"{script_text}\" >nul 2>&1\r\nendlocal\r\n"
    );
    std::fs::write(&script, content)?;
    Command::new("cmd.exe")
        .args(["/D", "/C"])
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|error| {
            update_error(format!("não foi possível agendar a atualização: {error}"))
        })?;
    Ok(())
}

#[cfg(windows)]
fn batch_path(path: &Path) -> Result<String> {
    path.to_str()
        .map(|value| value.replace('%', "%%"))
        .ok_or_else(|| update_error("o caminho do binário contém caracteres inválidos"))
}

fn remove_quietly(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_should_accept_github_url_and_git_suffix() {
        assert_eq!(
            normalize_repository("https://github.com/nitoba/pr-tools.git").unwrap(),
            "nitoba/pr-tools"
        );
    }

    #[test]
    fn repository_should_reject_extra_path_segments() {
        assert!(normalize_repository("nitoba/pr-tools/releases").is_err());
    }

    #[test]
    fn asset_should_use_primary_name_for_default_binary() {
        let suffix = platform_suffix().unwrap();
        let extension = if cfg!(windows) { ".exe" } else { "" };
        assert_eq!(
            asset_name(Path::new("/home/user/.local/bin/prt")).unwrap(),
            format!("prt-{suffix}{extension}")
        );
    }

    #[test]
    fn asset_should_preserve_explicit_rust_alias() {
        let suffix = platform_suffix().unwrap();
        let extension = if cfg!(windows) { ".exe" } else { "" };
        assert_eq!(
            asset_name(Path::new("/home/user/.local/bin/prt-rust-linux-x64")).unwrap(),
            format!("prt-rust-{suffix}{extension}")
        );
    }

    #[test]
    fn temporary_file_should_be_next_to_target() {
        let target = Path::new("/tmp/bin/prt");
        let temporary = temporary_path(target).unwrap();
        assert_eq!(temporary.parent(), target.parent());
        assert!(
            temporary
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".prt.update-"))
        );
    }
}
