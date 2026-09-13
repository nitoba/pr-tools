# Continuar do PR publicado para a preparação do Test Case

Sources:

- `.tasks/issue-13-continue-to-test-case-after-published-pr.md` - 24 critérios, fronteira, estados, superfície observável, sweep e exclusões
- `.design/issue-13-continue-to-test-case-after-published-pr.md` - **binding for the interface**: jornada, ação `t`/`T`, picker, contrato de handoff, fingerprint, confirmações e decisões irreversíveis
- `AGENTS.md` - comandos autoritativos de formatação, Clippy, testes e snapshots
- `apps/rust/Cargo.toml` e `.github/workflows/ci.yml` - manifest, lint e comandos realmente usados pela CI

## Out of scope

- criação automática de Test Case, um card por target, execução de testes, Test Plans/Suites e CI/CD - esta ação só prepara uma mudança funcional para QA
- transição automática do Work Item pai, sincronização após o fluxo, persistência, resume, rollback ou mutação dos PRs publicados - as duas escritas continuam separadas e confirmadas
- mudança da publicação de PR, reviewers, criação multi-target ou recovery de publicação - o handoff só começa após publicação completa
- alteração do contrato standalone de `prt test`, `prt update` ou novas flags de CLI - a entrada publicada é interna e estruturada
- conversão nativa de Steps do Test Case - pertence à issue separada e não é implementada aqui
- telemetria de tempo/frequência do handoff - a decisão registrada é observar três handoffs reais após o primeiro uso ponta a ponta

## Landing

O resultado publicado permanece mínimo em `azure::pull_requests::PublishedPr`; `desc` monta o contexto de continuação em memória e retorna um `LiveOutcome`. `test_card` prepara tanto o adapter CLI quanto o request publicado para o mesmo `TestCardPrep`, reutilizando `git::collect_for_refs`, o prompt, a revisão e os writers existentes. `main` mantém `DescribeApp` e `TestApp` como máquinas separadas e restaura o terminal entre elas.

| One-way door | Literal shape | Alternative rejected |
| --- | --- | --- |
| Request de preparação | `TestCardRequest::Cli(CliOptions)` e `TestCardRequest::PublishedPr(TestCardLaunchContext)` alimentam uma preparação comum | sintetizar `CliOptions` - recolheria o checkout e resolveria novamente o Work Item, quebrando a coerência do snapshot |
| Contexto publicado | `TestCardLaunchContext` carrega `PublishedPr`, `RepositoryRemote`, Work Item ID/snapshot, refs, seis valores de `TestSettings` e `GitContextFingerprint` | ampliar `PublishedPr` - espalharia metadados de continuação para consumidores que só precisam da receipt |
| Autoridade remota | lookup pelo ID retido, validação de repository/refs e coleta por `target...source`/`target..source` sem base inferida | usar a branch atual ou inferir `dev`, `sprint/*`, `main` ou `master` - pode preparar QA para outra mudança |
| Proteção local | comparar fingerprint antes da geração e oferecer exatamente `continuar com snapshot remoto` ou `voltar`; continuar usa refs/PR remotos | continuar silenciosamente ou abortar sempre - o primeiro é inseguro e o segundo penaliza desnecessariamente o caso raro |
| Seleção de PR | um PR segue por fast path; vários abrem picker finito em ordem publicada e `Enter` escolhe um único item | preferência automática de target ou um card por target - não há decisão de produto que autorize essas heurísticas |
| Limite de ciclo de vida | `LiveOutcome::PrepareTestCase` é consumido por `main` só depois de `run_describe_tui` restaurar o terminal; `create_initial = false` | um `WorkflowApp` único ou herdar `--create` - fundiria sessões e acoplaria escritas remotas |

- Nada mais nesta mudança é uma porta de mão única.

## Checks

### S1 - Resultado publicado e seleção do PR · 5 arquivos · ~141kB · ~36k

**C1** - Em `Phase::Done` completo, cada receipt mostra ID numérico, target e URL, e o footer oferece `t`/`T` enquanto `q` continua saindo.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::done_screen_should_render_published_ids_targets_urls_and_test_action`

**C2** - Publicação parcial/incerta não oferece handoff, preserva recovery e mantém cada PR confirmado na receipt.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::partial_publication_should_not_offer_test_case_handoff`

**C3** - Um único PR em `Done` retorna `LiveOutcome::PrepareTestCase` diretamente, com PR selecionado e receipt completa.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::single_published_pr_should_take_test_case_fast_path`

**C4** - Vários PRs abrem picker finito em ordem publicada, exibindo ID/target/URL, e `Enter` escolhe exatamente um PR.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::published_pr_picker_should_select_one_pr_in_publication_order`

**C5** - `Esc`/`q` no picker volta a `Done` sem handoff/remoto, e `q` em `Done` encerra sem efeito remoto.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::published_pr_picker_cancel_should_return_without_handoff`

### S2 - Contrato estruturado e preparação orientada ao PR · 4 arquivos · ~141kB · ~36k

**C6** - `TestCardRequest::Cli(CliOptions)` preserva o `prt test` standalone, incluindo `git::collect`, precedência CLI/branch/PR, flags e `TestCardPrep` consumido pelo fluxo atual.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked features::test_card::tests::cli_request_should_preserve_standalone_preparation`

