import { z } from 'zod'
import type { ReasoningLevel } from './types'

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

export const workItemIdSchema = z
  .string()
  .trim()
  .regex(/^\d+$/u, 'Use um ID numérico.')
  .refine((value) => {
    const id = Number(value)
    return Number.isSafeInteger(id) && id > 0
  }, 'Use um ID positivo.')

export const workItemIdPromptSchema = workItemIdSchema.optional()

const decimalSchema = z
  .string()
  .trim()
  .refine((value) => Number.isFinite(Number(value.replace(',', '.'))), 'Informe um número válido.')

export const positiveDecimalSchema = decimalSchema.refine(
  (value) => Number(value.replace(',', '.')) > 0,
  'Informe um número positivo.'
)
export const positiveDecimalPromptSchema = positiveDecimalSchema.optional()

export const nonNegativeDecimalSchema = decimalSchema.refine(
  (value) => Number(value.replace(',', '.')) >= 0,
  'Informe um número maior ou igual a zero.'
)
export const nonNegativeDecimalPromptSchema = nonNegativeDecimalSchema.optional()

export const examplesCountSchema = z
  .string()
  .trim()
  .regex(/^\d+$/u, 'Use um número entre 0 e 5.')
  .refine(
    (value) => Number.isSafeInteger(Number(value)) && Number(value) <= 5,
    'Use um número entre 0 e 5.'
  )

export const apiKeyPromptSchema = z.string().optional()

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

export function parseWorkItemId(value: string | undefined, label: string): number | undefined {
  if (!value?.trim()) return undefined
  const parsed = workItemIdSchema.safeParse(value)
  if (!parsed.success)
    throw new Error(
      `${label} inválido: ${parsed.error.issues[0]?.message ?? 'Use um ID numérico.'}`
    )
  return Number(parsed.data)
}

export function parseExamplesCount(value: string | undefined): number {
  if (value === undefined || value.trim() === '') return 2
  const parsed = examplesCountSchema.safeParse(value)
  if (!parsed.success) throw new Error('--examples deve ser um número entre 0 e 5.')
  return Number(parsed.data)
}

export function parsePositiveDecimal(
  value: string | undefined,
  fallback: number,
  option: string
): number {
  if (value === undefined || value.trim() === '') return fallback
  const parsed = positiveDecimalSchema.safeParse(value)
  if (!parsed.success) throw new Error(`${option} deve ser um número positivo.`)
  return Number(parsed.data.replace(',', '.'))
}
