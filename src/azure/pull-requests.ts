import { AzureDevOpsClient, pathSegment, withApiVersion } from './client'
import type {
  AzurePullRequest,
  AzurePullRequestChange,
  AzurePullRequestIteration,
  AzureRepository,
  CreatePullRequestInput
} from './types'

export class AzurePullRequestClient {
  constructor(private readonly client: AzureDevOpsClient) {}

  async getRepository(project: string, repository: string, signal?: AbortSignal): Promise<AzureRepository> {
    return (await this.client.request(
      withApiVersion(`/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}`),
      { signal }
    )) as AzureRepository
  }

  async create(
    project: string,
    repository: string,
    request: CreatePullRequestInput,
    signal?: AbortSignal
  ): Promise<AzurePullRequest> {
    return (await this.client.request(
      withApiVersion(
        `/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullrequests`
      ),
      { method: 'POST', body: request, signal }
    )) as AzurePullRequest
  }

  async get(
    project: string,
    repository: string,
    pullRequestId: number,
    signal?: AbortSignal
  ): Promise<AzurePullRequest> {
    return (await this.client.request(
      withApiVersion(
        `/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullRequests/${pullRequestId}`
      ),
      { signal }
    )) as AzurePullRequest
  }

  async linkedWorkItemIds(
    project: string,
    repository: string,
    pullRequestId: number,
    signal?: AbortSignal
  ): Promise<number[]> {
    const result = (await this.client.request(
      withApiVersion(
        `/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullRequests/${pullRequestId}/workitems`
      ),
      { signal }
    )) as { value: Array<{ id: string | number }> }
    return result.value.map((item) => {
      const id = typeof item.id === 'number' ? item.id : Number(item.id)
      if (!Number.isInteger(id)) throw new Error(`ID de work item inválido: ${String(item.id)}`)
      return id
    })
  }

  async iterations(
    project: string,
    repository: string,
    pullRequestId: number,
    signal?: AbortSignal
  ): Promise<AzurePullRequestIteration[]> {
    const result = (await this.client.request(
      withApiVersion(
        `/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullRequests/${pullRequestId}/iterations`,
        '7.0'
      ),
      { signal }
    )) as { value: AzurePullRequestIteration[] }
    return result.value
  }

  async changes(
    project: string,
    repository: string,
    pullRequestId: number,
    iterationId: number,
    signal?: AbortSignal
  ): Promise<AzurePullRequestChange[]> {
    const result = (await this.client.request(
      withApiVersion(
        `/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullRequests/${pullRequestId}/iterations/${iterationId}/changes?$top=200`,
        '7.0'
      ),
      { signal }
    )) as { changeEntries: AzurePullRequestChange[] }
    return result.changeEntries.slice(0, 50)
  }
}
