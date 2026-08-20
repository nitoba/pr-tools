import { color } from '../../infrastructure/terminal/terminal-style'
import type { TerminalOutput } from '../../infrastructure/terminal/terminal-ports'
import type { DoctorReport, DoctorCheck } from './doctor.models'

export class DoctorPresenter {
  constructor(private readonly output: TerminalOutput) {}

  show(report: DoctorReport): void {
    this.output.write(`${color('magenta', '┌ prt · doctor')}\n│`)
    for (const check of report.checks) this.showCheck(check)
    if (report.failures > 0) this.output.write(color('magenta', `└ Doctor encontrou ${report.failures} falha(s) e ${report.warnings} aviso(s).`))
    else if (report.warnings > 0) this.output.write(color('magenta', `└ Doctor concluído com ${report.warnings} aviso(s).`))
    else this.output.write(color('magenta', '└ Doctor concluído: todos os componentes estão prontos.'))
  }

  private showCheck(check: DoctorCheck): void {
    const message = `${check.component}: ${check.detail}`
    if (check.status === 'ok') this.output.write(color('green', `✓ OK · ${message}`))
    if (check.status === 'warn') this.output.write(color('yellow', `⚠ AVISO · ${message}`))
    if (check.status === 'fail') this.output.writeError(color('red', `✗ FALHA · ${message}`))
    if (check.fix) this.output.write(color('cyan', `•   Como resolver: ${check.fix}`))
  }
}
