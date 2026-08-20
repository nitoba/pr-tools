# pr-tools 3

CLI em TypeScript para gerar descrições de pull request a partir do contexto Git. Executa no Bun e usa o AI SDK com três caminhos de inferência:

- `codex`: `ai-sdk-provider-codex-cli` v2 (AI SDK v7), usando o `codex` local em `gpt-5.6-luna`, reasoning `high`;
- `opencode`: `ai-sdk-provider-opencode-sdk` v4 (AI SDK v7), usando o servidor local do OpenCode e os providers configurados nele;
- `openai-compatible`: qualquer endpoint compatível com a API OpenAI, via `@ai-sdk/openai-compatible`.

O modo Codex não usa harness, sandbox externo, 9Router ou API key própria: ele inicia o processo `codex` instalado na máquina e reaproveita a autenticação feita com `codex login` (normalmente em `~/.codex/auth.json`). O modo OpenCode inicia o servidor local e usa as credenciais configuradas pelo próprio OpenCode.

## O que existia no `pr-tools-2`

O projeto anterior era um executável Bash monolítico de 1.850 linhas. O fluxo era:

1. baixar bibliotecas Bash ausentes;
2. iniciar e autenticar um gateway 9Router local;
3. detectar branch, base, sprint, work item e diff/log;
4. montar o prompt de PR em português e chamar a LLM;
5. extrair título/corpo, copiar a descrição e exibir links Azure DevOps.

Ele também tinha `--init`, `--dry-run`, `--source`, `--target`, `--work-item`, `--raw` e atualização automática do script. A nova versão mantém o núcleo útil, remove o gateway acoplado e deixa a configuração do provider explícita.

## Uso

```bash
bun install
bun run src/index.ts init
bun run src/index.ts desc --dry-run
bun run src/index.ts desc --target dev --work-item 11763
bun run src/index.ts test --work-item 11763 --no-create
```

O pacote expõe o binário `pr-tools` quando instalado:

```bash
bunx pr-tools desc
```

Opções principais: `--source`, `--target` (repetível), `--work-item`, `--provider codex|opencode|openai-compatible`, `--model`, `--base-url`, `--api-key`, `--create`, `--dry-run`, `--raw` e `--no-copy`.

## Configuração

`pr-tools init` abre um wizard Clack para configurar o Azure PAT, os emails de review da sprint/dev e do card de teste, além do provider padrão. Para o provider escolhido, ele pergunta o modelo e o thinking level; no modo OpenAI-compatible também pergunta a Base URL e a API key. Os níveis disponíveis são `provider-default`, `none`, `minimal`, `low`, `medium`, `high` e `xhigh`.

O email do card de teste é salvo como `TEST_CARD_ASSIGNED_TO`, o campo `System.AssignedTo` usado pela API do Azure DevOps. Variáveis de ambiente sobrescrevem o arquivo:

```bash
PR_AI_PROVIDERS=codex,opencode,openai-compatible
PR_CODEX_MODEL=gpt-5.6-luna
PR_CODEX_REASONING=high
PR_OPENCODE_MODEL=openai/gpt-5.5
PR_OPENCODE_REASONING=provider-default
PR_AI_BASE_URL=https://api.openai.com/v1
PR_AI_MODEL=gpt-4o-mini
PR_AI_REASONING=provider-default
PR_AI_API_KEY=...
AZURE_PAT=...
PR_REVIEWER_DEV=reviewer@example.com
PR_REVIEWER_SPRINT=reviewer@example.com
TEST_CARD_AREA_PATH=AGROTRACE\\Devops
TEST_CARD_ASSIGNED_TO=qa@example.com
TEST_CARD_TEAM=DevOps
TEST_CARD_PROGRAM=Agrotrace
```

Para autenticar o Codex, execute uma vez:

```bash
codex login
```

`CODEX_HOME` pode apontar para outro diretório de credenciais. O provider compatível só é usado quando estiver na lista e o endpoint responder.

Para autenticar providers no OpenCode, use:

```bash
opencode auth login
```

O modelo do OpenCode usa o formato `provider/model`, por exemplo `openai/gpt-5.5`. O nível escolhido é enviado como `variant`; os valores disponíveis dependem do modelo configurado no OpenCode. A lista padrão tenta `codex`, depois `opencode` e por fim `openai-compatible`; altere `PR_AI_PROVIDERS` para escolher a ordem.

## Azure DevOps

Quando o `origin` é um remote Azure DevOps, a CLI usa diretamente a API REST com `AZURE_PAT`; não depende do `az` CLI. O PAT precisa do escopo de código para criar PRs. `pr-tools desc --create` exibe a descrição, pede confirmação explícita e solicita o reviewer de cada target (`dev` e/ou `sprint`) antes de criar. Sem `--create`, a CLI mantém o mesmo fluxo guiado, mas inicia a confirmação desmarcada.

