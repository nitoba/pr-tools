import { existsSync } from 'node:fs'
import { AzurePullRequestClient, pathSegment, withApiVersion } from '../../azure'
import { parseAzureRemote } from '../../infrastructure/git/git-context-service'
import type { AzureClientFactory } from '../../infrastructure/azure/azure-client-factory'
import type { ProcessRunner, ProcessResult } from '../../infrastructure/process/process-runner'
import type { ConfigService } from '../../infrastructure/config/config-service'
import type { GitContextReader, HttpFetcher } from '../../app/ports'
import { optionalEmailPromptSchema } from '../../infrastructure/config/config-validation'
import type { CliOptions } from '../../app/cli.models'
import type { Config } from '../../infrastructure/config/config.models'
import type { GitContext } from '../../infrastructure/git/git-context.models'
import type { DoctorCheck, DoctorReport } from './doctor.models'

const COMMAND_TIMEOUT_MS = 5_000
const ANSI_ESCAPE = new RegExp(`${String.fromCharCode(27)}\\[[0-?]*[ -/]*[@-~]`, 'g')

type AzureRemote = Pick<GitContext, 'azureOrg' | 'azureProject' | 'azureRepo'>
export class DoctorService {
  constructor(
    private readonly config: ConfigService,
    private readonly git: GitContextReader,
    private readonly processes: ProcessRunner,
    private readonly azureClients: AzureClientFactory,
    private readonly fetcher: HttpFetcher
  ) {}

  async inspect(options: CliOptions): Promise<DoctorReport> {
    const checks: DoctorCheck[] = []
    const git = this.inspectGit(options.source, checks)
    let loaded: Config | undefined
    try {
      loaded = this.config.load(options)
      this.inspectConfiguration(loaded, checks)
    } catch (error) {
      this.add(
        checks,
        'fail',
        'Configuração',
        this.errorMessage(error),
        'Execute `prt init` ou corrija o arquivo de configuração indicado.'
      )
    }

    if (loaded) {
      const readyProviders = await this.inspectProviders(loaded, checks)
      if (readyProviders === 0) {
        this.add(
          checks,
          'fail',
          'Providers de IA',
          'Nenhum provider configurado está pronto para gerar conteúdo.',
          'Instale/autentique um provider e execute `prt init` para selecioná-lo.'
        )
      } else {
        this.add(checks, 'ok', 'Providers de IA', `${readyProviders} provider(s) pronto(s) para geração.`)
      }
      await this.inspectAzure(loaded, git.azure, checks)
    }

    const failures = checks.filter((check) => check.status === 'fail').length
    const warnings = checks.filter((check) => check.status === 'warn').length
    return { checks, failures, warnings }
  }

  private inspectGit(sourceBranch: string | undefined, checks: DoctorCheck[]): { azure?: AzureRemote } {
    const gitVersion = this.run('git', ['--version'])
    if (!gitVersion.ok) {
      this.add(checks, 'fail', 'Git', 'O executável git não está disponível.', 'Instale o Git e abra um novo terminal antes de executar `prt doctor`.')
      return {}
    }
    this.add(checks, 'ok', 'Git', this.cleanLine(gitVersion.stdout) || 'Executável disponível.')

    const root = this.run('git', ['rev-parse', '--show-toplevel'])
    if (!root.ok) {
      this.add(checks, 'fail', 'Repositório Git', 'O diretório atual não está dentro de um repositório Git.', 'Entre no clone do projeto que será usado para gerar o contexto do PR ou Test Case.')
      return {}
    }
    this.add(checks, 'ok', 'Repositório Git', `Projeto detectado em ${root.stdout}.`)

    const currentBranch = this.run('git', ['branch', '--show-current']).stdout
    if (!currentBranch) {
      this.add(checks, 'warn', 'Branch de trabalho', 'O Git está em detached HEAD.', 'Mude para uma branch de trabalho ou use `--source <branch>`.')
    } else if (['dev', 'main', 'master'].includes(currentBranch)) {
      this.add(checks, 'warn', 'Branch de trabalho', `A branch atual (${currentBranch}) é uma branch base.`, 'Mude para a branch da alteração antes de gerar o contexto do PR.')
    } else {
      this.add(checks, 'ok', 'Branch de trabalho', currentBranch)
    }

    const remoteResult = this.run('git', ['remote', 'get-url', 'origin'])
    let azure: AzureRemote | undefined
    if (!remoteResult.ok || !remoteResult.stdout) {
      this.add(checks, 'fail', 'Remote origin', 'Nenhum remote origin foi encontrado.', 'Configure o remote Azure DevOps com `git remote add origin <url>`.')
    } else {
      const parsed = parseAzureRemote(remoteResult.stdout)
      if (!parsed.isAzureDevOps) {
        this.add(checks, 'fail', 'Remote Azure DevOps', `O origin atual não é Azure DevOps (${remoteResult.stdout}).`, 'Aponte o origin para o repositório Azure DevOps que receberá os PRs e Test Cases.')
      } else {
        this.add(checks, 'ok', 'Remote Azure DevOps', `${parsed.azureOrg}/${parsed.azureProject}/${parsed.azureRepo}.`)
        azure = { azureOrg: parsed.azureOrg, azureProject: parsed.azureProject, azureRepo: parsed.azureRepo }
      }
    }

    try {
      const context = this.git.collect(sourceBranch)
      this.add(checks, 'ok', 'Contexto de PR', `${context.diffOriginalLines} linhas de diff contra ${context.baseBranch}.`)
      if (context.workItemId) this.add(checks, 'ok', 'Work Item da branch', `#${context.workItemId}.`)
      else this.add(checks, 'warn', 'Work Item da branch', 'Nenhum ID numérico foi encontrado no nome da branch.', 'Use `--work-item <id>` ao gerar o PR ou Test Case.')
    } catch (error) {
      this.add(checks, 'fail', 'Contexto de PR/Test Case', this.errorMessage(error), 'Entre na branch da alteração ou use `--source <branch>`; ela precisa ter diff contra dev, main ou sprint.')
    }
    return azure ? { azure } : {}
  }

