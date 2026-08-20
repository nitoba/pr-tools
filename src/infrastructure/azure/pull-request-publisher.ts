import type { AzurePullRequest, CreatePullRequestInput } from '../../azure'
import type { Config } from '../config/config.models'
import type { GitContext } from '../git/git-context.models'

export interface PullRequestPublisher {
  publish(
    config: Config,
    context: GitContext,
    targets: string[],
    input: Pick<CreatePullRequestInput, 'title' | 'description' | 'workItemRefs'>,
    reviewerForTarget?: (target: string) => string
  ): Promise<Array<{ target: string; pullRequest: AzurePullRequest }>>
}
