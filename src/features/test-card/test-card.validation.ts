import { z } from 'zod'

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

export function validatePositiveDecimal(value: string | undefined): string | undefined {
  const parsed = positiveDecimalPromptSchema.safeParse(value ?? '')
  return parsed.success ? undefined : 'Informe um número positivo.'
}

export function validateNonNegativeDecimal(value: string | undefined): string | undefined {
  const parsed = nonNegativeDecimalPromptSchema.safeParse(value ?? '')
  return parsed.success ? undefined : 'Informe um número válido.'
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

export function parseExamplesCount(value: string | undefined): number {
  if (value === undefined || value.trim() === '') return 2
  const parsed = examplesCountSchema.safeParse(value)
  if (!parsed.success) throw new Error('--examples deve ser um número entre 0 e 5.')
  return Number(parsed.data)
}
