import { cancel, intro, isCancel, note, outro, password, select, text } from '@clack/prompts'
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import {
  COMPATIBLE_REASONING_EFFORT,
  CODEX_MODEL,
  CODEX_REASONING_EFFORT,
  DEFAULT_BASE_URL,
  DEFAULT_COMPATIBLE_MODEL,
  DEFAULT_TEMPLATE,
  OPENCODE_MODEL,
  OPENCODE_REASONING_EFFORT
} from './prompt'
import {
  apiKeyPromptSchema,
  optionalEmailPromptSchema,
  parseReasoningLevel,
  providerSchema
} from './validation'
import type { CliOptions, Config, ProviderName, ReasoningLevel } from './types'

export const CONFIG_DIR = join(
  process.env.XDG_CONFIG_HOME ?? join(homedir(), '.config'),
  'pr-tools'
)
export const CONFIG_FILE = join(CONFIG_DIR, 'config.json')
export const ENV_FILE = join(CONFIG_DIR, '.env')
export const TEMPLATE_FILE = join(CONFIG_DIR, 'pr-template.md')

export function parseProvider(value: string): ProviderName {
  const parsed = providerSchema.safeParse(value)
  if (parsed.success) return parsed.data
  throw new Error(`Provider inválido: ${value}. Use codex, opencode ou openai-compatible.`)
}

const reasoningOptions: Array<{ value: ReasoningLevel; label: string; hint: string }> = [
  { value: 'provider-default', label: 'Padrão do provider', hint: 'usa a configuração nativa' },
  { value: 'none', label: 'None', hint: 'sem reasoning adicional' },
  { value: 'minimal', label: 'Minimal', hint: 'resposta mais rápida' },
  { value: 'low', label: 'Low', hint: 'reasoning leve' },
  { value: 'medium', label: 'Medium', hint: 'equilíbrio entre custo e profundidade' },
  { value: 'high', label: 'High', hint: 'reasoning aprofundado' },
  { value: 'xhigh', label: 'XHigh', hint: 'máxima profundidade quando suportado' }
]

