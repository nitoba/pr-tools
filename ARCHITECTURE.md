# Arquitetura do pr-tools

Este documento descreve a organização do código Rust do `prt` e as fronteiras que devem orientar novas mudanças.

A organização foi inspirada nos princípios observados no `ghuntley/loom`: workspace explícito, código de aplicação em `crates/`, módulos agrupados por responsabilidade, integrações isoladas do fluxo de negócio e uma camada de composição que conecta as partes. O objetivo não é reproduzir as funcionalidades nem a quantidade de crates do Loom; é aplicar as mesmas ideias de separação ao tamanho e ao domínio do `pr-tools`.

## Estrutura

```text
crates/prt/
├── Cargo.toml
├── build.rs
├── src/
│   ├── main.rs          # entrada do executável e dispatch dos comandos
│   ├── lib.rs           # fachada pública e mapa das fronteiras internas
│   ├── cli.rs           # contrato/parsing da CLI
│   ├── config/          # modelo, persistência e migração de configuração
│   ├── core/            # primitivas transversais sem caso de uso específico
│   │   ├── error.rs
│   │   └── process.rs
│   ├── integrations/    # comunicação com sistemas/processos externos
│   │   ├── ai/
│   │   ├── azure/
│   │   └── git/
│   ├── features/        # casos de uso e orquestração da aplicação
│   └── tui/             # apresentação Ratatui e fluxos interativos
└── tests/               # testes de integração do binário
```

## Responsabilidades

### `core`

Contém peças compartilhadas que não representam uma feature. Hoje concentra o erro raiz da aplicação e a execução segura de processos. Não deve conhecer TUI nem casos de uso.

### `integrations`

Contém adaptadores para dependências externas: providers de IA, Azure DevOps e Git. Esses módulos podem usar `core` e configuração, mas não devem decidir fluxos de produto ou navegação da interface.

### `features`

Contém os casos de uso do `prt`: preparar e gerar descrições, criar Test Cases, diagnóstico, onboarding, atualização e sessões. É a camada que combina configuração e integrações para entregar comportamento da aplicação.

### `tui`

Contém somente apresentação e interação de terminal. Pode chamar features e adaptar seus resultados para a interface, mas regras reutilizáveis de domínio ou integração não devem nascer aqui.

### `cli` e `main`

`cli.rs` define o contrato de linha de comando. `main.rs` é a composição do executável: inicialização, parsing e dispatch. Novas regras de negócio devem preferencialmente entrar em `features/`, não crescer no entrypoint.

## Compatibilidade de módulos

A refatoração mantém a API interna/pública existente (`prt::ai`, `prt::azure`, `prt::error`, `prt::git`, `prt::tui`, etc.). O `lib.rs` usa `#[path]` para mapear esses nomes para a nova organização física. Isso permite melhorar a estrutura sem transformar uma mudança arquitetural em uma reescrita funcional.

## Regras para evolução

1. Novos serviços externos entram em `integrations/`, não diretamente em `features/` ou `tui/`.
2. Casos de uso entram em `features/` e recebem/compõem as integrações necessárias.
3. Código visual e estado de widgets permanecem em `tui/`.
4. Código transversal só entra em `core/` quando é realmente compartilhado e não representa um caso de uso.
5. `main.rs` deve continuar sendo uma camada de composição; lógica nova deve ser empurrada para módulos de feature.
6. A separação em novos crates deve acontecer quando uma fronteira passar a ter ciclo de vida, testes ou reutilização independentes. Não criar crates apenas para reduzir o tamanho de arquivos.

## Validação

Mudanças estruturais devem manter os mesmos comandos de qualidade usados pelo CI:

```bash
cargo fmt --manifest-path crates/prt/Cargo.toml -- --check
cargo clippy --manifest-path crates/prt/Cargo.toml --locked --all-targets -- -D clippy::correctness
cargo test --manifest-path crates/prt/Cargo.toml --locked
cargo build --manifest-path crates/prt/Cargo.toml --locked --all-targets
```
