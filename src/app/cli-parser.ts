import { parseArgs as parseNodeArgs } from 'node:util'
import { parseProvider } from '../infrastructure/config/config-validation'
import { VERSION } from './version'
import { CliExit } from './exit'
import type { CliOptions } from './cli.models'

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
  if (values.help) throw new CliExit(0, helpText())
  if (values.version) throw new CliExit(0, `prt v${VERSION}`)

  const command = parsed.positionals.at(0) ?? 'desc'
  if (parsed.positionals.length > 1)
    throw new Error(`Argumentos posicionais inesperados: ${parsed.positionals.slice(1).join(' ')}`)
  if (command !== 'desc' && command !== 'test' && command !== 'init' && command !== 'doctor')
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

export function helpText(): string {
  return `prt v${VERSION}

Gera uma descrição de PR a partir do contexto Git.

Uso:
  prt desc [opções]
  prt test [opções]
  prt init
  prt doctor

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
  --help, -h              Mostra esta ajuda`
}
