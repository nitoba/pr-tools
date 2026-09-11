<# Instalador interativo principal do `prt` Rust — Windows PowerShell.
#
#   $installer = Join-Path $env:TEMP 'pr-tools-install.ps1'
#   Invoke-WebRequest 'https://raw.githubusercontent.com/nitoba/pr-tools/main/scripts/install.ps1' -OutFile $installer
#   PowerShell -ExecutionPolicy Bypass -File $installer
#
# Env (opcionais, têm precedência sobre as perguntas): PR_TOOLS_VERSION,
# PR_TOOLS_REPOSITORY, PR_TOOLS_INSTALL_DIR, PR_TOOLS_BINARY, PR_TOOLS_GITHUB_TOKEN,
# PR_TOOLS_FLAVOR (rust, padrão, ou dart).
# Params: -Yes (não pergunta nada), -Version, -InstallDir, -Repository.
#>
[CmdletBinding()]
param(
  [switch]$Yes,
  [string]$Version = '',
  [string]$InstallDir = '',
  [string]$Repository = ''
)

$ErrorActionPreference = 'Stop'

$flavor = if ($env:PR_TOOLS_FLAVOR) { $env:PR_TOOLS_FLAVOR } else { 'rust' }
$assetPrefix = switch ($flavor) {
  'rust' { 'prt' }
  'dart' { 'prt-dart' }
  default { throw "Implementação inválida: $flavor (use rust ou dart)." }
}

function Write-Step { param([string]$Text) Write-Host "→ $Text" -ForegroundColor Cyan }
function Write-Ok { param([string]$Text) Write-Host "✔ $Text" -ForegroundColor Green }
function Write-Info { param([string]$Text) Write-Host $Text -ForegroundColor DarkGray }
function Fail { param([string]$Text) Write-Host "✘ $Text" -ForegroundColor Red; exit 1 }

function Remove-TemporaryFile {
  param([string]$Path)
  if (-not $Path) { return }

  for ($attempt = 1; $attempt -le 5; $attempt++) {
    try {
      Remove-Item -LiteralPath $Path -Force -ErrorAction Stop
      return
    } catch {
      if ($attempt -lt 5) {
        Start-Sleep -Milliseconds 500
      }
    }
  }

  Write-Host "! Não foi possível remover o arquivo temporário; remova-o depois: $Path" -ForegroundColor Yellow
}

function Read-Answer {
  param([string]$Question, [string]$Default)
  if ($Yes) { return $Default }
  try {
    $answer = Read-Host "$Question [$Default]"
  } catch {
    return $Default
  }
  if ([string]::IsNullOrWhiteSpace($answer)) { return $Default }
  return $answer.Trim()
}

function Confirm-Answer {
  param([string]$Question)
  (Read-Answer $Question 'Y') -match '^[SsYy]?$'
}

Write-Host ''
Write-Host '  ◆ prt — instalador' -ForegroundColor Cyan
Write-Host '  descrições de PR e Test Cases a partir do Git' -ForegroundColor DarkGray
Write-Host ''

# ---------- plataforma ----------
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -notmatch '^(AMD64|ARM64)$') {
  Fail "Arquitetura não suportada: $arch (suportado: x64)."
}
$assetName = "$assetPrefix-windows-x64.exe"
Write-Step "Sistema detectado: Windows $arch ($assetName)"

if (-not $Version) {
  $Version = if ($env:PR_TOOLS_VERSION) { $env:PR_TOOLS_VERSION } else { 'latest' }
}
if (-not $Repository) {
  $Repository = if ($env:PR_TOOLS_REPOSITORY) { $env:PR_TOOLS_REPOSITORY } else { 'nitoba/pr-tools' }
}
$Repository = $Repository -replace '^https?://github\.com/', ''
$Repository = $Repository -replace '^git@github\.com:', ''
$Repository = $Repository -replace '\.git$', ''
$Repository = $Repository.TrimEnd('/')
if ($Repository -notmatch '^[^/]+/[^/]+$') {
  Fail "Repositório inválido: $Repository (use owner/repo ou PR_TOOLS_REPOSITORY)."
}

$localBinary = $env:PR_TOOLS_BINARY

