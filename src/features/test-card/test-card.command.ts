import { azureWorkItemUrl } from '../../shared/azure/azure-urls'
import {
  parsePositiveDecimal,
  validateNonNegativeDecimal,
  validatePositiveDecimal
} from './test-card.validation'
import { validateWorkItemId } from '../../shared/validation/work-item'
import type { ProgressReporter, PromptPort } from '../../infrastructure/terminal/terminal-ports'
import type { CliOptions } from '../../app/cli.models'
import {
  buildCreateTestCaseInput,
  workItemNumber,
  workItemText,
  type TestCardPreparation,
  type TestCardSettings
} from './test-card.models'
import { TestCardPresenter } from './test-card.presenter'
import { TestCardService } from './test-card.service'

export class TestCardCommand {
  constructor(
    private readonly service: TestCardService,
    private readonly presenter: TestCardPresenter,
    private readonly prompts: PromptPort,
    private readonly progress: ProgressReporter,
    private readonly interactive: boolean
  ) {}

  async execute(options: CliOptions): Promise<number> {
    const interactive = this.interactive
    const progress = this.progress
    let preparation: TestCardPreparation
    try {
      preparation = await this.service.prepare(
        options,
        interactive,
        async () =>
          this.prompts.text({
            message: 'ID do Work Item pai',
            validate: validateWorkItemId
        }),
        (message) => progress.message(message),
        (context) => {
          this.presenter.intro(context.branch)
          progress.start('Coletando contexto Azure DevOps')
        }
      )
    } catch (error) {
      progress.error('Falha ao coletar contexto Azure DevOps')
      throw error
    }
    progress.stop(`Contexto resolvido (Work Item #${preparation.context.workItem.id})`)

    if (options.dryRun) {
      this.presenter.showDryRun(preparation.config, preparation.prompt)
      this.presenter.outro('Concluído.')
      return 0
    }

    progress.start('Gerando card via IA')
    let generated: Awaited<ReturnType<TestCardService['generate']>>
    try {
      generated = await this.service.generate(preparation, (provider, model) => {
        progress.message(`Tentando ${provider} (${model})`)
      })
    } catch (error) {
      progress.error('Falha ao gerar card')
      throw error
    }
    progress.stop(`Card gerado (${generated.provider}/${generated.model})`)

    const { title, body } = generated.description
    if (options.raw) {
      this.presenter.raw(body)
      this.presenter.outro('Concluído.')
      return 0
    }
    this.presenter.showSummary(preparation, generated.provider, generated.model, title, body, options)
    if (options.noCreate) {
      this.presenter.outro('Concluído.')
      return 0
    }

    const shouldCreate = interactive
      ? (await this.prompts.confirm({
          message: 'Criar este Test Case no Azure DevOps?',
          initialValue: options.create
        })) === true
      : false
    if (typeof shouldCreate !== 'boolean') return 0
    if (!shouldCreate) {
      if (!interactive) this.presenter.info('Ambiente não interativo; criação requer confirmação no terminal.')
      this.presenter.outro('Concluído.')
      return 0
    }

    const settings = await this.promptSettings(preparation, options, interactive)
    const input = buildCreateTestCaseInput(settings, preparation.context.workItem.id, title, body)
    const created = await this.service.create(preparation, input)
    this.presenter.success(
      `Test Case #${created.id} criado: ${azureWorkItemUrl(preparation.context.git, String(created.id))}`
    )
    if (interactive) await this.maybeUpdateParent(preparation)
    this.presenter.outro('Concluído.')
    return 0
  }

  private async promptSettings(
    preparation: TestCardPreparation,
    options: CliOptions,
    interactive: boolean
  ): Promise<TestCardSettings> {
    const settings: TestCardSettings = {
      areaPath: options.areaPath ?? preparation.config.testAreaPath,
      assignedTo: options.assignedTo ?? preparation.config.testAssignedTo,
      iterationPath:
        options.iterationPath ?? workItemText(preparation.context.workItem, 'System.IterationPath') ?? '',
      priority: parsePositiveDecimal(options.priority, 2, '--priority'),
      team: options.team ?? preparation.config.testTeam,
      program: options.program ?? preparation.config.testProgram
    }
    if (!interactive) return settings
    settings.areaPath = await this.promptOptional('AreaPath do Test Case', settings.areaPath)
    settings.assignedTo = await this.promptOptional('Responsável do Test Case', settings.assignedTo)
    settings.iterationPath = await this.promptOptional('IterationPath do Test Case', settings.iterationPath)
    settings.priority = await this.promptNumber('Prioridade do Test Case', settings.priority, validatePositiveDecimal)
    settings.team = await this.promptOptional('Custom.Team', settings.team)
    settings.program = await this.promptOptional('Custom.ProgramasAgrotrace', settings.program)
    return settings
  }

  private async maybeUpdateParent(preparation: TestCardPreparation): Promise<void> {
    const update = await this.prompts.confirm({
      message: `Atualizar o Work Item #${preparation.context.workItem.id} para Test QA?`,
      initialValue: false
    })
    if (typeof update !== 'boolean' || !update) return
    const effort = workItemNumber(preparation.context.workItem, 'Microsoft.VSTS.Scheduling.Effort')
    const realEffort = workItemNumber(preparation.context.workItem, 'Custom.RealEffort')
    const nextEffort =
      effort === undefined ? await this.promptNumber('Effort (horas decimais)', 0.5, validateNonNegativeDecimal) : undefined
    const nextRealEffort =
      realEffort === undefined
        ? await this.promptNumber(
            'Real Effort (horas decimais)',
            nextEffort ?? effort ?? 0.5,
            validateNonNegativeDecimal
          )
        : undefined
    await this.service.updateParent(preparation, nextEffort, nextRealEffort)
    this.presenter.success(`Work Item #${preparation.context.workItem.id} atualizado para Test QA.`)
  }

  private async promptOptional(message: string, initialValue: string): Promise<string> {
    const value = await this.prompts.text({ message: `${message} (opcional)`, initialValue })
    if (typeof value !== 'string') throw new Error('Operação cancelada.')
    return value.trim()
  }

  private async promptNumber(
    message: string,
    initialValue: number,
    validate: (value: string | undefined) => string | undefined
  ): Promise<number> {
    const value = await this.prompts.text({
      message,
      initialValue: String(initialValue),
      validate
    })
    if (typeof value !== 'string') throw new Error('Operação cancelada.')
    return Number(value.replace(',', '.'))
  }
}
