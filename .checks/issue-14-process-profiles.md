# Perfis de processo Azure por repositório

Sources:

- `.tasks/issue-14-process-profiles.md` - **binding for scope**: intent, 23 critérios, estados, superfície, decisões, fontes e limites da issue
- [GitHub issue #14](https://github.com/nitoba/pr-tools/issues/14) - **binding for scope**: perfis por repositório, compatibilidade, validação pré-escrita, UX, segurança e fora de escopo
- Solicitação do usuário em 15/09/2026 - **binding for scope**: perfis `Agrotrace` e `CheckMilk`, campos, reviewers, processo `CHECKMILK` e estado `Test QA`
- `apps/rust/src/config/mod.rs` - configuração JSON, precedência e defaults atuais
- `apps/rust/src/features/init.rs` - draft, defaults, reviewers e persistência segura
- `apps/rust/src/tui/init_wizard.rs` - formulário, revisão e armazenamento de segredos
- `apps/rust/src/git/mod.rs` - `RepositoryRemote` e parsing do remote Azure
- `apps/rust/src/features/test_card.rs` - preparação, settings, criação, recovery e atualização do pai
- `apps/rust/src/tui/test_flow.rs` - revisão e fluxo interativo do Test Case
- `apps/rust/src/tui/describe_app.rs` e `apps/rust/src/tui/live.rs` - reviewers e publicação de PRs
- `apps/rust/src/azure/work_items.rs` - payloads JSON Patch e transição atual
- `apps/rust/src/azure/mod.rs` - autenticação, erros HTTP e `api-version=7.1`
- `apps/rust/src/features/doctor.rs` e `apps/rust/src/tui/doctor_flow.rs` - checks e exit code do diagnóstico
- `README.md` e `AGENTS.md` - contrato documentado e comandos de validação
- [Azure DevOps REST - List Work Item Types](https://learn.microsoft.com/en-us/rest/api/azure/devops/wit/work-item-types/list?view=azure-devops-rest-7.1) - resposta e presença de `Test Case`
- [Azure DevOps REST - List Work Item Type Fields](https://learn.microsoft.com/en-us/rest/api/azure/devops/wit/work-item-types-field/list?view=azure-devops-rest-7.1) - campos e metadados de required/default/allowed values
- [Azure DevOps REST - List Work Item Type States](https://learn.microsoft.com/en-us/rest/api/azure/devops/wit/work-item-type-states/list?view=azure-devops-rest-7.1) - estados válidos do Work Item pai

## Out of scope

- suporte genérico a custom fields, tipos ou regras Azure - V1 suporta somente os schemas fixos `Agrotrace` e `CheckMilk`
- descoberta irrestrita de fields, editor visual de Process Template, criação de processos, migração de Work Items, sincronização remota, multi-tenancy e credencial por perfil
- alterações na geração/publicação de `prt desc`, providers de IA ou integrações GitHub/Jira além da resolução dos reviewers pelo perfil
- comportamento adicional de QA além dos fields/defaults e da transição opcional declarados no perfil
- binding real do repositório `CHECKMILK` - a implementação usa remote de fixture até o repositório ser informado

## Landing

Os perfis e bindings vivem aditivamente em `config.json`, com migração legada centralizada no módulo de configuração. A seleção reutiliza `RepositoryRemote`; a validação remota reutiliza `AzureClient`/Work Items antes dos writers existentes, e o snapshot selecionado viaja nos objetos de preparação e recovery em vez de duplicar fluxos.

| One-way door | Literal shape | Alternative rejected |
| --- | --- | --- |
| Configuração persistida | `profiles: [{name, areaPath, assignedTo, team, program, priority, inheritIterationPath, parentTransition, reviewerDev, reviewerSprint}]`, `bindings: [{profile, organization, project, repository}]`, `defaultProfile` em `config.json` | YAML ou arquivo paralelo - quebraria o loader, wizard e compatibilidade JSON existentes |
| Compatibilidade legada | perfil `Agrotrace` materializado de `testAreaPath`, `testAssignedTo`, `testTeam`, `testProgram`, reviewers, prioridade `2`, herança de iteração e `Test QA`; `defaultProfile: "Agrotrace"` | exigir edição manual ou quebrar configs antigas - contradiz a migração imediata e idempotente |
| Identidade de binding | tupla exata `(organization, project, repository)` de `RepositoryRemote` | diretório local ou nome isolado - clones e organizações diferentes não seriam distinguidos corretamente |
| Unicidade | zero ou um match; mais de um binding para o mesmo remote é erro explícito antes de qualquer write | primeiro match ou sobrescrita silenciosa - poderia aplicar o processo errado |
| Fronteira de segredo | PAT/API key permanecem no `.env`/ambiente/mecanismo atual; nenhum `ProcessProfile` serializa segredos | credencial por perfil - duplicaria e exporia credenciais fora do escopo |
| Schemas suportados | `Agrotrace` fixa `Custom.Team` + `Custom.ProgramasAgrotrace`; `CheckMilk` fixa `Custom.Team` + `Custom.ProgramasCheckmilk` | fields arbitrários - transformaria a V1 em uma linguagem genérica Azure |
| Validação pré-escrita | listar Work Item Type, fields com `$expand=all` e states antes do primeiro POST/PATCH | deixar o POST descobrir incompatibilidade - produz erro tardio e pouco acionável |
| Snapshot da tentativa | `ProfileSelection` acompanha `TestCardPrep`; recovery só relê PAT, não reseleciona o processo | reselecionar em cada retry - poderia misturar processos depois de resposta incerta |
| Transição do pai | `parentTransition: Option<String>`; sem valor não há PATCH; com `Test QA` o path mantém esforços | estado fixo global - impede processos sem transição e mistura regras entre perfis |

- Nada mais nesta mudança é uma porta de mão única.

## Checks

### S1 - Seleção, migração e compatibilidade legada · 3 arquivos · ~52 KB · ~13k

**C1** - Com dois perfis e dois bindings, cada remote seleciona exatamente o perfil associado e expõe seu identificador.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked process_profiles::tests::selection_should_match_each_bound_remote`

**C2** - Bindings duplicados para o mesmo remote falham na validação com remote e nomes dos perfis conflitantes, antes de qualquer escrita.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked process_profiles::tests::duplicate_remote_bindings_should_fail_with_profiles`

**C3** - Configuração sem perfis materializa `Agrotrace`, `defaultProfile`, prioridade `2`, herança de iteração e `Test QA`, mantendo `prt test`/`prt desc` compatíveis.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked config::tests::legacy_config_should_materialize_agrotrace_and_default`

**C4** - A mesma tupla de remote seleciona o mesmo perfil independentemente do diretório local.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked process_profiles::tests::selection_should_ignore_local_checkout_path`

**C5** - A migração legada é atômica e repetível, cria um único perfil/binding e nunca copia PAT, API key ou outro segredo para o perfil.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked config::tests::legacy_migration_should_be_atomic_idempotent_and_secret_free`

### S2 - Descoberta e validação remota antes da escrita · 3 arquivos · ~141 KB · ~35k

**C6** - A preparação/validação consulta os tipos do projeto e bloqueia quando `Test Case` não está disponível.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::metadata_validation_should_require_test_case_type`

**C7** - Somente os fields fixos do schema selecionado são aceitos; `referenceName`, tipo, allowed values e o campo de programa correto são validados.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::metadata_validation_should_require_supported_profile_fields`

**C8** - Field obrigatório sem default e sem valor final produz erro contendo o `referenceName` e não inicia write.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::metadata_validation_should_report_required_field_without_default`

**C9** - Estado configurado no perfil precisa existir no metadata do tipo do pai; `Test QA` é tratado literalmente.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::metadata_validation_should_require_configured_parent_state`

**C10** - HTTP, timeout ou transporte durante metadata bloqueia POST/PATCH e retorna erro de etapa sem expor credenciais.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::metadata_failure_should_prevent_creation_request`

**C11** - Seleção, fields, candidatos e retry usam o mesmo snapshot do perfil, mesmo quando o recovery relê o PAT atual.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::recovery_should_keep_profile_snapshot_when_pat_is_reloaded`

### S3 - Revisão, payload, transição e recovery · 3 arquivos · ~279 KB · ~70k

**C12** - A revisão mostra o identificador do perfil, settings, campo de programa e reviewers default por target.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_flow::tests::review_should_show_selected_profile_and_settings`

**C13** - `Agrotrace` mantém payload e defaults atuais, com `Custom.Team` e `Custom.ProgramasAgrotrace`; `--team`/`--program` continuam prevalecendo.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::agrotrace_payload_should_keep_defaults_and_cli_overrides`

**C14** - `CheckMilk` troca somente o caminho para `/fields/Custom.ProgramasCheckmilk`, envia `Checkmilk` e não envia o field Agrotrace.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked work_items::tests::checkmilk_patch_should_use_only_checkmilk_program_field`

**C15** - Sem transição não há PATCH de estado; com `Test QA` o patch preserva esforços e grava exatamente `System.State = Test QA`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked work_items::tests::optional_parent_transition_should_preserve_efforts`

**C16** - Falha pós-envio conserva perfil/fields enviados, recovery busca por esses valores e não reenvia sem decisão explícita.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_flow::tests::create_failure_should_keep_snapshot_and_require_explicit_retry`

### S4 - Wizard, reviewers e diagnóstico · 8 arquivos · ~387 KB · ~97k

**C17** - `prt desc` usa `reviewerDev`/`reviewerSprint` do perfil selecionado, mantendo edição manual e precedência dos overrides.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked describe_app::tests::profile_reviewers_should_override_legacy_defaults`

**C18** - Targets `dev` e `sprint` no mesmo remote resolvem reviewers distintos do mesmo perfil, sem misturar legado/outro repositório.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked describe_app::tests::profile_reviewers_should_follow_target`

**C19** - O wizard permite criar/editar perfil, reviewers e binding do remote atual e mostra esses dados na revisão antes de salvar.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked init_wizard::tests::wizard_review_should_show_profile_binding_and_reviewers`

**C20** - O wizard rejeita schema, field, tipo ou configuração fora dos dois schemas suportados antes de persistir.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked init::tests::unsupported_profile_schema_should_fail_before_save`

**C21** - Alterações de perfil/fields/reviewers/transição aparecem na revisão final antes da confirmação e `--no-create` não escreve no Azure.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_flow::tests::no_create_should_not_write_and_review_should_show_final_profile`

**C22** - `doctor` emite `[FALHA]`, remote/perfil/correção acionável e exit code `1` para binding ausente/ambíguo, field/state/reviewer inválido; válido emite `[OK]`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked doctor::tests::profile_binding_validation_should_report_actionable_failure`

### S5 - Documentação e suíte · 1 arquivo · ~10 KB · ~3k

**C23** - A documentação descreve formato, identidade, perfis/schemas, legado, reviewers, ausência de segredos e `prt init`/`prt doctor`; os testes requeridos existem e a suíte Rust passa.
Proof: `rg -n "defaultProfile|repository binding|Agrotrace|CheckMilk|Custom\.ProgramasAgrotrace|Custom\.ProgramasCheckmilk|reviewerDev|reviewerSprint|segredo|prt init|prt doctor" README.md && cargo test --manifest-path apps/rust/Cargo.toml --locked`

## Swept

- validation: C2, C6, C7, C8, C9, C10, C18, C20, C22
- failure modes: C2, C6, C7, C8, C9, C10, C16, C22
- idempotency and retry: C5, C11, C16
- authorization: existing `client_for`/`AzureClient` continuam exigindo PAT e preservando 401/403; C5 prova que perfis não carregam segredos
- concurrency and ordering: C1, C4, C11, C12, C17, C18 - seleção e snapshot precedem writers
- data lifecycle: C3, C5, C23 - migração apenas local, sem backfill de Work Items; chaves legadas não secretas permanecem durante compatibilidade
- external-dependency failure: C6, C7, C9, C10, C16
- state transitions: C3, C9, C15, C16
- observability: C1, C12, C17, C22, C23 - perfil/reviewers aparecem na revisão e diagnóstico não exibe segredo

## Handoff

As fronteiras foram desenhadas por superfície, porque a issue não trouxe slices upstream: S1 (~13k) permanece em configuração/seleção; S2 (~35k) entra em Azure/validação; S3 (~70k) entra na TUI de Test Case; S4 (~97k) cobre wizard, `desc` e doctor; S5 (~3k) fecha documentação. Cada slice é inteiro e fica abaixo do limite padrão de 150k tokens; handoff pretendido após S2, antes de entrar na TUI.

- Boundary: ainda não houve batch fechado; o próximo agente deve iniciar por S1 e S2 e deixar seus commits visíveis no diff.
- User settlements: as três questões `Unresolved` seguem os defaults da issue: fixture para o binding real de `CHECKMILK`, nenhum comportamento adicional de QA e bloqueio quando metadata não puder ser consultado.
- Abandoned: nenhuma abordagem foi implementada ou descartada durante o build.
