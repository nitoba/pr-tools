import { azurePrUrl, azurePullRequestUrl, azureWorkItemUrl } from '../../shared/azure/azure-urls'
import { color } from '../../infrastructure/terminal/terminal-style'
import type { TerminalOutput } from '../../app/ports'
import type { CliOptions } from '../../app/cli.models'
import type { DescribePreparation, GeneratedDescription } from './describe.models'

export class DescribePresenter {
  constructor(private readonly output: TerminalOutput) {}

  showDryRun(preparation: DescribePreparation): void {
    const models = preparation.config.providers
      .map((provider) => {
        const model =
          provider === 'codex'
            ? preparation.config.codexModel
            : provider === 'opencode'
              ? preparation.config.opencodeModel
              : preparation.config.compatibleModel
        const reasoning =
          provider === 'codex'
            ? preparation.config.codexReasoning
            : provider === 'opencode'
              ? preparation.config.opencodeReasoning
              : preparation.config.compatibleReasoning
        return `${provider}/${model} (thinking ${reasoning})`
      })
      .join(', ')
    this.output.write(`Provider/model: ${models}`)
    this.output.write('\n[SYSTEM]\n')
    this.output.write(preparation.system)
    this.output.write('\n[USER]\n')
    this.output.write(preparation.prompt)
  }

  showDescription(
    preparation: DescribePreparation,
    generated: GeneratedDescription,
    options: CliOptions
  ): void {
    const { title, body } = generated.description
    if (options.raw) {
      this.output.write(body)
      return
    }
    this.output.write(
      `\n${color(['bold', 'cyan'], 'Título')}: ${title}\n\n${color(['bold', 'cyan'], 'Descrição')}:\n${body}`
    )
    this.output.write(`\n${color('blue', 'Branch')}: ${preparation.context.branch}`)
    this.output.write(`${color('blue', 'Targets')}: ${preparation.targets.join(', ')}`)
    if (preparation.workItemId)
      this.output.write(`${color('blue', 'Work Item')}: #${preparation.workItemId}`)
    if (preparation.context.isAzureDevOps) {
      if (preparation.workItemId) {
        this.output.write(
          `${color('blue', 'Work Item URL')}: ${azureWorkItemUrl(preparation.context, preparation.workItemId)}`
        )
      }
      for (const target of preparation.targets) {
        this.output.write(
          `${color('blue', `PR ${target}`)}: ${azurePrUrl(preparation.context, target)}`
        )
      }
    }
  }

  showPublished(
    preparation: DescribePreparation,
    published: Array<{ target: string; pullRequest: { webUrl?: string; pullRequestId: number; url?: string } }>
  ): void {
    for (const { target, pullRequest } of published) {
      const url =
        pullRequest.webUrl ??
        (pullRequest.pullRequestId
          ? azurePullRequestUrl(preparation.context, pullRequest.pullRequestId)
          : (pullRequest.url ?? azurePrUrl(preparation.context, target)))
      this.success(`PR ${target} criado: ${url}`)
    }
  }

  intro(branch: string): void {
    this.output.write(`${color('magenta', `┌ prt · PR ${branch}`)}\n│`)
  }

  outro(message: string): void {
    this.output.write(color('magenta', `└ ${message}`))
  }

  success(message: string): void {
    this.output.write(color('green', `✓ ${message}`))
  }

  info(message: string): void {
    this.output.write(color('cyan', `• ${message}`))
  }
}
