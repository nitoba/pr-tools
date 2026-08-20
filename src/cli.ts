import { confirm, intro, log, outro, spinner, text } from '@clack/prompts'
import { parseArgs as parseNodeArgs } from 'node:util'
import { AzureDevOpsClient, publishPullRequests } from './azure'
import { initConfig, loadConfig, parseProvider } from './config'
import { collectGitContext, resolveTargets } from './git'
import { generateDescription } from './llm'
import { buildPrompt, VERSION } from './prompt'
import { azurePrUrl, azurePullRequestUrl, azureWorkItemUrl, copyToClipboard } from './output'
import { runTestCard } from './test-card'
import { parseWorkItemId } from './validation'
import type { CliOptions } from './types'

type ParsedValues = {
  source?: string
  target?: string[]
  'work-item'?: string
  provider?: string
  model?: string
  'base-url'?: string
  'api-key'?: string
  create?: boolean
  'no-create'?: boolean
  pr?: string
  'area-path'?: string
  'assigned-to'?: string
  'iteration-path'?: string
  priority?: string
  team?: string
  program?: string
  examples?: string
  'dry-run'?: boolean
  raw?: boolean
  'no-copy'?: boolean
  help?: boolean
  version?: boolean
}

export function parseArgs(argv: string[]): CliOptions {
  const parsed = parseNodeArgs({
    args: argv,
    allowPositionals: true,
    strict: true,
    options: {
      source: { type: 'string' },
      target: { type: 'string', multiple: true },
      'work-item': { type: 'string' },
      provider: { type: 'string' },
      model: { type: 'string' },
      'base-url': { type: 'string' },
      'api-key': { type: 'string' },
      create: { type: 'boolean' },
      'no-create': { type: 'boolean' },
      pr: { type: 'string' },
      'area-path': { type: 'string' },
      'assigned-to': { type: 'string' },
      'iteration-path': { type: 'string' },
      priority: { type: 'string' },
      team: { type: 'string' },
      program: { type: 'string' },
      examples: { type: 'string' },
      'dry-run': { type: 'boolean' },
      raw: { type: 'boolean' },
      'no-copy': { type: 'boolean' },
      help: { type: 'boolean', short: 'h' },
      version: { type: 'boolean', short: 'v' }
    }
  })
  const values = parsed.values as ParsedValues
  if (values.help) {
    printHelp()
    process.exit(0)
  }
  if (values.version) {
    console.log(`pr-tools v${VERSION}`)
    process.exit(0)
  }

  const command = parsed.positionals[0] ?? 'desc'
  if (parsed.positionals.length > 1)
    throw new Error(`Argumentos posicionais inesperados: ${parsed.positionals.slice(1).join(' ')}`)
  if (command !== 'desc' && command !== 'test' && command !== 'init')
    throw new Error(`Comando desconhecido: ${command}`)
  const provider = values.provider ? parseProvider(values.provider) : undefined
  if (values.create && values['no-create'])
    throw new Error('--create e --no-create não podem ser usados juntos.')
  const targets = values.target ?? []
  for (const target of targets) {
    if (target !== 'dev' && target !== 'sprint' && !target.startsWith('sprint/')) {
      throw new Error(`Target inválido: ${target}. Use dev, sprint ou sprint/<número>.`)
    }
  }
  return {
    command,
    source: values.source,
    targets,
    workItem: values['work-item'],
    provider,
    model: values.model,
    baseUrl: values['base-url'],
    apiKey: values['api-key'],
    create: values.create ?? false,
    noCreate: values['no-create'] ?? false,
    pr: values.pr,
    areaPath: values['area-path'],
    assignedTo: values['assigned-to'],
    iterationPath: values['iteration-path'],
    priority: values.priority,
    team: values.team,
    program: values.program,
    examples: values.examples,
    dryRun: values['dry-run'] ?? false,
    raw: values.raw ?? false,
    copy: !(values['no-copy'] ?? false)
  }
}

function printHelp(): void {
  console.log(`pr-tools v${VERSION}

Gera uma descrição de PR a partir do contexto Git.

Uso:
  pr-tools desc [opções]
  pr-tools test [opções]
  pr-tools init

Opções:
  --source <branch>       Branch de origem (padrão: branch atual)
  --target <branch>       Target (dev, sprint ou sprint/<número>); pode repetir
  --work-item <id>        Vincula um work item
  --provider <nome>       codex, opencode ou openai-compatible
  --model <nome>          Sobrescreve o modelo do provider escolhido
  --base-url <url>        Endpoint OpenAI-compatible
  --api-key <key>         API key do endpoint compatível
  --create                Abre o fluxo de criação com confirmação após gerar a descrição
  --dry-run               Mostra o prompt sem chamar o modelo
  --raw                   Imprime somente o Markdown do body
  --no-copy               Não tenta copiar a descrição para o clipboard

Test Cases:
  --work-item <id>        Work Item pai (usa o ID da branch quando omitido)
  --pr <id>               PR Azure DevOps para complementar o contexto
  --area-path <path>      AreaPath do Test Case
  --assigned-to <valor>   Responsável do Test Case
  --iteration-path <path> IterationPath do Test Case
  --priority <n>          Prioridade do Test Case (padrão: 2)
  --team <nome>           Campo Custom.Team (padrão: DevOps)
  --program <nome>        Campo Custom.ProgramasAgrotrace (padrão: Agrotrace)
  --examples <n>          Quantidade de exemplos (0-5; padrão: 2)
  --create                Abre o fluxo de criação com confirmação após gerar o card
  --no-create             Apenas gera o Markdown
  --version, -v           Mostra a versão
  --help, -h              Mostra esta ajuda`)
}

