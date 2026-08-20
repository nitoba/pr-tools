import { styleText } from 'node:util'

export * from './cli'
export * from './azure'
export * from './config'
export * from './doctor'
export * from './git'
export * from './llm'
export * from './output'
export * from './prompt'
export * from './test-card'
export * from './types'

import { main } from './cli'
import { setFormatter } from './ui'

setFormatter((format, message) => styleText(format as Parameters<typeof styleText>[0], message))

if (import.meta.main) void main()
