import type { GitContext } from './git-context.models'
import type { GitClient } from './git-client'
import type { GitContextReader } from '../../app/ports'

const MAX_DIFF_LINES = 8000

export class GitContextService implements GitContextReader {
  constructor(private readonly git: GitClient) {}

  latestSprintBranch(): string {
    const branches = this.git.run(['branch', '-r']).stdout.split('\n')
    const sprints: Array<{ branch: string; number: number }> = []
    for (const line of branches) {
      const branch = line.trim().replace(/^origin\//, '')
      const match = /^sprint\/(\d+)(?:$|[-/].*)/.exec(branch)
      const numberText = match ? match[1] : undefined
      if (match && numberText) sprints.push({ branch: match[0], number: Number(numberText) })
    }
    sprints.sort((left, right) => right.number - left.number)
    return sprints.at(0)?.branch ?? ''
  }

  collect(sourceBranch?: string): GitContext {
    const currentBranch = this.gitOutput(['branch', '--show-current'], 'Não é um repositório git')
    const branch = sourceBranch ?? currentBranch
    if (!branch) throw new Error('Branch não determinada (detached HEAD). Use --source.')
    if (['dev', 'main', 'master'].includes(branch))
      throw new Error(`A branch de origem (${branch}) é uma branch base.`)
    const sourceRef = this.resolveRef(branch)
    if (!sourceRef) throw new Error(`Branch '${branch}' não encontrada localmente ou em origin.`)

    const sprintBranch = this.latestSprintBranch()
    const baseBranch = this.detectBaseBranch(sprintBranch)
    const diffAttempts: string[][] = [
      ['diff', `${baseBranch}...${sourceRef}`],
      ['diff', `${baseBranch}..${sourceRef}`],
      ['diff', baseBranch, sourceRef]
    ]
    let diff = ''
    for (const attempt of diffAttempts) {
      const result = this.git.run(attempt)
      if (result.ok && result.stdout) {
        diff = result.stdout
        break
      }
    }
    if (!diff) throw new Error(`Nenhuma alteração encontrada em relação a ${baseBranch}.`)
    const diffLines = diff.split('\n')
    const diffOriginalLines = diffLines.length
    if (diffLines.length > MAX_DIFF_LINES) {
      diff = `${diffLines.slice(0, MAX_DIFF_LINES).join('\n')}\n\n[diff truncado: ${diffOriginalLines} -> ${MAX_DIFF_LINES} linhas]`
    }

    const logResult = this.git.run([
      'log',
      `${baseBranch}...${sourceRef}`,
      '--oneline',
      '--max-count=50'
    ])
    const azure = parseAzureRemote(this.git.run(['remote', 'get-url', 'origin']).stdout)
    return {
      branch,
      sourceRef,
      baseBranch,
      sprintBranch,
      diff,
      diffOriginalLines,
      log: logResult.ok ? logResult.stdout : '(log não disponível)',
      workItemId: branchWorkItem(branch),
      isAzureDevOps: azure.isAzureDevOps,
      azureOrg: azure.azureOrg,
      azureProject: azure.azureProject,
      azureRepo: azure.azureRepo
    }
  }

  private detectBaseBranch(sprintBranch: string): string {
    for (const candidate of [sprintBranch, 'dev', 'main', 'master']) {
      const resolved = candidate && this.resolveRef(candidate)
      if (resolved) return resolved
    }
    throw new Error('Branch base não encontrada. Esperado dev, main, master ou sprint/<número>.')
  }

  private resolveRef(branch: string): string | undefined {
    if (this.git.run(['rev-parse', '--verify', branch]).ok) return branch
    if (this.git.run(['rev-parse', '--verify', `origin/${branch}`]).ok) return `origin/${branch}`
    return undefined
  }

  private gitOutput(args: string[], errorMessage: string): string {
    const result = this.git.run(args)
    if (!result.ok) throw new Error(`${errorMessage}${result.stderr ? `: ${result.stderr}` : ''}`)
    return result.stdout
  }
}

function branchWorkItem(branch: string): string {
  const match = /(?:^|[/_-])(\d+)(?:$|[/_-])/.exec(branch)
  return match ? match[1] ?? '' : ''
}

export function parseAzureRemote(
  remote: string
): Pick<GitContext, 'isAzureDevOps' | 'azureOrg' | 'azureProject' | 'azureRepo'> {
  const normalized = remote.replace(/\.git$/, '')
  const ssh = /(?:^|@)ssh\.dev\.azure\.com:v3\/([^/]+)\/([^/]+)\/([^/]+)/i.exec(normalized)
  if (ssh) {
    const org = ssh[1] ?? ''
    const project = ssh[2] ?? ''
    const repo = ssh[3] ?? ''
    if (!org || !project || !repo) return nonAzureRemote()
    return {
      isAzureDevOps: true,
      azureOrg: decodeURIComponent(org),
      azureProject: decodeURIComponent(project),
      azureRepo: decodeURIComponent(repo)
    }
  }
  const modern = /dev\.azure\.com\/([^/]+)\/([^/]+)\/_git\/([^/]+)/i.exec(normalized)
  if (modern) {
    const org = modern[1] ?? ''
    const project = modern[2] ?? ''
    const repo = modern[3] ?? ''
    if (!org || !project || !repo) return nonAzureRemote()
    return {
      isAzureDevOps: true,
      azureOrg: decodeURIComponent(org),
      azureProject: decodeURIComponent(project),
      azureRepo: decodeURIComponent(repo)
    }
  }
  const legacy = /([^/]+)\.visualstudio\.com\/([^/]+)\/_git\/([^/]+)/i.exec(normalized)
  if (legacy) {
    const org = legacy[1] ?? ''
    const project = legacy[2] ?? ''
    const repo = legacy[3] ?? ''
    if (!org || !project || !repo) return nonAzureRemote()
    return {
      isAzureDevOps: true,
      azureOrg: org,
      azureProject: decodeURIComponent(project),
      azureRepo: decodeURIComponent(repo)
    }
  }
  return nonAzureRemote()
}

function nonAzureRemote(): Pick<GitContext, 'isAzureDevOps' | 'azureOrg' | 'azureProject' | 'azureRepo'> {
  return { isAzureDevOps: false, azureOrg: '', azureProject: '', azureRepo: '' }
}

export function resolveTargets(context: GitContext, requested: string[]): string[] {
  if (requested.length > 0) {
    return requested
      .map((target) => (target === 'sprint' ? context.sprintBranch : target))
      .filter(Boolean)
  }
  return [context.sprintBranch, context.baseBranch.replace(/^origin\//, '')]
    .filter(Boolean)
    .filter((target, index, targets) => targets.indexOf(target) === index)
}
