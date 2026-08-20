import { createOpenAICompatible } from '@ai-sdk/openai-compatible'
import { generateText, Output } from 'ai'
import { z } from 'zod'
import { generateCodex } from './providers/codex'
import { generateOpenCode } from './providers/opencode'
import type { Config, PrDescription, ProviderName } from './types'

export const DESCRIPTION_SCHEMA = z.object({
  title: z.string().describe('PR title, at most 80 characters'),
  body: z.string().describe('Markdown PR description')
})

function normalizeBaseUrl(value: string): string {
  const url = value.replace(/\/$/, '')
  return url.endsWith('/chat/completions') ? url.slice(0, -'/chat/completions'.length) : url
}

async function generateOpenAICompatible(
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

function stripThinkBlocks(value: string): string {
  return value
    .replace(/<think>[\s\S]*?<\/think>/gi, '')
    .replace(/<\/?think>/gi, '')
    .trim()
}

export function normalizeDescription(output: unknown, text: string, branch: string): PrDescription {
  if (isDescription(output)) return cleanDescription(output)
  const cleanText = stripThinkBlocks(text)
  const jsonText = cleanText.replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '')
  try {
    const parsed: unknown = JSON.parse(jsonText)
    if (isDescription(parsed)) return cleanDescription(parsed)
  } catch {
    // Keep compatibility with models that ignore the JSON response format.
  }
  const titleMatch = /^\s*T[IÍ]TULO\s*:\s*(.+)$/im.exec(cleanText)
  const title =
    titleMatch?.[1]?.trim() || cleanText.split('\n').find(Boolean)?.slice(0, 80) || branch
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

function isDescription(value: unknown): value is PrDescription {
  if (!value || typeof value !== 'object') return false
  const candidate = value as { title?: string; body?: string }
  return typeof candidate.title === 'string' && typeof candidate.body === 'string'
}

export async function generateDescription(
  config: Config,
  system: string,
  prompt: string,
  branch: string,
  report: (provider: ProviderName, model: string) => void
): Promise<{ description: PrDescription; provider: ProviderName; model: string }> {
  const errors: string[] = []
  for (const provider of config.providers) {
    const model =
      provider === 'codex'
        ? config.codexModel
        : provider === 'opencode'
          ? config.opencodeModel
          : config.compatibleModel
    report(provider, model)
    try {
      const description =
        provider === 'codex'
          ? normalizeDescription(undefined, generateCodex(config, system, prompt), branch)
          : provider === 'opencode'
            ? normalizeDescription(undefined, generateOpenCode(config, system, prompt), branch)
            : await generateOpenAICompatible(config, system, prompt, branch)
      return { description, provider, model }
    } catch (error) {
      errors.push(`${provider}: ${error instanceof Error ? error.message : String(error)}`)
    }
  }
  throw new Error(`Todos os providers falharam:\n${errors.map((error) => `  ${error}`).join('\n')}`)
}
