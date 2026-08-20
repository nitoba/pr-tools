import { spawnSync } from 'node:child_process'
import type { ProcessResult, ProcessRunner } from './process-runner'

export class NodeProcessRunner implements ProcessRunner {
  run(command: string, args: string[], timeoutMs?: number): ProcessResult {
    const result =
      timeoutMs === undefined
        ? spawnSync(command, [...args], { encoding: 'utf8' })
        : spawnSync(command, [...args], { encoding: 'utf8', timeout: timeoutMs })
    const stdout = (result.stdout ?? '').trim()
    const stderr = (result.stderr ?? '').trim()
    return {
      exitCode: result.status ?? 1,
      stdout,
      stderr,
      error: result.error?.message
    }
  }
}
