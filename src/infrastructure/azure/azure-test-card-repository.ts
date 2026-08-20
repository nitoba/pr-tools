import { AzurePullRequestClient, AzureWorkItemClient } from '../../azure'
import type { TestCardRepository, TestCardRepositoryFactory } from './test-card-repository'
import type { AzureClientFactory } from './azure-client-factory'
import type { Config } from '../config/config.models'
import type { GitContext } from '../git/git-context.models'
import type { CreateTestCaseInput } from '../../azure'
import type { AzurePullRequestChange } from '../../azure'

export class AzureTestCardRepository implements TestCardRepository {
  constructor(
    private readonly pullRequests: AzurePullRequestClient,
    private readonly workItems: AzureWorkItemClient
  ) {}

  getPullRequest(project: string, repository: string, id: number) {
    return this.pullRequests.get(project, repository, id)
  }

  getPullRequestWorkItemIds(project: string, repository: string, id: number) {
    return this.pullRequests.linkedWorkItemIds(project, repository, id)
  }

  getWorkItem(project: string, id: number) {
    return this.workItems.get(project, id)
  }

  async getPullRequestChanges(
    project: string,
    repository: string,
    pullRequest: Parameters<TestCardRepository['getPullRequestChanges']>[2]
  ) {
    const iterations = await this.pullRequests.iterations(
      project,
      repository,
      pullRequest.pullRequestId
    )
    const last = iterations.at(-1)
    if (!last) return new Array<AzurePullRequestChange>()
    return this.pullRequests.changes(project, repository, pullRequest.pullRequestId, last.id)
  }

  async queryTestCaseIds(project: string): Promise<number[]> {
    const escapedProject = project.replaceAll("'", "''")
    return this.workItems.query(
      project,
      `SELECT [System.Id],[System.Title] FROM WorkItems WHERE [System.WorkItemType]='Test Case' AND [System.TeamProject]='${escapedProject}' ORDER BY [System.ChangedDate] DESC`
    )
  }

  createTestCase(project: string, input: CreateTestCaseInput) {
    return this.workItems.createTestCase(project, input)
  }

  updateWorkItemToTestQA(project: string, id: number, effort?: number, realEffort?: number) {
    return this.workItems.updateToTestQA(project, id, effort, realEffort)
  }
}

export class AzureTestCardRepositoryFactory implements TestCardRepositoryFactory {
  constructor(private readonly clients: AzureClientFactory) {}

  create(config: Config, context: GitContext): TestCardRepository {
    const client = this.clients.create(config, context)
    const repository = new AzureTestCardRepository(
      new AzurePullRequestClient(client),
      new AzureWorkItemClient(client)
    )
    return {
      getPullRequest: (project, repositoryName, id) =>
        repository.getPullRequest(project, repositoryName, id),
      getPullRequestWorkItemIds: (project, repositoryName, id) =>
        repository.getPullRequestWorkItemIds(project, repositoryName, id),
      getWorkItem: (project, id) => repository.getWorkItem(project, id),
      getPullRequestChanges: (project, repositoryName, pullRequest) =>
        repository.getPullRequestChanges(project, repositoryName, pullRequest),
      queryTestCaseIds: (project) => repository.queryTestCaseIds(project),
      createTestCase: (project, input) => repository.createTestCase(project, input),
      updateWorkItemToTestQA: (project, id, effort, realEffort) =>
        repository.updateWorkItemToTestQA(project, id, effort, realEffort)
    }
  }
}
