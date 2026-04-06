# PRT Phase 3 — Migration and Deprecation Design

**Date:** 2026-04-06
**Status:** Approved

## Goal

Complete the migration from the Bash CLI to the Go `prt` binary. Remove the Bash implementation entirely, ship cross-platform installers for the Go binary, simplify the release pipeline, and update documentation to reflect the Go CLI as the only supported path.

## Scope

Four deliverables:

1. **Remove Bash CLI** — delete `apps/cli/` entirely
2. **`apps/cli-go/install.sh`** — Linux/macOS installer
3. **`apps/cli-go/install.ps1`** — Windows installer
4. **`release.yml` simplification** — publish only goreleaser artifacts
5. **README rewrite** — document `prt` as primary CLI

## 1. Remove Bash CLI

Delete the entire `apps/cli/` directory. This includes:
- `apps/cli/src/bin/create-pr-description`
- `apps/cli/src/bin/create-test-card`
- `apps/cli/src/lib/*.sh`
- `apps/cli/install.sh`

No compatibility wrappers. No redirects. Clean break.

Any CI references to `apps/cli/` paths must be removed alongside the directory.

## 2. `apps/cli-go/install.sh` — Linux/macOS

Shell script that downloads the correct pre-built `prt` binary from GitHub Releases.

### Requirements

- **Dependencies:** only `curl` and `tar` — no `jq`, no `git`
- **OS detection:** `uname -s` → `linux` or `darwin`
- **Arch detection:** `uname -m` → `amd64` (x86_64) or `arm64` (aarch64)
- **Version resolution:** `INSTALL_VERSION` env var if set; otherwise fetch latest tag from GitHub API with `curl`
- **Install path:** `~/.local/bin/prt`
- **PATH check:** warn if `~/.local/bin` is not in `$PATH`
- **Smoke test:** run `prt --version` after install

### Download URL pattern

```
https://github.com/nitoba/pr-tools/releases/download/v<version>/prt_<version>_<os>_<arch>.tar.gz
```

OS mapping: `Linux` → `linux`, `Darwin` → `darwin`
Arch mapping: `x86_64` → `amd64`, `aarch64` / `arm64` → `arm64`

### Usage

```bash
# Latest version
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.sh | bash

# Specific version
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.sh | INSTALL_VERSION=v1.0.0 bash
```

### Error handling

- Unsupported OS or arch → print clear error and exit 1
- Version not found (HTTP 404) → print error with the attempted URL and exit 1
- `prt --version` fails after install → warn but do not fail (install succeeded)

## 3. `apps/cli-go/install.ps1` — Windows

PowerShell script that downloads the correct pre-built `prt.exe` binary from GitHub Releases.

### Requirements

- **Dependencies:** built-in PowerShell cmdlets only (`Invoke-WebRequest`, `Expand-Archive`)
- **Arch detection:** `$env:PROCESSOR_ARCHITECTURE` → `AMD64` or `ARM64`
- **Version resolution:** `$env:INSTALL_VERSION` if set; otherwise fetch latest tag from GitHub API
- **Install path:** `$env:LOCALAPPDATA\prt\bin\prt.exe`
- **PATH update:** add install directory to user PATH (`[Environment]::SetEnvironmentVariable`) if not already present
- **Smoke test:** run `prt --version` after install

### Download URL pattern

```
https://github.com/nitoba/pr-tools/releases/download/v<version>/prt_<version>_windows_<arch>.zip
```

Arch mapping: `AMD64` → `amd64`, `ARM64` → `arm64`

### Usage

```powershell
# Latest version
irm https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.ps1 | iex

# Specific version
$env:INSTALL_VERSION="v1.0.0"; irm https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.ps1 | iex
```

### Error handling

- Unsupported arch → print clear error and exit
- Version not found → print error with attempted URL and exit
- PATH update failure → warn, do not fail

## 4. `release.yml` Simplification

### Current state

The workflow has two separate publication paths:
1. A manual step that zips `apps/cli/` Bash scripts into a `.zip` asset
2. A goreleaser step that builds and publishes `prt` binaries

With `apps/cli/` removed, the Bash packaging step is deleted entirely.

### Target state

The goreleaser step handles everything: cross-platform builds, archive creation, checksum generation, and GitHub Release creation. The separate `softprops/action-gh-release` step is removed — goreleaser publishes the release directly.

### Simplified workflow steps

```
1. Checkout (fetch-depth: 0)
2. Install git-cliff
3. Generate changelog → pass to goreleaser via GORELEASER_CURRENT_TAG or release notes file
4. Setup Go
5. Install GoReleaser
6. Run: goreleaser release --clean --config apps/cli-go/.goreleaser.prt.yml
```

### Published artifacts per release

```
prt_<version>_linux_amd64.tar.gz
prt_<version>_linux_arm64.tar.gz
prt_<version>_darwin_amd64.tar.gz
prt_<version>_darwin_arm64.tar.gz
prt_<version>_windows_amd64.zip
prt_<version>_windows_arm64.zip
prt_<version>_checksums.txt
```

### goreleaser config

The existing `apps/cli-go/.goreleaser.prt.yml` is already correct. The `release.disable: false` setting allows goreleaser to create the GitHub Release. The changelog body from `git-cliff` is passed via a release notes file.

## 5. README Rewrite

The root `README.md` is rewritten to document `prt` as the only CLI. All references to `create-pr-description` and `create-test-card` are removed.

### Sections

**Installation**
```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.sh | bash

# Windows (PowerShell)
irm https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.ps1 | iex
```

**Quick start**
```bash
prt init          # create ~/.config/pr-tools/.env
prt doctor        # verify configuration
prt desc          # generate PR description
prt test          # generate Azure DevOps test card
```

**Command reference** — key flags for each command (`desc`, `test`, `init`, `doctor`)

**Configuration** — list of all supported env vars in `~/.config/pr-tools/.env`:
- `PR_PROVIDERS`, `OPENROUTER_API_KEY`, `GROQ_API_KEY`, `GEMINI_API_KEY`, `OLLAMA_API_KEY`
- `OPENROUTER_MODEL`, `GROQ_MODEL`, `GEMINI_MODEL`, `OLLAMA_MODEL`
- `AZURE_PAT`, `PR_REVIEWER_DEV`, `PR_REVIEWER_SPRINT`
- `TEST_CARD_AREA_PATH`, `TEST_CARD_ASSIGNED_TO`
- `PRT_NO_COLOR`, `PRT_DEBUG`

## File Changes Summary

| File | Action |
|------|--------|
| `apps/cli/` | Delete entirely |
| `apps/cli-go/install.sh` | Create |
| `apps/cli-go/install.ps1` | Create |
| `.github/workflows/release.yml` | Simplify (remove Bash packaging) |
| `README.md` | Rewrite |

## Success Criteria

- `apps/cli/` no longer exists in the repo
- `curl .../install.sh | bash` installs `prt` on Linux/macOS
- `irm .../install.ps1 | iex` installs `prt` on Windows (PowerShell)
- `release.yml` produces only `prt` artifacts on tag push
- README documents `prt` as the only CLI with correct install instructions
