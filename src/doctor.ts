import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { intro, log, outro } from '@clack/prompts'
import { AzureDevOpsClient, getRepository, pathSegment, withApiVersion } from './azure'
import { CONFIG_FILE, ENV_FILE, TEMPLATE_FILE, loadConfig } from './config'
import { collectGitContext, parseAzureRemote } from './git'
import { optionalEmailPromptSchema } from './validation'
import type { CliOptions, Config, GitContext } from './types'

type DoctorStatus = 'ok' | 'warn' | 'fail'

type DoctorCheck = {
  component: string
  status: DoctorStatus
  detail: string
  fix?: string
}

type CommandResult = {
  ok: boolean
  stdout: string
  stderr: string
  error?: string
}

type AzureRemote = Pick<GitContext, 'azureOrg' | 'azureProject' | 'azureRepo'>

type GitInspection = {
  azure?: AzureRemote
}

const COMMAND_TIMEOUT_MS = 5_000
const ANSI_ESCAPE = new RegExp(`${String.fromCharCode(27)}\\[[0-?]*[ -/]*[@-~]`, 'g')

export async function runDoctor(options: CliOptions): Promise<boolean> {
  const checks: DoctorCheck[] = []
  intro('prt · doctor')

  const git = inspectGit(options.source, checks)
  let config: Config | undefined
  try {
    config = loadConfig(options)
    inspectConfiguration(config, checks)
  } catch (error) {
    addCheck(
      checks,
      'fail',
      'Configuração',
      errorMessage(error),
      'Execute `prt init` ou corrija o arquivo de configuração indicado.'
    )
  }

  if (config) {
    const readyProviders = await inspectProviders(config, checks)
    if (readyProviders === 0) {
      addCheck(
        checks,
        'fail',
        'Providers de IA',
        'Nenhum provider configurado está pronto para gerar conteúdo.',
        'Instale/autentique um provider e execute `prt init` para selecioná-lo.'
      )
    } else {
      addCheck(
        checks,
        'ok',
        'Providers de IA',
        `${readyProviders} provider(s) pronto(s) para geração.`
      )
    }
  }

  if (config) await inspectAzure(config, git.azure, checks)

  for (const check of checks) printCheck(check)
  const failures = checks.filter((check) => check.status === 'fail').length
  const warnings = checks.filter((check) => check.status === 'warn').length
  if (failures > 0) {
    outro(`Doctor encontrou ${failures} falha(s) e ${warnings} aviso(s).`)
  } else if (warnings > 0) {
    outro(`Doctor concluído com ${warnings} aviso(s).`)
  } else {
    outro('Doctor concluído: todos os componentes estão prontos.')
  }
  return failures === 0
}

