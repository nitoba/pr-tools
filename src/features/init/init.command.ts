import type { CliOptions } from '../../app/cli.models'
import type { ConfigService } from '../../infrastructure/config/config-service'
import type { TerminalOutput } from '../../app/ports'
import { color } from '../../infrastructure/terminal/terminal-style'

export class InitCommand {
  constructor(
    private readonly config: ConfigService,
    private readonly output: TerminalOutput,
    private readonly interactive: boolean
  ) {}

  async execute(_options: CliOptions): Promise<number> {
    if (this.interactive) this.output.write(`${color('magenta', '┌ prt · configuração')}\n│`)
    const result = await this.config.initialize()
    if (!result) {
      this.output.write(color('yellow', '✗ Operação cancelada.'))
      return 0
    }
    if (result.interactive) {
      const patStatus = result.azurePatConfigured
        ? `PAT Azure salvo em ${result.paths.envFile} (AZURE_PAT)`
        : 'PAT Azure não configurado'
      this.output.write(
        `◆ prt\nConfiguração salva em ${result.paths.configFile}\n${patStatus}\nTemplate salvo em ${result.paths.templateFile}`
      )
      this.output.write(color('magenta', '└ Pronto. Execute `prt desc --dry-run`.'))
    } else {
      this.output.write(`Configuração salva em ${result.paths.configFile}`)
    }
    return 0
  }
}
