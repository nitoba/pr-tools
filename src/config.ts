import { cancel, intro, note, outro, password, select, text } from '@clack/prompts'
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import {
  CODEX_MODEL,
  DEFAULT_BASE_URL,
  DEFAULT_COMPATIBLE_MODEL,
  DEFAULT_TEMPLATE,
  OPENCODE_MODEL
} from './prompt'
import { apiKeyPromptSchema, providerSchema } from './validation'
import type { CliOptions, Config, ProviderName } from './types'

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
    codexModel:
      options.model ??
      env.PR_CODEX_MODEL ??
      dotEnv.PR_CODEX_MODEL ??
      json.codexModel ??
      CODEX_MODEL,
    opencodeModel:
      options.model ??
      env.PR_OPENCODE_MODEL ??
      dotEnv.PR_OPENCODE_MODEL ??
      json.opencodeModel ??
      OPENCODE_MODEL,
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
    intro('pr-tools · configuração')
    const providerValue = await select({
      message: 'Provider padrão',
      options: [
        {
          value: 'codex',
          label: 'Codex local',
          hint: `${CODEX_MODEL} · reasoning high · login local`
        },
        {
          value: 'opencode',
          label: 'OpenCode local',
          hint: `${OPENCODE_MODEL} · usa a configuração do OpenCode`
        },
        {
          value: 'openai-compatible',
          label: 'OpenAI-compatible',
          hint: 'qualquer endpoint compatível'
        }
      ],
      initialValue: provider
    })
    if (typeof providerValue !== 'string') return cancelled()
    provider = parseProvider(String(providerValue))

    const baseUrlValue = await text({
      message: 'Endpoint OpenAI-compatible',
      initialValue: baseUrl,
      placeholder: DEFAULT_BASE_URL
    })
    if (typeof baseUrlValue !== 'string') return cancelled()
    baseUrl = baseUrlValue.trim() || DEFAULT_BASE_URL

    const modelValue = await text({
      message: 'Modelo OpenAI-compatible',
      initialValue: compatibleModel,
      placeholder: DEFAULT_COMPATIBLE_MODEL
    })
    if (typeof modelValue !== 'string') return cancelled()
    compatibleModel = modelValue.trim() || DEFAULT_COMPATIBLE_MODEL

    if (provider === 'openai-compatible') {
      const apiKeyValue = await password({
        message: 'API key (Enter para deixar vazia)',
        validate: apiKeyPromptSchema
      })
      if (typeof apiKeyValue !== 'string') return cancelled()
      apiKey = apiKeyValue
    }

    const azurePatValue = await password({
      message: 'Azure DevOps PAT (Enter para manter o atual)'
    })
    if (typeof azurePatValue !== 'string') return cancelled()
    if (azurePatValue.trim()) azurePat = azurePatValue.trim()

    const reviewerDevValue = await text({
      message: 'Reviewer padrão para PRs -> dev (opcional)',
      initialValue: reviewerDev
    })
    if (typeof reviewerDevValue !== 'string') return cancelled()
    reviewerDev = reviewerDevValue.trim()

    const reviewerSprintValue = await text({
      message: 'Reviewer padrão para PRs -> sprint (opcional)',
      initialValue: reviewerSprint
    })
    if (typeof reviewerSprintValue !== 'string') return cancelled()
    reviewerSprint = reviewerSprintValue.trim()

    const testAreaPathValue = await text({
      message: 'AreaPath padrão para Test Cases (opcional)',
      initialValue: testAreaPath
    })
    if (typeof testAreaPathValue !== 'string') return cancelled()
    testAreaPath = testAreaPathValue.trim()

    const testAssignedToValue = await text({
      message: 'Responsável padrão para Test Cases (opcional)',
      initialValue: testAssignedTo
    })
    if (typeof testAssignedToValue !== 'string') return cancelled()
    testAssignedTo = testAssignedToValue.trim()

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
    `${JSON.stringify({ providers: [provider], baseUrl, compatibleModel, codexModel: CODEX_MODEL, opencodeModel: OPENCODE_MODEL, reviewerDev, reviewerSprint, testAreaPath, testAssignedTo, testTeam, testProgram, apiKey }, null, 2)}\n`,
    { mode: 0o600 }
  )
  const dotEnvValues: Record<string, string> = {}
  if (azurePat) dotEnvValues.AZURE_PAT = azurePat
  if (interactive || reviewerDev) dotEnvValues.PR_REVIEWER_DEV = reviewerDev
  if (interactive || reviewerSprint) dotEnvValues.PR_REVIEWER_SPRINT = reviewerSprint
  if (Object.keys(dotEnvValues).length > 0) writeDotEnvValues(dotEnvValues)
  if (!existsSync(TEMPLATE_FILE))
    writeFileSync(TEMPLATE_FILE, `${DEFAULT_TEMPLATE}\n`, { mode: 0o600 })
  if (interactive) {
    note(`Configuração salva em ${CONFIG_FILE}\nTemplate salvo em ${TEMPLATE_FILE}`, 'pr-tools')
    outro('Pronto. Execute `pr-tools desc --dry-run`.')
  } else {
    console.log(`Configuração salva em ${CONFIG_FILE}`)
  }
}

function cancelled(): never {
  cancel('Operação cancelada.')
  process.exit(0)
}
