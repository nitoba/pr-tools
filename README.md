# pr-tools

Ferramentas de produtividade para Pull Requests e Test Cases no Azure DevOps.
Gera descrições de PR e cards de teste automaticamente usando IA.

## Instalação

**Linux / macOS**

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.sh | bash
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.ps1 | iex
```

**Versão específica**

```bash
# Linux/macOS
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.sh | INSTALL_VERSION=v1.0.0 bash
```

```powershell
# Windows
$env:INSTALL_VERSION="v1.0.0"; irm https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.ps1 | iex
```

### Requisitos

- `curl` e `tar` (Linux/macOS) — sem dependências adicionais
- PowerShell 5+ (Windows)
- API key de pelo menos um provider de LLM

## Quick Start

```bash
prt init      # cria ~/.config/pr-tools/.env
# edite o arquivo com suas API keys
prt doctor    # verifica configuração
prt desc      # gera descrição de PR
prt test      # gera card de teste no Azure DevOps
```

## Configuração

Edite `~/.config/pr-tools/.env`:

```bash
# Providers (ordem de fallback)
PR_PROVIDERS="openrouter,groq,gemini,ollama"

# API Keys
OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."
GEMINI_API_KEY="..."
OLLAMA_API_KEY="..."

# Modelos (opcional — usa padrão se não definir)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# GEMINI_MODEL="gemini-2.0-flash"
# OLLAMA_MODEL="llama3.2"

# Azure DevOps
AZURE_PAT="seu-pat-token"

# Reviewers padrão para PRs (opcional)
# PR_REVIEWER_DEV="email@exemplo.com"
# PR_REVIEWER_SPRINT="email@exemplo.com"

# Defaults para Test Cases (opcional)
# TEST_CARD_AREA_PATH="PROJETO\Devops"
# TEST_CARD_ASSIGNED_TO="nome@exemplo.com"

# Debug
# PRT_DEBUG=true
# PRT_NO_COLOR=true
```

Variáveis de ambiente do shell sobrescrevem o `.env`.

Precedência: flags CLI > variáveis de ambiente > `.env` > defaults internos.

## Comandos

### `prt desc` — Gera descrição de PR

```bash
# Gera descrição para a branch atual
prt desc

# Apenas mostra o prompt, sem chamar a LLM
prt desc --dry-run

# Define a branch de origem manualmente
prt desc --source feature/1234-login

# Vincula um work item ao PR
prt desc --work-item 11763

# Saída sem renderização Markdown
prt desc --raw

# Cria o PR no Azure DevOps automaticamente
prt desc --create
```

### `prt test` — Gera card de teste no Azure DevOps

```bash
# Gera card de teste para um work item
prt test --work-item 11763

# Especifica org/project do Azure DevOps
prt test --work-item 11763 --org myorg --project myproject

# Apenas gera o markdown, não cria no Azure DevOps
prt test --work-item 11763 --no-create

# Apenas mostra o prompt, sem chamar a LLM
prt test --work-item 11763 --dry-run
```

### `prt init` — Inicializa configuração

```bash
prt init
```

Cria ou atualiza `~/.config/pr-tools/.env` com os valores padrão.

### `prt doctor` — Verifica configuração

```bash
prt doctor
```

Reporta o estado da configuração, versão e ambiente.

## Providers suportados

| Provider | Modelo padrão |
|----------|---------------|
| [OpenRouter](https://openrouter.ai) | `meta-llama/llama-3.3-70b-instruct:free` |
| [Groq](https://console.groq.com) | `llama-3.3-70b-versatile` |
| [Google Gemini](https://aistudio.google.com) | `gemini-2.0-flash` |
| [Ollama](https://ollama.com) | `llama3.2` |

## Processo de Release

```bash
./release.sh 1.0.1
```

O script atualiza a versão, gera o CHANGELOG e abre um PR.
Após o merge, o workflow `auto-tag.yml` cria a tag e o `release.yml` publica os binários `prt` via goreleaser para Linux, macOS e Windows.

## Licença

MIT
