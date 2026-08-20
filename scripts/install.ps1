$ErrorActionPreference = 'Stop'

$version = if ($env:PR_TOOLS_VERSION) { $env:PR_TOOLS_VERSION } else { 'latest' }
$repository = if ($env:PR_TOOLS_REPOSITORY) { $env:PR_TOOLS_REPOSITORY } else { 'nitoba/pr-tools' }

if ($repository) {
  $repository = $repository -replace '^https?://github\.com/', ''
  $repository = $repository -replace '^git@github\.com:', ''
  $repository = $repository -replace '^ssh://git@github\.com/', ''
  $repository = $repository -replace '\.git$', ''
  $repository = $repository.TrimEnd('/')
}

$binaryPath = $env:PR_TOOLS_BINARY
$assetName = 'pr-tools-windows-x64.exe'
$temporaryPath = $null
if (-not $binaryPath) {
  if (-not $repository -or $repository -notmatch '^[^/]+/[^/]+$') {
    throw 'Não foi possível determinar o repositório GitHub. Defina PR_TOOLS_REPOSITORY=owner/repo.'
  }
  if ($version -eq 'latest') {
    $downloadUrl = "https://github.com/$repository/releases/latest/download/$assetName"
  } else {
    $releaseTag = $version -replace '^v', ''
    $downloadUrl = "https://github.com/$repository/releases/download/v$releaseTag/$assetName"
  }
  $temporaryPath = Join-Path ([IO.Path]::GetTempPath()) ("pr-tools-$([guid]::NewGuid()).exe")
  $headers = @{}
  if ($env:PR_TOOLS_GITHUB_TOKEN) {
    $headers.Authorization = "Bearer $env:PR_TOOLS_GITHUB_TOKEN"
  }
  Invoke-WebRequest -Uri $downloadUrl -Headers $headers -OutFile $temporaryPath
  $binaryPath = $temporaryPath
}

$installDir = if ($env:PR_TOOLS_INSTALL_DIR) {
  $env:PR_TOOLS_INSTALL_DIR
} else {
  Join-Path $env:LOCALAPPDATA 'pr-tools/bin'
}
$targetPath = Join-Path $installDir 'pr-tools.exe'

if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
  throw "Binário não encontrado em $binaryPath. Gere o alvo windows-x64 antes da instalação."
}

New-Item -ItemType Directory -Path $installDir -Force | Out-Null
Copy-Item -LiteralPath $binaryPath -Destination $targetPath -Force
if ($temporaryPath) { Remove-Item -LiteralPath $temporaryPath -Force }

Write-Host "pr-tools instalado em $targetPath"
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathEntries = @($userPath -split ';' | Where-Object { $_ })
$pathAlreadyConfigured = $pathEntries | Where-Object {
  $_.TrimEnd('\') -ieq $installDir.TrimEnd('\')
}
if (-not $pathAlreadyConfigured) {
  $newUserPath = (@($pathEntries) + $installDir) -join ';'
  [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
  $env:Path = "$installDir;$env:Path"
  Write-Host "PATH do usuário atualizado. Abra um novo terminal para usar pr-tools."
}
