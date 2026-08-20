import type { ProviderName, PrDescription } from './ai.models'
import type { Config } from '../config/config.models'

export interface DescriptionGenerator {
  generate(input: {
    config: Config
    system: string
    prompt: string
    branch: string
    report: (provider: ProviderName, model: string) => void
  }): Promise<{ description: PrDescription; provider: ProviderName; model: string }>
}
