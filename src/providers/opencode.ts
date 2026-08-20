import { unlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runCli } from './cli'
import type { Config } from '../types'

export function generateOpenCode(config: Config, system: string, prompt: string): string {
  const promptPath = join(tmpdir(), `prt-opencode-${Date.now()}.md`)
  writeFileSync(promptPath, `${system}\n\n${prompt}`)
  const args = [
    'run',
    '--format',
    'default',
    '--pure',
    '--agent',
    'general',
    '--model',
    config.opencodeModel,
    '--file',
    promptPath,
    'Gere o JSON solicitado usando o arquivo anexado. Não execute ferramentas.'
  ]
  if (config.opencodeReasoning !== 'provider-default')
    args.push('--variant', config.opencodeReasoning)

  try {
    return runCli('opencode', args)
  } finally {
    unlinkSync(promptPath)
  }
}
