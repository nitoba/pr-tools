import type { CliOptions } from '../../app/cli.models'
import { DoctorPresenter } from './doctor.presenter'
import { DoctorService } from './doctor.service'

export class DoctorCommand {
  constructor(
    private readonly service: DoctorService,
    private readonly presenter: DoctorPresenter
  ) {}

  async execute(options: CliOptions): Promise<number> {
    const report = await this.service.inspect(options)
    this.presenter.show(report)
    return report.failures === 0 ? 0 : 1
  }
}
