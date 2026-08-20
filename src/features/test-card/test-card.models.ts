import { workItemField, workItemType } from '../../azure/work-items'
import type {
  AzurePullRequest,
  AzurePullRequestChange,
  AzureWorkItem,
  CreateTestCaseInput
} from '../../azure'
import type { Config } from '../../infrastructure/config/config.models'
import type { GitContext } from '../../infrastructure/git/git-context.models'
import type { TestCardRepository } from '../../infrastructure/azure/test-card-repository'

export const TEST_CARD_SYSTEM_PROMPT = `Você é um analista de QA técnico.

Gere um card de teste em português brasileiro para Azure DevOps com base no Work Item pai, no PR, nas alterações e nos exemplos fornecidos.

Retorne um objeto JSON com exatamente estes campos:
- "title": título curto, objetivo e testável.
- "body": Markdown com estas seções, nesta ordem:
  - ## Objetivo
  - ## Cenário base
  - ## Checklist de testes
  - ## Resultado esperado

Regras:
- Não invente comportamento que não esteja sustentado pelo contexto.
- Foque em cobertura funcional, validações e regressão.
- O checklist deve ser acionável para QA.
- Não cite nomes de arquivos, classes, funções, APIs internas ou detalhes de implementação.
- Descreva apenas cenários observáveis e validáveis pelo usuário final ou pelo analista de QA.`

export type TestCardSettings = {
  areaPath: string
  assignedTo: string
  iterationPath: string
  priority: number
  team: string
  program: string
}

export type TestCardContext = {
  git: GitContext
  workItem: AzureWorkItem
  pullRequest?: AzurePullRequest
  changes: AzurePullRequestChange[]
  examples: string[]
}

export type TestCardPreparation = {
  config: Config
  context: TestCardContext
  prompt: string
  repository: TestCardRepository
}

export function selectParentWorkItem(workItems: AzureWorkItem[]): number | undefined {
  const ordered = [...workItems].sort((left, right) => left.id - right.id)
  const parent = ordered.find((item) => workItemType(item) !== 'Test Case')
  if (parent) return parent.id
  return ordered.at(0)?.id
}

export function workItemText(item: AzureWorkItem, field: string): string {
  return workItemField(item, field)
}

export function workItemNumber(item: AzureWorkItem, field: string): number | undefined {
  const value = item.fields[field]
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value)
    if (Number.isFinite(parsed)) return parsed
  }
  return undefined
}

export function buildTestCardPrompt(context: TestCardContext): string {
  const { git, workItem, pullRequest, changes, examples } = context
  const lines = [
    '## Contexto do Work Item',
    '',
    `ID: ${workItem.id}`,
    `Título: ${workItemText(workItem, 'System.Title')}`,
    `Tipo: ${workItemText(workItem, 'System.WorkItemType')}`
  ]
  const areaPath = workItemText(workItem, 'System.AreaPath')
  const description = workItemText(workItem, 'System.Description')
  if (areaPath) lines.push(`Área: ${areaPath}`)
  if (description) lines.push(`Descrição: ${description}`)

  if (pullRequest) {
    lines.push(
      '',
      '## Contexto do PR',
      '',
      `PR ID: ${pullRequest.pullRequestId}`,
      `Título: ${pullRequest.title}`,
      `Branch origem: ${pullRequest.sourceRefName}`,
      `Branch destino: ${pullRequest.targetRefName}`
    )
    if (pullRequest.description) lines.push(`Descrição: ${pullRequest.description}`)
  }

  lines.push('', '## Contexto Git', '', `Branch atual: ${git.branch}`, `Base: ${git.baseBranch}`)
  if (changes.length > 0) {
    lines.push('', '## Arquivos alterados')
    for (const change of changes) lines.push(`- [${change.changeType}] ${change.item.path}`)
  }
  if (git.diff) lines.push('', '## Diff resumido', '', '```diff', git.diff, '```')
  if (git.log) lines.push('', '## Commits', '', '```', git.log, '```')
  if (examples.length > 0) {
    lines.push('', '## Exemplos de Test Case', '')
    for (const example of examples) lines.push(example)
  }
  lines.push('', '## Instruções finais', '', 'Gere o card conforme o formato definido no system prompt.')
  return `${lines.join('\n')}\n`
}

export function buildCreateTestCaseInput(
  settings: TestCardSettings,
  parentId: number,
  title: string,
  body: string
): CreateTestCaseInput {
  return {
    title,
    descriptionHtml: body,
    areaPath: settings.areaPath || undefined,
    parentId,
    iterationPath: settings.iterationPath || undefined,
    priority: settings.priority,
    team: settings.team || undefined,
    program: settings.program || undefined,
    assignedTo: settings.assignedTo || undefined
  }
}
