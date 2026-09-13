# Verificação independente — issue 12

Verificado no commit `266b195`, contra a base `646bf2a`. Resultado: **PASS**.

| Critério | Resultado | Evidência |
| --- | --- | --- |
| C1 | PASS | `preparation_without_work_item_should_not_request_functional_context` e `no_work_item_should_skip_azure_and_functional_prompt` passaram; o loader retorna `NotRequested` antes de criar cliente Azure. |
| C2 | PASS | `describe_prompt_should_include_projected_functional_context` passou; a seção funcional contém ID, título, tipo e opcionais disponíveis antes de `## Contexto Git`. |
| C3 | PASS | `functional_context_should_omit_absent_or_non_text_optional_fields` passou; opcionais ausentes, vazios ou não textuais viram `None`. |
| C4 | PASS | `functional_context_should_normalize_and_bound_rich_text` passou; markup/entidades/separadores são tratados e os limites de 3000/6000 com marcador são mantidos. |
| C5 | PASS | `describe_prompt_should_separate_intent_from_git_evidence` passou; regras distinguem requisito funcional de evidência Git e proíbem copiar o Work Item. |
| C6 | PASS | `desc_output_should_show_functional_context_but_raw_should_be_body_only` passou; plain/TUI mostram a indicação projetada e `--raw` retorna somente o body. |
| C7 | PASS | `functional_context_failure_should_require_explicit_git_only_confirmation` passou; a janela de indisponibilidade é renderizada, `n`/Esc abortam e o backend só inicia após confirmação Git-only. |
| C8 | PASS | `non_interactive_functional_context_failure_should_be_actionable` passou; o erro é acionável e tem código de saída 1. |
| C9 | PASS | `functional_context_should_not_leak_raw_work_item_data` passou; logs/saídas usam somente a projeção segura, e a suíte completa preservou os fluxos existentes. |

## Comandos executados

- `$env:TERM='xterm-256color'; $env:INSTA_UPDATE='no'; cargo test --manifest-path apps/rust/Cargo.toml --locked`
- `cargo fmt --manifest-path apps/rust/Cargo.toml -- --check`
- `cargo clippy --manifest-path apps/rust/Cargo.toml --locked --all-targets -- -D clippy::correctness`
- `cargo build --manifest-path apps/rust/Cargo.toml --locked --all-targets`
- `git diff --check 646bf2a 266b195`

Resultados: 204 testes de biblioteca passaram, 1 teste de binário, 2 integrações e 3 doctests; 1 teste intencionalmente ignorado. Formatação, build e diff passaram; Clippy passou a política solicitada com avisos não bloqueantes preexistentes.

## Riscos residuais

- Não foi feita leitura/publicação real no Azure, por depender de credenciais e efeitos externos; falhas, limites, fallback e recuperação foram provados deterministicamente.
- A interação foi validada por `TestBackend`, snapshots e eventos sintéticos; terminais reais podem variar em largura Unicode.