function readDotEnv(path: string): Record<string, string> {
  if (!existsSync(path)) return {}
  const values: Record<string, string> = {}
  for (const line of readFileSync(path, 'utf8').split('\n')) {
    const match = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$/.exec(line)
    if (!match) continue
    const key = match[1]
    const rawValue = match[2]
    if (!key || rawValue === undefined) continue
    values[key] = rawValue.replace(/^['"]|['"]$/g, '')
  }
  return values
}

function writeDotEnvValues(values: Record<string, string>): void {
  const lines = existsSync(ENV_FILE) ? readFileSync(ENV_FILE, 'utf8').split('\n') : []
  for (const [key, value] of Object.entries(values)) {
    const escaped = value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')
    const line = `${key}="${escaped}"`
    const index = lines.findIndex((item) => new RegExp(`^\\s*${key}\\s*=`).test(item))
    if (index >= 0) lines[index] = line
    else lines.push(line)
  }
  writeFileSync(ENV_FILE, `${lines.join('\n').replace(/\n+$/u, '')}\n`, { mode: 0o600 })
  chmodSync(ENV_FILE, 0o600)
}

function readJsonConfig(path: string): Partial<Config> {
  if (!existsSync(path)) return {}
  try {
    return JSON.parse(readFileSync(path, 'utf8')) as Partial<Config>
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error)
    throw new Error(`Configuração inválida em ${path}: ${reason}`)
  }
}

function readTemplate(): string {
  if (!existsSync(TEMPLATE_FILE)) return DEFAULT_TEMPLATE
  const template = readFileSync(TEMPLATE_FILE, 'utf8').trim()
  return template || DEFAULT_TEMPLATE
}

function parseProviderList(value: string): ProviderName[] {
  const providers = value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
    .map(parseProvider)
  if (providers.length === 0) throw new Error('Configure pelo menos um provider.')
  return providers
}

export function loadConfig(options: CliOptions): Config {
  const dotEnv = readDotEnv(ENV_FILE)
  const json = readJsonConfig(CONFIG_FILE)
  const env = process.env
  const defaultProviders: ProviderName[] = ['codex', 'opencode', 'openai-compatible']
  const providers: ProviderName[] = options.provider
    ? [options.provider]
    : env.PR_AI_PROVIDERS || env.PR_PROVIDERS || dotEnv.PR_AI_PROVIDERS || dotEnv.PR_PROVIDERS
      ? parseProviderList(
          env.PR_AI_PROVIDERS ??
            env.PR_PROVIDERS ??
            dotEnv.PR_AI_PROVIDERS ??
            dotEnv.PR_PROVIDERS ??
            ''
        )
      : json.providers?.length
        ? json.providers.map(parseProvider)
        : defaultProviders

  return {
    providers,
    baseUrl:
      options.baseUrl ??
      env.PR_AI_BASE_URL ??
      dotEnv.PR_AI_BASE_URL ??
      json.baseUrl ??
      DEFAULT_BASE_URL,
    compatibleModel:
      options.model ??
      env.PR_AI_MODEL ??
      dotEnv.PR_AI_MODEL ??
      json.compatibleModel ??
      DEFAULT_COMPATIBLE_MODEL,
    compatibleReasoning: parseReasoningLevel(
      env.PR_AI_REASONING ??
        env.PR_AI_THINKING_LEVEL ??
        dotEnv.PR_AI_REASONING ??
        dotEnv.PR_AI_THINKING_LEVEL ??
        json.compatibleReasoning,
      COMPATIBLE_REASONING_EFFORT
    ),
    codexModel:
      options.model ??
      env.PR_CODEX_MODEL ??
      dotEnv.PR_CODEX_MODEL ??
      json.codexModel ??
      CODEX_MODEL,
    codexReasoning: parseReasoningLevel(
      env.PR_CODEX_REASONING ??
        env.PR_CODEX_THINKING_LEVEL ??
        dotEnv.PR_CODEX_REASONING ??
        dotEnv.PR_CODEX_THINKING_LEVEL ??
        json.codexReasoning,
      CODEX_REASONING_EFFORT
    ),
    opencodeModel:
      options.model ??
      env.PR_OPENCODE_MODEL ??
      dotEnv.PR_OPENCODE_MODEL ??
      json.opencodeModel ??
      OPENCODE_MODEL,
    opencodeReasoning: parseReasoningLevel(
      env.PR_OPENCODE_REASONING ??
        env.PR_OPENCODE_THINKING_LEVEL ??
        dotEnv.PR_OPENCODE_REASONING ??
        dotEnv.PR_OPENCODE_THINKING_LEVEL ??
        json.opencodeReasoning,
      OPENCODE_REASONING_EFFORT
    ),
    azurePat:
      env.AZURE_DEVOPS_PAT ??
      env.AZURE_PAT ??
      dotEnv.AZURE_DEVOPS_PAT ??
      dotEnv.AZURE_PAT ??
      json.azurePat ??
      '',
    reviewerDev: env.PR_REVIEWER_DEV ?? dotEnv.PR_REVIEWER_DEV ?? json.reviewerDev ?? '',
    reviewerSprint:
      env.PR_REVIEWER_SPRINT ?? dotEnv.PR_REVIEWER_SPRINT ?? json.reviewerSprint ?? '',
    testAreaPath: env.TEST_CARD_AREA_PATH ?? dotEnv.TEST_CARD_AREA_PATH ?? json.testAreaPath ?? '',
    testAssignedTo:
      env.TEST_CARD_ASSIGNED_TO ?? dotEnv.TEST_CARD_ASSIGNED_TO ?? json.testAssignedTo ?? '',
    testTeam: env.TEST_CARD_TEAM ?? dotEnv.TEST_CARD_TEAM ?? json.testTeam ?? 'DevOps',
    testProgram:
      env.TEST_CARD_PROGRAM ?? dotEnv.TEST_CARD_PROGRAM ?? json.testProgram ?? 'Agrotrace',
    apiKey:
      options.apiKey ??
      env.PR_AI_API_KEY ??
      env.OPENAI_API_KEY ??
      dotEnv.PR_AI_API_KEY ??
      dotEnv.OPENAI_API_KEY ??
      json.apiKey ??
      '',
    template:
      typeof json.template === 'string' && json.template.trim() ? json.template : readTemplate()
  }
}

export async function initConfig(): Promise<void> {
  mkdirSync(CONFIG_DIR, { recursive: true })
  const existing = readJsonConfig(CONFIG_FILE)
  const interactive = Boolean(process.stdin.isTTY && process.stdout.isTTY)
  let provider: ProviderName = existing.providers?.[0] ?? 'codex'
  let baseUrl = existing.baseUrl ?? DEFAULT_BASE_URL
  let compatibleModel = existing.compatibleModel ?? DEFAULT_COMPATIBLE_MODEL
  let compatibleReasoning = parseReasoningLevel(
    existing.compatibleReasoning,
    COMPATIBLE_REASONING_EFFORT
  )
  let codexModel = existing.codexModel ?? CODEX_MODEL
  let codexReasoning = parseReasoningLevel(existing.codexReasoning, CODEX_REASONING_EFFORT)
  let opencodeModel = existing.opencodeModel ?? OPENCODE_MODEL
  let opencodeReasoning = parseReasoningLevel(existing.opencodeReasoning, OPENCODE_REASONING_EFFORT)
  let apiKey = existing.apiKey ?? ''
  const existingDotEnv = readDotEnv(ENV_FILE)
  let azurePat =
    process.env.AZURE_DEVOPS_PAT ??
    process.env.AZURE_PAT ??
    existingDotEnv.AZURE_DEVOPS_PAT ??
    existingDotEnv.AZURE_PAT ??
    ''
  let reviewerDev = existing.reviewerDev ?? existingDotEnv.PR_REVIEWER_DEV ?? ''
  let reviewerSprint = existing.reviewerSprint ?? existingDotEnv.PR_REVIEWER_SPRINT ?? ''
  let testAreaPath = existing.testAreaPath ?? existingDotEnv.TEST_CARD_AREA_PATH ?? ''
  let testAssignedTo = existing.testAssignedTo ?? existingDotEnv.TEST_CARD_ASSIGNED_TO ?? ''
  let testTeam = existing.testTeam ?? existingDotEnv.TEST_CARD_TEAM ?? 'DevOps'
  let testProgram = existing.testProgram ?? existingDotEnv.TEST_CARD_PROGRAM ?? 'Agrotrace'

  if (interactive) {
    intro('prt · configuração')

    const azurePatValue = await password({
      message: 'Azure DevOps PAT (Enter para manter o atual)'
    })
    if (isCancel(azurePatValue)) return cancelled()
    if (typeof azurePatValue === 'string' && azurePatValue.trim()) azurePat = azurePatValue.trim()

    reviewerSprint = await promptEmail(
      'Email de review da sprint',
      reviewerSprint,
      'opcional; Enter mantém o valor atual'
    )
    reviewerDev = await promptEmail(
      'Email de review de dev',
      reviewerDev,
      'opcional; Enter mantém o valor atual'
    )
    testAssignedTo = await promptEmail(
      'Email de review/responsável do card de teste',
      testAssignedTo,
      'opcional; usado em System.AssignedTo'
    )

    const providerValue = await select({
      message: 'Provider padrão',
      options: [
        {
          value: 'codex',
          label: 'Codex local',
          hint: `${codexModel} · thinking ${codexReasoning} · login local`
        },
        {
          value: 'opencode',
          label: 'OpenCode local',
          hint: `${opencodeModel} · thinking ${opencodeReasoning}`
        },
        {
          value: 'openai-compatible',
          label: 'OpenAI-compatible',
          hint: `${compatibleModel} · thinking ${compatibleReasoning}`
        }
      ],
      initialValue: provider
    })
    provider = parseProvider(promptString(providerValue))

    if (provider === 'codex') {
      codexModel = await promptModel('Modelo do Codex', codexModel, CODEX_MODEL)
      codexReasoning = await promptReasoning('Thinking level do Codex', codexReasoning)
    }

    if (provider === 'opencode') {
      opencodeModel = await promptModel('Modelo do OpenCode', opencodeModel, OPENCODE_MODEL)
      opencodeReasoning = await promptReasoning('Thinking level do OpenCode', opencodeReasoning)
    }

    if (provider === 'openai-compatible') {
      baseUrl = await promptModel('Base URL OpenAI-compatible', baseUrl, DEFAULT_BASE_URL)
      compatibleModel = await promptModel(
        'Modelo OpenAI-compatible',
        compatibleModel,
        DEFAULT_COMPATIBLE_MODEL
      )
      compatibleReasoning = await promptReasoning(
        'Thinking level OpenAI-compatible',
        compatibleReasoning
      )
      const apiKeyValue = await password({
        message: 'API key (Enter para manter a atual ou deixar vazia)',
        validate: apiKeyPromptSchema
      })
      const apiKeyInput = promptString(apiKeyValue)
      if (apiKeyInput.trim()) apiKey = apiKeyInput.trim()
    }

    const testAreaPathValue = await text({
      message: 'AreaPath padrão para Test Cases (opcional)',
      initialValue: testAreaPath
    })
    if (typeof testAreaPathValue !== 'string') return cancelled()
    testAreaPath = testAreaPathValue.trim()

    const testTeamValue = await text({
      message: 'Team padrão para Test Cases',
      initialValue: testTeam
    })
    if (typeof testTeamValue !== 'string') return cancelled()
    testTeam = testTeamValue.trim() || 'DevOps'

    const testProgramValue = await text({
      message: 'Program padrão para Test Cases',
      initialValue: testProgram
    })
    if (typeof testProgramValue !== 'string') return cancelled()
    testProgram = testProgramValue.trim() || 'Agrotrace'
  }

  writeFileSync(
    CONFIG_FILE,
    `${JSON.stringify(
      {
        providers: [provider],
        baseUrl,
        compatibleModel,
        compatibleReasoning,
        codexModel,
        codexReasoning,
        opencodeModel,
        opencodeReasoning,
        reviewerDev,
        reviewerSprint,
        testAreaPath,
        testAssignedTo,
        testTeam,
        testProgram,
        apiKey
      },
      null,
      2
    )}\n`,
    { mode: 0o600 }
  )
  const dotEnvValues: Record<string, string> = {}
  if (azurePat) dotEnvValues.AZURE_PAT = azurePat
  if (interactive || reviewerDev) dotEnvValues.PR_REVIEWER_DEV = reviewerDev
  if (interactive || reviewerSprint) dotEnvValues.PR_REVIEWER_SPRINT = reviewerSprint
  if (interactive || testAssignedTo) dotEnvValues.TEST_CARD_ASSIGNED_TO = testAssignedTo
  if (Object.keys(dotEnvValues).length > 0) writeDotEnvValues(dotEnvValues)
  if (!existsSync(TEMPLATE_FILE))
    writeFileSync(TEMPLATE_FILE, `${DEFAULT_TEMPLATE}\n`, { mode: 0o600 })
  if (interactive) {
    note(`Configuração salva em ${CONFIG_FILE}\nTemplate salvo em ${TEMPLATE_FILE}`, 'prt')
    outro('Pronto. Execute `prt desc --dry-run`.')
  } else {
    console.log(`Configuração salva em ${CONFIG_FILE}`)
  }
}

async function promptEmail(message: string, initialValue: string, hint: string): Promise<string> {
  const value = await text({
    message: `${message} (${hint})`,
    initialValue,
    validate: optionalEmailPromptSchema
  })
  return promptString(value).trim()
}

async function promptModel(
  message: string,
  initialValue: string,
  fallback: string
): Promise<string> {
  const value = await text({ message, initialValue, placeholder: fallback })
  return promptString(value).trim() || fallback
}

async function promptReasoning(
  message: string,
  initialValue: ReasoningLevel
): Promise<ReasoningLevel> {
  const value = await select({ message, options: reasoningOptions, initialValue })
  return parseReasoningLevel(promptString(value), initialValue)
}

function promptString(value: unknown): string {
  if (isCancel(value) || typeof value !== 'string') {
    cancelled()
    return ''
  }
  return value
}

function cancelled(): never {
  cancel('Operação cancelada.')
  process.exit(0)
}
