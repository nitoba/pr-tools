# pr-tools

Ferramentas de produtividade para Pull Requests. Gera descrições de PR automaticamente usando IA, com links para Azure DevOps.

## Instalação

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
```

### Requisitos

- `git`, `curl`, `jq`
- Bash 4+ (macOS, Linux, Windows WSL/Git Bash)
- API key de pelo menos um provider: [OpenRouter](https://openrouter.ai), [Groq](https://console.groq.com) ou [Google Gemini](https://aistudio.google.com)
- (Opcional) Renderizador Markdown no terminal: [`glow`](https://github.com/charmbracelet/glow) (recomendado), [`bat`](https://github.com/sharkdp/bat) ou `batcat` para visualização formatada da descrição

### Atualização

```bash
create-pr-description --update
```

## Configuração

Na primeira execução, um **wizard interativo** guia a configuração:

- Escolha de providers (OpenRouter, Groq, Gemini ou todos)
- API keys (com validação automática)
- Azure DevOps PAT (para links e criação automática de PR)
- Reviewers padrão para PRs (emails para criação automática)

Para reconfigurar a qualquer momento:

```bash
create-pr-description --init
```

### Configuração manual

Também é possível editar diretamente `~/.config/pr-tools/.env`:

```bash
vi ~/.config/pr-tools/.env
```

```bash
PR_PROVIDERS="openrouter,groq,gemini"
OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."
GEMINI_API_KEY="..."
AZURE_PAT="your-pat-token"

# Modelos (opcional - usa padrão gratuito se não definir)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# GEMINI_MODEL="gemini-3.1-flash-lite-preview"

# Reviewers padrão para criação automática de PRs
# PR_REVIEWER_DEV="email@exemplo.com"
# PR_REVIEWER_SPRINT="email@exemplo.com"
```

Variáveis de ambiente sobrescrevem o `.env`:

```bash
OPENROUTER_MODEL="qwen/qwen3-4b:free" create-pr-description
```

## Uso

De dentro de um repositório git, em uma feature branch:

```bash
# Gera PR para dev + sprint (padrão)
create-pr-description

# Apenas para dev
create-pr-description --target dev

# Apenas para sprint
create-pr-description --target sprint

# Saída sem renderização Markdown (texto puro)
create-pr-description --raw

# PR usando outra branch como origem (sem precisar fazer checkout)
create-pr-description --source feature/1234-login

# Vincular work item específico ao PR
create-pr-description --work-item 11763
```

### Output

```
==========================================
PR - feat/dark-mode
Target branches: dev, sprint/97
Provider: openrouter (meta-llama/llama-3.3-70b-instruct:free)
==========================================

Titulo: Adiciona suporte ao tema escuro

Descricao:
## Descrição
Adiciona suporte ao tema escuro em múltiplos componentes...

## Alterações
### Componentes atualizados
- **home-padrao**: Skeletons de loading adaptados para dark mode...

## Tipo de mudança
- [ ] Bug fix
- [x] Nova feature
- [ ] Breaking change
- [ ] Refactoring

==========================================
Abrir PR:

  -> dev:
     https://dev.azure.com/org/project/_git/repo/pullrequestcreate?sourceRef=feat/dark-mode&targetRef=dev&...

  -> sprint/97:
     https://dev.azure.com/org/project/_git/repo/pullrequestcreate?sourceRef=feat/dark-mode&targetRef=sprint/97&...

Descrição copiada para o clipboard!
==========================================
```

A descrição é copiada automaticamente para o clipboard. Os links são clicáveis no terminal.

## Funcionalidades

- Gera descrições de PR em português brasileiro via LLM
- Suporta OpenRouter, Groq e Gemini com fallback automático
- Detecta sprint vigente automaticamente (`sprint/*` branches)
- Cria PR automaticamente no Azure DevOps via API (com reviewers obrigatórios e work items)
- Vincula work items ao PR automaticamente (via branch ou flag `--work-item`)
- Gera links clicáveis para abrir PR no Azure DevOps
- Cacheia `repositoryId` e IDs de reviewers localmente
- Copia descrição para clipboard (pbcopy/wl-copy/xclip/xsel)
- Renderiza descrição com syntax highlight no terminal (glow/bat/batcat) com fallback para texto puro
- Permite definir a branch de origem do PR via `--source` sem precisar fazer checkout
- Extrai título do PR automaticamente da resposta do LLM
- Remove blocos `<think>` de modelos de raciocínio (ex: qwen3)
- Funciona em macOS, Linux e Windows (WSL/Git Bash)

## Comandos

```
create-pr-description [opções]

Opções:
  --init                        Inicializa arquivos de configuração
  --source <branch>             Branch de origem do PR (padrão: branch atual)
  --target <branch>             Target do PR: dev, sprint (pode repetir; padrão: ambos)
  --work-item <id>              ID do work item do Azure DevOps (ex: 11763)
  --set-openrouter-model <mod>  Salva modelo do OpenRouter no .env
  --set-groq-model <mod>        Salva modelo do Groq no .env
  --set-gemini-model <mod>      Salva modelo do Google Gemini no .env
  --dry-run                     Mostra o prompt sem chamar a LLM
  --raw                         Exibe a descrição sem renderização Markdown (texto puro)
  --update                      Atualiza o script para a versão mais recente
  --help                        Mostra ajuda
  --version                     Mostra a versão
```

## Como funciona

1. Coleta `git diff` e `git log` da branch atual vs branch base (sprint ou dev)
2. Detecta a sprint vigente (maior número em `origin/sprint/*`)
3. Detecta o work item a partir do nome da branch (ex: `feat/1234-descricao`) ou via `--work-item`
4. Parseia o remote para extrair org/project/repo do Azure DevOps
5. Envia o contexto para um LLM via API REST (com fallback entre providers)
6. Extrai título e descrição da resposta do LLM
7. Imprime a descrição formatada + links de PR
8. Copia a descrição para o clipboard
9. Oferece criar o PR automaticamente no Azure DevOps (com reviewers e work items)

## Providers suportados

| Provider | Modelo padrão (gratuito) |
|---|---|
| [OpenRouter](https://openrouter.ai) | `meta-llama/llama-3.3-70b-instruct:free` |
| [Groq](https://console.groq.com) | `llama-3.3-70b-versatile` |
| [Google Gemini](https://aistudio.google.com) | `gemini-3.1-flash-lite-preview` |

Você pode trocar o modelo via `.env` ou variável de ambiente:

```bash
OPENROUTER_MODEL="qwen/qwen3-4b:free" create-pr-description
```

## Estrutura de arquivos

```
~/.local/bin/create-pr-description    # Script principal
~/.config/pr-tools/pr-template.md     # Template da descrição (editável)
~/.config/pr-tools/.env               # API keys e configuração
~/.config/pr-tools/.cache             # Cache de repositoryId e reviewers
```

## Licença

MIT
