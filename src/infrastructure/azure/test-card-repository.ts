import type {
  AzurePullRequest,
  AzurePullRequestChange,
  AzureWorkItem,
  CreateTestCaseInput
} from '../../azure'
import type { Config } from '../config/config.models'
import type { GitContext } from '../git/git-context.models'

export interface TestCardRepository {
  getPullRequest(project: string, repository: string, id: number): Promise<AzurePullRequest>
  getPullRequestWorkItemIds(project: string, repository: string, id: number): Promise<number[]>
  getWorkItem(project: string, id: number): Promise<AzureWorkItem>
  getPullRequestChanges(
    project: string,
    repository: string,
    pullRequest: AzurePullRequest
  ): Promise<AzurePullRequestChange[]>
  queryTestCaseIds(project: string): Promise<number[]>
  createTestCase(project: string, input: CreateTestCaseInput): Promise<AzureWorkItem>
  updateWorkItemToTestQA(
    project: string,
    id: number,
    effort?: number,
    realEffort?: number
  ): Promise<void>
}

export interface TestCardRepositoryFactory {
  create(config: Config, context: GitContext): TestCardRepository
}
