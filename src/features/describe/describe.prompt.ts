import type { GitContext } from '../../infrastructure/git/git-context.models'

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
