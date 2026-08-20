import { AzurePullRequestClient } from '../../azure'
import type { AzureDevOpsClient } from '../../azure'
import type { CreatePullRequestInput } from '../../azure'
import type { PullRequestPublisher } from '../../app/ports'
import type { AzureClientFactory } from './azure-client-factory'

export class AzurePullRequestPublisher implements PullRequestPublisher {
  constructor(private readonly clients: AzureClientFactory) {}

  publish(
    config: Parameters<PullRequestPublisher['publish']>[0],
    context: Parameters<PullRequestPublisher['publish']>[1],
    targets: Parameters<PullRequestPublisher['publish']>[2],
    input: Parameters<PullRequestPublisher['publish']>[3],
    reviewerForTarget?: Parameters<PullRequestPublisher['publish']>[4]
  ): ReturnType<PullRequestPublisher['publish']> {
    return this.publishWithClient(
      this.clients.create(config, context),
      context,
      targets,
      input,
      reviewerForTarget
    )
  }

  private async publishWithClient(
    client: AzureDevOpsClient,
    context: Parameters<PullRequestPublisher['publish']>[1],
    targets: Parameters<PullRequestPublisher['publish']>[2],
    input: Pick<CreatePullRequestInput, 'title' | 'description' | 'workItemRefs'>,
    reviewerForTarget?: Parameters<PullRequestPublisher['publish']>[4]
  ): Promise<Array<{ target: string; pullRequest: Awaited<ReturnType<AzurePullRequestClient['create']>> }>> {
    const api = new AzurePullRequestClient(client)
    const repository = await api.getRepository(context.azureProject, context.azureRepo)
    if (typeof repository.id !== 'string' || !repository.id.trim())
      throw new Error('Azure DevOps não retornou o ID do repositório.')
    const results: Array<{
      target: string
      pullRequest: Awaited<ReturnType<AzurePullRequestClient['create']>>
    }> = []
    for (const target of targets) {
      const reviewer = (reviewerForTarget?.(target) ?? '').trim()
      const request: CreatePullRequestInput = {
        title: input.title,
        description: input.description,
        sourceRefName: `refs/heads/${context.branch}`,
        targetRefName: `refs/heads/${target}`,
        ...(reviewer ? { reviewers: [{ uniqueName: reviewer }] } : {}),
        ...(input.workItemRefs?.length ? { workItemRefs: input.workItemRefs } : {})
      }
      results.push({ target, pullRequest: await api.create(context.azureProject, repository.id, request) })
    }
    return results
  }
}
