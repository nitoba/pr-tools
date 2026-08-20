import { expect, test } from 'bun:test'
import { AiDescriptionGenerator } from './ai-description-generator'
import type { ProcessRunner } from '../process/process-runner'
import type { Config } from '../config/config.models'

const config: Config = {
  providers: ['codex'],
  baseUrl: 'https://api.openai.com/v1',
  compatibleModel: 'compatible',
  compatibleReasoning: 'provider-default',
  codexModel: 'codex-model',
  codexReasoning: 'high',
  opencodeModel: 'opencode-model',
  opencodeReasoning: 'provider-default',
  azurePat: '',
  reviewerDev: '',
  reviewerSprint: '',
  testAreaPath: '',
  testAssignedTo: '',
  testTeam: 'DevOps',
  testProgram: 'Agrotrace',
  apiKey: '',
  template: 'template'
}

test('AiDescriptionGenerator uses the injected process runner', async () => {
  let command = ''
  let args: string[] = []
  const processes: ProcessRunner = {
    run(name, values) {
      command = name
      args = values
      return { exitCode: 0, stdout: '{"title":"Ajusta login","body":"## Descrição"}', stderr: '' }
    }
  }
  const reports: string[] = []
  const generator = new AiDescriptionGenerator(processes)
  const result = await generator.generate({
    config,
    system: 'system',
    prompt: 'prompt',
    branch: 'feature/1-login',
    report: (provider, model) => reports.push(`${provider}/${model}`)
  })

  expect(command).toBe('codex')
  expect(args).toContain('codex-model')
  expect(reports).toEqual(['codex/codex-model'])
  expect(result.description).toEqual({ title: 'Ajusta login', body: '## Descrição' })
})