**C7** - Request publicado busca o PR pelo ID retido no remote atual e valida repository, target e refs não vazios antes de gerar.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked features::test_card::tests::published_request_should_lookup_and_validate_selected_pr`

**C8** - Refs locais ou `origin/<branch>` produzem `source_ref` remoto exato, diff `target...source`, log `target..source` e prompt com PR/source/target sem base substituta.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked features::test_card::tests::published_request_should_use_exact_remote_source_and_target_context`

**C9** - Work Item resolvido por `desc` mantém ID/snapshot, é validado contra links do PR selecionado e mismatch interrompe sem fallback.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked features::test_card::tests::published_request_should_reject_incompatible_work_item_without_fallback`

**C10** - Sem Work Item vindo de `desc`, a preparação usa os links do PR e `select_parent_work_item`; sem pai, não gera nem escreve.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked features::test_card::tests::published_request_should_resolve_parent_from_pr_links_only_when_needed`

**C11** - Preparação válida expõe PR ID, remote, Work Item, os seis settings resolvidos e o prompt enriquecido com o contexto selecionado.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked features::test_card::tests::published_request_should_expose_complete_launch_context_and_prompt`

**C12** - Falha de PR, repository/refs, Work Item, coleta ou configuração — inclusive 401/403 — chega a `TestPhase::Erro` antes da geração e sem chamar os writers.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::test_flow::tests::published_preparation_failure_should_not_start_generation_or_remote_writes`

### S3 - Proteção contra divergência do checkout · 3 arquivos · ~134kB · ~34k

**C13** - `GitContextFingerprint` guarda identidade do repositório, branch source, OID source e OID de cada target solicitado na fronteira de publicação.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked git::tests::fingerprint_should_capture_repository_branch_and_requested_ref_oids`

**C14** - Checkout igual ao fingerprint não mostra aviso e gera com o PR remoto/Work Item já validados.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::test_flow::tests::matching_git_fingerprint_should_skip_divergence_gate`

**C15** - Mudança de identidade, branch, OID source ou qualquer target mostra exatamente as opções de snapshot remoto e volta antes da geração.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::test_flow::tests::changed_git_fingerprint_should_open_remote_snapshot_gate`

**C16** - Continuar mantém diff/log/prompt nos refs remotos; voltar não gera/escreve e preserva a receipt.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::test_flow::tests::remote_snapshot_choice_should_never_use_current_checkout_as_context`

### S4 - Handoff entre ciclos de vida e confirmações · 4 arquivos · ~139kB · ~35k

**C17** - `PrepareTestCase` carrega launch context e receipt, e `main::run_desc` só inicia `test_flow` após restauração do terminal.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked main::tests::desc_prepare_test_case_should_start_flow_after_terminal_restoration`

**C18** - Entrada publicada inicia `TestApp` com `create_initial = false` e alcança a revisão com PR/refs, pai e config resolvidos, sem reentrada.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::test_flow::tests::published_request_should_enter_review_without_reprompting_context`

**C19** - Uma ativação de `t`/`T` em receipt multi-target produz uma única preparação/geração/revisão e no máximo uma criação confirmada.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::test_flow::tests::one_handoff_activation_should_prepare_one_test_case_for_multiple_targets`

**C20** - Cancelamento no picker, gate ou revisão antes da criação não chama writers e reporta todos os PRs intactos.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked main::tests::cancelled_test_case_handoff_should_preserve_published_receipt`

**C21** - Revisão publicada mantém confirmação separada de criação e de `Test QA`; recovery, edição e retry continuam usando o mesmo `TestCardPrep`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::test_flow::tests::published_flow_should_keep_create_and_test_qa_confirmations_separate`

**C22** - Falha após handoff não vira sucesso/criação parcial, não altera PR publicado e preserva a receipt reportada.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked main::tests::failed_test_case_handoff_should_preserve_published_receipt_and_writes`

### S5 - Provas da fronteira · 3 arquivos · ~107kB · ~27k

**C23** - `PublishedPr` permanece limitado a `target`, `id` e `url`; publisher multi-target, reviewers, recovery de publicação e standalone não ganham estado remoto novo.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked azure::pull_requests::tests::published_pr_receipt_should_remain_minimal`

**C24** - A suíte cobre as fronteiras do issue, atualiza `desc_done_100x30` com IDs/ação `t` e `INSTA_UPDATE=no cargo test --manifest-path apps/rust/Cargo.toml --locked` passa sem criar snapshots automaticamente.
Proof: `INSTA_UPDATE=no cargo test --manifest-path apps/rust/Cargo.toml --locked`

## Swept

- validation: C7, C8, C9, C10, C11, C12, C13, C14, C15
- failure modes: C2, C12, C16, C20, C22
- idempotency and retry: C19, C20, C21, C23
- authorization: existing `client_for`/`client_for_with_timeout` e PAT; C12 cobre 401/403 antes de geração/writers
- concurrency and ordering: C1, C3, C4, C7, C8, C13, C14, C15, C19
- data lifecycle: n/a - launch context e fingerprint são somente memória; não há persistência, migração ou resume
- external-dependency failure: C7, C9, C10, C12, C16, C22
- state transitions: C1, C2, C3, C4, C5, C12, C15, C16, C17, C18, C20, C21, C22
- observability: Unresolved 1; telemetria foi explicitamente deixada fora do build

## Handoff

S1-S5 compartilham módulos; a superfície fonte única medida antes da implementação foi 405.631 bytes, aproximadamente 101.408 tokens (`bytes / 4`). Isso cabe em uma batch abaixo de 150k, então não há handoff intermediário. A boundary final é após C24 e a atualização do snapshot; a verificação independente deve usar a faixa da base da feature até `HEAD`.
