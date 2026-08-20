import { createApplication } from './app/bootstrap'
import { CliExit } from './app/exit'
import { parseArgs } from './app/cli-parser'
import { color } from './infrastructure/terminal/terminal-style'

export { helpText, parseArgs } from './app/cli-parser'
export { Application } from './app/application'
export { DescribeCommand } from './features/describe'
export { InitCommand } from './features/init'
export { DoctorCommand } from './features/doctor'
export { createApplication } from './app/bootstrap'

export async function main(argv = process.argv.slice(2)): Promise<number> {
  try {
    const options = parseArgs(argv)
    return await createApplication().run(options)
  } catch (error) {
    if (error instanceof CliExit) {
      if (error.output) console.log(error.output)
      return error.code
    }
    console.error(color('red', `✗ ${error instanceof Error ? error.message : String(error)}`))
    return 1
  }
}
