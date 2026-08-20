import type { TerminalOutput } from '../../app/ports'

export class NodeTerminalOutput implements TerminalOutput {
  write(message: string): void {
    console.log(message)
  }

  writeError(message: string): void {
    console.error(message)
  }
}
