# Atualizar título e descrição de PR existente

> Planejar com **tlc-plan** (`.agents/skills/tlc-plan/SKILL.md`).
> As decisões abaixo carregam a forma literal; copie-as, não as rederive.

## Situation

- Project: em construção ativa.
- Decision: comprometida pelo requester em 2026-09-12, nesta sessão, a partir da [issue #10](https://github.com/nitoba/pr-tools/issues/10).
- In flight: o HEAD está em `feat/issue-10-update-title-or-description-from-pr-after-new-commits`, sobre o merge da issue 9. A issue 9 entregou `ContentEditState` e o editor compartilhado, mas deixou explicitamente a atualização de PRs já publicados fora do escopo.
- At stake: alteração remota e visível; uma proposta errada pode sobrescrever notas humanas do PR, alterar o PR errado ou criar um novo PR se o fluxo de criação for reutilizado sem isolamento.

## Problem

Quem mantém PRs do Azure DevOps não consegue usar o `pr-tools` para atualizar o PR depois que novos commits mudam o escopo real. Hoje a pessoa precisa abrir o Azure DevOps e editar título/descrição manualmente, aproximadamente 3 vezes por semana. Sem esta capacidade, o custo recorrente e o risco de deixar o registro remoto desatualizado permanecem; recriar o PR não é um substituto aceitável porque perde a identidade e o histórico do PR existente.

## Evidence

- Aproximadamente 3 edições manuais por semana — estimativa confirmada pelo requester.
- O tempo gasto por edição não é medido — não há telemetria; o proxy disponível será a quantidade de casos que ainda exigem abrir o editor do Azure.
- `PullRequest` já expõe `pullRequestId`, `title`, `description`, `sourceRefName` e `targetRefName`, e `get_pull_request` já faz a leitura por ID; não existe operação de atualização de Git PR — inspeção de `apps/rust/src/azure/pull_requests.rs`.
- `git::collect` escolhe automaticamente sprint/dev/main/master como base; isso não é seguro para esta jornada porque a base deve ser o `targetRefName` remoto — inspeção de `apps/rust/src/git/mod.rs`.
- O contrato documentado do Azure permite atualizar mais campos que os necessários e não documenta `ETag`/`If-Match` para esta operação — [Pull Requests - Update](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/update?view=azure-devops-rest-7.1).

## Journey

1. A pessoa informa explicitamente um único PR existente no clone/repositório Azure DevOps correspondente.
2. O fluxo lê o PR antes de chamar o provider. A leitura fixa o repositório, o source branch, o target branch, o título e a descrição atuais.
3. Se o PR não existir, não for `active`, não pertencer ao repositório/contexto local ou se source/target não puderem ser resolvidos localmente, a operação para com uma mensagem acionável. O fluxo nunca troca o target remoto por `dev`, `sprint/*` ou outra base inferida.
4. Com as refs exatas disponíveis, o fluxo coleta diff e log de `target..source` e gera uma proposta de título/descrição. O conteúdo remoto atual continua visível; a proposta não é autorização para descartar notas humanas.
5. A revisão mostra `Atual`, `Proposta` e o resultado final editável. A edição altera somente a proposta; cancelar descarta o rascunho. O título não pode ficar vazio e a descrição permanece abaixo de 4000 caracteres.
6. Ao confirmar, o fluxo relê o PR. Se status, repositório, refs, título ou descrição divergirem da leitura inicial, não escreve e exige nova revisão/reconciliação.
7. Se a proposta for byte a byte idêntica ao título/descrição remotos, o fluxo confirma o estado e não envia `PATCH`.
8. Caso haja mudança, envia somente a atualização aprovada ao mesmo PR. Depois da resposta, relê o PR e só declara sucesso quando o título e a descrição remotos coincidirem com o resultado pretendido.
9. Timeout, erro de transporte ou resposta 2xx sem payload válido entra em resultado incerto: uma nova leitura reconcilia o estado antes de oferecer qualquer retry; não há repetição cega.
10. Sair antes da confirmação não escreve nada. A execução nunca atualiza vários PRs, reviewers, labels, status, target branch ou opções de merge.

Estados confirmados:

- PR inexistente: falha acionável; nenhuma proposta é gerada.
- PR concluído ou abandonado: bloqueado; a primeira versão aceita apenas `active`.
- Repositório/contexto incompatível: bloqueado; a pessoa deve executar no clone do repositório do PR.
- Ref ausente: bloqueado com orientação para atualizar/fetch das refs; não há fallback silencioso.
- Descrição atual vazia: permitida; somente o limite de 4000 caracteres é aplicado à proposta final.
- Título final vazio: bloqueado na revisão e antes da escrita.
- Cancelamento/abandono: nenhuma alteração remota.
- Mudança concorrente: bloqueado e devolvido à revisão; não sobrescreve alterações humanas ou de automação.
- Conteúdo idêntico: sucesso sem chamada de escrita.
- Resultado remoto incerto: reconciliado por GET antes de retry ou sucesso.

## Verdict

Already committed - see Situation. O custo confirmado é manter aproximadamente 3 edições manuais por semana e deixar o PR sujeito a conteúdo desatualizado; a capacidade comprometida reduz esse custo sem recriar o registro remoto. Confirmado pelo requester em 2026-09-12.

Cheaper paths considered:

- Continuar editando no Azure DevOps — não exige desenvolvimento, mas preserva as 3 intervenções semanais e tira a revisão do fluxo do `pr-tools`.
- Recriar o PR depois dos novos commits — evita uma API de update, mas quebra a identidade/histórico do PR e pode duplicar revisão e reviewers.
- Gerar novamente sem leitura nem revisão do conteúdo atual — reduz código, mas aumenta o risco de perder notas humanas e usar uma base Git incorreta.

## Success

- Worked if: pelo menos 3 de 3 casos observados na primeira rodada de uso conseguem atualizar o mesmo PR pelo `pr-tools`, sem abrir o editor manual do Azure e sem alterar propriedades fora de título/descrição — na primeira versão liberada.
- Early signal: em dias, um smoke test cobre PR ativo, PR fechado, ref ausente, conflito concorrente, conteúdo idêntico e timeout simulado; o bet está errado se houver `POST`/criação, payload com campos extras ou sobrescrita após uma releitura divergente.
- Review: após a primeira versão e os três primeiros casos reais, ou em duas semanas, o maintainer e o requester conferem os casos e os testes automatizados.
- Não há métrica de tempo por edição. Durante a validação, registrar quantos dos três casos ainda exigiram edição manual no Azure; instrumentação permanente não faz parte desta issue.

## Boundary

In: um PR ativo por execução, informado por ID; leitura inicial e releitura concorrente; validação de repositório/source/target; coleta Git com refs exatas; geração de proposta com contexto de commits; visualização de conteúdo atual e proposto; edição local antes da confirmação; atualização mínima de título e descrição; no-op sem escrita; reconciliação pós-escrita; mensagens para falhas de rede, PR inexistente, estado incompatível e refs ausentes; `--dry-run` sem chamada ao provider nem escrita remota.

Out: merge/completion, auto-complete, reviewers, labels/checks/policies, alteração de target branch, criação de PR, atualização em lote, resolução automática de conflitos de conteúdo, sincronização em background, histórico persistente de versões, drafts entre execuções e edição de Test Cases. Esses comportamentos pertencem a outras decisões ou aumentariam o blast radius desta primeira versão.

## Prior art

- A issue 9 já resolveu uma versão adjacente do problema com `ContentEditState` draft-only, conteúdo aprovado canônico e snapshot congelado antes da primeira chamada remota; reutilizamos o editor e a disciplina de confirmar, sem reutilizar o publisher de criação — [design da issue 9](../.design/issue-9-edit-content-before-publish.md).
- A documentação oficial do Azure DevOps converge para `PATCH` do mesmo PR com `title` e `description`; a API lista outros campos atualizáveis, portanto a allowlist local de dois campos é necessária — [Pull Requests - Update](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/update?view=azure-devops-rest-7.1).
- A documentação oficial não declara ETag, `If-Match` ou precondição de revisão para esse PATCH; o controle de concorrência será a comparação explícita feita pelo `pr-tools`, não uma garantia atômica do Azure — [Get Pull Request](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/get-pull-request?view=azure-devops-rest-7.1).

## Shape

O shape escolhido adiciona `--pr <id>` a `prt desc`, mas encaminha essa variante para um fluxo de atualização isolado. O fluxo compartilha `ContentEditState`, renderização Markdown, autenticação e coleta básica, porém tem estado próprio para `current`, `proposal`, `conflict` e `outcome`; assim, a confirmação nunca cai no publisher que cria PRs. O pagamento imediato é uma nova máquina de revisão pequena; ele compra uma fronteira clara entre criar e atualizar, que é a proteção mais valiosa para uma escrita externa.

### Adds

- `apps/rust/src/features/update_pull_request.rs` com `UpdatePrep`, geração da proposta, elegibilidade, comparação de snapshots e reconciliação do resultado.
- `apps/rust/src/tui/update_flow.rs` com `UpdateApp`, fases de leitura/geração/revisão/atualização/conflito e revisão de `Atual` versus `Proposta`.
- `git::collect_for_refs(source_ref, target_ref)` para resolver somente as refs do PR e produzir diff/log de `target...source`; ausência de qualquer ref retorna erro com orientação de fetch.
- `azure::pull_requests::UpdatePullRequestInput` e `update_pull_request`, com corpo serializado exatamente como `{ "title": ..., "description": ... }`.
- Um método JSON `PATCH` separado no `AzureClient`, com `application/json`; o helper `patch` de Work Items continua reservado a `application/json-patch+json`.
- Modelagem dos campos remotos necessários para elegibilidade e comparação: status, repositório, source ref, target ref, título e descrição.
- Casos de teste puros para payload/allowlist, elegibilidade, refs, no-op, conflito, resultado incerto e snapshots da revisão de update.

### Changes

- `cli::DescOpts` e `CliOptions` → `--pr <id>` passa a selecionar o fluxo de atualização quando usado com `prt desc`; `prt update` mantém o significado de auto-update do binário.
- `main::run_desc` → detecta `options.pr` antes de preparar a criação normal e encaminha para a preparação/execução de update; o caminho sem `--pr` permanece criação.
- `features::describe`/`ai` → a geração comum é reutilizada ou extraída para aceitar o prompt de update, que inclui source/target remotos, diff/log e conteúdo atual; o texto aprovado pelo usuário não passa novamente por normalização.
- `azure::pull_requests::PullRequest` → passa a carregar status e a referência do repositório necessários para bloquear PR incompatível/fechado e comparar a releitura.
- `azure::pull_requests` → mantém `get_pull_request` para GET por ID e adiciona o PATCH mínimo; `create_pull_request` e `publish_pull_requests` não mudam.
- TUI compartilhada → o editor da issue 9 é reutilizado para editar a proposta, enquanto o review de update mostra o snapshot remoto atual e o resultado final antes da confirmação.

### Leaves

- `prt update` como comando de atualização do próprio binário.
- A criação multi-target, `publish_pull_requests`, reviewers, Work Items vinculados, merge settings e qualquer outra propriedade do PR.
- A semântica body-only do clipboard e o fluxo de `prt test`.
- Persistência de drafts, histórico/versionamento, auto-sync e resolução automática de conflitos.

A alternativa mais pesada é criar um comando top-level separado, como `prt pr update`, com uma nova entrada de CLI além do fluxo isolado. Ela só vence se o produto passar a ter várias operações do ciclo de vida de PR ou se `prt desc` deixar de ser o ponto natural para gerar/atualizar conteúdo; com um único PR, apenas título/descrição e o `prt update` já ocupado, a extensão `prt desc --pr` tem menor custo de adoção.

Como perspectiva, colocar um `Update` mode dentro de `DescribeApp` teria menos arquivos novos, mas mistura estados de criação, reviewers, targets e recovery com uma operação que nunca pode criar; essa opção é descartada pelo risco de condicionais e regressão no publisher existente.

## Roadmap

| Block | Delivers | Clarity |
|---|---|---|
| CLI e contexto remoto | `prt desc --pr <id>`, leitura do PR ativo no repositório local, validação de status/repositório e coleta exata de source/target | clear |
| Geração de proposta | prompt com diff/log do target remoto e conteúdo atual, geração validada e preparação de current/proposal | clear |
| Revisão e confirmação | `UpdateApp` com atual/proposta/final, editor compartilhado, cancelamento, validação, no-op e confirmação explícita | clear |
| Escrita e reconciliação | PATCH JSON mínimo, releitura antes/depois, conflito concorrente, resultado incerto e mensagens acionáveis | clear |
| Provas de fronteira | testes de payload HTTP/mock, estados de erro/retry, refs, no-op e snapshots TUI; suíte completa sem atualizar snapshots automaticamente | clear |

## Decisions

| Decision | Choice | Why this | Alternative, and what would make it win | Reversibility |
|---|---|---|---|---|
| Entry point | `prt desc --pr <id>` seleciona update; `prt update` permanece auto-update do binário | adiciona a capacidade no comando que já gera conteúdo de PR sem criar uma colisão semântica | `prt pr update --id <id>`; vence quando houver várias operações de ciclo de vida de PR | reversible |
| Update isolation | `UpdateApp`/`update_flow.rs` separado de `DescribeApp`/`live.rs`, com primitives compartilhadas | impede que confirmação de update alcance `publish_pull_requests` e reduz condicionais de criação | um único `DescribeApp` com `Mode::Create/Update`; vence somente se os fluxos passarem a compartilhar também targets/reviewers/recovery | costly |
| Remote authority | GET inicial por ID no remote Azure atual; status, repository, `sourceRefName` e `targetRefName` remotos são a autoridade | evita inferir `dev`, `sprint/*` ou refs do checkout para um PR que aponta para outra base | aceitar flags locais de source/target; vence apenas se a feature deixar de ser orientada ao PR existente | one-way |
| Git context | `collect_for_refs` resolve source e target localmente, usando local ou `origin/<branch>` sem fallback de base | uma descrição gerada sobre outro target é semanticamente errada, mesmo que o Git consiga produzir um diff | fetch automático ou base inferida; vence apenas com uma decisão explícita de permitir efeitos de rede e aproximações | costly |
| Review content | current remoto permanece visível; `ContentEditState` edita somente proposal; confirmação envia o resultado final | protege notas humanas e mantém uma única etapa explícita de aprovação | substituir silenciosamente o conteúdo remoto pela saída da IA; vence somente se houver uma política de descarte deliberada | costly |
| Remote payload | `PATCH` com `Content-Type: application/json` e somente `title`/`description` | o contrato do Git PR é distinto do JSON Patch de Work Items e a allowlist evita alterar reviewers/status/refs | enviar o objeto completo ou reutilizar `AzureClient::patch`; vence apenas com contrato oficial e testes que provem preservação | one-way |
| Concurrency | reler antes do PATCH e comparar status, repository, refs, title e description; divergência bloqueia nova revisão | não há ETag/If-Match documentado; comparação local é o controle verificável disponível | ignorar a releitura ou inventar `If-Match`/`revision`; vence apenas se o Azure documentar precondição suportada | costly |
| No-op and uncertainty | conteúdo idêntico não escreve; falha após envio faz GET e só oferece retry após reconciliação | evita escrita desnecessária e duplicação cega quando o resultado do transporte é incerto | repetir imediatamente; vence somente quando houver idempotência externa comprovada | reversible |
| Eligibility | somente PR `active`; concluído, abandonado ou incompatível é bloqueado | a issue pede atualização de um PR em revisão e não define mutação de estado fechado | permitir edição de PR fechado; vence como requisito separado com política de produto explícita | reversible |

## Open

1. O tempo médio da edição manual — default: manter como não instrumentado e revisar apenas a contagem de 3 casos reais, pois a frequência já foi confirmada e não bloqueia o shape.

## Sources

- [GitHub issue #10](https://github.com/nitoba/pr-tools/issues/10) — escopo, critérios de aceitação, riscos e exclusões.
- [`apps/rust/src/cli.rs`](../apps/rust/src/cli.rs) — comandos e opções atuais; `prt update` já é o auto-update e `prt desc` ainda não aceita PR ID.
- [`apps/rust/src/main.rs`](../apps/rust/src/main.rs) — dispatch de `run_desc` e separação entre TUI interativa e caminho sem TTY.
- [`apps/rust/src/azure/pull_requests.rs`](../apps/rust/src/azure/pull_requests.rs) — modelo/GET existentes e publisher de criação que não deve ser reutilizado para update.
- [`apps/rust/src/azure/mod.rs`](../apps/rust/src/azure/mod.rs) — cliente JSON existente e helper `application/json-patch+json` de Work Items.
- [`apps/rust/src/git/mod.rs`](../apps/rust/src/git/mod.rs) — coleta atual com base inferida e resolução local/`origin` que precisa de variante orientada a refs.
- [`apps/rust/src/tui/content_editor.rs`](../apps/rust/src/tui/content_editor.rs) — editor compartilhado entregue pela issue 9.
- [`apps/rust/src/tui/live.rs`](../apps/rust/src/tui/live.rs) — fluxo de criação/review/publicação que permanece separado.
- [Pull Requests - Update](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/update?view=azure-devops-rest-7.1) — endpoint `PATCH`, `application/json`, campos atualizáveis e limite de 4000 caracteres.
- [Get Pull Request](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/get-pull-request?view=azure-devops-rest-7.1) — leitura por ID e campos de status/repositório/refs/conteúdo.
- [Get Pull Requests](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/get-pull-requests?view=azure-devops-rest-7.1) — alerta de que listagens podem truncar description; update usa GET por ID.
- [Azure DevOps REST API guide](https://learn.microsoft.com/en-us/azure/devops/integrate/how-to/call-rest-api?view=azure-devops) — uso de `application/json` em PATCH e distinção de contratos REST.
