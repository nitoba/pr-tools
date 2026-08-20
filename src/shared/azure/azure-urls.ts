import type { GitContext } from '../../infrastructure/git/git-context.models'

export function azurePrUrl(context: GitContext, target: string): string {
  return `https://dev.azure.com/${encodeURIComponent(context.azureOrg)}/${encodeURIComponent(context.azureProject)}/_git/${encodeURIComponent(context.azureRepo)}/pullrequestcreate?sourceRef=refs/heads/${encodeURIComponent(context.branch)}&targetRef=refs/heads/${encodeURIComponent(target)}`
}

export function azurePullRequestUrl(context: GitContext, pullRequestId: number): string {
  return `https://dev.azure.com/${encodeURIComponent(context.azureOrg)}/${encodeURIComponent(context.azureProject)}/_git/${encodeURIComponent(context.azureRepo)}/pullrequest/${pullRequestId}`
}

export function azureWorkItemUrl(context: GitContext, workItemId: string): string {
  return `https://dev.azure.com/${encodeURIComponent(context.azureOrg)}/${encodeURIComponent(context.azureProject)}/_workitems/edit/${encodeURIComponent(workItemId)}`
}
