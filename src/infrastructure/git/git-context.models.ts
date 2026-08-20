export type GitContext = {
  branch: string
  sourceRef: string
  baseBranch: string
  sprintBranch: string
  diff: string
  diffOriginalLines: number
  log: string
  workItemId: string
  isAzureDevOps: boolean
  azureOrg: string
  azureProject: string
  azureRepo: string
}
