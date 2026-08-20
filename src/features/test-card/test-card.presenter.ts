import { color } from '../../infrastructure/terminal/terminal-style'
import type { TerminalOutput } from '../../app/ports'
import type { Config } from '../../infrastructure/config/config.models'
import {
  TEST_CARD_SYSTEM_PROMPT,
  workItemText,
  type TestCardPreparation
} from './test-card.models'

export class TestCardPresenter {
  constructor(private readonly output: TerminalOutput) {}

  intro(branch: string): void {
    this.output.write(`${color('magenta', `┌ prt · Test Case ${branch}`)}\n│`)
  }

  outro(message: string): void {
    this.output.write(color('magenta', `└ ${message}`))
  }

  showDryRun(config: Config, prompt: string): void {
    const models = config.providers
      .map((provider) => {
        const model =
          provider === 'codex'
            ? config.codexModel
            : provider === 'opencode'
              ? config.opencodeModel
              : config.compatibleModel
        const reasoning =
          provider === 'codex'
            ? config.codexReasoning
            : provider === 'opencode'
              ? config.opencodeReasoning
              : config.compatibleReasoning
        return `${provider}/${model} (thinking ${reasoning})`
      })
      .join(', ')
    this.output.write(`Provider/model: ${models}`)
    this.output.write('\n[SYSTEM]\n')
    this.output.write(TEST_CARD_SYSTEM_PROMPT)
    this.output.write('\n[USER]\n')
    this.output.write(prompt)
  }

  showSummary(
    preparation: TestCardPreparation,
    provider: string,
    model: string,
    title: string,
    body: string,
    options: { areaPath?: string; assignedTo?: string }
  ): void {
    const { context, config } = preparation
    this.output.write(`\nTest Card${context.pullRequest ? ` · PR #${context.pullRequest.pullRequestId}` : ''}`)
    this.output.write(`Provider: ${provider}/${model}`)
    this.output.write(
      `Work Item: #${context.workItem.id} — ${workItemText(context.workItem, 'System.Title')}`
    )
    this.output.write(`AreaPath: ${(options.areaPath ?? config.testAreaPath) || '(não configurado)'}`)
    const assignedTo = options.assignedTo ?? config.testAssignedTo
    if (assignedTo) this.output.write(`Responsável: ${assignedTo}`)
    this.output.write(`\nTítulo: ${title}\n\n${body}`)
  }

  raw(body: string): void {
    this.output.write(body)
  }

  success(message: string): void {
    this.output.write(color('green', `✓ ${message}`))
  }

  info(message: string): void {
    this.output.write(color('cyan', `• ${message}`))
  }
}
