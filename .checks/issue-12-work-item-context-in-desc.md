# Contexto funcional do Work Item em `prt desc`

Sources:

- `.tasks/issue-12-work-item-context-in-desc.md` - critérios observáveis, estados, fronteira, superfícies, varredura e requisito aberto
- `.design/issue-12-work-item-context-in-desc.md` - **binding para a interface**: projeção funcional, prompt, limites de texto, fallback da TUI e comportamento não interativo
- `apps/rust/src/features/describe.rs` - preparação atual, resolução do Work Item, geração e recuperação de publicação
- `apps/rust/src/ai/mod.rs` - prompt atual, regras de grounding, normalização da resposta e limite de 3999 caracteres
- `apps/rust/src/azure/mod.rs` - cliente Azure e modelo genérico de Work Item
- `apps/rust/src/azure/work_items.rs` - acesso aos campos e fronteira de leitura de Work Items
- `apps/rust/src/features/test_card.rs` - convenções existentes de leitura de campos e escopo que não deve mudar em `prt test`
- `apps/rust/src/main.rs` - dry-run, saída plain/raw, detecção de TTY e códigos de saída
- `apps/rust/src/tui/describe_app.rs` - estado e revisão da descrição
- `apps/rust/src/tui/live.rs` - backend de geração, renderização da indicação de contexto e confirmação/publicação
- `apps/rust/src/tui/events.rs` - eventos entre backend e TUI
- `README.md` - contrato existente de saída, clipboard e publicação
- [Get Work Item REST API](https://learn.microsoft.com/en-us/rest/api/azure/devops/wit/work-items/get-work-item?view=azure-devops-rest-7.1) - campos retornados pela leitura Azure
- [Query by title, ID, or rich-text fields](https://learn.microsoft.com/en-au/azure/devops/boards/queries/titles-ids-descriptions?view=azure-devops) - disponibilidade e tipo HTML dos campos funcionais

## Out of scope

- alterar o prompt ou a semântica de `prt test` - a reutilização de helpers não muda esse fluxo
- buscar comentários, discussões, anexos, relações ou histórico - a primeira versão usa somente os seis campos funcionais projetados
- descobrir schema ou aliases dinamicamente - não há evidência de múltiplos processos Azure incompatíveis
- sincronizar, atualizar ou executar critérios no Azure DevOps - esta mudança é somente leitura e geração
- inferir outro Work Item, classificar completude com IA, adicionar telemetria, flag de bypass, tela nova ou editor novo - não são necessários para a entrega decidida

## Landing

`FunctionalWorkItemContext` será a projeção segura na fronteira `azure::work_items`; `describe::prepare` fará a leitura somente quando houver ID e produzirá `FunctionalContextStatus`. O prompt recebe a projeção opcional, enquanto a TUI reutiliza o estado/modal e a revisão/publicação existentes.

| One-way door | Literal shape | Alternative rejected |
| --- | --- | --- |
| Projeção funcional no limite Azure | `FunctionalWorkItemContext { id, title, work_item_type, area_path, description, acceptance_criteria }`, com os dois campos ricos opcionais já normalizados e limitados | transportar o mapa bruto de `fields` para `DescribePrep` e selecionar em cada consumidor - duplica a política de campos e expõe conteúdo não necessário |
| Estado de falha e fallback | `FunctionalContextStatus::{NotRequested, Loaded(FunctionalWorkItemContext), Unavailable(String)}`; `Unavailable` exige confirmação Git-only na TUI e falha em modo não interativo | fallback automático - silencia a perda do requisito funcional e contradiz os critérios 7 e 8 |
| Grounding do provider | seção `## Contexto funcional do Work Item` rotulada como intenção/requisito, seguida de `## Contexto Git` como evidência | misturar as duas fontes - permite apresentar critério planejado como implementação entregue |

- Nada mais nesta mudança é difícil de reverter; o fluxo de publicação, os payloads Azure e o limite `< 4000` permanecem existentes.

## Checks

### S1 - Projeção funcional e grounding do prompt · 3 arquivos · ~52 KB · ~13k

**C1 [critério 1]** - sem ID resolvido, a preparação retorna `NotRequested`, não cria cliente Azure e o prompt não contém `## Contexto funcional do Work Item`.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked preparation_without_work_item_should_not_request_functional_context`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked no_work_item_should_skip_azure_and_functional_prompt`

**C2 [critério 2]** - contexto carregado inclui ID, Título e Tipo, inclui Área/Descrição/Critérios somente quando disponíveis e mantém `## Contexto Git` separado.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked describe_prompt_should_include_projected_functional_context`

**C3 [critério 3]** - valores ausentes, vazios e não-textuais de Description/Acceptance Criteria viram `None`, sem erro, preservando os demais campos projetados.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked functional_context_should_omit_absent_or_non_text_optional_fields`

**C4 [critério 4]** - HTML/markup é convertido para texto sem tags, entidades comuns são decodificadas, separadores de bloco/lista/quebra viram linhas, cada campo rico fica em até 3000 caracteres, o par em até 6000 e qualquer corte contém marcador explícito.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked functional_context_should_normalize_and_bound_rich_text`

**C5 [critério 5]** - as instruções identificam Work Item como intenção/requisito, Git Log/Diff como evidência, exigem evidência Git para chamar critério de implementado e proíbem copiar o Work Item como corpo do PR.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked describe_prompt_should_separate_intent_from_git_evidence`

### S2 - Preparação, falha, revisão e privacidade · 5 arquivos · ~139 KB · ~35k

**C6 [critério 6]** - revisão/saída plain exibem `Contexto funcional: Work Item #ID — título` quando carregado, a geração começa somente após a preparação e `--raw` imprime apenas o body Markdown.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked desc_output_should_show_functional_context_but_raw_should_be_body_only`

**C7 [critério 7]** - falha Azure, inclusive 401/403, aparece como contexto indisponível; a TUI só inicia Git-only após confirmação e `n`/`Esc` termina sem iniciar backend/provider.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked functional_context_failure_should_require_explicit_git_only_confirmation`

**C8 [critério 8]** - falha com ID em `--raw`, `--dry-run` ou sem TTY produz erro acionável e código 1 antes de qualquer chamada ao provider ou fallback automático.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked non_interactive_functional_context_failure_should_be_actionable`

**C9 [critério 9]** - logs, receipt e preparação de publicação carregam somente status/ID/título projetados, nunca mapa ou rich text bruto, enquanto revisão, edição, clipboard, limite, retry e recuperação existentes continuam cobertos.
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked functional_context_should_not_leak_raw_work_item_data`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked`

## Swept

- validation: C1, C2, C3 e C4 - ID e campos opcionais são resolvidos/projetados deterministicamente, com limites concretos
- failure modes: C7 e C8 - falha Azure, 401/403 e ausência de confirmação têm saídas definidas
- idempotency and retry: C9 - leitura é GET sem persistência; retry/publicação reutiliza o estado existente
- authorization: C7 e C8 - cliente Azure existente mantém PAT e 401/403 são explícitos, sem ocultar a falha
- concurrency and ordering: C6 e C7 - preparação/leitura termina antes de gerar; falha aguarda confirmação antes do backend
- data lifecycle: C9 - nenhum Work Item, draft ou status novo é persistido
- external-dependency failure: C7 e C8 - TUI confirma fallback; raw, dry-run e sem TTY falham sem gerar
- state transitions: C1, C6, C7 e C8 - sem ID, carregado, indisponível, Git-only, geração e abandono têm caminhos definidos
- observability: C9 - logs e receipt usam somente status/ID/título, nunca mapa ou conteúdo rico

## Handoff

S1-S2 cabem em uma única batch: os arquivos atualmente tocados somam aproximadamente 190 KB pela regra `wc -c / 4` (~48k tokens), muito abaixo do teto de 150k; a fronteira natural é entre Azure/prompt e TUI/saída, mas não há necessidade de handoff de build.
