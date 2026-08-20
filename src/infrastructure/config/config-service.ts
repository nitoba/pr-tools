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
} from './config-defaults'
import { parseReasoningLevel, validateOptionalEmail, parseProvider } from './config-validation'
import type { PromptPort } from '../terminal/terminal-ports'
import type { CliOptions } from '../../app/cli.models'
import type { Config } from './config.models'
import type { ProviderName, ReasoningLevel } from '../ai/ai.models'

export type ConfigPaths = {
  directory: string
  configFile: string
  envFile: string
  templateFile: string
}

export const DEFAULT_CONFIG_PATHS: ConfigPaths = (() => {
  const directory = join(process.env.XDG_CONFIG_HOME ?? join(homedir(), '.config'), 'pr-tools')
  return {
    directory,
    configFile: join(directory, 'config.json'),
    envFile: join(directory, '.env'),
    templateFile: join(directory, 'pr-template.md')
  }
})()

export type ConfigInitialization = {
  paths: ConfigPaths
  interactive: boolean
  azurePatConfigured: boolean
}

class ConfigurationCancelled extends Error {}

export class ConfigService {
  constructor(
    readonly paths: ConfigPaths = DEFAULT_CONFIG_PATHS,
    private readonly prompts?: PromptPort,
    private readonly environment: NodeJS.ProcessEnv = process.env,
    private readonly interactive: boolean = Boolean(process.stdin.isTTY && process.stdout.isTTY)
  ) {}

  load(options: CliOptions): Config {
    const dotEnv = this.readDotEnv(this.paths.envFile)
    const json = this.readJsonConfig(this.paths.configFile)
    const env = this.environment
    const defaultProviders: ProviderName[] = ['codex', 'opencode', 'openai-compatible']
    const providers: ProviderName[] = options.provider
      ? [options.provider]
      : env.PR_AI_PROVIDERS || env.PR_PROVIDERS || dotEnv.PR_AI_PROVIDERS || dotEnv.PR_PROVIDERS
        ? this.parseProviderList(
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
        options.model ?? env.PR_CODEX_MODEL ?? dotEnv.PR_CODEX_MODEL ?? json.codexModel ?? CODEX_MODEL,
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
        typeof json.template === 'string' && json.template.trim()
          ? json.template
          : this.readTemplate()
    }
  }

