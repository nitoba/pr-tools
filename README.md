# pr-tools

Ferramentas de produtividade para Pull Requests e Test Cases no Azure DevOps. Gera descrições de PR e cards de teste automaticamente usando IA.

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
create-test-card --update
```

## Configuração

Na primeira execução com TTY disponível, um **wizard interativo** guia a configuração. Em instalações não interativas, rode `--init` manualmente depois:

- Escolha de providers (OpenRouter, Groq, Gemini ou todos)
- API keys (com validação automática)
- Azure DevOps PAT (para links, leitura de contexto e criação automática de PR/Test Case)
- Reviewers padrão para PRs (emails para criação automática)

Para reconfigurar a qualquer momento:

```bash
create-pr-description --init
create-test-card --init
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

# Defaults para Test Cases
# TEST_CARD_AREA_PATH="AGROTRACE\\Devops"
# TEST_CARD_ASSIGNED_TO="nome@exemplo.com"
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

# Gerar card de teste detectando PR e work item a partir da branch atual
create-test-card

# Gerar card de teste para um PR específico
create-test-card --pr 10513

# Gerar o card sem tentar criar o Test Case no Azure DevOps
create-test-card --no-create

# Inspecionar o prompt e o payload sem chamar a LLM
create-test-card --dry-run --debug
```

### Configuração do create-test-card

Você pode definir defaults para o Test Case no mesmo `~/.config/pr-tools/.env`:

```bash
# AreaPath padrão para cards de teste
# TEST_CARD_AREA_PATH="AGROTRACE\Devops"

# Responsável padrão para cards de teste
# TEST_CARD_ASSIGNED_TO="nome@empresa.com"
```

Precedência de configuração:

1. flags CLI
2. variáveis de ambiente do shell
3. `.env`
4. defaults internos

No projeto `AGROTRACE`, o `AreaPath` padrão do Test Case é `AGROTRACE\Devops`.

Na criação de `Test Case` para `AGROTRACE`, o script também envia estes defaults do processo:

- `Priority = 2`
- `Team = DevOps`
- `Programas Agrotrace = Agrotrace`

Além disso, o comando mostra logs de progresso no modo normal para indicar fases longas como resolução de PR, busca de changes e geração via LLM.

Antes da criação real do `Test Case`, o script mostra o texto gerado e pede confirmação interativa, em linha com o fluxo do `create-pr-description`.

### Output

```
==========================================
Test Card - PR #10513
==========================================
Provider: groq (qwen/qwen3-32b)
Work Item pai: #11796 - Novo tipo de pergunta: Anexo (upload de documentos)
AreaPath Teste: AGROTRACE\Devops
Responsável: qa@empresa.com

Titulo: Testar novo tipo de pergunta 'Anexo' no CMS e formulários

## Objetivo
Validar a implementação do novo tipo de pergunta "Anexo"...

## Cenario base
1. Cadastro no CMS...
2. Configuração da estrutura...
3. Resposta do produtor...

## Checklist de testes
- [ ] Opção Anexo aparece no CMS...
- [ ] Campo de tamanho máximo respeita valor padrão...
- [ ] Upload acima do limite exibe erro...

## Resultado esperado
O tipo de pergunta Anexo funciona corretamente...

[OK] Test Case criado com sucesso: #12345
https://dev.azure.com/org/project/_workitems/edit/12345
==========================================
```

Se a criação falhar por regra do processo do Azure DevOps, o comando mantém o Markdown gerado visível no terminal e informa que pode ser necessário criar o card manualmente.

## Funcionalidades

- Gera descrições de PR em português brasileiro via LLM
- Gera cards de teste em português brasileiro a partir de PR + Work Item
- Suporta OpenRouter, Groq e Gemini com fallback automático
- Detecta sprint vigente automaticamente (`sprint/*` branches)
- Cria PR automaticamente no Azure DevOps via API (com reviewers obrigatórios e work items)
- Tenta detectar automaticamente PR e Work Item da branch atual para criar Test Cases
- Tenta criar `Test Case` filho no Azure DevOps com fallback para Markdown quando regras do processo bloqueiam a criação
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

### `create-test-card`

Veja os exemplos na seção `Uso` e rode `create-test-card --help` para a lista completa de flags.

Exemplo de saída do `create-test-card`:

```text
========================================
Test Card - PR #10513
========================================
Provider: groq (qwen/qwen3-32b)
Work Item pai: #11796 - Novo tipo de pergunta: Anexo (upload de documentos)
AreaPath Teste: AGROTRACE\Devops
Responsável: qa@empresa.com

Titulo: Testar novo tipo de pergunta 'Anexo' no CMS e formulários

## Objetivo
...

## Cenario base
...

## Checklist de testes
...

## Resultado esperado
...

[OK] Test Case criado com sucesso: #12345
https://dev.azure.com/org/project/_workitems/edit/12345
```

Se a criação falhar por regra do processo no Azure DevOps, o comando mantém o Markdown visível e informa que pode ser necessário criar o card manualmente.

## Como funciona

### `create-pr-description`

1. Coleta `git diff` e `git log` da branch atual vs branch base (sprint ou dev)
2. Detecta a sprint vigente (maior número em `origin/sprint/*`)
3. Detecta o work item a partir do nome da branch (ex: `feat/1234-descrição`) ou via `--work-item`
4. Parseia o remote para extrair org/project/repo do Azure DevOps
5. Envia o contexto para um LLM via API REST (com fallback entre providers)
6. Extrai título e descrição da resposta do LLM
7. Imprime a descrição formatada + links de PR
8. Copia a descrição para o clipboard
9. Oferece criar o PR automaticamente no Azure DevOps (com reviewers e work items)

### `create-test-card`

O fluxo do `create-test-card` está documentado nas seções `Uso` e `Output`, incluindo autodetecção de PR/work item, geração em Markdown e fallback para criação manual quando o Azure DevOps bloquear a criação automática.

## Providers suportados

| Provider                                     | Modelo padrão (gratuito)                 |
| -------------------------------------------- | ---------------------------------------- |
| [OpenRouter](https://openrouter.ai)          | `meta-llama/llama-3.3-70b-instruct:free` |
| [Groq](https://console.groq.com)             | `llama-3.3-70b-versatile`                |
| [Google Gemini](https://aistudio.google.com) | `gemini-3.1-flash-lite-preview`          |

Você pode trocar o modelo via `.env` ou variável de ambiente:

```bash
OPENROUTER_MODEL="qwen/qwen3-4b:free" create-pr-description
```

## Estrutura de arquivos

```
~/.local/bin/create-pr-description    # Script principal
~/.local/bin/create-test-card         # Script para gerar/criar Test Cases
~/.config/pr-tools/pr-template.md     # Template da descrição (editável)
~/.config/pr-tools/.env               # API keys e configuração
~/.config/pr-tools/.cache             # Cache de repositoryId e reviewers
```

## Licença

MIT
