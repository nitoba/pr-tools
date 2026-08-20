export type ProviderName = 'codex' | 'opencode' | 'openai-compatible'

export type ReasoningLevel =
  | 'provider-default'
  | 'none'
  | 'minimal'
  | 'low'
  | 'medium'
  | 'high'
  | 'xhigh'

export type PrDescription = {
  title: string
  body: string
}
