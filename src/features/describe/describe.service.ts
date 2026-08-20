import { buildPrompt } from './describe.prompt'
import { resolveTargets } from '../../infrastructure/git/git-context-service'
import { parseWorkItemId } from '../../shared/validation/work-item'
import type { DescriptionGenerator } from '../../infrastructure/ai/description-generator'
import type { PullRequestPublisher } from '../../infrastructure/azure/pull-request-publisher'
import type { GitContextReader } from '../../infrastructure/git/git-context-reader'
import type { ConfigService } from '../../infrastructure/config/config-service'
import type { CliOptions } from '../../app/cli.models'
import type { PrDescription, ProviderName } from '../../infrastructure/ai/ai.models'
import type { DescribePreparation, GeneratedDescription } from './describe.models'

export class DescribeService {
  constructor(
    private readonly config: ConfigService,
    private readonly git: GitContextReader,
    private readonly generator: DescriptionGenerator,
    private readonly publisher: PullRequestPublisher
  ) {}

  prepare(options: CliOptions, interactive: boolean): DescribePreparation {
    const config = this.config.load(options)
    const context = this.git.collect(options.source)
    if (options.targets.includes('sprint') && !context.sprintBranch) {
      throw new Error('Target sprint solicitado, mas nenhuma branch sprint/<número> foi encontrada.')
    }
    const workItemId = options.workItem ?? context.workItemId
    if (workItemId) parseWorkItemId(workItemId, 'Work Item')
    const targets = resolveTargets(context, options.targets)
    if (targets.length === 0) throw new Error('Nenhum target disponível.')
    return {
      config,
      context,
      targets,
      workItemId,
      system: config.template,
      prompt: buildPrompt(context, targets, workItemId ?? ''),
      interactive
    }
  }

  validateCreation(preparation: DescribePreparation, requested: boolean): void {
    if (requested && !preparation.interactive) {
      throw new Error('--create requer terminal interativo para confirmar a descrição e os revisores.')
    }
    if (requested && !preparation.context.isAzureDevOps) {
      throw new Error('--create requer um remote Git do Azure DevOps.')
    }
    if (requested && !preparation.config.azurePat.trim()) {
      throw new Error('--create requer AZURE_PAT ou AZURE_DEVOPS_PAT configurado.')
    }
  }

  generate(
    preparation: DescribePreparation,
    report: (provider: ProviderName, model: string) => void
  ): Promise<GeneratedDescription> {
    return this.generator.generate({
      config: preparation.config,
      system: preparation.system,
      prompt: preparation.prompt,
      branch: preparation.context.branch,
      report
    })
  }

  publish(
    preparation: DescribePreparation,
    description: PrDescription,
    reviewerForTarget?: (target: string) => string
  ) {
    return this.publisher.publish(
      preparation.config,
      preparation.context,
      preparation.targets,
      {
        title: description.title,
        description: description.body,
        workItemRefs: preparation.workItemId ? [{ id: preparation.workItemId }] : undefined
      },
      reviewerForTarget
    )
  }
}
