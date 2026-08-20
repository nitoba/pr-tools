import { confirm, intro, log, outro, spinner, text } from '@clack/prompts'
import {
  AzureDevOpsClient,
  createTestCase,
  getPullRequest,
  getPullRequestChanges,
  getPullRequestIterations,
  getPullRequestWorkItemIds,
  getWorkItem,
  queryWorkItems,
  updateWorkItemToTestQA
} from './azure'
import { workItemField, workItemType } from './azure/work-items'
import { loadConfig } from './config'
import { collectGitContext } from './git'
import { generateDescription } from './llm'
import { azureWorkItemUrl } from './output'
import {
  nonNegativeDecimalPromptSchema,
  parseExamplesCount,
  parsePositiveDecimal,
  parseWorkItemId,
  positiveDecimalPromptSchema,
  workItemIdPromptSchema
} from './validation'
import type {
  AzurePullRequest,
  AzurePullRequestChange,
  AzureWorkItem,
  CreateTestCaseInput
} from './azure'
import type { CliOptions, Config, GitContext } from './types'

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

export function selectParentWorkItem(workItems: AzureWorkItem[]): number | undefined {
  const ordered = [...workItems].sort((left, right) => left.id - right.id)
  return ordered.find((item) => testWorkItemType(item) !== 'Test Case')?.id ?? ordered[0]?.id
}

