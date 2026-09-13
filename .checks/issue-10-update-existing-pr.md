# Atualizar título e descrição de PR existente

Sources:

- `.tasks/issue-10-update-existing-pr.md` - critérios observáveis, estados, fronteira, superfícies, decisões e varredura de requisitos
- `.design/issue-10-update-existing-pr.md` - **binding para a interface**: jornada, shape, decisões, roadmap e estados de `prt desc --pr`
- `apps/rust/src/cli.rs` - opções, normalização de argv, `WorkItemId` e o contrato já ocupado por `prt update`
- `apps/rust/src/main.rs` - dispatch de `desc`, modo TUI/plain e dry-run
- `apps/rust/src/azure/pull_requests.rs` - modelo/GET de PR, criação multi-target que deve permanecer isolada e novo publisher de update
- `apps/rust/src/azure/mod.rs` - autenticação, `api-version=7.1`, erros Azure e helpers JSON/JSON Patch
- `apps/rust/src/git/mod.rs` - remote Azure, resolução de refs e coleta de diff/log
- `apps/rust/src/features/describe.rs` - configuração, geração comum e mensagens de falha
- `apps/rust/src/tui/content_editor.rs` - editor draft-only, validação e preservação exata do conteúdo
- `apps/rust/src/tui/describe_app.rs` e `apps/rust/src/tui/live.rs` - fronteira existente de criação que não pode ser reutilizada para update
- `apps/rust/src/error.rs` - erros e códigos de saída
- `apps/rust/Cargo.toml` - dependências e comandos de validação da suíte Rust
- [Pull Requests - Update](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/update?view=azure-devops-rest-7.1) - contrato oficial do PATCH, `application/json` e limite de descrição
- [Get Pull Request](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/get-pull-request?view=azure-devops-rest-7.1) - contrato oficial da leitura por ID e campos remotos
- [Azure DevOps REST API guide](https://learn.microsoft.com/en-us/azure/devops/integrate/how-to/call-rest-api?view=azure-devops) - autenticação e distinção entre JSON e JSON Patch

## Out of scope

- merge/completion, auto-complete, reviewers, labels/checks/policies, alteração de target branch ou opções de merge - a primeira versão só altera título e descrição
- criação de PR, atualização em lote, reutilização de `publish_pull_requests` e alteração de `prt update` - a jornada é um único update isolado
- resolução automática de conflitos, sincronização em background, histórico persistente, drafts entre execuções e edição de Test Cases - não fazem parte da decisão registrada
- `--raw`, update sem TTY e combinações com `--create`/`--no-create` - a decisão adotada para esta implementação é rejeitar com código 2 antes do provider/escrita; `--dry-run` permanece permitido

## Landing

O update terá `features/update_pull_request.rs` para elegibilidade, prompt, comparação e reconciliação e `tui/update_flow.rs` para a máquina de estados própria. O fluxo reutilizará `ContentEditState`, autenticação, renderização básica e coleta Git, mas chamará apenas GET/PATCH do mesmo PR; `DescribeApp`, `live.rs` e `publish_pull_requests` continuarão exclusivos da criação.

| One-way door | Literal shape | Alternative rejected |
| --- | --- | --- |
| Isolamento da atualização | `UpdatePrep` + `UpdateApp` separados de `DescribePrep` + `DescribeApp`; a confirmação chama somente `update_pull_request` | um `DescribeApp` com `Mode::Create/Update` - mistura confirmação de update com targets, reviewers, recovery e criação multi-target |
| Autoridade remota | snapshot inicial e releitura usam status, repository, `sourceRefName`, `targetRefName`, `title` e `description` retornados pelo GET do mesmo PR | flags locais ou base inferida - poderiam atualizar outro conteúdo/target que não pertence ao PR informado |
| Coleta por refs | `collect_for_refs(source_ref, target_ref)` resolve apenas as refs fornecidas localmente ou `origin/<branch>` e usa `target...source`/`target..source` | fetch automático ou fallback para `sprint/dev/main/master` - introduz efeito de rede ou aproximação silenciosa |
| Payload remoto | `UpdatePullRequestInput` serializado como objeto com apenas `title` e `description`, enviado por `PATCH` `application/json` | objeto completo do PR ou `AzureClient::patch` de JSON Patch - pode alterar metadados ou usar o content-type errado |
| Concorrência e incerteza | releitura antes do PATCH; no-op sem PATCH; falha incerta seguida por GET de reconciliação antes de sucesso ou retry | ignorar mudança remota, inventar `If-Match` ou repetir PATCH cegamente - não é contrato documentado/verificável aqui |
| Nada mais nesta mudança é difícil de reverter | - | - |

## Checks

### S1 - CLI, leitura remota e contexto Git · 8 arquivos · ~140 KB · ~35k

**C1** - `prt desc --pr 42` normaliza o ID `42` em uma única jornada de atualização
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked desc_pr_should_select_one_update_journey`

**C2** - `prt update` continua selecionando o auto-update do binário e não a operação de atualização de PR
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_command_should_remain_binary_update`

**C3** - `--pr` vazio, não numérico ou menor/igual a zero falha com código 2 e a mensagem existente de ID inválido
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked desc_pr_should_reject_invalid_ids_before_external_effects`

**C4** - uma leitura válida chama o endpoint GET do mesmo projeto/repositório/ID com `api-version=7.1` e captura status, repository, source, target, title e description
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked pull_request_should_deserialize_update_snapshot_fields_and_get_should_build_exact_route`

**C5** - PR inexistente ou falha da leitura inicial termina com erro acionável, sem proposta e sem PATCH/POST
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked initial_update_read_failure_should_not_create_proposal_or_write`

**C6** - PR com status diferente de `active` é bloqueado antes da geração e da escrita
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_should_reject_non_active_pull_requests_before_generation`

**C7** - repository retornado pelo PR que não corresponde ao clone local bloqueia a operação com orientação para o clone correspondente
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_should_reject_pull_request_from_another_repository`

**C8** - source/target ausentes ou não resolvíveis bloqueiam com orientação de fetch/atualização e nunca usam `dev`, `sprint/*`, `main` ou `master` como fallback
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked exact_pr_refs_should_reject_missing_or_unresolvable_refs_without_fallback`

**C9** - com refs resolvidas, a coleta usa o range `target...source` para diff e `target..source` para log, mantendo as refs remotas do PR como autoridade
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked collect_for_refs_should_construct_exact_diff_and_log_ranges`

**C10** - `--dry-run` mostra o prompt/contexto da jornada sem chamar provider de IA, PATCH, POST ou publisher de criação; update sem TTY/`--raw`/`--create` é rejeitado com código 2 conforme a decisão adotada
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_dry_run_and_non_interactive_combinations_should_not_start_provider_or_writer`

### S2 - Geração de proposta e conteúdo de revisão · 3 arquivos · ~75 KB · ~19k

**C11** - a proposta recebe source/target remotos, diff, log `target..source`, título atual e descrição atual; falha ou proposta inválida não inicia escrita
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_prompt_and_generation_should_include_remote_snapshot_and_validate_proposal`

**C12** - a revisão mostra separadamente `Atual` (snapshot remoto) e `Proposta`, e só a proposta abre `ContentEditState`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_review_should_separate_current_snapshot_from_editable_proposal`

**C13** - save válido substitui a proposta exatamente pelos buffers editados, preserva `Atual` byte a byte e não normaliza novamente whitespace, Unicode, quebras ou Markdown
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_save_should_preserve_exact_proposal_and_unchanged_current_snapshot`

**C14** - título vazio após `trim` e body com 4000 caracteres mantêm o editor aberto com erro no campo correto; body com 3999 é aceito e body vazio é permitido
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_editor_should_enforce_title_and_body_boundaries`

**C15** - `Esc` cancela a revisão, descarta o draft em memória e não altera snapshot nem inicia escrita/persistência
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_cancel_should_discard_draft_without_remote_write`

### S3 - Confirmação, escrita e reconciliação · 5 arquivos · ~115 KB · ~29k

**C16** - ao confirmar, título/descrição são congelados antes da primeira chamada remota e edição fica indisponível durante confirmação, conflito e recuperação
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_should_freeze_approved_content_before_remote_operation`

**C17** - releitura pré-escrita que diverge em status, repository, source, target, title ou description bloqueia o PATCH e exige nova revisão/reconciliação
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_should_block_patch_when_any_remote_snapshot_field_changed`

**C18** - releitura coincidente com proposta byte a byte idêntica termina como no-op confirmado e não envia PATCH
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_should_confirm_noop_without_patch`

**C19** - releitura coincidente com mudança aprovada envia exatamente um PATCH ao mesmo PR, com `Content-Type: application/json` e somente `title`/`description`, sem POST ou campos extras
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_should_send_minimal_json_patch_to_same_pull_request`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked pull_request_should_deserialize_update_snapshot_fields_and_get_should_build_exact_route`

**C20** - resposta 2xx válida só conclui após GET confirmatório com title/description exatos; qualquer divergência permanece não confirmada
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_should_confirm_only_after_exact_post_patch_get`

**C21** - timeout, transporte ou 2xx inválido entram em reconciliação antes de sucesso/retry e não repetem PATCH cegamente
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_should_reconcile_success_timeout_transport_and_invalid_response`

**C22** - PAT ausente e HTTP 401/403 bloqueiam a escrita e mostram orientação acionável sobre PAT, permissão e `prt doctor`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_authorization_failures_should_be_actionable_and_write_nothing`

### S4 - Isolamento da fronteira e regressão · 5 arquivos · ~95 KB · ~24k

**C23** - qualquer confirmação da jornada update alcança apenas a operação de update isolada e nunca `publish_pull_requests` ou o endpoint POST de criação; criação multi-target existente permanece inalterada
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked update_confirmation_should_have_no_creation_publisher_path`

**C24** - a suíte contém provas para parse/isolamento da CLI, payload/header, elegibilidade, refs exatas, PR inexistente/fechado, repository incompatível, no-op, conflito, timeout/resposta inválida, reconciliação, autorização e snapshots da revisão/editor; testes passam sem atualizar snapshots automaticamente
Proof: `INSTA_UPDATE=no cargo test --manifest-path apps/rust/Cargo.toml --locked`
Proof: `cargo fmt --manifest-path apps/rust/Cargo.toml -- --check`
Proof: `cargo clippy --manifest-path apps/rust/Cargo.toml --locked --all-targets -- -D clippy::correctness`
Proof: `cargo build --manifest-path apps/rust/Cargo.toml --locked --all-targets`

## Swept

- validation: C2, C7, C8, C13
- failure modes: C4, C5, C6, C7, C10, C19, C20
- idempotency and retry: C17, C19
- authorization: C20; `client_for` continua exigindo PAT e remote e 401/403 permanecem acionáveis
- concurrency and ordering: C15, C16, C17, C18, C19
- data lifecycle: C12, C14, C15; snapshot e draft vivem somente na execução atual
- external-dependency failure: C4, C10, C19, C20
- state transitions: C4, C5, C11, C13, C14, C15, C16, C17, C19
- observability: C3, C4, C5, C6, C7, C9, C19, C20; não há telemetria nova

## Handoff

S1-S4 cabem em uma única batch: os arquivos existentes das quatro superfícies somam aproximadamente 95k tokens pela regra `wc -c / 4`, e as duas novas unidades (`update_pull_request.rs` e `update_flow.rs`) mais seus testes permanecem abaixo do teto de 150k; a fronteira natural entre S1/S2/S3/S4 acompanha remote/Git, geração, escrita e regressão, mas não há necessidade de handoff de build.

- Boundary: única batch a ser fechada após C24; o verificador independente deve revisar todo o diff da base da feature até `HEAD` e prestar contas de C1-C24.
- User-settled mid-build: nenhuma renegociação; a questão aberta de `--raw`/não-TTY/`--create` foi resolvida pelo default recomendado do ticket: rejeição com código 2, preservando `--dry-run`.
- Abandoned: nenhum além das alternativas rejeitadas em `Landing`; não será adicionada dependência de cliente HTTP mock sem necessidade.
