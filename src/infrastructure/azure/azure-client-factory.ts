import { AzureDevOpsClient } from '../../azure'
import type { Config } from '../config/config.models'
import type { GitContext } from '../git/git-context.models'

export class AzureClientFactory {
  create(config: Config, context: GitContext): AzureDevOpsClient {
    return this.createForOrganization(config, context.azureOrg)
  }

  createForOrganization(config: Config, organization: string): AzureDevOpsClient {
    return new AzureDevOpsClient({
      pat: config.azurePat,
      organization
    })
  }
}
