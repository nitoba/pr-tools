import type { GitContext, ReasoningLevel } from './types'

export const VERSION = '3.0.0'
export const MAX_DIFF_LINES = 8000
export const CODEX_MODEL = 'gpt-5.6-luna'
export const CODEX_REASONING_EFFORT: ReasoningLevel = 'high'
export const OPENCODE_MODEL = 'openai/gpt-5.5'
export const OPENCODE_REASONING_EFFORT: ReasoningLevel = 'provider-default'
export const DEFAULT_BASE_URL = 'https://api.openai.com/v1'
export const DEFAULT_COMPATIBLE_MODEL = 'gpt-4o-mini'
export const COMPATIBLE_REASONING_EFFORT: ReasoningLevel = 'provider-default'

export const DEFAULT_TEMPLATE = `Analise o diff e o log do git fornecidos e gere uma descrição de pull request em português brasileiro.

Retorne um objeto JSON com exatamente estes campos:
- "title": título curto, técnico e descritivo, com no máximo 80 caracteres.
- "body": descrição em Markdown.

O body deve seguir este formato:

## Descrição

Resumo conciso em 1 ou 2 frases do que mudou e por quê.

## Alterações

Liste componentes ou arquivos relevantes e descreva a mudança funcional.

## Tipo de mudança

- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Refactoring

Não invente alterações que não estejam no diff.`

export function buildPrompt(context: GitContext, targets: string[], workItemId: string): string {
  return `## Contexto Git

**Branch:** ${context.branch}
**Base branches alvo:** ${targets.join(', ')}
${workItemId ? `**Work Item:** #${workItemId}\n` : ''}
### Git Log (commits desde a base)

\`\`\`
${context.log}
\`\`\`

### Git Diff

\`\`\`diff
${context.diff}
\`\`\`
`
}
