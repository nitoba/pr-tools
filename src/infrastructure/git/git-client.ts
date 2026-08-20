import { spawnSync } from 'node:child_process'

export type GitCommandResult = {
  ok: boolean
  stdout: string
  stderr: string
}

export interface GitClient {
  run(args: string[]): GitCommandResult
}

export class NodeGitClient implements GitClient {
  run(args: string[]): GitCommandResult {
    const result = spawnSync('git', [...args], { encoding: 'utf8' })
    return {
      ok: result.status === 0,
      stdout: result.stdout.trim(),
      stderr: result.stderr.trim()
    }
  }
}
