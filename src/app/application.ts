import type { CliOptions } from './cli.models'
import type { DescribeCommand } from '../features/describe'
import type { TestCardCommand } from '../features/test-card'
import type { DoctorCommand } from '../features/doctor'
import type { InitCommand } from '../features/init'

export class Application {
  constructor(
    readonly describe: DescribeCommand,
    readonly testCard: TestCardCommand,
    readonly init: InitCommand,
    readonly doctor: DoctorCommand
  ) {}

  run(options: CliOptions): Promise<number> {
    if (options.command === 'init') return this.init.execute(options)
    if (options.command === 'test') return this.testCard.execute(options)
    if (options.command === 'doctor') return this.doctor.execute(options)
    return this.describe.execute(options)
  }
}
