# Editar título e Markdown antes da publicação

Sources:

- `.tasks/issue-9-edit-content-before-publish.md` - critérios observáveis, estados, fronteira, superfícies e varredura de requisitos
- `.design/issue-9-edit-content-before-publish.md` - **binding para a interface**: jornada, shape, decisões, roadmap e telas `prt desc/test / Revisão/Editor`
- `apps/rust/src/tui/live.rs` - ciclo de revisão/publicação PR, clipboard, multi-target e recuperação
- `apps/rust/src/tui/describe_app.rs` - estado canônico do conteúdo PR e estados de publicação
- `apps/rust/src/tui/test_flow.rs` - revisão/settings do Test Case, criação, clipboard e recuperação
- `apps/rust/src/features/test_card.rs` - `validate_card`, transformação de Markdown e limite do create
- `apps/rust/src/ai/mod.rs` - `PrDescription`, normalização e limite `< 4000` do PR
- `apps/rust/src/azure/pull_requests.rs` - payload e publicação sequencial por target
- `apps/rust/src/azure/work_items.rs` - json-patch e builders de `System.Description`/steps
- `README.md` - contrato existente de clipboard body-only e regras de conteúdo

## Out of scope

- regeneração parcial via IA ou instruções de reescrita - correção manual aprovada é a única capacidade desta issue
- histórico, versionamento, sincronização ou persistência de drafts - o rascunho vive somente durante a execução
- atualização de PR já publicado - a edição termina antes da primeira publicação
- seleção, undo/redo, mouse, busca, editor IDE ou edição rica - o editor é limitado a Unicode/multilinha e navegação definida
- alteração de reviewers ou dos seis campos de settings do Test Case - os editores existentes permanecem separados

## Landing

O editor compartilhado ficará em `apps/rust/src/tui/content_editor.rs` e será registrado em `tui::mod`; os dois fluxos reutilizarão o mesmo modelo de rascunho, navegação Unicode-safe, renderização e validação de conteúdo. O conteúdo salvo continuará na representação canônica já consumida por preview/clipboard, e os builders Azure existentes continuarão montando os payloads.

| One-way door | Literal shape | Alternative rejected |
| --- | --- | --- |
| Editor compartilhado | `TextEditor`, `ContentField` e `ContentEditState` em `tui::content_editor`, com título single-line, body multiline, `Tab` entre os dois e `Ctrl+S`/`Esc` para salvar/cancelar | duplicar dois editores nos fluxos - permitiria divergência de cursor, atalhos e regras Unicode |
| Conteúdo aprovado congelado por tentativa | `DescribeApp.frozen_publish_content: Option<PrDescription>` e `TestApp.frozen_create_content: Option<PrDescription>`, preenchidos antes da primeira chamada remota e reutilizados em retry/targets pendentes | reler campos mutáveis após falha - pode misturar versões e quebrar a busca por título exato |
| Nada mais nesta mudança é difícil de reverter | - | - |

## Checks

### S1 - Editor compartilhado e ciclo de draft · 4 arquivos · ~54 KB · ~14k

**C1** - `prt desc` em revisão abre `ContentEditState` com `ContentField::Title` e `ContentField::Body` iguais ao `PrDescription` ativo, com título single-line e body multiline
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked editing_desc_should_open_content_editor_with_generated_content`

**C2** - `prt test` em revisão abre o editor com título/body ativos sem alterar os seis settings nem compartilhar o foco de settings
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked editing_test_should_open_content_editor_without_touching_settings`

**C3** - inserir/colar `Título — ação ✅` e `ação concluída\n- [ ] validar\nlinha final` preserva Unicode e quebras de linha, e setas/Home/End movimentam o cursor por caracteres
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked content_editor_unicode_and_multiline_navigation_should_preserve_text`

**C4** - `Tab` alterna somente entre `ContentField::Title` e `ContentField::Body`, sem alcançar settings, target ou diálogo
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked content_editor_tab_should_cycle_only_title_and_body`

**C5** - `Enter` no body insere `\n` no cursor; `Enter` no título é consumido sem inserir quebra nem salvar
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked content_editor_enter_should_insert_body_newline_only`

**C6** - `q`, `j`, `k`, `c` e atalhos equivalentes são consumidos pelo editor ativo e não alteram revisão, scroll, clipboard, target ou painel
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked content_editor_should_consume_review_shortcuts`

### S2 - Save/cancel, validação e conteúdo canônico · 5 arquivos · ~206 KB · ~52k

