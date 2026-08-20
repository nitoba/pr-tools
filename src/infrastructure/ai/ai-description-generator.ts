import { createOpenAICompatible } from '@ai-sdk/openai-compatible'
import { generateText, Output } from 'ai'
import { writeFileSync, unlinkSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { z } from 'zod'
import type { DescriptionGenerator } from './description-generator'
import type { Config } from '../config/config.models'
import type { PrDescription, ProviderName } from './ai.models'
import type { ProcessRunner } from '../process/process-runner'

export const DESCRIPTION_SCHEMA = z.object({
  title: z.string().describe('PR title, at most 80 characters'),
  body: z.string().describe('Markdown PR description')
})

export class AiDescriptionGenerator implements DescriptionGenerator {
  constructor(private readonly processes: ProcessRunner) {}

  async generate(input: Parameters<DescriptionGenerator['generate']>[0]) {
    const errors: string[] = []
    for (const provider of input.config.providers) {
      const model = this.modelFor(input.config, provider)
      input.report(provider, model)
      try {
        const description =
          provider === 'codex'
            ? normalizeDescription(undefined, this.runCodex(input.config, input.system, input.prompt), input.branch)
            : provider === 'opencode'
              ? normalizeDescription(undefined, this.runOpenCode(input.config, input.system, input.prompt), input.branch)
              : await this.runOpenAICompatible(input.config, input.system, input.prompt, input.branch)
        return { description, provider, model }
      } catch (error) {
        errors.push(`${provider}: ${error instanceof Error ? error.message : String(error)}`)
      }
    }
    throw new Error(`Todos os providers falharam:\n${errors.map((error) => `  ${error}`).join('\n')}`)
  }

  private modelFor(config: Config, provider: ProviderName): string {
    if (provider === 'codex') return config.codexModel
    if (provider === 'opencode') return config.opencodeModel
    return config.compatibleModel
  }

  private runCodex(config: Config, system: string, prompt: string): string {
    const args = [
      'exec',
      '-m',
      config.codexModel,
      '-c',
      'approval_policy=never',
      '-c',
      'sandbox_mode=read-only',
      '--skip-git-repo-check',
      '--color',
      'never'
    ]
    if (config.codexReasoning !== 'provider-default')
      args.push('-c', `model_reasoning_effort=${config.codexReasoning}`)
    args.push(`${system}\n\n${prompt}`)
    const result = this.processes.run('codex', args, undefined)
    if (result.error || result.exitCode !== 0)
      throw new Error(`codex falhou: ${result.error ?? (result.stderr || `código ${result.exitCode}`)}`)
    if (!result.stdout.trim()) throw new Error('codex não retornou texto.')
    return result.stdout
  }

  private runOpenCode(config: Config, system: string, prompt: string): string {
    const promptPath = join(tmpdir(), `prt-opencode-${Date.now()}.md`)
    writeFileSync(promptPath, `${system}\n\n${prompt}`)
    const args = [
      'run',
      '--format',
      'default',
      '--pure',
      '--agent',
      'general',
      '--model',
      config.opencodeModel,
      '--file',
      promptPath,
      'Gere o JSON solicitado usando o arquivo anexado. Não execute ferramentas.'
    ]
    if (config.opencodeReasoning !== 'provider-default')
      args.push('--variant', config.opencodeReasoning)
    try {
      const result = this.processes.run('opencode', args, undefined)
      if (result.error || result.exitCode !== 0)
        throw new Error(`opencode falhou: ${result.error ?? (result.stderr || `código ${result.exitCode}`)}`)
      if (!result.stdout.trim()) throw new Error('opencode não retornou texto.')
      return result.stdout
    } finally {
      unlinkSync(promptPath)
    }
  }

  private async runOpenAICompatible(
    config: Config,
    system: string,
    prompt: string,
    branch: string
  ): Promise<PrDescription> {
    const provider = createOpenAICompatible<string, string, string, string>({
      name: 'pr-tools-openai-compatible',
      baseURL: normalizeBaseUrl(config.baseUrl),
      apiKey: config.apiKey || undefined,
      supportsStructuredOutputs: true
    })
    try {
      const result = await generateText({
        model: provider(config.compatibleModel),
        instructions: system,
        prompt,
        output: Output.object({ schema: DESCRIPTION_SCHEMA }),
        reasoning: config.compatibleReasoning,
        maxRetries: 1
      })
      return normalizeDescription(result.output, result.text, branch)
    } catch (structuredError) {
      const result = await generateText({
        model: provider(config.compatibleModel),
        instructions: `${system}\n\nResponda com JSON contendo title e body.`,
        prompt,
        reasoning: config.compatibleReasoning,
        maxRetries: 1
      })
      if (!result.text.trim()) throw structuredError
      return normalizeDescription(undefined, result.text, branch)
    }
  }
}

export function normalizeDescription(output: unknown, text: string, branch: string): PrDescription {
  if (isDescription(output)) return cleanDescription(output)
  const cleanText = stripThinkBlocks(text)
  const jsonText = cleanText.replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '')
  try {
    const parsed: unknown = JSON.parse(jsonText)
    if (isDescription(parsed)) return cleanDescription(parsed)
  } catch {
    // Compatibilidade com providers que ignoram o formato JSON solicitado.
  }
  const titleMatch = /^\s*T[IÍ]TULO\s*:\s*(.+)$/im.exec(cleanText)
  const titleFromMarker = titleMatch ? titleMatch[1] ?? '' : ''
  const title =
    titleFromMarker.trim() || cleanText.split('\n').find(Boolean)?.slice(0, 80) || branch
  const body = titleMatch
    ? cleanText.slice(cleanText.indexOf(titleMatch[0]) + titleMatch[0].length).trim()
    : cleanText
  return cleanDescription({ title, body })
}

export function cleanDescription(description: PrDescription): PrDescription {
  const title = description.title
    .replace(/^['"]|['"]$/g, '')
    .trim()
    .slice(0, 80)
  const body = description.body.replace(/^\s*---\s*/u, '').trim()
  return { title: title || 'Atualiza código', body: body || 'Sem descrição gerada.' }
}

function normalizeBaseUrl(value: string): string {
  const url = value.replace(/\/$/, '')
  return url.endsWith('/chat/completions') ? url.slice(0, -'/chat/completions'.length) : url
}

function stripThinkBlocks(value: string): string {
  return value
    .replace(/<think>[\s\S]*?<\/think>/gi, '')
    .replace(/<\/?think>/gi, '')
    .trim()
}

function isDescription(value: unknown): value is PrDescription {
  if (!value || typeof value !== 'object') return false
  const candidate = value as { title?: string; body?: string }
  return typeof candidate.title === 'string' && typeof candidate.body === 'string'
}
