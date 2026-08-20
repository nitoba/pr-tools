import { execFileSync } from 'node:child_process'
import type { GitContext } from './types'

export function azurePrUrl(context: GitContext, target: string): string {
  return `https://dev.azure.com/${encodeURIComponent(context.azureOrg)}/${encodeURIComponent(context.azureProject)}/_git/${encodeURIComponent(context.azureRepo)}/pullrequestcreate?sourceRef=refs/heads/${encodeURIComponent(context.branch)}&targetRef=refs/heads/${encodeURIComponent(target)}`
}

export function azurePullRequestUrl(context: GitContext, pullRequestId: number): string {
  return `https://dev.azure.com/${encodeURIComponent(context.azureOrg)}/${encodeURIComponent(context.azureProject)}/_git/${encodeURIComponent(context.azureRepo)}/pullrequest/${pullRequestId}`
}

export function azureWorkItemUrl(context: GitContext, workItemId: string): string {
  return `https://dev.azure.com/${encodeURIComponent(context.azureOrg)}/${encodeURIComponent(context.azureProject)}/_workitems/edit/${encodeURIComponent(workItemId)}`
}

export function copyToClipboard(value: string): boolean {
  const commands: string[][] = [
    ['pbcopy'],
    ['wl-copy'],
    ['xclip', '-selection', 'clipboard'],
    ['xsel', '--clipboard', '--input']
  ]
  for (const command of commands) {
    try {
      const executable = command[0]
      if (!executable) continue
      execFileSync(executable, command.slice(1), {
        encoding: 'utf8',
        input: value,
        stdio: ['pipe', 'ignore', 'ignore']
      })
      return true
    } catch {
      // Clipboard is optional; continue with the next native command.
    }
  }
  return false
}
