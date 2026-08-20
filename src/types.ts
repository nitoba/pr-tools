export type ProviderName = 'codex' | 'opencode' | 'openai-compatible'

export type PrDescription = {
  title: string
  body: string
}

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

export type Config = {
  providers: ProviderName[]
  baseUrl: string
  compatibleModel: string
  codexModel: string
  opencodeModel: string
  azurePat: string
  reviewerDev: string
  reviewerSprint: string
  testAreaPath: string
  testAssignedTo: string
  testTeam: string
  testProgram: string
  apiKey: string
  template: string
}

export type CliOptions = {
  command: 'desc' | 'test' | 'init'
  source?: string
  targets: string[]
  workItem?: string
  provider?: ProviderName
  model?: string
  baseUrl?: string
  apiKey?: string
  create: boolean
  noCreate: boolean
  pr?: string
  areaPath?: string
  assignedTo?: string
  iterationPath?: string
  priority?: string
  team?: string
  program?: string
  examples?: string
  dryRun: boolean
  raw: boolean
  copy: boolean
}
