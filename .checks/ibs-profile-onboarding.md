# Onboarding de perfis IBS por remote Azure

Sources:

- `.tasks/ibs-profile-onboarding.md` - critérios, estados, superfícies e decisões vinculantes
- `.tasks/issue-14-process-profiles.md` - comportamento legado de perfis, fallback, settings e handoff
- `apps/rust/src/config/mod.rs`, `apps/rust/src/features/process_profiles.rs` - formato persistido e seleção por remote
- `apps/rust/src/features/describe.rs`, `apps/rust/src/features/test_card.rs`, `apps/rust/src/tui/live.rs`, `apps/rust/src/tui/test_flow.rs` - preparação, geração, publicação, criação e recovery
- `README.md` - contrato público de configuração e comandos

## Out of scope

- Descoberta remota de fields ou onboarding fora da organização `ibsbiosistemico` - a decisão ocorre somente com Git/configuração local.
- Credenciais por perfil, alteração de provider/prompt e alteração de writers remotos - PAT/API key permanecem globais e os fluxos remotos existentes continuam responsáveis por escrever.

## Landing

O domínio de configuração receberá perfis genéricos com `programField` persistido, mantendo o mapeamento legado quando a chave não existe. Um módulo de onboarding compartilhado resolverá o remote e persistirá um perfil/binding atômico; `desc` e `test` passarão um `ProfileSelection` congelado aos fluxos existentes, sem duplicar a decisão.

| One-way door | Literal shape | Alternative rejected |
| --- | --- | --- |
| Perfil genérico deixa de depender de schema fechado | `ProcessProfile.name` e `programField` livres; perfis legados sem a chave derivam Agrotrace/CheckMilk | Inferir o field pelo nome, que não representa processos novos |
| Identidade do binding | `(organization, project, repository)` exata, sem caminho local e com organização IBS case-insensitive para o gatilho | Binding por checkout ou apenas por projeto |
| Salvamento do onboarding | Um perfil novo + um binding, sem alterar `defaultProfile` ou incluir segredos | Tornar o perfil novo default global ou duplicar credenciais |
| Decisão antes de efeitos externos | Resolver/onboard antes do provider e de qualquer writer; após salvar usar um snapshot | Reselecionar em retry/handoff |
| Execução sem canal de escolha | `--dry-run`, `--raw` e sem TTY não alteram config nem perguntam; imprimem orientação acionável | Prompt bloqueante ou criação automática |
| Importação | Copiar todos os campos de processo para draft novo; somente `name` é identidade editável | Sobrescrever ou reutilizar a origem |
| Transição do pai | `parentTransition` vazio não oferece patch; valor não vazio é aplicado literalmente | Aplicar estado implícito após o usuário limpar o campo |

## Checks

### S1 - Domínio, resolução e persistência · ~12 arquivos · ~25k

**C1** - Binding único de um remote IBS seleciona exatamente o perfil, mostra nome/campo e não sinaliza onboarding.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked process_profiles::tests::bound_ibs_remote_selects_profile_without_onboarding`

**C2** - A mesma tupla Azure em checkouts locais distintos produz a mesma seleção e não consulta caminho local.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked process_profiles::tests::selection_is_independent_of_local_checkout`

**C3** - Remote IBS sem binding é identificado com os três segmentos e produz as ações de onboarding sem efeitos externos.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::missing_ibs_binding_exposes_remote_and_actions`

**C4** - Remote de outra organização sem binding não inicia onboarding, não persiste e usa default/fallback legado.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::non_ibs_remote_uses_existing_fallback`

**C5** - Erro de configuração é retornado antes da decisão de onboarding ou de qualquer operação remota.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::invalid_config_blocks_onboarding_and_remote_effects`

**C6** - Bindings duplicados para o mesmo remote falham mencionando remote e todos os perfis conflitantes.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked process_profiles::tests::duplicate_remote_bindings_should_fail_with_profiles`

**C7** - Novo draft expõe todos os campos persistidos com `priority = 2` e `inheritIterationPath = true`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::new_draft_has_required_fields_and_defaults`

**C8** - Draft inválido identifica o campo, permanece editável e não altera config nem começa geração.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::invalid_draft_is_rejected_without_persisting`