  private inspectConfiguration(config: Config, checks: DoctorCheck[]): void {
    const { configFile, envFile, templateFile } = this.config.paths
    if (existsSync(configFile) || existsSync(envFile)) {
      this.add(checks, 'ok', 'Configuração local', `${existsSync(configFile) ? configFile : envFile} carregado.`)
    } else {
      this.add(checks, 'warn', 'Configuração local', 'Nenhum arquivo de configuração foi criado; defaults estão sendo usados.', 'Execute `prt init` para salvar PAT, reviewers, provider e modelos.')
    }
    this.add(
      checks,
      existsSync(templateFile) ? 'ok' : 'warn',
      'Template de prompt',
      existsSync(templateFile) ? 'Template personalizado encontrado.' : 'O template padrão embutido será usado.',
      existsSync(templateFile) ? undefined : 'Execute `prt init` para criar o template editável.'
    )
    if (config.providers.length === 0) this.add(checks, 'fail', 'Providers configurados', 'A lista de providers está vazia.', 'Execute `prt init` e escolha pelo menos um provider.')
    else this.add(checks, 'ok', 'Providers configurados', config.providers.join(', '))
    this.inspectEmail('Reviewer de dev', config.reviewerDev, checks)
    this.inspectEmail('Reviewer de sprint', config.reviewerSprint, checks)
    this.inspectEmail('Reviewer do Test Case', config.testAssignedTo, checks)
    if (!config.testAreaPath.trim()) this.add(checks, 'warn', 'Defaults do Test Case', 'AreaPath não configurado.', 'Informe o AreaPath durante a criação ou configure-o em `prt init`.')
    else this.add(checks, 'ok', 'Defaults do Test Case', `AreaPath: ${config.testAreaPath}.`)
  }

  private inspectEmail(component: string, value: string, checks: DoctorCheck[]): void {
    if (!value.trim()) {
      this.add(checks, 'warn', component, 'Email não configurado; a confirmação solicitará o reviewer/responsável.', 'Execute `prt init` ou informe o email durante o fluxo interativo.')
      return
    }
    if (!optionalEmailPromptSchema.safeParse(value).success) {
      this.add(checks, 'warn', component, 'O valor configurado não parece ser um email válido.', 'Execute `prt init` e informe um email Azure DevOps válido.')
      return
    }
    this.add(checks, 'ok', component, 'Email configurado.')
  }

  private async inspectProviders(config: Config, checks: DoctorCheck[]): Promise<number> {
    let ready = 0
    for (const provider of config.providers) {
      if (provider === 'codex' && this.inspectCodex(config, checks)) ready += 1
      if (provider === 'opencode' && this.inspectOpenCode(config, checks)) ready += 1
      if (provider === 'openai-compatible' && (await this.inspectCompatible(config, checks))) ready += 1
    }
    return ready
  }

