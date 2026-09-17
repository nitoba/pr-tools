# pr-tools (`prt`)

CLI para gerar descrições de pull request e Test Cases a partir do contexto Git. Ela chama diretamente o Codex e o OpenCode instalados na máquina, ou usa o SDK Rust `aisdk` com endpoints compatíveis com a API da OpenAI.

O fluxo é guiado: a descrição/card é exibida antes da publicação e a criação sempre exige confirmação. O acesso ao Azure DevOps é feito pela API REST, sem depender do `az` CLI.

## Instalação

As releases publicam a implementação Rust para Linux x64/arm64, macOS arm64 e Windows x64. O instalador baixa automaticamente a versão mais recente e adiciona o diretório do executável ao PATH do usuário.

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
  | PR_TOOLS_VERSION=v6.0.1 bash
```

Abra um novo terminal após a instalação. O instalador configura `.profile`/`.bashrc`, `.zprofile` ou `config.fish`, conforme o shell usado.

### Windows PowerShell

```powershell
$installer = Join-Path $env:TEMP 'pr-tools-install.ps1'
Invoke-WebRequest 'https://raw.githubusercontent.com/nitoba/pr-tools/main/scripts/install.ps1' -OutFile $installer
pwsh -NoProfile -ExecutionPolicy Bypass -File $installer
```

Abra um novo PowerShell para que o PATH atualizado seja carregado.

## Primeira configuração

Depois de instalar, execute:

```bash
prt init
```

O wizard pergunta:

- Azure DevOps PAT;
- provider padrão;
- executável, modelo e thinking level do provider escolhido;
- Base URL e API key quando o provider for OpenAI-compatible.

O `prt init` é somente global: configura esses valores globais. Os reviewers e defaults de
Test Case pertencem a perfis de processo e são cadastrados pelo onboarding do
remote Azure durante `prt desc` ou `prt test`.

A configuração fica no diretório de configuração do usuário (`%APPDATA%/pr-tools` no Windows, `~/.config/pr-tools` no Linux e `~/Library/Application Support/pr-tools` no macOS; `XDG_CONFIG_HOME/pr-tools` pode substituir no Linux). Os arquivos são criados com permissão restrita.

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

### Perfis de processo por repositório

O perfil é associado ao remote Azure exato `(organization, project, repository)`;
o caminho local nunca participa do binding. Para qualquer remote Azure parseável
sem essa associação, `prt`, `prt desc` e `prt test` exibem a tela **Perfil do
repositório Azure**, antes de chamar IA ou escrever no Azure, com as ações `Novo
perfil`, `Importar perfil` e `Agora não`. Esta detecção ignora
maiúsculas/minúsculas somente no nome da organização; projeto e repositório
continuam exatos.

`Novo perfil` e `Importar perfil` abrem a edição dos campos `name`,
`testCard.programField`, `areaPath`, `testCard.assignedTo`,
`inheritIterationPath`, `parentTransition`, `priority`, `program`,
`reviewers.development`, `reviewers.sprint` e `testCard.team`. O novo draft começa com `priority: 2` e
`inheritIterationPath: true`; a importação copia os valores sem alterar a
origem e o nome do novo perfil continua editável. A revisão mostra o remote,
a origem e todos os valores, e exige confirmação explícita. `Agora não` não
altera a configuração e continua uma vez usando o fallback atual.

Os perfis legados abaixo continuam válidos. Perfis novos podem ter nome e
field Azure arbitrários, desde que `testCard.programField` seja informado. Esse
**repository binding** é persistido na seção `bindings` de `config.json`:

- `Agrotrace`: `Custom.Team` + `Custom.ProgramasAgrotrace`;
- `CheckMilk`: `Custom.Team` + `Custom.ProgramasCheckmilk`.

Os defaults de `areaPath`, `testCard.assignedTo`, `IterationPath`, prioridade,
valor de `program`, transição do pai e `reviewers` ficam em `config.json`. O
formato canônico, incluindo os providers, é:

```json
{
  "defaultProfile": "CheckMilk",
  "defaultProvider": "codex",
  "providers": [
    {
      "id": "codex",
      "type": "codex",
      "model": "gpt-5.6-luna",
      "reasoning": "high"
    },
    {
      "id": "opencode",
      "type": "opencode",
      "model": "openai/gpt-5.5"
    },
    {
      "id": "openai",
      "type": "openai-compatible",
      "baseUrl": "https://api.openai.com/v1",
      "model": "gpt-4o-mini"
    }
  ],
  "profiles": [
    {
      "name": "CheckMilk",
      "program": "Checkmilk",
      "areaPath": "CHECKMILK\\QA",
      "priority": 2,
      "inheritIterationPath": true,
      "parentTransition": "Test QA",
      "reviewers": {
        "development": "dev@example.com",
        "sprint": "sprint@example.com"
      },
      "testCard": {
        "assignedTo": "qa@example.com",
        "programField": "Custom.ProgramasCheckmilk",
        "team": "DevOps"
      }
    }
  ],
  "bindings": [
    {
      "profile": "CheckMilk",
      "organization": "org",
      "project": "CHECKMILK",
      "repository": "checkmilk"
    }
  ]
}
```

Ao carregar uma configuração, os formatos antigos — campos planos de perfil,
campos globais de provider (`codexModel`, `baseUrl` etc.) e `providers` como
lista de strings — continuam aceitos. O `load_config()` migra tudo para
`reviewers`/`testCard` e providers-objeto, removendo as chaves antigas; se
formatos antigo e novo coexistirem, o formato novo prevalece. Os modos sem
migração continuam sem escrever no arquivo.

Configurações antigas com as seis chaves de processo na raiz são migradas de
forma atômica e idempotente para `profiles[Agrotrace]`; as chaves legadas são
removidas, preservando os defaults, prioridade `2`, herança de iteração e
transição `Test QA`. O perfil legado implícito também usa `Agrotrace` como
fallback. O campo `testCard.programField` é o `referenceName` Azure do campo que
armazena o programa no Work Item `Test Case` (por exemplo,
`Custom.ProgramasCheckmilk`). PAT e API key são globais e não pertencem aos perfis: continuam no
`.env`/configuração global atual. O `prt test` consulta os metadados
de `Test Case`, fields e estados antes de qualquer `POST`/`PATCH`; rode
`prt doctor` para verificar bindings, schema, fields e reviewers antes de criar.

Em `--dry-run`, `--raw` ou sem TTY, o onboarding não pergunta nem modifica
`config.json`; o comando informa o remote e orienta executar o fluxo
interativo. `prt init` configura somente valores globais (PAT, provider,
modelos, reasoning e endpoint compatível), preserva perfis genéricos e seus
bindings, e `prt doctor` exibe `testCard.programField` e valida seus valores/reviewers
sem mostrar PAT ou API key.

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

O comando mostra título, descrição, targets e Work Item, copia o body para o clipboard quando possível e pede confirmação da criação e dos reviewers antes de publicar. O body do PR é mantido abaixo de 4000 caracteres; se a primeira geração exceder o limite, uma segunda chamada de IA o reescreve preservando o sentido. Os reviewers podem ser ajustados no próprio fluxo; os valores do perfil selecionado são usados como sugestão. Ao reutilizar uma branch após um PR mesclado, o histórico do último PR concluído é usado como baseline para incluir apenas as novas alterações.

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

## Atualizar

Para baixar a versão Rust mais recente diretamente do GitHub e substituir o
binário no mesmo caminho em que o `prt` está instalado:

```bash
prt update
```

## Releases e changelog

As releases são preparadas automaticamente pelo `release-plz`, usando o
`git-cliff` para atualizar o [CHANGELOG.md](CHANGELOG.md). O fluxo cria uma
Release PR com o incremento de versão em `crates/prt/Cargo.toml`, o changelog e,
após o merge, uma tag `vX.Y.Z` e uma GitHub Release em modo draft. O workflow
de release executa os testes, compila e anexa os binários para Linux, macOS e
Windows, gera uma descrição humanizada com um endpoint OpenAI-compatible e só
então publica a Release. Se a geração por IA falhar, a Release permanece como
draft.

Para que o workflow consiga criar a Release PR, a configuração do repositório
no GitHub precisa ter o secret `RELEASE_PLZ_TOKEN`: um fine-grained PAT com
permissão de leitura/escrita em `Contents` e `Pull requests`. Esse token é
necessário porque a tag criada pelo `GITHUB_TOKEN` padrão não dispara o
workflow de build da release.

Para gerar a descrição humanizada, adicione também o secret
`RELEASE_NOTES_API_KEY` com a chave da API compatível com OpenAI. Atualmente o
workflow usa:

```text
endpoint: https://api.groq.com/openai/v1/chat/completions
model: openai/gpt-oss-120b
reasoning_effort: medium
```

Se a API estiver indisponível ou o secret não existir, a release não falha: o
workflow mantém a descrição técnica gerada pelo `git-cliff`.

Use Conventional Commits nos títulos dos commits ou PRs:

```text
feat(cli): add a new command
fix(ai): handle an empty provider response
docs: improve the installation guide
refactor(tui): simplify navigation
feat!: change the configuration format
```

`feat` gera uma nova funcionalidade, `fix` uma correção e `!` indica uma
mudança incompatível. Para gerar o changelog localmente, instale o git-cliff e
execute `git-cliff --config git-cliff.toml -o CHANGELOG.md`.

## Estrutura do projeto

A aplicação Rust fica em `crates/prt/`, organizada por fronteiras arquiteturais explícitas:

```text
crates/prt/
├── src/
│   ├── core/          # erros e primitivas transversais
│   ├── integrations/  # AI, Azure DevOps e Git
│   ├── features/      # casos de uso e orquestração
│   ├── tui/           # interface Ratatui
│   ├── config/        # configuração e migração
│   ├── cli.rs         # contrato da linha de comando
│   ├── lib.rs         # fachada pública dos módulos
│   └── main.rs        # entrypoint e dispatch
└── tests/             # testes de integração
```

Veja [ARCHITECTURE.md](ARCHITECTURE.md) para as responsabilidades de cada camada e as regras de evolução. A raiz contém a configuração do workspace, a automação em `scripts/`, a documentação e os workflows do GitHub. O comando continua sendo `prt` e a configuração permanece em `~/.config/pr-tools`.

Os instaladores principais são `scripts/install.sh` e `scripts/install.ps1`.
Os aliases `scripts/install-rust.sh` e `scripts/install-rust.ps1` continuam
disponíveis para compatibilidade com instalações existentes.

## Desenvolvimento

```bash
cargo fmt --manifest-path crates/prt/Cargo.toml -- --check
cargo clippy --manifest-path crates/prt/Cargo.toml --locked --all-targets -- -D clippy::correctness
cargo test --manifest-path crates/prt/Cargo.toml --locked
cargo build --manifest-path crates/prt/Cargo.toml --locked --all-targets
```

`./scripts/build-rust.sh` gera `crates/prt/dist/prt-rust-<plataforma>` para o
host atual, depois de executar as verificações Rust por padrão. A plataforma
pode ser informada explicitamente, desde que corresponda ao host, por exemplo
`./scripts/build-rust.sh linux-x64`.

A release compila cada binário no runner nativo correspondente e publica o
mesmo executável como `prt-<plataforma>` (nome principal) e
`prt-rust-<plataforma>` (alias de compatibilidade). No Windows, os nomes têm
a extensão `.exe`.

Os executáveis externos `codex` e `opencode` continuam sendo instalados e
autenticados separadamente na máquina do usuário.

### Snapshots da TUI Rust

Os testes de renderização usam o Insta para comparar a interface com os arquivos
`.snap` em `crates/prt/src/tui/snapshots/`. Essas referências fazem parte dos
testes e devem ser versionadas junto com mudanças intencionais na interface.
Apenas os candidatos `.snap.new`, ainda pendentes de revisão, são ignorados.

Para atualizar as referências após uma mudança visual, execute na raiz do
repositório (Bash/Zsh):

```bash
# Instale a ferramenta de revisão uma vez; não é necessária para rodar os testes.
cargo install cargo-insta --locked

# Gere candidatos. Diferenças ou referências ausentes fazem esta etapa falhar.
INSTA_UPDATE=new cargo test --manifest-path crates/prt/Cargo.toml --locked

# Revise os diffs e aceite apenas as mudanças esperadas.
(cd crates/prt && cargo insta review)

# Confirme que a suíte passa sem gerar ou aceitar referências automaticamente.
INSTA_UPDATE=no cargo test --manifest-path crates/prt/Cargo.toml --locked

git add crates/prt/src/tui/snapshots/
```

Inclua os `.snap` revisados no mesmo commit da mudança visual. Não remova nem
ignore a pasta de referências: um checkout limpo no CI precisa desses arquivos.
Não habilite aceite automático de snapshots no CI, pois isso esconderia
regressões de renderização.