## Test Cases

`pr-tools test` busca o Work Item pai, opcionalmente complementa o contexto com um PR e gera um card com o provider configurado. O comando usa Clack para pedir o ID do Work Item, exibir o card, confirmar a criação e preencher AreaPath, responsável, IterationPath e campos customizados. As entradas numéricas e credenciais usam schemas Zod diretamente no `validate` do Clack via Standard Schema. Use `--no-create` para apenas gerar o Markdown; `--create` deixa a confirmação de criação pré-selecionada, mas nunca a remove. A criação usa JSON Patch no endpoint de Work Items e relaciona o novo Test Case ao pai.

## Estrutura

| Arquivo | Responsabilidade |
| --- | --- |
| `src/cli.ts` | `node:util.parseArgs`, comandos e fluxo da CLI |
| `src/bin.ts` | entrypoint do executável Bun/scriptc |
| `src/config.ts` | configuração JSON/`.env` e wizard Clack |
| `src/git.ts` | coleta segura de branch, base, diff, log, sprint e Azure remote |
| `src/llm.ts` | saída estruturada do AI SDK e fallback entre os três providers |
| `src/azure/` | cliente REST Azure DevOps, PRs, work items e publicação |
| `src/test-card.ts` | contexto, prompt, geração e criação interativa de Test Cases |
| `src/validation.ts` | schemas Zod reutilizáveis para prompts e opções da CLI |
| `src/output.ts` | clipboard e URLs Azure DevOps |
| `src/prompt.ts` | template e montagem do prompt |
| `src/types.ts` | contratos compartilhados |
| `scripts/` | build por plataforma e instaladores |

## Verificação

```bash
bun run typecheck
bun test
```

## Build do binário

O binário nativo é gerado pelo `scriptc`; `--dynamic` incorpora as dependências npm usadas pela CLI. O comando requer Node 24+ e clang:

```bash
bun run build
```

O binário não embute os executáveis externos `codex` e `opencode`; eles continuam sendo resolvidos na máquina, junto das credenciais locais dos providers.

### Artefatos por plataforma

O scriptc usa o clang nativo no macOS arm64. Linux e Windows usam Zig para cross-compilação:

```bash
bun run build -- linux-x64
bun run build -- linux-arm64
bun run build -- windows-x64
```

No Mac Apple Silicon:

```bash
bun run build -- macos-arm64
```

Os artefatos são gravados em `dist/` como `pr-tools-linux-x64`, `pr-tools-linux-arm64`, `pr-tools-windows-x64.exe` e `pr-tools-macos-arm64`. Instale o artefato correspondente com `./scripts/install.sh` no Linux/macOS ou `PowerShell -ExecutionPolicy Bypass -File .\scripts\install.ps1` no Windows.

Para os alvos Linux/Windows, instale o Zig e mantenha `zig` disponível no `PATH`.

## Instalação

Os instaladores baixam automaticamente o artefato da GitHub Release mais recente. Em um clone do repositório, o nome remoto é detectado automaticamente:

```bash
./scripts/install.sh
```

Para executar fora de um clone, informe o repositório. Para instalar uma versão específica, use `PR_TOOLS_VERSION`:

```bash
PR_TOOLS_REPOSITORY=owner/repo PR_TOOLS_VERSION=v3.0.0 ./scripts/install.sh
```

Também é possível executar o instalador diretamente do GitHub:

```bash
curl -fsSL https://raw.githubusercontent.com/owner/repo/main/scripts/install.sh \
  | PR_TOOLS_REPOSITORY=owner/repo bash
```

No Windows:

```powershell
$env:PR_TOOLS_REPOSITORY = 'owner/repo'
PowerShell -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

Por padrão, o executável é copiado para `~/.local/bin` no Linux/macOS e `%LOCALAPPDATA%\pr-tools\bin` no Windows. Os instaladores também persistem esse diretório no PATH do usuário (`.profile`/`.bashrc`, `.zprofile` ou o `config.fish` de `XDG_CONFIG_HOME` com `fish_add_path`); abra um novo terminal após a instalação. `PR_TOOLS_INSTALL_DIR` altera o destino.

## Releases

O CI roda em pull requests e na branch `main`. Para publicar os quatro artefatos, crie e envie uma tag semântica:

```bash
git tag v3.0.0
git push origin v3.0.0
```

O workflow de release valida o código, gera os binários para Linux x64/arm64, macOS arm64 e Windows x64 e publica todos na GitHub Release da tag.