  private inspectCodex(config: Config, checks: DoctorCheck[]): boolean {
    const executable = this.run('codex', ['--version'])
    if (!executable.ok) {
      this.add(checks, 'warn', 'Codex CLI', 'O executável `codex` não foi encontrado ou não iniciou.', 'Instale o Codex CLI e confirme que `codex` está no PATH.')
      return false
    }
    this.add(checks, 'ok', 'Codex CLI', `${this.cleanLine(executable.stdout)} · modelo ${config.codexModel} · thinking ${config.codexReasoning}.`)
    const login = this.run('codex', ['login', 'status'])
    if (!login.ok) {
      this.add(checks, 'warn', 'Autenticação Codex', this.cleanLine(login.stderr) || 'O Codex não está autenticado.', 'Execute `codex login` e repita `prt doctor`.')
      return false
    }
    this.add(checks, 'ok', 'Autenticação Codex', this.cleanLine(login.stdout) || 'Login local disponível.')
    return true
  }

  private inspectOpenCode(config: Config, checks: DoctorCheck[]): boolean {
    const executable = this.run('opencode', ['--version'])
    if (!executable.ok) {
      this.add(checks, 'warn', 'OpenCode CLI', 'O executável `opencode` não foi encontrado ou não iniciou.', 'Instale o OpenCode CLI e confirme que `opencode` está no PATH.')
      return false
    }
    this.add(checks, 'ok', 'OpenCode CLI', `${this.cleanLine(executable.stdout)} · modelo ${config.opencodeModel} · thinking ${config.opencodeReasoning}.`)
    const auth = this.run('opencode', ['auth', 'list'])
    const authOutput = this.cleanOutput(auth.stdout).toLowerCase()
    if (!auth.ok || !authOutput || /0 credentials|no credentials/u.test(authOutput)) {
      this.add(checks, 'warn', 'Autenticação OpenCode', 'Nenhuma credencial utilizável foi encontrada.', 'Execute `opencode auth login` para o provider usado pelo modelo.')
      return false
    }
    const modelProvider = (config.opencodeModel.split('/')[0] ?? '').trim().toLowerCase()
    if (modelProvider && !authOutput.includes(modelProvider)) {
      this.add(checks, 'warn', 'Autenticação OpenCode', `Há credenciais, mas não foi encontrada uma entrada clara para ${modelProvider}.`, `Execute \`opencode auth login ${modelProvider}\` ou escolha outro modelo em \`prt init\`.`)
      return false
    }
    this.add(checks, 'ok', 'Autenticação OpenCode', 'Credencial compatível encontrada.')
    return true
  }

