export type AzureRepository = {
  id: string
  name?: string
  remoteUrl?: string
  webUrl?: string
}

export type AzureIdentity = {
  id?: string
  uniqueName?: string
  displayName?: string
}

export type AzureResourceRef = {
  id: string
  url?: string
}

export type AzurePullRequest = {
  pullRequestId: number
  title: string
  description: string
  sourceRefName: string
  targetRefName: string
  url?: string
  webUrl?: string
  repository?: AzureRepository
  reviewers?: AzureIdentity[]
  workItemRefs?: AzureResourceRef[]
}

export type CreatePullRequestInput = {
  title: string
  description: string
  sourceRefName: string
  targetRefName: string
  reviewers?: AzureIdentity[]
  workItemRefs?: AzureResourceRef[]
}

export type AzureWorkItem = {
  id: number
  rev?: number
  url?: string
  fields: Record<string, string | number | undefined>
}

export type CreateTestCaseInput = {
  title: string
  descriptionHtml?: string
  stepsXml?: string
  areaPath?: string
  parentId?: number
  iterationPath?: string
  priority?: number
  team?: string
  program?: string
  assignedTo?: string
}

export type AzurePullRequestIteration = {
  id: number
}

export type AzurePullRequestChange = {
  changeType: string
  item: { path: string }
}