function inspectGit(sourceBranch: string | undefined, checks: DoctorCheck[]): GitInspection {
  const gitVersion = runCommand('git', ['--version'])
  if (!gitVersion.ok) {
    addCheck(
      checks,
      'fail',
      'Git',
      'O executável git não está disponível.',
      'Instale o Git e abra um novo terminal antes de executar `prt doctor`.'
    )
    return {}
  }
  addCheck(checks, 'ok', 'Git', cleanLine(gitVersion.stdout) || 'Executável disponível.')

  const root = runCommand('git', ['rev-parse', '--show-toplevel'])
  if (!root.ok) {
    addCheck(
      checks,
      'fail',
      'Repositório Git',
      'O diretório atual não está dentro de um repositório Git.',
      'Entre no clone do projeto que será usado para gerar o PR ou Test Case.'
    )
    return {}
  }
  addCheck(checks, 'ok', 'Repositório Git', `Projeto detectado em ${root.stdout}.`)

  const currentBranch = runCommand('git', ['branch', '--show-current']).stdout
  if (!currentBranch) {
    addCheck(
      checks,
      'warn',
      'Branch de trabalho',
      'O Git está em detached HEAD.',
      'Mude para uma branch de trabalho ou use `--source <branch>`.'
    )
  } else if (['dev', 'main', 'master'].includes(currentBranch)) {
    addCheck(
      checks,
      'warn',
      'Branch de trabalho',
      `A branch atual (${currentBranch}) é uma branch base.`,
      'Mude para a branch da alteração antes de gerar o contexto do PR.'
    )
  } else {
    addCheck(checks, 'ok', 'Branch de trabalho', currentBranch)
  }

  const remoteResult = runCommand('git', ['remote', 'get-url', 'origin'])
  let azure: AzureRemote | undefined
  if (!remoteResult.ok || !remoteResult.stdout) {
    addCheck(
      checks,
      'fail',
      'Remote origin',
      'Nenhum remote origin foi encontrado.',
      'Configure o remote Azure DevOps com `git remote add origin <url>`.'
    )
  } else {
    const parsedRemote = parseAzureRemote(remoteResult.stdout)
    if (!parsedRemote.isAzureDevOps) {
      addCheck(
        checks,
        'fail',
        'Remote Azure DevOps',
        `O origin atual não é Azure DevOps (${remoteResult.stdout}).`,
        'Aponte o origin para o repositório Azure DevOps que receberá os PRs e Test Cases.'
      )
    } else {
      addCheck(
        checks,
        'ok',
        'Remote Azure DevOps',
        `${parsedRemote.azureOrg}/${parsedRemote.azureProject}/${parsedRemote.azureRepo}.`
      )
      azure = {
        azureOrg: parsedRemote.azureOrg,
        azureProject: parsedRemote.azureProject,
        azureRepo: parsedRemote.azureRepo
      }
    }
  }

  try {
    const context = collectGitContext(sourceBranch)
    addCheck(
      checks,
      'ok',
      'Contexto de PR',
      `${context.diffOriginalLines} linhas de diff contra ${context.baseBranch}.`
    )
    if (context.workItemId) {
      addCheck(checks, 'ok', 'Work Item da branch', `#${context.workItemId}.`)
    } else {
      addCheck(
        checks,
        'warn',
        'Work Item da branch',
        'Nenhum ID numérico foi encontrado no nome da branch.',
        'Use `--work-item <id>` ao gerar o PR ou Test Case.'
      )
    }
  } catch (error) {
    addCheck(
      checks,
      'fail',
      'Contexto de PR/Test Case',
      errorMessage(error),
      'Entre na branch da alteração ou use `--source <branch>`; ela precisa ter diff contra dev, main ou sprint.'
    )
  }

  return azure ? { azure } : {}
}

function inspectConfiguration(config: Config, checks: DoctorCheck[]): void {
  const hasConfigFile = existsSync(CONFIG_FILE)
  const hasEnvFile = existsSync(ENV_FILE)
  if (hasConfigFile || hasEnvFile) {
    addCheck(
      checks,
      'ok',
      'Configuração local',
      `${hasConfigFile ? CONFIG_FILE : ENV_FILE} carregado.`
    )
  } else {
    addCheck(
      checks,
      'warn',
      'Configuração local',
      'Nenhum arquivo de configuração foi criado; defaults estão sendo usados.',
      'Execute `prt init` para salvar PAT, reviewers, provider e modelos.'
    )
  }

  addCheck(
    checks,
    existsSync(TEMPLATE_FILE) ? 'ok' : 'warn',
    'Template de prompt',
    existsSync(TEMPLATE_FILE)
      ? 'Template personalizado encontrado.'
      : 'O template padrão embutido será usado.',
    existsSync(TEMPLATE_FILE) ? undefined : 'Execute `prt init` para criar o template editável.'
  )

  if (config.providers.length === 0) {
    addCheck(
      checks,
      'fail',
      'Providers configurados',
      'A lista de providers está vazia.',
      'Execute `prt init` e escolha pelo menos um provider.'
    )
  } else {
    addCheck(checks, 'ok', 'Providers configurados', config.providers.join(', '))
  }

  inspectReviewEmail('Reviewer de dev', config.reviewerDev, checks)
  inspectReviewEmail('Reviewer de sprint', config.reviewerSprint, checks)
  inspectReviewEmail('Reviewer do Test Case', config.testAssignedTo, checks)

  if (!config.testAreaPath.trim()) {
    addCheck(
      checks,
      'warn',
      'Defaults do Test Case',
      'AreaPath não configurado.',
      'Informe o AreaPath durante a criação ou configure-o em `prt init`.'
    )
  } else {
    addCheck(checks, 'ok', 'Defaults do Test Case', `AreaPath: ${config.testAreaPath}.`)
  }
}

