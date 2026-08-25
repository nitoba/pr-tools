# pr-tools (`prt`)

CLI para gerar descrições de pull request e Test Cases a partir do contexto Git. Ela chama diretamente o Codex e o OpenCode instalados na máquina, ou usa Genkit com endpoints compatíveis com a API da OpenAI.

O fluxo é guiado: a descrição/card é exibida antes da publicação e a criação sempre exige confirmação. O acesso ao Azure DevOps é feito pela API REST, sem depender do `az` CLI.

## Instalação

As releases publicam binários para Linux x64/arm64, macOS arm64 e Windows x64. O instalador baixa automaticamente a versão mais recente e adiciona o diretório do executável ao PATH do usuário.

Depois da instalação, o comando disponível é `prt`.

### Linux e macOS

O comando funciona a partir de Bash, Zsh ou Fish:

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/scripts/install.sh \
  | bash
```

Para instalar uma versão específica:

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/scripts/install.sh \
  | PR_TOOLS_VERSION=v4.0.1 bash
```

Abra um novo terminal após a instalação. O instalador configura `.profile`/`.bashrc`, `.zprofile` ou `config.fish`, conforme o shell usado.

### Windows PowerShell

```powershell
$installer = Join-Path $env:TEMP 'pr-tools-install.ps1'
Invoke-WebRequest 'https://raw.githubusercontent.com/nitoba/pr-tools/main/scripts/install.ps1' -OutFile $installer
PowerShell -ExecutionPolicy Bypass -File $installer
```

Abra um novo PowerShell para que o PATH atualizado seja carregado.

## Primeira configuração

Depois de instalar, execute:

```bash
prt init
```

O wizard pergunta:

- Azure DevOps PAT;
- emails de review da sprint, de dev e do Test Case;
- provider padrão;
- modelo e thinking level do provider escolhido;
- Base URL e API key quando o provider for OpenAI-compatible.

A configuração fica em `~/.config/pr-tools/config.json` e `~/.config/pr-tools/.env` (ou em `XDG_CONFIG_HOME/pr-tools`). Os arquivos são criados com permissão restrita.

Autentique os providers locais antes de usá-los:

```bash
codex login
opencode auth login
```

O Codex usa por padrão `gpt-5.6-luna` com thinking `high`. Para OpenCode, informe o modelo no formato `provider/model`, como `openai/gpt-5.5`.

## Diagnóstico

Antes de gerar conteúdo, execute:

```bash
prt doctor
```

O diagnóstico verifica Git, remote Azure DevOps, PAT e acesso às APIs, configuração, autenticação dos providers e endpoint OpenAI-compatible. Cada componente informa o problema e como corrigi-lo; falhas críticas fazem o comando retornar código diferente de zero.

## Gerar e criar PRs

Execute os comandos dentro do clone do projeto que possui o remote Azure DevOps. Sem `--target`, o comando gera/publica PRs para a sprint mais recente e `dev`. Ao informar um ou mais `--target`, somente os destinos informados são usados.

```bash
# Conferir o prompt sem chamar o provider
prt desc --dry-run

# Gerar a descrição para um target e Work Item específicos
prt desc --target dev --work-item 11763

# Gerar para mais de um target e iniciar o fluxo de criação
prt desc --target dev --target sprint --create
```

O comando mostra título, descrição, targets e Work Item, copia o body para o clipboard quando possível e pede confirmação antes de criar cada PR. Os reviewers podem ser ajustados no próprio fluxo; os emails definidos no `init` são usados como sugestão. Ao reutilizar uma branch após um PR mesclado, o histórico do último PR concluído é usado como baseline para incluir apenas as novas alterações.

Opções úteis: `--source <branch>`, `--target <branch>` (repetível), `--provider <nome>`, `--model <nome>`, `--raw` e `--no-copy`.

## Criar Test Cases

```bash
# Gerar o card e apenas revisar o Markdown
prt test --work-item 11763 --no-create

# Gerar o card e abrir a confirmação de criação
prt test --work-item 11763 --create
```

O fluxo busca o Work Item pai, pode complementar o contexto com `--pr <id>`, mostra o card gerado e solicita confirmação antes de criar o Test Case. AreaPath, responsável, IterationPath e campos customizados também podem ser ajustados durante o fluxo.

## Azure DevOps

O remote Git precisa apontar para Azure DevOps. O PAT deve ter, no mínimo, permissão de leitura/escrita de código para PRs e de leitura/escrita de Work Items para Test Cases. O `init` salva o token em `.env`; também é possível usar `AZURE_PAT` ou `AZURE_DEVOPS_PAT` no ambiente.

Use `prt --help` para consultar todos os argumentos e `prt --version` para conferir a versão instalada.

## Desenvolvimento

```bash
dart pub get
dart analyze
dart test
dart run scripts/build.dart
```

`dart run scripts/build.dart` gera `dist/prt-<plataforma>` para o host atual.
A plataforma também pode ser informada explicitamente, desde que corresponda
ao host, por exemplo `dart run scripts/build.dart linux-x64`. A
release compila cada binário no runner nativo correspondente. Os executáveis
externos `codex` e `opencode` continuam sendo instalados e autenticados
separadamente na máquina do usuário.
