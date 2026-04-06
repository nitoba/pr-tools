# PRT Phase 2 — Functional Port: desc + test

## Problem

Phase 1 established the Go foundation with `prt init`, `prt doctor`, stub `prt desc`, and stub `prt test`. The Bash implementations in `apps/cli/src/bin/` contain fully functional behavior that needs migration to Go:

- `create-pr-description`: generates PR descriptions from git context using LLM, copies to clipboard, optionally creates PR in Azure DevOps
- `create-test-card`: generates Azure DevOps test cases from PR + Work Item context using LLM

## Goal

Port the functional behavior of both commands to Go while maintaining the UX contracts from Phase 1:
- `prt desc` generates PR descriptions and offers PR creation in Azure DevOps
- `prt test` generates test cards and offers Test Case creation in Azure DevOps

## Non-Goals

- Full parity with every flag and edge case in Bash scripts (intentional behavioral drift allowed)
- Preserve Bash command names as first-class entrypoints
- Port the interactive setup wizard wholesale (use simpler flag-based config)
- Port all future expansion points not yet implemented in Bash

## Product Direction

Phase 2 maintains the short command UX from Phase 1:
- `prt desc [flags]` — generates PR description
- `prt test [flags]` — generates test card

Both commands share the same config foundation (`~/.config/pr-tools/.env`) but may have command-specific config keys.

## Recommended Approach

Use Go with:
- `cobra` for CLI structure
- `testify` for assertions
- Custom HTTP client per provider (simple interfaces, no AI SDK abstraction)
- Azure DevOps REST client via `http.Client`

## Architecture

### High-Level Shape

Expand `apps/cli-go/internal/` with new packages:

```text
apps/cli-go/
  cmd/prt/
    main.go
  internal/
    cli/
      root.go
      desc.go
      test.go
      init.go
      doctor.go
    config/
      config.go
      env.go
      paths.go
    doctor/
      doctor.go
    setup/
      bootstrap.go
    platform/
      os.go
    ui/
      output.go
    version/
      version.go
    # NEW: Phase 2 packages
    git/
      context.go      # Collect git diff, log, branch
      azure.go        # Azure DevOps remote detection
    llm/
      client.go       # LLM client interface
      openrouter.go   # OpenRouter implementation
      groq.go         # Groq implementation
      gemini.go       # Google Gemini implementation
      ollama.go       # Ollama implementation
    azure/
      client.go       # Azure DevOps REST client
      pr.go           # PR creation
      workitem.go     # Work item queries
      testcase.go     # Test case creation
    clipboard/
      clipboard.go    # Cross-platform clipboard
```

### Boundaries

- `internal/git` — git context collection only (diff, log, branch detection)
- `internal/llm` — provider-agnostic LLM interface with concrete implementations
- `internal/azure` — Azure DevOps REST API client
- `internal/clipboard` — platform-specific clipboard write

### Command Contracts

#### `prt desc` Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--source` | Source branch | current branch |
| `--target` | Target branch (dev/sprint) | both |
| `--work-item` | Azure DevOps work item ID | auto-detect |
| `--dry-run` | Show prompt without calling LLM | false |
| `--raw` | Output without markdown rendering | false |
| `--no-stream` | Disable streaming | false |
| `--create` | Create PR in Azure DevOps | false |

#### `prt test` Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--work-item` | Parent work item ID | required |
| `--pr` | PR ID | auto-detect |
| `--org` | Azure organization | auto-detect |
| `--project` | Azure project | auto-detect |
| `--repo` | Azure repository | auto-detect |
| `--area-path` | Test Case area path | config |
| `--assigned-to` | Test Case assignee | config |
| `--examples` | Number of examples in prompt (0-5) | 2 |
| `--no-create` | Generate but don't create | false |
| `--dry-run` | Show prompts without calling LLM | false |
| `--raw` | Output only markdown | false |

### Config Keys

Phase 2 adds config keys to `.env`:

```bash
# LLM Providers (order of fallback)
PR_PROVIDERS="openrouter,groq,gemini,ollama"

# API Keys
OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."
GEMINI_API_KEY="..."
OLLAMA_API_KEY="..."

# Models (optional - uses free default if not set)
OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
GROQ_MODEL="llama-3.3-70b-versatile"
GEMINI_MODEL="gemini-2.0-flash"
OLLAMA_MODEL="qwen3:8b"

# Azure DevOps
AZURE_PAT="..."

# PR Creation
PR_REVIEWER_DEV="email@example.com"
PR_REVIEWER_SPRINT="email@example.com"

# Test Cards (prt test)
TEST_CARD_AREA_PATH="Team\\Area"
TEST_CARD_ASSIGNED_TO="email@example.com"
```

### LLM Interface

```go
type LLMClient interface {
    Name() string
    Model() string
    Chat(ctx context.Context, system, user string) (string, error)
    StreamChat(ctx context.Context, system, user string, onToken func(string)) error
}

type Provider interface {
    Name() string
    Models() []string
    DefaultModel() string
    Client(model string) (LLMClient, error)
}
```

