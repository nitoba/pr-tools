# Migração canônica de perfis e `prt init` global

Sources:

- `.tasks/profile-lifecycle-and-global-init.md` - critérios, decisões, limites e superfícies
- `.tasks/ibs-profile-onboarding.md` - comportamento de onboarding, fallback, seleção e recuperação que permanece compatível
- `apps/rust/src/config/mod.rs`, `apps/rust/src/features/init.rs`, `apps/rust/src/tui/init_wizard.rs` - configuração, persistência e wizard
- `apps/rust/src/features/onboarding.rs`, `apps/rust/src/tui/profile_onboarding.rs`, `apps/rust/src/main.rs` - decisão contextual, tela e fronteira antes de IA/writers
- `apps/rust/src/features/process_profiles.rs`, `apps/rust/src/features/doctor.rs`, `apps/rust/src/features/describe.rs`, `apps/rust/src/features/test_card.rs` - seleção, diagnóstico e consumidores
- `README.md` - contrato público de configuração e comandos

## Out of scope

- onboarding de remotes não Azure ou não parseáveis - esses remotes mantêm o fallback existente
- novo subcomando ou mudança do alias `prt` sem argumentos - continua encaminhando para `desc`
- edição automática de perfil já associado, credenciais por perfil, descoberta de schema no Azure e mudanças no provider, prompt, publicação ou recovery existentes

## Landing

`Config` continua sendo o contêiner de configuração global e `ProcessProfile` passa a ser a única fonte persistida de processo. A migração normaliza JSON legado antes dos consumidores, enquanto `init` preserva as coleções canônicas em vez de reconstruí-las.

O onboarding reutiliza a seleção por identidade remota existente e é executado uma única vez antes de preparação, IA ou writers; modos sem interação apenas orientam o próximo comando.

| One-way door | Literal shape | Alternative rejected |
| --- | --- | --- |
| Configuração de processo deixa a raiz | `profiles[]` contém os valores de processo; as seis chaves raiz são removidas e não são serializadas | manter duas fontes e permitir que `init` sobrescreva um perfil com vazio - preserva a divergência que a task elimina |
| `prt init` torna-se editor global | wizard expõe PAT, provider, executáveis/modelos/reasoning, endpoint compatível e template; não cria perfil/binding | continuar editando `Agrotrace` no init - mistura configuração global com identidade do remote |
| Gatilho contextual torna-se qualquer Azure parseável | ausência de binding exato para `(organization case-insensitive, project exact, repository exact)` abre as três ações | restringir a `ibsbiosistemico` - deixa remotes Azure válidos sem onboarding |
| Não-interativo não grava onboarding | `--dry-run`, `--raw` e stdout sem TTY emitem remote + orientação e retornam sem persistência | prompt parcial ou criação automática - quebra uso script-friendly e confirmação explícita |

## Checks

### S1 - Migração para a fonte única de perfil · 28,420 B / 4 ≈ 7.1k tokens

**C1** - JSON legado sem `profiles` materializa exatamente um perfil `Agrotrace`, com as seis entradas de processo, defaults `priority: 2`, `inheritIterationPath: true`, `parentTransition: "Test QA"` e `defaultProfile: "Agrotrace"`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked config::tests::legacy_json_migration_should_materialize_single_agrotrace_profile`

**C2** - JSON com perfil e chaves raiz duplicadas remove somente as seis chaves, preserva valores explícitos do perfil e mantém perfis, bindings, `defaultProfile` e configurações globais.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked config::tests::normalization_should_remove_legacy_keys_and_preserve_existing_profiles`

**C3** - Toda serialização canônica omite as seis chaves raiz e nenhum perfil serializado contém `azurePat` ou `apiKey`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked config::tests::canonical_serialization_should_omit_legacy_root_keys_and_profile_secrets`

**C4** - Overrides de processo antigos no `.env` não são aplicados nem regravados, enquanto `AZURE_PAT` e `PR_AI_API_KEY` continuam disponíveis como segredos globais.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked config::tests::legacy_process_dotenv_should_be_ignored_while_global_secrets_are_preserved`

**C5** - Nova carga/persistência da mesma configuração é idempotente, não duplica perfil/binding/chave legada e uma falha de substituição deixa o conteúdo original intacto.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked config::tests::legacy_migration_should_be_idempotent_and_keep_original_on_atomic_failure`

### S2 - `prt init` somente global · 75,662 B / 4 ≈ 18.9k tokens

**C6** - Campos e etapas do wizard de `prt init` contêm somente PAT, provider, executáveis/modelos/reasoning, endpoint compatível/API key e conteúdo global; não contêm `reviewerDev`, `reviewerSprint`, `testAreaPath`, `testAssignedTo`, `testProgram`, `testTeam`, `profile` ou `parentTransition`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::init_wizard::tests::init_wizard_should_expose_only_global_fields`