function inspectReviewEmail(component: string, value: string, checks: DoctorCheck[]): void {
  if (!value.trim()) {
    addCheck(
      checks,
      'warn',
      component,
      'Email não configurado; a confirmação solicitará o reviewer/responsável.',
      'Execute `prt init` ou informe o email durante o fluxo interativo.'
    )
    return
  }
  const parsed = optionalEmailPromptSchema.safeParse(value)
  if (!parsed.success) {
    addCheck(
      checks,
      'warn',
      component,
      'O valor configurado não parece ser um email válido.',
      'Execute `prt init` e informe um email Azure DevOps válido.'
    )
    return
  }
  addCheck(checks, 'ok', component, 'Email configurado.')
}

async function inspectProviders(config: Config, checks: DoctorCheck[]): Promise<number> {
  let ready = 0
  for (const provider of config.providers) {
    if (provider === 'codex' && inspectCodex(config, checks)) ready += 1
    if (provider === 'opencode' && inspectOpenCode(config, checks)) ready += 1
    if (provider === 'openai-compatible' && (await inspectOpenAICompatible(config, checks)))
      ready += 1
  }
  return ready
}

function inspectCodex(config: Config, checks: DoctorCheck[]): boolean {
  const executable = runCommand('codex', ['--version'])
  if (!executable.ok) {
    addCheck(
      checks,
      'warn',
      'Codex CLI',
      'O executável `codex` não foi encontrado ou não iniciou.',
      'Instale o Codex CLI e confirme que `codex` está no PATH.'
    )
    return false
  }
  addCheck(
    checks,
    'ok',
    'Codex CLI',
    `${cleanLine(executable.stdout)} · modelo ${config.codexModel} · thinking ${config.codexReasoning}.`
  )

  const login = runCommand('codex', ['login', 'status'])
  if (!login.ok) {
    addCheck(
      checks,
      'warn',
      'Autenticação Codex',
      cleanLine(login.stderr) || 'O Codex não está autenticado.',
      'Execute `codex login` e repita `prt doctor`.'
    )
    return false
  }
  addCheck(checks, 'ok', 'Autenticação Codex', cleanLine(login.stdout) || 'Login local disponível.')
  return true
}

function inspectOpenCode(config: Config, checks: DoctorCheck[]): boolean {
  const executable = runCommand('opencode', ['--version'])
  if (!executable.ok) {
    addCheck(
      checks,
      'warn',
      'OpenCode CLI',
      'O executável `opencode` não foi encontrado ou não iniciou.',
      'Instale o OpenCode CLI e confirme que `opencode` está no PATH.'
    )
    return false
  }
  addCheck(
    checks,
    'ok',
    'OpenCode CLI',
    `${cleanLine(executable.stdout)} · modelo ${config.opencodeModel} · thinking ${config.opencodeReasoning}.`
  )

  const auth = runCommand('opencode', ['auth', 'list'])
  const authOutput = cleanOutput(auth.stdout).toLowerCase()
  if (!auth.ok || !authOutput || /0 credentials|no credentials/u.test(authOutput)) {
    addCheck(
      checks,
      'warn',
      'Autenticação OpenCode',
      'Nenhuma credencial utilizável foi encontrada.',
      'Execute `opencode auth login` para o provider usado pelo modelo.'
    )
    return false
  }

  const modelProvider = (config.opencodeModel.split('/')[0] ?? '').trim().toLowerCase()
  if (modelProvider && !authOutput.includes(modelProvider)) {
    addCheck(
      checks,
      'warn',
      'Autenticação OpenCode',
      `Há credenciais, mas não foi encontrada uma entrada clara para ${modelProvider}.`,
      `Execute \`opencode auth login ${modelProvider}\` ou escolha outro modelo em \`prt init\`.`
    )
    return false
  }
  addCheck(checks, 'ok', 'Autenticação OpenCode', 'Credencial compatível encontrada.')
  return true
}

