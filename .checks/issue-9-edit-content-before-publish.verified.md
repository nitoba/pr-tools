# Verificação independente — issue 9

Verificado em `HEAD` = `5bc160e`, contra a base `0d230e1`. Resultado: **PASS**.

| Critério | Resultado | Evidência |
| --- | --- | --- |
| C1 | PASS | `DescribeApp::open_content_edit` cria `ContentEditState::for_pr`; teste `editing_desc_should_open_content_editor_with_generated_content`. |
| C2 | PASS | `TestApp::open_content_edit` usa `for_test` sem tocar em `fields`; teste `editing_test_should_open_content_editor_without_touching_settings`. |
| C3 | PASS | `TextEditor` indexa cursor por `char`, trata navegação e inserção Unicode/multilinha; os loops recebem `Event::Paste`; teste `content_editor_unicode_and_multiline_navigation_should_preserve_text`. |
| C4 | PASS | `ContentEditState::handle_key` alterna somente `Title`/`Body`; teste `content_editor_tab_should_cycle_only_title_and_body`. |
| C5 | PASS | `Enter` no título é consumido e no corpo insere `\n`; teste `content_editor_enter_should_insert_body_newline_only`. |
| C6 | PASS | Ambos os fluxos despacham para o editor antes dos atalhos globais; teste `content_editor_should_consume_review_shortcuts`. |
| C7 | PASS | Save substitui apenas a representação canônica e fecha o draft; testes `saving_valid_content_should_update_desc_preview_exactly` e `saving_valid_content_should_update_test_preview_exactly`. |
| C8 | PASS | `ContentEditState::validate`, `start_publish` e `start_create` bloqueiam conteúdo inválido e mantêm o editor; testes `invalid_desc_content_should_stay_in_editor_without_remote_start` e `invalid_test_content_should_stay_in_editor_without_remote_start`. |
| C9 | PASS | `Esc` descarta somente `content_edit`; testes `canceling_content_edit_should_discard_desc_draft` e `canceling_content_edit_should_discard_test_draft`. |
| C10 | PASS | Validação PR usa `chars().count() >= 4000`; teste `edited_pr_body_should_use_existing_3999_character_boundary`. |
| C11 | PASS | Test Case usa `validate_card` e não recebe limite de 4000; teste `edited_test_content_should_use_validate_card_without_pr_body_limit`. |
| C12 | PASS | Save retorna `PrDescription` sem normalização; teste `saved_content_should_preserve_exact_whitespace_unicode_and_markdown`. |
| C13 | PASS | Clipboard lê body canônico/congelado e `c` no editor vira texto; teste `copy_should_use_approved_body_only_and_editor_should_consume_c`. |
| C14 | PASS | `start_publish` congela `frozen_publish_content` antes de `publish_task`; snapshot alimenta cada target e bloqueia edição; teste `publish_should_freeze_approved_content_before_first_remote_call`. |
| C15 | PASS | `start_create` congela `frozen_create_content` antes de `backend_create`; o teste `create_should_freeze_approved_content_and_use_existing_builders` confirma título, HTML e steps. |
| C16 | PASS | Retry e busca de candidatos preferem o snapshot congelado; testes `publish_and_create_retry_should_reuse_frozen_content_and_exact_title` nos dois fluxos. |
| C17 | PASS | Targets pendentes usam `remaining_publish_targets` com o mesmo snapshot; reviewers/settings permanecem em fluxos próprios; teste `pending_publish_targets_should_reuse_one_frozen_content_snapshot`. |
| C18 | PASS | Suíte, snapshots dos dois editores e os testes de fluxo passaram sem atualizar snapshots. |

## Comandos executados

- `$env:TERM='xterm-256color'; $env:INSTA_UPDATE='no'; cargo test --manifest-path apps/rust/Cargo.toml --locked`
- `cargo fmt --manifest-path apps/rust/Cargo.toml -- --check`
- `cargo clippy --manifest-path apps/rust/Cargo.toml --locked --all-targets -- -D clippy::correctness`
- `git diff --check 0d230e1 HEAD`

Resultados: testes `188 passed, 0 failed, 1 ignored`; integrações `2 passed`; doctests `3 passed`; formatação e `git diff --check` passaram; Clippy passou a política solicitada. Há avisos preexistentes de variáveis não usadas e `unnecessary_wraps`, fora do escopo e sem erro de correção.

## Riscos residuais

- Não foi feito publish/create real no Azure, por depender de credenciais e de efeitos externos; os limites de payload, retry e recuperação foram provados por testes determinísticos.
- A interação foi validada por `TestBackend`/snapshots e eventos sintéticos; permanece a variação normal de terminais reais para largura Unicode e modo de paste.
