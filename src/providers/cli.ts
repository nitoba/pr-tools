import { spawnSync } from 'node:child_process'

export function runCli(command: string, args: string[]): string {
  const result = spawnSync(command, args, { encoding: 'utf8' })
  if (result.error) throw new Error(`${command} falhou: ${result.error.message}`)
  if (result.status !== 0) {
    const stderr = result.stderr.trim()
    throw new Error(`${command} falhou: ${stderr || `código ${result.status ?? 'desconhecido'}`}`)
  }
  const text = result.stdout
  if (!text.trim()) throw new Error(`${command} não retornou texto.`)
  return text
}
