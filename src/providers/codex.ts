import { runCli } from './cli'
import type { Config } from '../types'

export function generateCodex(config: Config, system: string, prompt: string): string {
  const args = [
    'exec',
    '-m',
    config.codexModel,
    '-c',
    'approval_policy=never',
    '-c',
    'sandbox_mode=read-only',
    '--skip-git-repo-check',
    '--color',
    'never'
  ]
  if (config.codexReasoning !== 'provider-default')
    args.push('-c', `model_reasoning_effort=${config.codexReasoning}`)
  args.push(`${system}\n\n${prompt}`)

  return runCli('codex', args)
}
