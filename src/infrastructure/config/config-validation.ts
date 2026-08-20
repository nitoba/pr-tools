import { z } from 'zod'
import type { ReasoningLevel } from '../ai/ai.models'

export type PromptValidator = (value: string | undefined) => string | undefined

export const providerSchema = z.enum(['codex', 'opencode', 'openai-compatible'])
export const reasoningLevelSchema = z.enum([
  'provider-default',
  'none',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh'
])
export const reasoningLevelPromptSchema = reasoningLevelSchema

const emailSchema = z.string().trim().email('Informe um email válido.')
export const optionalEmailPromptSchema = z
  .string()
  .trim()
  .refine(
    (value) => value === '' || emailSchema.safeParse(value).success,
    'Informe um email válido ou deixe vazio.'
  )

function firstIssueMessage(error: { issues: ReadonlyArray<{ message: string }> }): string | undefined {
  return error.issues.at(0)?.message
}

export const validateOptionalEmail: PromptValidator = (value) => {
  const parsed = optionalEmailPromptSchema.safeParse(value ?? '')
  return parsed.success
    ? undefined
    : (firstIssueMessage(parsed.error) ?? 'Informe um email válido.')
}

export function parseReasoningLevel(value: unknown, fallback: ReasoningLevel): ReasoningLevel {
  if (value === undefined || value === null) return fallback
  if (typeof value === 'string' && value.trim() === '') return fallback
  const parsed = reasoningLevelSchema.safeParse(value)
  if (!parsed.success) {
    throw new Error(
      `Nível de thinking inválido (${typeof value}). Use provider-default, none, minimal, low, medium, high ou xhigh.`
    )
  }
  return parsed.data
}

export function parseProvider(value: string) {
  const parsed = providerSchema.safeParse(value)
  if (parsed.success) return parsed.data
  throw new Error(`Provider inválido: ${value}. Use codex, opencode ou openai-compatible.`)
}
