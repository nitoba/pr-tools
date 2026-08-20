import { AzureDevOpsClient } from './client'
import { createPullRequest, getRepository } from './pull-requests'
import type { AzurePullRequest, CreatePullRequestInput } from './types'
import type { GitContext } from '../types'

export type PublishedPullRequest = {
  target: string
  pullRequest: AzurePullRequest
}

export async function publishPullRequests(
  client: AzureDevOpsClient,
  context: GitContext,
  targets: string[],
  input: Pick<CreatePullRequestInput, 'title' | 'description' | 'workItemRefs'>,
  reviewerForTarget?: (target: string) => string
): Promise<PublishedPullRequest[]> {
  const repository = await getRepository(client, context.azureProject, context.azureRepo)
  if (typeof repository.id !== 'string' || !repository.id.trim())
    throw new Error('Azure DevOps não retornou o ID do repositório.')
  const results: PublishedPullRequest[] = []

  for (const target of targets) {
    const reviewer = (reviewerForTarget?.(target) ?? '').trim()
    const pullRequest = await createPullRequest(client, context.azureProject, repository.id, {
      title: input.title,
      description: input.description,
      sourceRefName: `refs/heads/${context.branch}`,
      targetRefName: `refs/heads/${target}`,
      ...(reviewer ? { reviewers: [{ uniqueName: reviewer }] } : {}),
      ...(input.workItemRefs?.length ? { workItemRefs: input.workItemRefs } : {})
    })
    results.push({ target, pullRequest })
  }

  return results
}
