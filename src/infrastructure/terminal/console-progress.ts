import { color } from './terminal-style'
import type { ProgressReporter } from '../../app/ports'

export class ConsoleProgress implements ProgressReporter {
  private active = false

  start(message: string): void {
    this.active = true
    console.log(color('cyan', `… ${message}`))
  }

  message(message: string): void {
    if (this.active) console.log(color('cyan', `… ${message}`))
  }

  stop(message: string): void {
    this.active = false
    console.log(color('green', `✓ ${message}`))
  }

  error(message: string): void {
    this.active = false
    console.error(color('red', `✗ ${message}`))
  }
}
