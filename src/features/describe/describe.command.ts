import type { Clipboard } from '../../infrastructure/clipboard/clipboard'
import type { ProgressReporter, PromptPort } from '../../infrastructure/terminal/terminal-ports'
import type { CliOptions } from '../../app/cli.models'
import { DescribePresenter } from './describe.presenter'
import { DescribeService } from './describe.service'

export class DescribeCommand {
  constructor(
    private readonly service: DescribeService,
    private readonly presenter: DescribePresenter,
    private readonly prompts: PromptPort,
    private readonly progress: ProgressReporter,
    private readonly clipboard: Clipboard,
    private readonly interactive: boolean
  ) {}

  async execute(options: CliOptions): Promise<number> {
    const preparation = this.service.prepare(options, this.interactive)
    this.service.validateCreation(preparation, options.create)
    if (options.dryRun) {
      this.presenter.showDryRun(preparation)
      return 0
    }

    this.presenter.intro(preparation.context.branch)
    const progress = this.progress
    progress.start('Gerando descrição via IA')
    let generated = false
    try {
      const result = await this.service.generate(preparation, (provider, model) => {
        progress.message(`Tentando ${provider} (${model})`)
      })
      progress.stop(`Descrição gerada (${result.provider}/${result.model})`)
      generated = true
      this.presenter.showDescription(preparation, result, options)
      if (options.copy && this.clipboard.copy(result.description.body)) {
        this.presenter.success('Descrição copiada para o clipboard.')
      }

      if (preparation.context.isAzureDevOps && preparation.config.azurePat.trim() && preparation.interactive) {
        const shouldCreate = await this.prompts.confirm({
          message: 'Criar PR(s) no Azure DevOps?',
          initialValue: options.create
        })
        if (typeof shouldCreate !== 'boolean') return 0
        if (shouldCreate) {
          const reviewerByTarget = new Map<string, string>()
          for (const target of preparation.targets) {
            const defaultReviewer = target.includes('sprint')
              ? preparation.config.reviewerSprint || preparation.config.reviewerDev
              : preparation.config.reviewerDev
            const reviewer = await this.prompts.text({
              message: `Reviewer para ${target} (opcional; Enter mantém o padrão)`,
              initialValue: defaultReviewer
            })
            if (typeof reviewer !== 'string') return 0
            reviewerByTarget.set(target, reviewer.trim())
          }
          const published = await this.service.publish(
            preparation,
            result.description,
            (target) => reviewerByTarget.get(target) ?? ''
          )
          this.presenter.showPublished(preparation, published)
        }
      }
      this.presenter.outro('Concluído.')
      return 0
    } catch (error) {
      progress.error(generated ? 'Falha ao publicar no Azure DevOps' : 'Falha ao gerar descrição')
      throw error
    }
  }
}