  private async inspectCompatible(config: Config, checks: DoctorCheck[]): Promise<boolean> {
    const baseUrl = config.baseUrl.replace(/\/+$/u, '').replace(/\/chat\/completions$/u, '')
    let url: URL
    try {
      url = new URL(baseUrl)
      if (!['http:', 'https:'].includes(url.protocol)) throw new Error('protocolo inválido')
    } catch {
      this.add(checks, 'warn', 'OpenAI-compatible URL', `Base URL inválida: ${config.baseUrl}.`, 'Execute `prt init` e informe uma URL HTTP/HTTPS compatível.')
      return false
    }
    const hasApiKey = Boolean(config.apiKey.trim())
    this.add(checks, hasApiKey ? 'ok' : 'warn', 'OpenAI-compatible API key', hasApiKey ? 'API key configurada (valor oculto).' : 'API key não configurada; alguns endpoints locais não exigem uma.', hasApiKey ? undefined : 'Configure a API key em `prt init` se o endpoint exigir autenticação.')
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS)
    try {
      const response = await this.fetcher.fetch(`${url.toString().replace(/\/$/u, '')}/models`, {
        headers: hasApiKey ? { Authorization: `Bearer ${config.apiKey}` } : {},
        signal: controller.signal
      })
      if (response.status === 401 || response.status === 403) {
        this.add(checks, 'warn', 'OpenAI-compatible endpoint', `O endpoint respondeu HTTP ${response.status}; a autenticação foi rejeitada.`, 'Revise a API key e a Base URL em `prt init`.')
        return false
      }
      if (response.status >= 500) {
        this.add(checks, 'warn', 'OpenAI-compatible endpoint', `O endpoint está acessível, mas respondeu HTTP ${response.status}.`, 'Verifique se o serviço está ativo e tente novamente.')
        return false
      }
      if (response.status === 404) this.add(checks, 'warn', 'OpenAI-compatible endpoint', 'A URL respondeu, mas não expõe /models; a rota de geração pode ainda funcionar.', 'Confirme se a Base URL termina na raiz compatível, normalmente /v1.')
      else this.add(checks, 'ok', 'OpenAI-compatible endpoint', `Endpoint acessível (HTTP ${response.status}).`)
      return true
    } catch (error) {
      this.add(checks, 'warn', 'OpenAI-compatible endpoint', `Não foi possível alcançar ${url.toString()}. ${this.errorMessage(error)}`, 'Confirme a Base URL, a rede e se o serviço está em execução.')
      return false
    } finally {
      clearTimeout(timeout)
    }
  }

  private async inspectAzure(config: Config, remote: AzureRemote | undefined, checks: DoctorCheck[]): Promise<void> {
    if (!config.azurePat.trim()) {
      this.add(checks, 'fail', 'Azure DevOps PAT', 'PAT não configurado; PRs e Test Cases não poderão ser publicados.', 'Execute `prt init` ou defina AZURE_PAT/AZURE_DEVOPS_PAT.')
      return
    }
    this.add(checks, 'ok', 'Azure DevOps PAT', 'PAT configurado (valor oculto).')
    if (!remote) {
      this.add(checks, 'warn', 'Azure DevOps APIs', 'PAT disponível, mas o remote Azure DevOps não pôde ser identificado; as permissões não foram testadas.', 'Configure um remote Azure DevOps válido e repita `prt doctor`.')
      return
    }
    const client = this.azureClients.createForOrganization(config, remote.azureOrg)
    const pullRequests = new AzurePullRequestClient(client)
    const repositoryProbe = await this.probe((signal) =>
      pullRequests.getRepository(remote.azureProject, remote.azureRepo, signal)
    )
    if (repositoryProbe.ok) this.add(checks, 'ok', 'Azure Code API', 'PAT consegue ler o repositório Azure DevOps; escrita não testada para evitar alterações.')
    else this.add(checks, 'fail', 'Azure Code API', this.azureError(repositoryProbe), this.azureFix(repositoryProbe, 'Code Read'))
    const workItemProbe = await this.probe((signal) =>
      client.request(withApiVersion(`/${pathSegment(remote.azureProject)}/_apis/wit/workitemtypes`), {
        signal
      })
    )
    if (workItemProbe.ok) this.add(checks, 'ok', 'Azure Work Items API', 'PAT consegue consultar Work Items; escrita não testada para evitar criação ou alteração.')
    else this.add(checks, 'fail', 'Azure Work Items API', this.azureError(workItemProbe), this.azureFix(workItemProbe, 'Work Items Read'))
  }

  private async probe(
    request: (signal: AbortSignal) => Promise<unknown>
  ): Promise<{ ok: boolean; status?: number; error?: string }> {
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS)
    try {
      await request(controller.signal)
      return { ok: true }
    } catch (error) {
      const message = this.errorMessage(error)
      const match = /Azure DevOps API respondeu (\d+)/u.exec(message)
      const status = match ? match[1] : undefined
      return { ok: false, status: status ? Number(status) : undefined, error: message }
    } finally {
      clearTimeout(timeout)
    }
  }

  private run(command: string, args: string[]): ProcessResult & { ok: boolean } {
    const result = this.processes.run(command, args, COMMAND_TIMEOUT_MS)
    return { ...result, ok: result.exitCode === 0 && !result.error }
  }

  private add(checks: DoctorCheck[], status: DoctorCheck['status'], component: string, detail: string, fix?: string): void {
    checks.push({ component, status, detail, fix })
  }

  private cleanLine(value: string): string {
    return this.cleanOutput(value).split('\n').map((line) => line.trim()).find(Boolean) ?? ''
  }

  private cleanOutput(value: string): string {
    return value.replace(ANSI_ESCAPE, '').replace(/\r/gu, '').trim()
  }

  private azureError(error: { status?: number; error?: string }): string {
    return error.status ? `Azure DevOps respondeu HTTP ${error.status}.` : `Não foi possível consultar o Azure DevOps: ${error.error ?? 'erro desconhecido.'}`
  }

  private azureFix(error: { status?: number }, scope: string): string {
    if (error.status === 401 || error.status === 403) return `Revise o PAT e conceda o escopo Azure DevOps “${scope}”.`
    return 'Confirme a rede, a organização/projeto do remote e repita `prt doctor`.'
  }

  private errorMessage(error: unknown): string {
    if (error instanceof Error) return error.message
    if (typeof error === 'string') return error
    return 'Erro não identificado.'
  }
}
