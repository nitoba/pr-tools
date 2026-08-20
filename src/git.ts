import { spawnSync } from 'node:child_process'
import { MAX_DIFF_LINES } from './prompt'
import type { GitContext } from './types'

type GitResult = {
  ok: boolean
  stdout: string
  stderr: string
}

function runGit(args: string[]): GitResult {
  const result = spawnSync('git', args, { encoding: 'utf8' })
  return {
    ok: result.status === 0,
    stdout: result.stdout.trim(),
    stderr: result.stderr.trim()
  }
}

function gitOutput(args: string[], errorMessage: string): string {
  const result = runGit(args)
  if (!result.ok) throw new Error(`${errorMessage}${result.stderr ? `: ${result.stderr}` : ''}`)
  return result.stdout
}

function resolveRef(branch: string): string | undefined {
  if (runGit(['rev-parse', '--verify', branch]).ok) return branch
  if (runGit(['rev-parse', '--verify', `origin/${branch}`]).ok) return `origin/${branch}`
  return undefined
}

export function latestSprintBranch(): string {
  const branches = runGit(['branch', '-r']).stdout.split('\n')
  const sprints: Array<{ branch: string; number: number }> = []
  for (const line of branches) {
    const branch = line.trim().replace(/^origin\//, '')
    const match = /^sprint\/(\d+)(?:$|[-/].*)/.exec(branch)
    const numberText = match?.[1]
    if (match && numberText) sprints.push({ branch: match[0], number: Number(numberText) })
  }
  sprints.sort((left, right) => right.number - left.number)
  return sprints[0]?.branch ?? ''
}

function detectBaseBranch(sprintBranch: string): string {
  for (const candidate of [sprintBranch, 'dev', 'main', 'master']) {
    const resolved = candidate && resolveRef(candidate)
    if (resolved) return resolved
  }
  throw new Error('Branch base não encontrada. Esperado dev, main, master ou sprint/<número>.')
}

function branchWorkItem(branch: string): string {
  return /(?:^|[/_-])(\d+)(?:$|[/_-])/.exec(branch)?.[1] ?? ''
}

export function parseAzureRemote(
  remote: string
): Pick<GitContext, 'isAzureDevOps' | 'azureOrg' | 'azureProject' | 'azureRepo'> {
  const normalized = remote.replace(/\.git$/, '')
  const ssh = /(?:^|@)ssh\.dev\.azure\.com:v3\/([^/]+)\/([^/]+)\/([^/]+)/i.exec(normalized)
  if (ssh) {
    const org = ssh[1]
    const project = ssh[2]
    const repo = ssh[3]
    if (!org || !project || !repo)
      return { isAzureDevOps: false, azureOrg: '', azureProject: '', azureRepo: '' }
    return {
      isAzureDevOps: true,
      azureOrg: decodeURIComponent(org),
      azureProject: decodeURIComponent(project),
      azureRepo: decodeURIComponent(repo)
    }
  }
  const modern = /dev\.azure\.com\/([^/]+)\/([^/]+)\/_git\/([^/]+)/i.exec(normalized)
  if (modern) {
    const org = modern[1]
    const project = modern[2]
    const repo = modern[3]
    if (!org || !project || !repo)
      return { isAzureDevOps: false, azureOrg: '', azureProject: '', azureRepo: '' }
    return {
      isAzureDevOps: true,
      azureOrg: decodeURIComponent(org),
      azureProject: decodeURIComponent(project),
      azureRepo: decodeURIComponent(repo)
    }
  }
  const legacy = /([^/]+)\.visualstudio\.com\/([^/]+)\/_git\/([^/]+)/i.exec(normalized)
  if (legacy) {
    const org = legacy[1]
    const project = legacy[2]
    const repo = legacy[3]
    if (!org || !project || !repo)
      return { isAzureDevOps: false, azureOrg: '', azureProject: '', azureRepo: '' }
    return {
      isAzureDevOps: true,
      azureOrg: org,
      azureProject: decodeURIComponent(project),
      azureRepo: decodeURIComponent(repo)
    }
  }
  return { isAzureDevOps: false, azureOrg: '', azureProject: '', azureRepo: '' }
}

export function collectGitContext(sourceBranch?: string): GitContext {
  const currentBranch = gitOutput(['branch', '--show-current'], 'Não é um repositório git')
  const branch = sourceBranch ?? currentBranch
  if (!branch) throw new Error('Branch não determinada (detached HEAD). Use --source.')
  if (['dev', 'main', 'master'].includes(branch))
    throw new Error(`A branch de origem (${branch}) é uma branch base.`)
  const sourceRef = resolveRef(branch)
  if (!sourceRef) throw new Error(`Branch '${branch}' não encontrada localmente ou em origin.`)

  const sprintBranch = latestSprintBranch()
  const baseBranch = detectBaseBranch(sprintBranch)
  const diffAttempts: string[][] = [
    ['diff', `${baseBranch}...${sourceRef}`],
    ['diff', `${baseBranch}..${sourceRef}`],
    ['diff', baseBranch, sourceRef]
  ]
  let diff = ''
  for (const attempt of diffAttempts) {
    const result = runGit(attempt)
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

  const logResult = runGit(['log', `${baseBranch}...${sourceRef}`, '--oneline', '--max-count=50'])
  const azure = parseAzureRemote(runGit(['remote', 'get-url', 'origin']).stdout)
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
