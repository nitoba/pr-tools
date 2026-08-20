import type { ProviderName } from '../infrastructure/ai/ai.models'

export type CliOptions = {
  command: 'desc' | 'test' | 'init' | 'doctor'
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
