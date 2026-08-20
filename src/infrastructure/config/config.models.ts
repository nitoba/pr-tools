import type { ProviderName, ReasoningLevel } from '../ai/ai.models'

export type Config = {
  providers: ProviderName[]
  baseUrl: string
  compatibleModel: string
  compatibleReasoning: ReasoningLevel
  codexModel: string
  codexReasoning: ReasoningLevel
  opencodeModel: string
  opencodeReasoning: ReasoningLevel
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
