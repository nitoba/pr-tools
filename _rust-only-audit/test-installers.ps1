param([string]$Root = (Get-Location).Path)
$ErrorActionPreference = 'Stop'
$temp = Join-Path ([IO.Path]::GetTempPath()) ('prt-installer-check-' + [guid]::NewGuid())
$originalUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$originalProcessPath = $env:Path
New-Item -ItemType Directory -Path $temp | Out-Null
try {
  $global:RustOnlyFixture = Join-Path $temp 'fixture.exe'
  $builder = Join-Path $temp 'build-fixture.ps1'
  @'
param([string]$Destination)
Add-Type -TypeDefinition 'public static class InstallerFixture { public static void Main(string[] args) { System.Console.WriteLine("prt installer-test-fixture"); } }' -OutputAssembly $Destination -OutputType ConsoleApplication
'@ | Set-Content -LiteralPath $builder -Encoding ascii
  & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -File $builder -Destination $global:RustOnlyFixture
  if ($LASTEXITCODE -ne 0) { throw 'Failed to compile installer fixture' }
  $fixtureHash = (Get-FileHash -LiteralPath $global:RustOnlyFixture -Algorithm SHA256).Hash
  function global:Invoke-WebRequest {
    param($Uri, $Headers, $OutFile)
    $global:RustOnlyDownloads += [string]$Uri
    Copy-Item -LiteralPath $global:RustOnlyFixture -Destination $OutFile
  }
  $env:PR_TOOLS_REPOSITORY = 'nitoba/pr-tools'
  $env:PR_TOOLS_GITHUB_TOKEN = $null
  $env:PR_TOOLS_BINARY = $null
  $cases = 0
  foreach ($installer in @('install.ps1', 'install-rust.ps1')) {
    $tokens = $null; $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile((Join-Path $Root "scripts/$installer"), [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw "PowerShell parse errors in ${installer}: $errors" }
    $asset = if ($installer -eq 'install.ps1') { 'prt-windows-x64.exe' } else { 'prt-rust-windows-x64.exe' }
    foreach ($flavor in @('', 'rust', 'dart', 'other')) {
      foreach ($version in @('latest', '4.0.11', 'v4.0.11')) {
        $cases++
        $env:PR_TOOLS_FLAVOR = $flavor
        $global:RustOnlyDownloads = @()
        $destination = Join-Path $temp "case $cases"
        & (Join-Path $Root "scripts/$installer") -Yes -Version $version -InstallDir $destination
        $segment = if ($version -eq 'latest') { 'latest/download' } else { 'download/v4.0.11' }
        $expected = "https://github.com/nitoba/pr-tools/releases/$segment/$asset"
        if ($global:RustOnlyDownloads.Count -ne 1 -or $global:RustOnlyDownloads[0] -ne $expected) {
          throw "Unexpected download for $installer / $flavor / ${version}: $global:RustOnlyDownloads"
        }
        $installed = Join-Path $destination 'prt.exe'
        if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -ne $fixtureHash) { throw 'Installed bytes differ' }
        if ((& $installed --version) -ne 'prt installer-test-fixture') { throw 'Installed fixture did not execute' }
      }
    }
  }
  $env:PR_TOOLS_BINARY = $global:RustOnlyFixture
  $global:RustOnlyDownloads = @()
  $destination = Join-Path $temp 'local install'
  1..2 | ForEach-Object { & (Join-Path $Root 'scripts/install.ps1') -Yes -InstallDir $destination }
  if ($global:RustOnlyDownloads.Count -ne 0) { throw 'Local install downloaded a binary' }
  $entries = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_ -eq $destination })
  if ($entries.Count -ne 1) { throw 'Repeated install duplicated the PATH entry' }
  Write-Output "PASS: $cases download cases, local install, reinstall/PATH idempotence, both PowerShell parsers."
} finally {
  [Environment]::SetEnvironmentVariable('Path', $originalUserPath, 'User')
  $env:Path = $originalProcessPath
  Remove-Item Env:PR_TOOLS_BINARY, Env:PR_TOOLS_FLAVOR -ErrorAction SilentlyContinue
  Remove-Item function:global:Invoke-WebRequest -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}
