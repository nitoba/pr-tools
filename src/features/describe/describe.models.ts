import type { Config } from '../../infrastructure/config/config.models'
import type { GitContext } from '../../infrastructure/git/git-context.models'
import type { PrDescription } from '../../infrastructure/ai/ai.models'

export type DescribePreparation = {
  config: Config
  context: GitContext
  targets: string[]
  workItemId?: string
  system: string
  prompt: string
  interactive: boolean
}

export type GeneratedDescription = {
  description: PrDescription
  provider: string
  model: string
}
