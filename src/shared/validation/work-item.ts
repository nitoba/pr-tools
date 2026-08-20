import { z } from 'zod'

export const workItemIdSchema = z
  .string()
  .trim()
  .regex(/^\d+$/u, 'Use um ID numérico.')
  .refine((value) => {
    const id = Number(value)
    return Number.isSafeInteger(id) && id > 0
  }, 'Use um ID positivo.')

export const workItemIdPromptSchema = workItemIdSchema.optional()

function firstIssueMessage(error: { issues: ReadonlyArray<{ message: string }> }): string | undefined {
  return error.issues.at(0)?.message
}

export function validateWorkItemId(value: string | undefined): string | undefined {
  const parsed = workItemIdPromptSchema.safeParse(value ?? '')
  return parsed.success ? undefined : (firstIssueMessage(parsed.error) ?? 'Use um ID numérico.')
}

export function parseWorkItemId(value: string | undefined, label: string): number | undefined {
  if (!value?.trim()) return undefined
  const parsed = workItemIdSchema.safeParse(value)
  if (!parsed.success)
    throw new Error(
      `${label} inválido: ${firstIssueMessage(parsed.error) ?? 'Use um ID numérico.'}`
    )
  return Number(parsed.data)
}