  async initialize(): Promise<ConfigInitialization | undefined> {
    mkdirSync(this.paths.directory, { recursive: true })
    const existing = this.readJsonConfig(this.paths.configFile)
    const isInteractive = this.interactive
    const configuredProvider = existing.providers?.length ? existing.providers[0] : undefined
    let provider: ProviderName = configuredProvider ?? 'codex'
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
    const existingDotEnv = this.readDotEnv(this.paths.envFile)
    let azurePat =
      this.environment.AZURE_DEVOPS_PAT ??
      this.environment.AZURE_PAT ??
      existingDotEnv.AZURE_DEVOPS_PAT ??
      existingDotEnv.AZURE_PAT ??
      ''
    let reviewerDev = existing.reviewerDev ?? existingDotEnv.PR_REVIEWER_DEV ?? ''
    let reviewerSprint = existing.reviewerSprint ?? existingDotEnv.PR_REVIEWER_SPRINT ?? ''
    let testAreaPath = existing.testAreaPath ?? existingDotEnv.TEST_CARD_AREA_PATH ?? ''
    let testAssignedTo = existing.testAssignedTo ?? existingDotEnv.TEST_CARD_ASSIGNED_TO ?? ''
    let testTeam = existing.testTeam ?? existingDotEnv.TEST_CARD_TEAM ?? 'DevOps'
    let testProgram = existing.testProgram ?? existingDotEnv.TEST_CARD_PROGRAM ?? 'Agrotrace'

    try {
      if (isInteractive) {
        const prompts = this.requirePrompts()
        const azurePatValue = await prompts.password({
          message: 'Azure DevOps PAT (Enter para manter o atual)'
        })
        if (typeof azurePatValue !== 'string') throw new ConfigurationCancelled()
        if (azurePatValue.trim()) azurePat = azurePatValue.trim()

        reviewerSprint = await this.promptEmail(
          'Email de review da sprint',
          reviewerSprint,
          'opcional; Enter mantém o valor atual'
        )
        reviewerDev = await this.promptEmail(
          'Email de review de dev',
          reviewerDev,
          'opcional; Enter mantém o valor atual'
        )
        testAssignedTo = await this.promptEmail(
          'Email de review/responsável do card de teste',
          testAssignedTo,
          'opcional; usado em System.AssignedTo'
        )

        const providerValue = await prompts.select({
          message: 'Provider padrão',
          options: [
            { value: 'codex', label: 'Codex local', hint: `${codexModel} · thinking ${codexReasoning}` },
            { value: 'opencode', label: 'OpenCode local', hint: `${opencodeModel} · thinking ${opencodeReasoning}` },
            { value: 'openai-compatible', label: 'OpenAI-compatible', hint: `${compatibleModel} · thinking ${compatibleReasoning}` }
          ],
          initialValue: provider
        })
        provider = parseProvider(this.promptString(providerValue))

        if (provider === 'codex') {
          codexModel = await this.promptModel('Modelo do Codex', codexModel, CODEX_MODEL)
          codexReasoning = await this.promptReasoning('Thinking level do Codex', codexReasoning)
        }
        if (provider === 'opencode') {
          opencodeModel = await this.promptModel('Modelo do OpenCode', opencodeModel, OPENCODE_MODEL)
          opencodeReasoning = await this.promptReasoning('Thinking level do OpenCode', opencodeReasoning)
        }
        if (provider === 'openai-compatible') {
          baseUrl = await this.promptModel('Base URL OpenAI-compatible', baseUrl, DEFAULT_BASE_URL)
          compatibleModel = await this.promptModel(
            'Modelo OpenAI-compatible',
            compatibleModel,
            DEFAULT_COMPATIBLE_MODEL
          )
          compatibleReasoning = await this.promptReasoning(
            'Thinking level OpenAI-compatible',
            compatibleReasoning
          )
          const apiKeyValue = await prompts.password({
            message: 'API key (Enter para manter a atual ou deixar vazia)'
          })
          const apiKeyInput = this.promptString(apiKeyValue)
          if (apiKeyInput.trim()) apiKey = apiKeyInput.trim()
        }

        testAreaPath = await this.promptValue('AreaPath padrão para Test Cases (opcional)', testAreaPath)
        testTeam = (await this.promptValue('Team padrão para Test Cases', testTeam)).trim() || 'DevOps'
        testProgram = (await this.promptValue('Program padrão para Test Cases', testProgram)).trim() || 'Agrotrace'
      }
    } catch (error) {
      if (error instanceof ConfigurationCancelled) return undefined
      throw error
    }

    writeFileSync(
      this.paths.configFile,
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
    if (isInteractive || reviewerDev) dotEnvValues.PR_REVIEWER_DEV = reviewerDev
    if (isInteractive || reviewerSprint) dotEnvValues.PR_REVIEWER_SPRINT = reviewerSprint
    if (isInteractive || testAssignedTo) dotEnvValues.TEST_CARD_ASSIGNED_TO = testAssignedTo
    if (Object.keys(dotEnvValues).length > 0) this.writeDotEnvValues(dotEnvValues)
    if (!existsSync(this.paths.templateFile))
      writeFileSync(this.paths.templateFile, `${DEFAULT_TEMPLATE}\n`, { mode: 0o600 })
    return { paths: this.paths, interactive: isInteractive, azurePatConfigured: Boolean(azurePat) }
  }

  private requirePrompts(): PromptPort {
    if (!this.prompts) throw new Error('Prompts de configuração não foram compostos.')
    return this.prompts
  }

  private readDotEnv(path: string): Record<string, string> {
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

  private writeDotEnvValues(values: Record<string, string>): void {
    const lines = existsSync(this.paths.envFile)
      ? readFileSync(this.paths.envFile, 'utf8').split('\n')
      : []
    for (const [key, value] of Object.entries(values)) {
      const escaped = value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')
      const line = `${key}="${escaped}"`
      const index = lines.findIndex((item) => new RegExp(`^\\s*${key}\\s*=`).test(item))
      if (index >= 0) lines[index] = line
      else lines.push(line)
    }
    writeFileSync(this.paths.envFile, `${lines.join('\n').replace(/\n+$/u, '')}\n`, { mode: 0o600 })
    chmodSync(this.paths.envFile, 0o600)
  }

  private readJsonConfig(path: string): Partial<Config> {
    if (!existsSync(path)) return {}
    try {
      return JSON.parse(readFileSync(path, 'utf8')) as Partial<Config>
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error)
      throw new Error(`Configuração inválida em ${path}: ${reason}`)
    }
  }

  private readTemplate(): string {
    if (!existsSync(this.paths.templateFile)) return DEFAULT_TEMPLATE
    const template = readFileSync(this.paths.templateFile, 'utf8').trim()
    return template || DEFAULT_TEMPLATE
  }

  private parseProviderList(value: string): ProviderName[] {
    const providers = value
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean)
      .map(parseProvider)
    if (providers.length === 0) throw new Error('Configure pelo menos um provider.')
    return providers
  }

  private async promptEmail(message: string, initialValue: string, hint: string): Promise<string> {
    const value = await this.requirePrompts().text({
      message: `${message} (${hint})`,
      initialValue,
      validate: validateOptionalEmail
    })
    return this.promptString(value).trim()
  }

  private async promptModel(message: string, initialValue: string, fallback: string): Promise<string> {
    const value = await this.requirePrompts().text({ message, initialValue, placeholder: fallback })
    return this.promptString(value).trim() || fallback
  }

  private async promptReasoning(message: string, initialValue: ReasoningLevel): Promise<ReasoningLevel> {
    const value = await this.requirePrompts().select({
      message,
      options: [
        { value: 'provider-default', label: 'Padrão do provider', hint: 'usa a configuração nativa' },
        { value: 'none', label: 'None', hint: 'sem reasoning adicional' },
        { value: 'minimal', label: 'Minimal', hint: 'resposta mais rápida' },
        { value: 'low', label: 'Low', hint: 'reasoning leve' },
        { value: 'medium', label: 'Medium', hint: 'equilíbrio entre custo e profundidade' },
        { value: 'high', label: 'High', hint: 'reasoning aprofundado' },
        { value: 'xhigh', label: 'XHigh', hint: 'máxima profundidade quando suportado' }
      ],
      initialValue
    })
    return parseReasoningLevel(this.promptString(value), initialValue)
  }

  private async promptValue(message: string, initialValue: string): Promise<string> {
    const value = await this.requirePrompts().text({ message, initialValue })
    return this.promptString(value)
  }

  private promptString(value: unknown): string {
    if (typeof value !== 'string') throw new ConfigurationCancelled()
    return value
  }
}