async function inspectOpenAICompatible(config: Config, checks: DoctorCheck[]): Promise<boolean> {
  const baseUrl = config.baseUrl.replace(/\/+$/u, '').replace(/\/chat\/completions$/u, '')
  let url: URL
  try {
    url = new URL(baseUrl)
    if (!['http:', 'https:'].includes(url.protocol)) throw new Error('protocolo inválido')
  } catch {
    addCheck(
      checks,
      'warn',
      'OpenAI-compatible URL',
      `Base URL inválida: ${config.baseUrl}.`,
      'Execute `prt init` e informe uma URL HTTP/HTTPS compatível.'
    )
    return false
  }

  const hasApiKey = Boolean(config.apiKey.trim())
  addCheck(
    checks,
    hasApiKey ? 'ok' : 'warn',
    'OpenAI-compatible API key',
    hasApiKey
      ? 'API key configurada (valor oculto).'
      : 'API key não configurada; alguns endpoints locais não exigem uma.',
    hasApiKey ? undefined : 'Configure a API key em `prt init` se o endpoint exigir autenticação.'
  )

  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS)
  try {
    const response = await fetch(`${url.toString().replace(/\/$/u, '')}/models`, {
      headers: hasApiKey ? { Authorization: `Bearer ${config.apiKey}` } : {},
      signal: controller.signal
    })
    if (response.status === 401 || response.status === 403) {
      addCheck(
        checks,
        'warn',
        'OpenAI-compatible endpoint',
        `O endpoint respondeu HTTP ${response.status}; a autenticação foi rejeitada.`,
        'Revise a API key e a Base URL em `prt init`.'
      )
      return false
    }
    if (response.status >= 500) {
      addCheck(
        checks,
        'warn',
        'OpenAI-compatible endpoint',
        `O endpoint está acessível, mas respondeu HTTP ${response.status}.`,
        'Verifique se o serviço está ativo e tente novamente.'
      )
      return false
    }
    if (response.status === 404) {
      addCheck(
        checks,
        'warn',
        'OpenAI-compatible endpoint',
        'A URL respondeu, mas não expõe /models; a rota de geração pode ainda funcionar.',
        'Confirme se a Base URL termina na raiz compatível, normalmente /v1.'
      )
    } else {
      addCheck(
        checks,
        'ok',
        'OpenAI-compatible endpoint',
        `Endpoint acessível (HTTP ${response.status}).`
      )
    }
    return true
  } catch (error) {
    addCheck(
      checks,
      'warn',
      'OpenAI-compatible endpoint',
      `Não foi possível alcançar ${url.toString()}. ${errorMessage(error)}`,
      'Confirme a Base URL, a rede e se o serviço está em execução.'
    )
    return false
  } finally {
    clearTimeout(timeout)
  }
}