export async function runDesc(options: CliOptions): Promise<void> {
  const config = loadConfig(options)
  const context = collectGitContext(options.source)
  if (options.targets.includes('sprint') && !context.sprintBranch) {
    throw new Error('Target sprint solicitado, mas nenhuma branch sprint/<número> foi encontrada.')
  }
  const workItemId = options.workItem ?? context.workItemId
  if (workItemId) parseWorkItemId(workItemId, 'Work Item')
  const targets = resolveTargets(context, options.targets)
  if (targets.length === 0) throw new Error('Nenhum target disponível.')
  const system = config.template
  const prompt = buildPrompt(context, targets, workItemId)

  if (options.dryRun) {
    const models = config.providers
      .map(
        (provider) =>
          `${provider}/${provider === 'codex' ? config.codexModel : provider === 'opencode' ? config.opencodeModel : config.compatibleModel}`
      )
      .join(', ')
    console.log(`Provider/model: ${models}`)
    console.log('\n[SYSTEM]\n')
    console.log(system)
    console.log('\n[USER]\n')
    console.log(prompt)
    return
  }

  const interactive = Boolean(process.stdin.isTTY && process.stdout.isTTY)
  if (options.create && !interactive)
    throw new Error(
      '--create requer terminal interativo para confirmar a descrição e os revisores.'
    )
  if (options.create && !context.isAzureDevOps)
    throw new Error('--create requer um remote Git do Azure DevOps.')
  if (options.create && !config.azurePat.trim())
    throw new Error('--create requer AZURE_PAT ou AZURE_DEVOPS_PAT configurado.')

  intro(`pr-tools · PR ${context.branch}`)
  const progress = spinner()
  progress.start('Gerando descrição via IA')
  let descriptionGenerated = false
  try {
    const generated = await generateDescription(
      config,
      system,
      prompt,
      context.branch,
      (provider, model) => {
        progress.message(`Tentando ${provider} (${model})`)
      }
    )
    progress.stop(`Descrição gerada (${generated.provider}/${generated.model})`)
    descriptionGenerated = true

    const { title, body } = generated.description
    if (options.raw) console.log(body)
    else {
      console.log(`\nTítulo: ${title}\n\nDescrição:\n${body}`)
      console.log(`\nBranch: ${context.branch}`)
      console.log(`Targets: ${targets.join(', ')}`)
      if (workItemId) console.log(`Work Item: #${workItemId}`)
      if (context.isAzureDevOps) {
        if (workItemId) console.log(`Work Item URL: ${azureWorkItemUrl(context, workItemId)}`)
        for (const target of targets) console.log(`PR ${target}: ${azurePrUrl(context, target)}`)
      }
    }
    if (options.copy && copyToClipboard(body)) log.success('Descrição copiada para o clipboard.')

    if (context.isAzureDevOps && config.azurePat.trim() && interactive) {
      const shouldCreate = await confirm({
        message: 'Criar PR(s) no Azure DevOps?',
        initialValue: options.create
      })
      if (typeof shouldCreate !== 'boolean') return
      if (shouldCreate) {
        const reviewerByTarget = new Map<string, string>()
        for (const target of targets) {
          const defaultReviewer = target.includes('sprint')
            ? config.reviewerSprint || config.reviewerDev
            : config.reviewerDev
          const reviewerValue = await text({
            message: `Reviewer para ${target} (opcional; Enter mantém o padrão)`,
            initialValue: defaultReviewer
          })
          if (typeof reviewerValue !== 'string') return
          reviewerByTarget.set(target, reviewerValue.trim())
        }

        const client = new AzureDevOpsClient({
          pat: config.azurePat,
          organization: context.azureOrg
        })
        const workItemRefs = workItemId ? [{ id: workItemId }] : undefined
        const published = await publishPullRequests(
          client,
          context,
          targets,
          { title, description: body, workItemRefs },
          (target) => reviewerByTarget.get(target) ?? ''
        )
        for (const { target, pullRequest } of published) {
          log.success(
            `PR ${target} criado: ${pullRequest.webUrl ?? (pullRequest.pullRequestId ? azurePullRequestUrl(context, pullRequest.pullRequestId) : (pullRequest.url ?? azurePrUrl(context, target)))}`
          )
        }
      }
    }
    outro('Concluído.')
  } catch (error) {
    progress.error(
      descriptionGenerated ? 'Falha ao publicar no Azure DevOps' : 'Falha ao gerar descrição'
    )
    throw error
  }
}

export async function main(argv = process.argv.slice(2)): Promise<void> {
  try {
    const options = parseArgs(argv)
    if (options.command === 'init') await initConfig()
    else if (options.command === 'test') await runTestCard(options)
    else await runDesc(options)
  } catch (error) {
    log.error(error instanceof Error ? error.message : String(error))
    process.exit(1)
  }
}
