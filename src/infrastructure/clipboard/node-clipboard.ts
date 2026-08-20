import { execFileSync } from 'node:child_process'
import type { Clipboard } from '../../app/ports'

export class NodeClipboard implements Clipboard {
  copy(value: string): boolean {
    const commands: string[][] = [
      ['pbcopy'],
      ['wl-copy'],
      ['xclip', '-selection', 'clipboard'],
      ['xsel', '--clipboard', '--input']
    ]
    for (const command of commands) {
      const executable = command[0]
      if (!executable) continue
      try {
        execFileSync(executable, command.slice(1), {
          encoding: 'utf8',
          input: value,
          stdio: ['pipe', 'ignore', 'ignore']
        })
        return true
      } catch {
        // Clipboard opcional; tenta o próximo comando nativo.
      }
    }
    return false
  }
}
