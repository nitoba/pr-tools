import type {
  AzurePullRequest,
  AzurePullRequestChange,
  AzureWorkItem,
  CreatePullRequestInput,
  CreateTestCaseInput
} from '../azure'
import type { ProviderName, PrDescription } from '../infrastructure/ai/ai.models'
import type { Config } from '../infrastructure/config/config.models'
import type { GitContext } from '../infrastructure/git/git-context.models'

export interface TerminalOutput {
  write(message: string): void
  writeError(message: string): void
}

export interface ProgressReporter {
  start(message: string): void
  message(message: string): void
  stop(message: string): void
  error(message: string): void
}

export interface PromptPort {
  text(options: {
    message: string
    initialValue?: string
    defaultValue?: string
    placeholder?: string
    validate?: (value: string | undefined) => string | undefined
  }): Promise<string | undefined>
  password(options: {
    message: string
    validate?: (value: string | undefined) => string | undefined
  }): Promise<string | undefined>
  select(options: {
    message: string
    options: Array<{ value: string; label: string; hint?: string; disabled?: boolean }>
    initialValue?: string
  }): Promise<string | undefined>
  confirm(options: { message: string; initialValue?: boolean }): Promise<boolean | undefined>
}

export interface Clipboard {
  copy(value: string): boolean
}

export interface HttpFetcher {
  fetch(input: string, init?: RequestInit): Promise<Response>
}

export interface DescriptionGenerator {
  generate(input: {
    config: Config
    system: string
    prompt: string
    branch: string
    report: (provider: ProviderName, model: string) => void
  }): Promise<{ description: PrDescription; provider: ProviderName; model: string }>
}

export interface PullRequestPublisher {
  publish(
    config: Config,
    context: GitContext,
    targets: string[],
    input: Pick<CreatePullRequestInput, 'title' | 'description' | 'workItemRefs'>,
    reviewerForTarget?: (target: string) => string
  ): Promise<Array<{ target: string; pullRequest: AzurePullRequest }>>
}

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

export interface GitContextReader {
  collect(sourceBranch?: string): GitContext
}
