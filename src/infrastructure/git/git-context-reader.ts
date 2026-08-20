import type { GitContext } from './git-context.models'

export interface GitContextReader {
  collect(sourceBranch?: string): GitContext
}
