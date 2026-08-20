import { parseWorkItemId } from '../../shared/validation/work-item'
import type {
  DescriptionGenerator,
  GitContextReader,
  TestCardRepository,
  TestCardRepositoryFactory
} from '../../app/ports'
import type { ConfigService } from '../../infrastructure/config/config-service'
import type { CliOptions } from '../../app/cli.models'
import type { ProviderName } from '../../infrastructure/ai/ai.models'
import type { GitContext } from '../../infrastructure/git/git-context.models'
import type { AzurePullRequestChange, AzureWorkItem } from '../../azure'
import {
  buildTestCardPrompt,
  selectParentWorkItem,
  workItemText,
  TEST_CARD_SYSTEM_PROMPT,
  type TestCardContext,
  type TestCardPreparation
} from './test-card.models'
import { parseExamplesCount } from './test-card.validation'

export class TestCardService {
  constructor(
    private readonly config: ConfigService,
    private readonly git: GitContextReader,
    private readonly repositories: TestCardRepositoryFactory,
    private readonly generator: DescriptionGenerator
  ) {}

  async prepare(
    options: CliOptions,
    interactive: boolean,
    askForWorkItem: () => Promise<string | undefined>,
    report: (message: string) => void,
    onContext: (context: GitContext) => void
  ): Promise<TestCardPreparation> {
    const config = this.config.load(options)
    if (!config.azurePat.trim())
      throw new Error('O comando test requer AZURE_PAT ou AZURE_DEVOPS_PAT configurado.')
    if (options.create && !interactive)
      throw new Error('--create requer terminal interativo para confirmar o card.')

    const git = this.git.collect(options.source)
    onContext(git)
    if (!git.isAzureDevOps) throw new Error('O comando test requer um remote Git do Azure DevOps.')
    const repository = this.repositories.create(config, git)
    const pullRequestId = parseWorkItemId(options.pr, '--pr')
    const pullRequest = pullRequestId
      ? await repository.getPullRequest(git.azureProject, git.azureRepo, pullRequestId)
      : undefined

    let workItemId = parseWorkItemId(options.workItem, '--work-item')
    if (!workItemId && git.workItemId) workItemId = parseWorkItemId(git.workItemId, 'branch')
    if (!workItemId && pullRequest) {
      report('Resolvendo Work Item vinculado ao PR')
      const linkedIds = await repository.getPullRequestWorkItemIds(
        git.azureProject,
        git.azureRepo,
        pullRequest.pullRequestId
      )
      const linkedItems = await this.loadLinkedItems(repository, git.azureProject, linkedIds)
      workItemId = selectParentWorkItem(linkedItems)
    }
    if (!workItemId && interactive) {
      const value = await askForWorkItem()
      if (typeof value !== 'string') throw new Error('Operação cancelada.')
      workItemId = parseWorkItemId(value, 'Work Item')
    }
    if (!workItemId)
      throw new Error('Não foi possível resolver o Work Item pai; use --work-item explicitamente.')

    report(`Buscando Work Item #${workItemId}`)
    const workItem = await repository.getWorkItem(git.azureProject, workItemId)
    const changes = await this.loadChanges(repository, git, pullRequest)
    const examples = await this.loadExamples(repository, git.azureProject, options.examples)
    const context: TestCardContext = { git, workItem, pullRequest, changes, examples }
    return { config, context, repository, prompt: buildTestCardPrompt(context) }
  }

  generate(
    preparation: TestCardPreparation,
    report: (provider: ProviderName, model: string) => void
  ) {
    return this.generator.generate({
      config: preparation.config,
      system: TEST_CARD_SYSTEM_PROMPT,
      prompt: preparation.prompt,
      branch: `test-card/${preparation.context.workItem.id}`,
      report
    })
  }

  create(
    preparation: TestCardPreparation,
    input: Parameters<TestCardRepository['createTestCase']>[1]
  ) {
    return preparation.repository.createTestCase(preparation.context.git.azureProject, input)
  }

  updateParent(
    preparation: TestCardPreparation,
    effort?: number,
    realEffort?: number
  ): Promise<void> {
    return preparation.repository.updateWorkItemToTestQA(
      preparation.context.git.azureProject,
      preparation.context.workItem.id,
      effort,
      realEffort
    )
  }

  private async loadLinkedItems(
    repository: TestCardRepository,
    project: string,
    ids: number[]
  ) {
    const items: AzureWorkItem[] = []
    for (const id of ids) {
      try {
        items.push(await repository.getWorkItem(project, id))
      } catch {
        // Um link antigo ou sem permissão não bloqueia os demais.
      }
    }
    return items
  }

  private async loadChanges(
    repository: TestCardRepository,
    git: GitContext,
    pullRequest: TestCardContext['pullRequest']
  ): Promise<AzurePullRequestChange[]> {
    if (!pullRequest) return new Array<AzurePullRequestChange>()
    try {
      return await repository.getPullRequestChanges(git.azureProject, git.azureRepo, pullRequest)
    } catch {
      return new Array<AzurePullRequestChange>()
    }
  }

  private async loadExamples(
    repository: TestCardRepository,
    project: string,
    value: string | undefined
  ): Promise<string[]> {
    const count = parseExamplesCount(value)
    if (count === 0) return new Array<string>()
    try {
      const ids = await repository.queryTestCaseIds(project)
      const examples: string[] = []
      for (const id of ids.slice(0, count)) {
        try {
          const item = await repository.getWorkItem(project, id)
          const title = workItemText(item, 'System.Title')
          if (title) examples.push(`- #${id} ${title}`)
        } catch {
          // Um exemplo inacessível não impede a geração do card.
        }
      }
      return examples
    } catch {
      return new Array<string>()
    }
  }
}
