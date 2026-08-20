export type ProcessResult = {
  exitCode: number
  stdout: string
  stderr: string
  error?: string
}

export interface ProcessRunner {
  run(command: string, args: string[], timeoutMs?: number): ProcessResult
}
