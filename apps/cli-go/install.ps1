# prt installer — Windows (PowerShell)
# Usage:
#   irm https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.ps1 | iex
#   $env:INSTALL_VERSION="v1.0.0"; irm .../install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "nitoba/pr-tools"
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "prt\bin" }
$GithubApi = "https://api.github.com/repos/$Repo/releases/latest"
$ReleasesUrl = "https://github.com/$Repo/releases/download"

function Write-Info { param($msg) Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Write-Ok   { param($msg) Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Warn { param($msg) Write-Host "[AVISO] $msg" -ForegroundColor Yellow }
function Write-Err  { param($msg) Write-Host "[ERRO] $msg" -ForegroundColor Red; exit 1 }

Write-Host ""
Write-Host "prt installer" -ForegroundColor White
Write-Host "============="
Write-Host ""

# --- Detect arch ---
$CpuArch = $env:PROCESSOR_ARCHITECTURE
$Arch = switch ($CpuArch) {
    "AMD64" { "amd64" }
    "ARM64" { "arm64" }
    default  { Write-Err "Arquitetura nao suportada: $CpuArch" }
}

Write-Info "Plataforma detectada: windows/$Arch"

# --- Resolve version ---
if ($env:INSTALL_VERSION) {
    $Version = $env:INSTALL_VERSION.TrimStart("v")
    Write-Info "Versao solicitada: v$Version"
} else {
    Write-Info "Buscando ultima versao..."
    try {
        $LatestJson = Invoke-RestMethod -Uri $GithubApi -Headers @{ "User-Agent" = "prt-installer" }
        $Version = $LatestJson.tag_name.TrimStart("v")
    } catch {
        Write-Err "Nao foi possivel determinar a ultima versao. Defina INSTALL_VERSION manualmente."
    }
    if (-not $Version) { Write-Err "Versao vazia retornada pela API do GitHub." }
    Write-Info "Ultima versao: v$Version"
}

# --- Build download URL ---
$Archive = "prt_${Version}_windows_${Arch}.zip"
$Url = "$ReleasesUrl/v$Version/$Archive"

# --- Download ---
$TmpDir = Join-Path $env:TEMP "prt-install-$(New-Guid)"
New-Item -ItemType Directory -Path $TmpDir | Out-Null
$ArchivePath = Join-Path $TmpDir $Archive

Write-Info "Baixando $Archive..."
try {
    Invoke-WebRequest -Uri $Url -OutFile $ArchivePath -UseBasicParsing
} catch {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
    if ($_.Exception.Response -and $_.Exception.Response.StatusCode -eq 404) {
        Write-Err "Versao v$Version nao encontrada: $Url"
    }
    Write-Err "Erro ao baixar: $($_.Exception.Message)"
}

# --- Extract ---
Write-Info "Extraindo..."
Expand-Archive -Path $ArchivePath -DestinationPath $TmpDir -Force

# --- Install ---
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir | Out-Null
}

$ExeSrc = Join-Path $TmpDir "prt.exe"
$ExeDst = Join-Path $InstallDir "prt.exe"
Move-Item -Path $ExeSrc -Destination $ExeDst -Force

Write-Ok "prt instalado em $ExeDst"

# --- Cleanup ---
Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue

# --- PATH update ---
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$UserPath;$InstallDir", "User")
    Write-Ok "Adicionado ao PATH do usuario: $InstallDir"
    Write-Warn "Reinicie o terminal para que o PATH seja atualizado."
} else {
    Write-Info "$InstallDir ja esta no PATH."
}

# --- Smoke test ---
try {
    $VersionOut = & $ExeDst --version 2>&1
    Write-Ok "Instalacao verificada: $VersionOut"
} catch {
    Write-Warn "Instalacao concluida, mas 'prt --version' retornou erro."
}

Write-Host ""
Write-Ok "Instalacao completa! Execute: prt init"
Write-Host ""