**C7** - `Ctrl+S` com conteúdo válido descarta o draft, fecha o editor e faz o preview ler exatamente os buffers salvos
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked saving_valid_content_should_update_desc_preview_exactly`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked saving_valid_content_should_update_test_preview_exactly`

**C8** - título vazio, body PR com 4000 chars e body Test Case vazio impedem save/publicação/criação e deixam o editor aberto com erro no campo correspondente
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked invalid_desc_content_should_stay_in_editor_without_remote_start`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked invalid_test_content_should_stay_in_editor_without_remote_start`

**C9** - `Esc` descarta o draft e mantém título/body/preview byte a byte; sair antes da primeira chamada não persiste draft
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked canceling_content_edit_should_discard_desc_draft`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked canceling_content_edit_should_discard_test_draft`

**C10** - body PR com 3999 caracteres é aceito e body com 4000 é rejeitado, preservando a regra existente de `< 4000`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked edited_pr_body_should_use_existing_3999_character_boundary`

**C11** - Test Case rejeita body somente whitespace e título vazio por `validate_card`, mas aceita body não vazio de 4000 chars sem aplicar o limite de PR
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked edited_test_content_should_use_validate_card_without_pr_body_limit`

**C12** - save válido preserva leading/trailing whitespace, Unicode, quebras e marcadores Markdown exatamente como digitados e não chama `normalize_description`
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked saved_content_should_preserve_exact_whitespace_unicode_and_markdown`

### S3 - Clipboard, payload, retry e multi-target · 6 arquivos · ~181 KB · ~45k

**C13** - `c` em revisão copia somente o body aprovado byte a byte; `c` no editor é inserido no buffer e não chama clipboard
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked copy_should_use_approved_body_only_and_editor_should_consume_c`

**C14** - antes da primeira publicação PR o snapshot aprovado é preenchido, cada `CreatePrInput` usa o mesmo título/body e a edição fica indisponível em publicação/recuperação
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked publish_should_freeze_approved_content_before_first_remote_call`

**C15** - antes do primeiro create Test Case o snapshot aprovado alimenta `System.Title`, `System.Description` e `Microsoft.VSTS.TCM.Steps` pelos builders existentes sem reler draft mutável
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked create_should_freeze_approved_content_and_use_existing_builders`

**C16** - falha pós-chamada mantém conteúdo aprovado visível, retry reutiliza snapshot sem regenerar, e busca de candidatos usa o título exato da tentativa
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked publish_and_create_retry_should_reuse_frozen_content_and_exact_title`

**C17** - publicação PR multi-target usa o mesmo snapshot para targets pendentes e não envia uma segunda versão sem nova confirmação; reviewers/settings continuam editáveis
Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked pending_publish_targets_should_reuse_one_frozen_content_snapshot`

### S4 - Fronteira UI e regressão visual · 4 arquivos · ~170 KB · ~42k

**C18** - a suíte contém provas unitárias/de fluxo para save, cancel, Unicode/multilinha, isolamento de atalhos, validação PR/Test Case, clipboard, payload inicial, failure/retry, snapshot multi-target e snapshots TUI dos dois editores; snapshots não são atualizados automaticamente
Proof: `INSTA_UPDATE=no cargo test --manifest-path apps/rust/Cargo.toml --locked`

## Swept

- validation: C8, C10, C11, C12
- failure modes: C8, C16
- idempotency and retry: C16, C17
- authorization: existing - guards de PAT/remote em `make_publish_parts` e `test_card::prepare` permanecem no limite remoto
- concurrency and ordering: C14, C15, C17 - snapshot criado antes da primeira chamada e ordem de targets existente preservada
- data lifecycle: C9 - draft/snapshot somente em memória, sem persistência ou migração
- external-dependency failure: C16 - `PublishFailure`/`CreateFailure` e recovery dialogs existentes continuam sendo usados
- state transitions: C4, C7, C8, C9, C14, C15, C16
- observability: not in scope - não há telemetria nova; igualdade exata é o proxy exigido

## Handoff

S1-S4 cabem em uma única batch: os arquivos existentes somam aproximadamente 89k tokens pela regra `wc -c / 4`, e o novo editor/testes permanecem abaixo do teto de 150k; a fronteira natural é entre editor, integração e payload, mas não há necessidade de handoff de build.

- Boundary: única batch fechada após C18; implementação em `5bc160e` e verificação independente em `.checks/issue-9-edit-content-before-publish.verified.md`.
- User-settled mid-build: nenhum; o ticket declara 0 questões abertas.
- Abandoned: nenhum; alternativas rejeitadas estão registradas em `Landing`.