**C9** - Importação lista nomes e copia byte a byte todos os campos de processo, sem mutar a origem.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::import_copies_profile_values_without_mutating_source`

**C10** - Edição parcial do draft importado conserva campos não editados e inclui o valor editado no resumo.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::partial_edit_preserves_unedited_values`

**C11** - Revisão mostra remote, origem, nome e todos os valores finais e exige confirmação explícita.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::review_requires_explicit_save_confirmation`

**C12** - Salvamento confirmado grava um perfil genérico, `programField`/campos camelCase e um binding exato, preservando default e excluindo segredos.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::confirmed_save_is_exact_and_secret_free`

**C13** - Reexecução ou continuação após salvar encontra o binding existente e não duplica perfil/binding nem reabre onboarding.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::saved_binding_is_idempotent`

**C14** - Cancelamento, draft inválido ou falha de persistência preservam config anterior e não iniciam provider/writer.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::cancel_or_persist_failure_preserves_previous_config`

**C15** - `Agora não` não altera config e continua uma única vez com a seleção fallback existente.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::skip_keeps_fallback_without_persisting`

### S2 - Integração dos fluxos e snapshot · ~14 arquivos · ~35k

**C16** - Salvamento fecha onboarding e retoma exatamente uma execução com argumentos e overrides preservados.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked onboarding::tests::save_resumes_original_execution_once`

**C17** - `desc` retém o perfil ativo, usa reviewers por target e mantém edição manual com precedência.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked describe::tests::profile_reviewers_are_frozen_with_manual_override`

**C18** - `test` inicia settings a partir do perfil, herdando IterationPath quando configurado, e CLI prevalece.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::profile_settings_are_used_before_cli_overrides`

**C19** - Payload usa `Custom.Team` e o `programField` selecionado, sem enviar field de programa legado quando o campo é genérico.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::payload_uses_selected_program_field_only`

**C20** - Parent transition não vazia gera patch literal preservando esforços; vazia não oferece/executa patch.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::parent_transition_is_optional_and_literal`

**C21** - Handoff `desc` → `test` carrega o mesmo `ProfileSelection` em reviewers, settings, payload e recovery sem novo onboarding.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::published_handoff_preserves_profile_selection`

**C22** - Falhas após seleção preservam estado/mensagem/retry existentes, sem associação automática nova.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked test_card::tests::post_selection_failure_preserves_profile_for_retry`

### S3 - Init, doctor, documentação e suíte · ~8 arquivos · ~20k

**C23** - `prt init` preserva perfis genéricos/bindings e `prt doctor` valida nome, field e reviewers sem tratar schema genérico válido como unsupported ou revelar segredos.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked init::tests::init_preserves_generic_profiles && cargo test --manifest-path apps/rust/Cargo.toml --locked doctor::tests::doctor_accepts_generic_profile_and_reports_invalid_values`

**C24** - Documentação descreve gatilho/binding IBS, camelCase/`programField`, campos/defaults, importação, ausência de segredos e as três ações; suíte completa passa.
Proof: `grep -q "ibsbiosistemico" README.md && grep -q "programField" README.md && cargo test --manifest-path apps/rust/Cargo.toml --locked`

## Test policy

| Surface | Policy | Verdict |
| --- | --- | --- |
| Seleção/persistência | Testes unitários determinísticos com `Config` e `RepositoryRemote` sintéticos | required |
| TUI onboarding | Testes de estado/render existentes ou novos; nenhum teste depende de terminal real para domínio | required |
| Provider/Azure writer boundary | Asserts de preparação e payload antes de chamadas; sem credenciais reais | required |
| Init/doctor/docs | Testes de preservação/diagnóstico + grep documental + suíte completa | required |

## Coverage

| Criterion | Proof | Assertion surface |
| --- | --- | --- |
| C1-C6 | seleção/onboarding/process profiles | remote identity, conflict, fallback, gate |
| C7-C15 | onboarding domain/TUI/persistence | draft validation, review, atomic config, idempotency |
| C16-C22 | describe/test/handoff | frozen selection, reviewers, settings, payload, recovery |
| C23-C24 | init/doctor/README/full suite | compatibility, diagnostics, documentation |
