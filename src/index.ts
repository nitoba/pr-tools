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

if (import.meta.main) void main()