# ---------- perguntas ----------
$Version = Read-Answer "Versão a instalar ('latest' ou vX.Y.Z)" $Version
if (-not $InstallDir) {
  $defaultDir = if ($env:PR_TOOLS_INSTALL_DIR) { $env:PR_TOOLS_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'pr-tools\bin' }
  $InstallDir = Read-Answer 'Diretório de instalação' $defaultDir
}
$targetPath = Join-Path $InstallDir 'prt.exe'

Write-Host ''
Write-Host 'Resumo:' -ForegroundColor White
Write-Host "  release   $Repository @ $Version" -ForegroundColor Cyan
Write-Host "  asset     $assetName" -ForegroundColor Cyan
Write-Host "  destino   $targetPath" -ForegroundColor Cyan
Write-Host ''
if (-not (Confirm-Answer 'Prosseguir com a instalação?')) {
  Write-Host 'Instalação cancelada.'
  exit 0
}
Write-Host ''

# ---------- download ----------
$temporaryPath = $null
$binaryPath = $localBinary
if (-not $binaryPath) {
  if ($Version -eq 'latest') {
    $downloadUrl = "https://github.com/$Repository/releases/latest/download/$assetName"
  } else {
    $tag = "v$($Version -replace '^v', '')"
    $downloadUrl = "https://github.com/$Repository/releases/download/$tag/$assetName"
  }
  Write-Step "Baixando $downloadUrl"
  $temporaryPath = Join-Path ([IO.Path]::GetTempPath()) ("prt-$([guid]::NewGuid()).exe")
  $headers = @{}
  if ($env:PR_TOOLS_GITHUB_TOKEN) {
    $headers.Authorization = "Bearer $env:PR_TOOLS_GITHUB_TOKEN"
  }
  try {
    $progressBackup = $ProgressPreference
    $ProgressPreference = 'Continue'
    Invoke-WebRequest -Uri $downloadUrl -Headers $headers -OutFile $temporaryPath
    $ProgressPreference = $progressBackup
  } catch {
    Fail "Falha no download: $($_.Exception.Message)"
  }
  $binaryPath = $temporaryPath
} else {
  Write-Step "Usando binário local: $binaryPath"
}

if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
  Fail "Binário não encontrado em $binaryPath."
}
if ((Get-Item -LiteralPath $binaryPath).Length -eq 0) {
  Fail "O arquivo $binaryPath está vazio."
}

try {
  $installedVersion = & $binaryPath --version 2>$null
  Write-Ok "Binário verificado: $installedVersion"
} catch {
  Write-Host '! Não foi possível executar o binário; seguindo assim mesmo.' -ForegroundColor Yellow
}

# ---------- instalação ----------
Write-Step "Instalando em $targetPath"
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
Copy-Item -LiteralPath $binaryPath -Destination $targetPath -Force
Remove-TemporaryFile $temporaryPath
Write-Ok "prt instalado em $targetPath"

# ---------- PATH ----------
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathEntries = @($userPath -split ';' | Where-Object { $_ })
$directorySeparator = [IO.Path]::DirectorySeparatorChar
$normalizedInstallDir = $InstallDir.TrimEnd($directorySeparator)
$alreadyThere = $pathEntries | Where-Object {
  $_.TrimEnd($directorySeparator) -ieq $normalizedInstallDir
}
if (-not $alreadyThere) {
  if (Confirm-Answer "Adicionar $InstallDir ao PATH do usuário?") {
    $newUserPath = (@($pathEntries) + $InstallDir) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
    $env:Path = "$InstallDir;$env:Path"
    Write-Ok 'PATH do usuário atualizado.'
  } else {
    Write-Host '! Adicione manualmente o diretório ao PATH para usar prt.' -ForegroundColor Yellow
  }
} else {
  Write-Ok "$InstallDir já está no PATH"
}

# ---------- fim ----------
Write-Host ''
Write-Host '  ✔ Pronto! Execute:' -ForegroundColor Green
Write-Host '      prt init     # primeira configuração' -ForegroundColor DarkGray
Write-Host '      prt doctor   # diagnóstico do ambiente' -ForegroundColor DarkGray
Write-Host ''
Write-Host 'Abra um novo PowerShell para que o PATH atualizado seja carregado.' -ForegroundColor Yellow