async function inspectAzure(
  config: Config,
  remote: AzureRemote | undefined,
  checks: DoctorCheck[]
): Promise<void> {
  if (!config.azurePat.trim()) {
    addCheck(
      checks,
      'fail',
      'Azure DevOps PAT',
      'PAT não configurado; PRs e Test Cases não poderão ser publicados.',
      'Execute `prt init` ou defina AZURE_PAT/AZURE_DEVOPS_PAT.'
    )
    return
  }
  addCheck(checks, 'ok', 'Azure DevOps PAT', 'PAT configurado (valor oculto).')

  if (!remote) {
    addCheck(
      checks,
      'warn',
      'Azure DevOps APIs',
      'PAT disponível, mas o remote Azure DevOps não pôde ser identificado; as permissões não foram testadas.',
      'Configure um remote Azure DevOps válido e repita `prt doctor`.'
    )
    return
  }

  const client = new AzureDevOpsClient({ pat: config.azurePat, organization: remote.azureOrg })
  const repositoryProbe = await probeAzure((signal) =>
    getRepository(client, remote.azureProject, remote.azureRepo, signal)
  )
  if (repositoryProbe.ok) {
    addCheck(
      checks,
      'ok',
      'Azure Code API',
      'PAT consegue ler o repositório Azure DevOps; escrita não testada para evitar alterações.'
    )
  } else {
    addCheck(
      checks,
      'fail',
      'Azure Code API',
      azureErrorDetail(repositoryProbe),
      azureFix(repositoryProbe, 'Code Read')
    )
  }

  const workItemProbe = await probeAzure((signal) =>
    client.request(withApiVersion(`/${pathSegment(remote.azureProject)}/_apis/wit/workitemtypes`), {
      signal
    })
  )
  if (workItemProbe.ok) {
    addCheck(
      checks,
      'ok',
      'Azure Work Items API',
      'PAT consegue consultar Work Items; escrita não testada para evitar criação ou alteração.'
    )
  } else {
    addCheck(
      checks,
      'fail',
      'Azure Work Items API',
      azureErrorDetail(workItemProbe),
      azureFix(workItemProbe, 'Work Items Read')
    )
  }
}

async function probeAzure(
  request: (signal: AbortSignal) => Promise<unknown>
): Promise<{ ok: boolean; status?: number; error?: string }> {
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS)
  try {
    await request(controller.signal)
    return { ok: true }
  } catch (error) {
    const message = errorMessage(error)
    const status = /Azure DevOps API respondeu (\d+)/u.exec(message)?.[1]
    return { ok: false, status: status ? Number(status) : undefined, error: message }
  } finally {
    clearTimeout(timeout)
  }
}

function azureErrorDetail(error: { status?: number; error?: string }): string {
  if (error.status) return `Azure DevOps respondeu HTTP ${error.status}.`
  return `Não foi possível consultar o Azure DevOps: ${error.error ?? 'erro desconhecido.'}`
}

function azureFix(error: { status?: number }, scope: string): string {
  if (error.status === 401 || error.status === 403) {
    return `Revise o PAT e conceda o escopo Azure DevOps “${scope}”.`
  }
  return 'Confirme a rede, a organização/projeto do remote e repita `prt doctor`.'
}

function runCommand(command: string, args: string[]): CommandResult {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: COMMAND_TIMEOUT_MS
  })
  const stdout = (result.stdout ?? '').trim()
  const stderr = (result.stderr ?? '').trim()
  return {
    ok: result.status === 0,
    stdout,
    stderr,
    error: result.error?.message
  }
}

function cleanLine(value: string): string {
  return (
    cleanOutput(value)
      .split('\n')
      .map((line) => line.trim())
      .find(Boolean) ?? ''
  )
}

function cleanOutput(value: string): string {
  return value.replace(ANSI_ESCAPE, '').replace(/\r/gu, '').trim()
}

function addCheck(
  checks: DoctorCheck[],
  status: DoctorStatus,
  component: string,
  detail: string,
  fix?: string
): void {
  checks.push({ component, status, detail, fix })
}

function printCheck(check: DoctorCheck): void {
  const message = `${check.component}: ${check.detail}`
  if (check.status === 'ok') log.success(`OK · ${message}`)
  if (check.status === 'warn') log.warn(`AVISO · ${message}`)
  if (check.status === 'fail') log.error(`FALHA · ${message}`)
  if (check.fix) log.info(`  Como resolver: ${check.fix}`)
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  return 'Erro não identificado.'
}