**C7** - Salvar alteração global preserva byte a byte `profiles`, `bindings` e `defaultProfile`, sem criar/editar perfil e sem reintroduzir chaves raiz.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked init::tests::global_init_should_preserve_profiles_bindings_and_default`

**C8** - Init sem remote ou sem TTY consegue salvar apenas configuração global e não cria `Agrotrace`, binding ou associação de repositório.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked init::tests::global_init_without_remote_should_not_create_process_association`

### S3 - Onboarding contextual do remote Azure · 382,769 B / 4 ≈ 95.7k tokens

**C9** - Qualquer remote Azure parseável sem binding exato produz `NeedsOnboarding` com a tupla atual, e a tela oferece `Novo perfil`, `Importar perfil` e `Agora não` antes do fluxo remoto.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::any_azure_remote_without_binding_requires_onboarding`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::profile_onboarding::tests::action_screen_should_show_remote_and_all_three_decisions`

**C10** - Um único binding exato evita onboarding e seleciona o perfil apontado; a resolução de `prt` sem subcomando continua usando o caminho de `desc`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::exact_binding_selects_profile_without_onboarding`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tests::bare_prt_uses_desc_execution_path`

**C11** - Remote não Azure ou não parseável não abre onboarding, não altera arquivos e conserva o fallback existente.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked git::tests::non_azure_remote_is_not_parseable_for_onboarding`

**C12** - Novo/importado edita exatamente os onze campos definidos, exige confirmação explícita e salva um perfil e um binding para a tupla atual, sem alterar `defaultProfile` ou inserir credenciais.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::confirmed_save_is_exact_and_secret_free`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::profile_onboarding::tests::review_screen_should_show_all_fields_and_explicit_save`

**C13** - Draft inválido, cancelamento, recusa de confirmação e falha de persistência não alteram `config.json`/`.env`, não iniciam IA/writer e exibem orientação acionável; `Agora não` executa uma vez com o fallback original.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::invalid_cancelled_or_failed_onboarding_preserves_previous_configuration`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::skip_keeps_fallback_without_persisting`

**C14** - Após salvar, repetir o mesmo remote reutiliza o binding e não cria segundo perfil/binding nem mostra onboarding.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::saved_remote_reuses_binding_without_duplicate_onboarding`

**C15** - `--dry-run`, `--raw` e stdout sem TTY não perguntam nem persistem migração/onboarding; informam organization/project/repository e orientam o fluxo interativo.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::non_interactive_guidance_names_remote_and_preserves_no_write_contract`

**C16** - `desc` e `test` usam reviewers, settings de Test Case e `programField` do `ProfileSelection` escolhido por binding, sem consultar chaves raiz removidas.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked describe::tests::selected_profile_provides_reviewers_to_desc`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::selected_profile_provides_test_settings_and_program_field`

### S4 - Diagnóstico, documentação e prova · 64,235 B / 4 ≈ 16.1k tokens

**C17** - `doctor` identifica remote e perfil selecionado nos checks de reviewers, Team, programa, prioridade e metadata; ausência de raiz não gera aviso falso nem recomenda editar perfil com `prt init`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked doctor::tests::doctor_reports_selected_profile_without_legacy_root_defaults`

**C18** - README documenta init global-only, onboarding para qualquer Azure sem binding, remoção das seis chaves raiz e segredos globais; o exemplo JSON não duplica processo na raiz.
Proof: `grep -n -E "init.*global|qualquer.*Azure|seis chaves|PAT.*API key|profiles" README.md`

**C19** - Suíte Rust cobre as superfícies desta task e os quatro comandos de validação do projeto passam.
Proof: `cargo fmt --manifest-path apps/rust/Cargo.toml -- --check`
Proof: `cargo clippy --manifest-path apps/rust/Cargo.toml --locked --all-targets -- -D clippy::correctness`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked`
Proof: `cargo build --manifest-path apps/rust/Cargo.toml --locked --all-targets`

## Swept

- validation: C1, C2, C6, C9, C12, C16, C17
- failure modes: C5, C7, C8, C11, C13, C15
- idempotency and retry: C5, C14, C16
- authorization: existing global PAT/client remains; onboarding and init are local
- concurrency and ordering: C9, C12, C15, C16 - selection/persistence precede IA and writers
- data lifecycle: C1-C5, C7, C14 - legacy roots are removed and canonical collections are retained
- external-dependency failure: existing remote/provider failure paths remain outside onboarding
- state transitions: C9, C12-C15
- observability: C9, C12, C15, C17, C18 - remote/profile/field/correction are visible without secrets

## Handoff

S1-S4 fit in one build batch: approximately 137.8k tokens by the `wc -c / 4` floor, below the 150k default; the work crosses config, init, onboarding/TUI, consumers, doctor and docs, so no mid-feature handoff is planned.

- Boundary: none yet; all 19 checks remain open until the feature commit(s).
- User decisions settled before build: profile values win conflicts, onboarding covers any parseable Azure remote, identity comparison is organization-insensitive/project-and-repository-exact, and non-interactive modes never persist.
- Abandoned: IBS-only onboarding was superseded by the current task; root process fields and profile editing in `prt init` are deliberately not retained.