function testWorkItemType(item: AzureWorkItem): string {
  return workItemType(item)
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
    `Tipo: ${testWorkItemType(workItem)}`
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
  lines.push(
    '',
    '## Instruções finais',
    '',
    'Gere o card conforme o formato definido no system prompt.'
  )
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

export async function runTestCard(options: CliOptions): Promise<void> {
  const config = loadConfig(options)
  if (!config.azurePat.trim())
    throw new Error('O comando test requer AZURE_PAT ou AZURE_DEVOPS_PAT configurado.')

  const interactive = Boolean(process.stdin.isTTY && process.stdout.isTTY)
  if (options.create && !interactive)
    throw new Error('--create requer terminal interativo para confirmar o card.')

  const git = collectGitContext(options.source)
  if (!git.isAzureDevOps) throw new Error('O comando test requer um remote Git do Azure DevOps.')

  const client = new AzureDevOpsClient({ pat: config.azurePat, organization: git.azureOrg })
  const progress = spinner()
  intro(`pr-tools · Test Case ${git.branch}`)
  progress.start('Coletando contexto Azure DevOps')

  let resolved: TestCardContext
  try {
    resolved = await resolveTestCardContext(client, git, options, interactive, progress)
  } catch (error) {
    progress.error('Falha ao coletar contexto Azure DevOps')
    throw error
  }
  progress.stop(`Contexto resolvido (Work Item #${resolved.workItem.id})`)
  const prompt = buildTestCardPrompt(resolved)

  if (options.dryRun) {
    printDryRun(config, prompt)
    outro('Concluído.')
    return
  }

  progress.start('Gerando card via IA')
  let generated: Awaited<ReturnType<typeof generateDescription>>
  try {
    generated = await generateDescription(
      config,
      TEST_CARD_SYSTEM_PROMPT,
      prompt,
      `test-card/${resolved.workItem.id}`,
      (provider, model) => progress.message(`Tentando ${provider} (${model})`)
    )
  } catch (error) {
    progress.error('Falha ao gerar card')
    throw error
  }
  progress.stop(`Card gerado (${generated.provider}/${generated.model})`)

  const { title, body } = generated.description
  if (options.raw) {
    console.log(body)
    outro('Concluído.')
    return
  }
  printTestCardSummary(resolved, generated.provider, generated.model, title, body, config, options)

  if (options.noCreate) {
    outro('Concluído.')
    return
  }

  const shouldCreate = interactive
    ? (await confirm({
        message: 'Criar este Test Case no Azure DevOps?',
        initialValue: options.create
      })) === true
    : false
  if (typeof shouldCreate !== 'boolean') return
  if (!shouldCreate) {
    if (!interactive) log.info('Ambiente não interativo; criação requer confirmação no terminal.')
    outro('Concluído.')
    return
  }

  const settings = await promptTestCardSettings(config, options, resolved.workItem, interactive)
  const input = buildCreateTestCaseInput(settings, resolved.workItem.id, title, body)
  const created = await createTestCase(client, git.azureProject, input)
  log.success(`Test Case #${created.id} criado: ${azureWorkItemUrl(git, String(created.id))}`)

  if (interactive) await maybeUpdateParent(client, git, resolved.workItem)
  outro('Concluído.')
}

async function resolveTestCardContext(
  client: AzureDevOpsClient,
  git: GitContext,
  options: CliOptions,
  interactive: boolean,
  progress: ReturnType<typeof spinner>
): Promise<TestCardContext> {
  const pullRequestId = parseWorkItemId(options.pr, '--pr')
  const pullRequest = pullRequestId
    ? await getPullRequest(client, git.azureProject, git.azureRepo, pullRequestId)
    : undefined

  let workItemId = parseWorkItemId(options.workItem, '--work-item')
  if (!workItemId && git.workItemId) workItemId = parseWorkItemId(git.workItemId, 'branch')
  if (!workItemId && pullRequest) {
    progress.message('Resolvendo Work Item vinculado ao PR')
    const linkedIds = await getPullRequestWorkItemIds(
      client,
      git.azureProject,
      git.azureRepo,
      pullRequest.pullRequestId
    )
    const linkedItems: AzureWorkItem[] = []
    for (const id of linkedIds) {
      try {
        linkedItems.push(await getWorkItem(client, git.azureProject, id))
      } catch {
        // Ignore stale or inaccessible links and keep resolving the remaining items.
      }
    }
    workItemId = selectParentWorkItem(linkedItems)
  }
  if (!workItemId && interactive) {
    const value = await text({
      message: 'ID do Work Item pai',
      validate: workItemIdPromptSchema
    })
    if (typeof value !== 'string') throw new Error('Operação cancelada.')
    workItemId = parseWorkItemId(value, 'Work Item')
  }
  if (!workItemId)
    throw new Error('Não foi possível resolver o Work Item pai; use --work-item explicitamente.')

  progress.message(`Buscando Work Item #${workItemId}`)
  const workItem = await getWorkItem(client, git.azureProject, workItemId)
  const changes = await getPullRequestChangesSafe(client, git, pullRequest)
  const examples = await getExampleTestCases(client, git.azureProject, options.examples)
  return { git, workItem, pullRequest, changes, examples }
}

async function getPullRequestChangesSafe(
  client: AzureDevOpsClient,
  git: GitContext,
  pullRequest?: AzurePullRequest
): Promise<AzurePullRequestChange[]> {
  if (!pullRequest) return new Array<AzurePullRequestChange>()
  try {
    const iterations = await getPullRequestIterations(
      client,
      git.azureProject,
      git.azureRepo,
      pullRequest.pullRequestId
    )
    const last = iterations.at(-1)
    return last
      ? await getPullRequestChanges(
          client,
          git.azureProject,
          git.azureRepo,
          pullRequest.pullRequestId,
          last.id
        )
      : new Array<AzurePullRequestChange>()
  } catch {
    return new Array<AzurePullRequestChange>()
  }
}

async function getExampleTestCases(
  client: AzureDevOpsClient,
  project: string,
  value: string | undefined
): Promise<string[]> {
  const count = parseExamplesCount(value)
  if (count === 0) return new Array<string>()
  const escapedProject = project.replaceAll("'", "''")
  const wiql = `SELECT [System.Id],[System.Title] FROM WorkItems WHERE [System.WorkItemType]='Test Case' AND [System.TeamProject]='${escapedProject}' ORDER BY [System.ChangedDate] DESC`
  try {
    const ids = await queryWorkItems(client, project, wiql)
    const examples: string[] = []
    for (const id of ids.slice(0, count)) {
      try {
        const item = await getWorkItem(client, project, id)
        const title = workItemText(item, 'System.Title')
        if (title) examples.push(`- #${id} ${title}`)
      } catch {
        // An inaccessible example should not prevent card generation.
      }
    }
    return examples
  } catch {
    return new Array<string>()
  }
}

async function promptTestCardSettings(
  config: Config,
  options: CliOptions,
  parent: AzureWorkItem,
  interactive: boolean
): Promise<TestCardSettings> {
  const settings: TestCardSettings = {
    areaPath: options.areaPath ?? config.testAreaPath,
    assignedTo: options.assignedTo ?? config.testAssignedTo,
    iterationPath: options.iterationPath ?? workItemText(parent, 'System.IterationPath') ?? '',
    priority: parsePositiveDecimal(options.priority, 2, '--priority'),
    team: options.team ?? config.testTeam,
    program: options.program ?? config.testProgram
  }
  if (!interactive) return settings

  settings.areaPath = await promptOptional('AreaPath do Test Case', settings.areaPath)
  settings.assignedTo = await promptOptional('Responsável do Test Case', settings.assignedTo)
  settings.iterationPath = await promptOptional(
    'IterationPath do Test Case',
    settings.iterationPath
  )
  settings.priority = await promptNumber(
    'Prioridade do Test Case',
    settings.priority,
    positiveDecimalPromptSchema
  )
  settings.team = await promptOptional('Custom.Team', settings.team)
  settings.program = await promptOptional('Custom.ProgramasAgrotrace', settings.program)
  return settings
}

async function maybeUpdateParent(
  client: AzureDevOpsClient,
  git: GitContext,
  parent: AzureWorkItem
): Promise<void> {
  const update = await confirm({
    message: `Atualizar o Work Item #${parent.id} para Test QA?`,
    initialValue: false
  })
  if (typeof update !== 'boolean' || !update) return

  const effort = workItemNumber(parent, 'Microsoft.VSTS.Scheduling.Effort')
  const realEffort = workItemNumber(parent, 'Custom.RealEffort')
  const nextEffort =
    effort === undefined
      ? await promptNumber('Effort (horas decimais)', 0.5, nonNegativeDecimalPromptSchema)
      : undefined
  const nextRealEffort =
    realEffort === undefined
      ? await promptNumber(
          'Real Effort (horas decimais)',
          nextEffort ?? effort ?? 0.5,
          nonNegativeDecimalPromptSchema
        )
      : undefined
  await updateWorkItemToTestQA(client, git.azureProject, parent.id, nextEffort, nextRealEffort)
  log.success(`Work Item #${parent.id} atualizado para Test QA.`)
}

async function promptOptional(message: string, initialValue: string): Promise<string> {
  const value = await text({ message: `${message} (opcional)`, initialValue })
  if (typeof value !== 'string') throw new Error('Operação cancelada.')
  return value.trim()
}

async function promptNumber(
  message: string,
  initialValue: number,
  validate: NonNullable<Parameters<typeof text>[0]['validate']>
): Promise<number> {
  const value = await text({
    message,
    initialValue: String(initialValue),
    validate
  })
  if (typeof value !== 'string') throw new Error('Operação cancelada.')
  return Number(value.replace(',', '.'))
}

function printDryRun(config: Config, prompt: string): void {
  const models = config.providers
    .map(
      (provider) =>
        `${provider}/${provider === 'codex' ? config.codexModel : provider === 'opencode' ? config.opencodeModel : config.compatibleModel}`
    )
    .join(', ')
  console.log(`Provider/model: ${models}`)
  console.log('\n[SYSTEM]\n')
  console.log(TEST_CARD_SYSTEM_PROMPT)
  console.log('\n[USER]\n')
  console.log(prompt)
}

function printTestCardSummary(
  context: TestCardContext,
  provider: string,
  model: string,
  title: string,
  body: string,
  config: Config,
  options: CliOptions
): void {
  console.log(
    `\nTest Card${context.pullRequest ? ` · PR #${context.pullRequest.pullRequestId}` : ''}`
  )
  console.log(`Provider: ${provider}/${model}`)
  console.log(
    `Work Item: #${context.workItem.id} — ${workItemText(context.workItem, 'System.Title')}`
  )
  console.log(`AreaPath: ${(options.areaPath ?? config.testAreaPath) || '(não configurado)'}`)
  const assignedTo = options.assignedTo ?? config.testAssignedTo
  if (assignedTo) console.log(`Responsável: ${assignedTo}`)
  console.log(`\nTítulo: ${title}\n\n${body}`)
}
