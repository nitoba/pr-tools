import { styleText } from 'node:util'

export * from './cli'
export * from './azure'
export * from './infrastructure/git/git-context-service'
export * from './features/describe'
export * from './features/test-card'
export * from './app/cli.models'
export * from './app/version'
export * from './infrastructure/ai/ai.models'
export * from './infrastructure/ai/ai-description-generator'
export * from './infrastructure/config/config.models'
export * from './infrastructure/config/config-service'
export * from './infrastructure/config/config-validation'
export * from './features/doctor'
export * from './features/init'
export * from './shared/azure/azure-urls'
export * from './shared/validation/work-item'

import { main } from './cli'
import { setFormatter } from './infrastructure/terminal/terminal-style'

setFormatter((format, message) => styleText(format as Parameters<typeof styleText>[0], message))

if (import.meta.main) {
  void main().then((code) => {
    process.exit(code)
  })
}