Each provider (OpenRouter, Groq, Gemini, Ollama) implements `Provider` and `LLMClient`.

### Fallback Strategy

Providers are tried in order from `PR_PROVIDERS`. If one fails, the next is tried. Errors are logged but execution continues.

### Git Context Collection

- `Branch()` — current branch name
- `SourceBranch(branch string)` — validate and resolve source branch
- `BaseBranch()` — detect base (sprint > dev > main)
- `Diff(base, source string, maxLines int) (string, error)` — collect diff with truncation
- `Log(base, source string, maxCommits int) (string, error)` — collect commit log

### Azure DevOps Integration

- `GetPullRequest(org, project, repo string, prID int) (*PullRequest, error)`
- `CreatePullRequest(org, project, repo string, req CreatePRRequest) (*PullRequest, error)`
- `GetWorkItem(org, project string, id int) (*WorkItem, error)`
- `CreateTestCase(org, project string, req CreateTestCaseRequest) (*WorkItem, error)`
- `UpdateWorkItem(org, project string, id int, updates map[string]interface{}) (*WorkItem, error)`

### Clipboard

Cross-platform clipboard write:
- macOS: `pbcopy`
- Linux (Wayland): `wl-copy`
- Linux (X11): `xclip -selection clipboard`
- Fallback: error with instruction

### Template System

`prt desc` uses a built-in template (no external file required):

```
Analise o diff e log do git fornecidos e gere um TITULO e uma DESCRIÇÃO de PR
em portugues brasileiro.

IMPORTANTE: A PRIMEIRA LINHA da sua resposta DEVE ser o titulo neste formato exato:
TITULO: <texto curto e descritivo, max 80 caracteres>

Depois do titulo, siga este formato para a descrição:

## Descrição
<Resumo conciso>

## Alteracoes
### Componentes atualizados
<Lista de componentes>

### Correcoes / Melhorias tecnicas
<Se houver>

## Tipo de mudanca
- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Refactoring
```

`prt test` uses a built-in template:

```
Voce é um analista de QA tecnico.

Sua tarefa é gerar um card de teste em portugues brasileiro para Azure DevOps com base em:
1. Work item pai
2. Pull request relacionado
3. Arquivos alterados e resumo tecnico do PR
4. Exemplos de test cases existentes

IMPORTANTE: A PRIMEIRA LINHA da sua resposta DEVE ser exatamente:
TITULO: <titulo curto e objetivo>

Depois disso, responda em Markdown com estas secoes nesta ordem:
## Objetivo
## Cenario base
## Checklist de testes
## Resultado esperado
```

## Testing Strategy

### Unit Tests

- LLM client mocking via interface
- Git context with mock exec
- Azure client with mock HTTP
- Config precedence and parsing

### Integration Tests

- Full `prt desc` flow with real LLM (can be skip in CI with build tag)
- Full `prt test` flow with real Azure (can be skip in CI with build tag)

### Test Coverage Requirements

- Config loading from `.env`
- Environment variable override
- Provider fallback behavior
- Git context collection
- Azure DevOps PR creation
- Azure DevOps Test Case creation
- Clipboard detection

## Build and Release

Phase 2 builds on the Phase 1 release pipeline. The same `goreleaser` config publishes both:
- Phase 1 foundation artifacts
- Phase 2 functional artifacts

No changes to release workflow required.

## CLI Execution Contract

Exit codes:
- `0` — success
- `1` — validation/runtime error
- `2` — intentionally unimplemented (not used in Phase 2)

## Migration Strategy

### Port Order

1. **Config expansion** — add PR/Test config keys to env parsing
2. **Git context** — implement `internal/git`
3. **LLM clients** — implement OpenRouter, Groq, Gemini, Ollama
4. **Azure client** — implement PR and WorkItem operations
5. **`prt desc`** — wire full command
6. **`prt test`** — wire full command

### Behavioral Drift Allowed

- Remove external template file requirement (use built-in)
- Simplify interactive prompts (favor flags over wizard)
- Different output formatting (Go idiomatic)
- Truncation limits may differ

### Config Compatibility

- Read existing `.env` keys from Bash era
- Write new keys with `PRT_` prefix where applicable
- Preserve unknown keys

## Risks

### Scope Creep

Porting every Bash flag exactly would delay Phase 2. Focus on core functionality first.

### Provider SDK Changes

LLM provider APIs change frequently. Use versioned dependencies and validate with integration tests.

### Azure API Rate Limits

Azure DevOps API has rate limits. Add retry logic with backoff.

## Success Criteria

Phase 2 is successful when:
- `prt desc` generates PR descriptions from git context using LLM
- `prt desc` offers PR creation in Azure DevOps
- `prt test` generates test cards from PR + Work Item using LLM
- `prt test` offers Test Case creation in Azure DevOps
- Config keys are shared between commands where applicable
- Tests cover core logic paths
- Both commands work on Linux, macOS, Windows

## Summary

Phase 2 ports the functional behavior from Bash to Go using simple interfaces for LLM providers and Azure DevOps. The architecture stays lean with clear package boundaries: `git`, `llm`, `azure`, `clipboard`.
