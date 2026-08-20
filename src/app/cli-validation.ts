import { z } from 'zod'
import { providerSchema } from '../infrastructure/config/config-validation'
import type { ProviderName } from '../infrastructure/ai/ai.models'
import type { CliOptions } from './cli.models'

const commands = ['desc', 'test', 'init', 'doctor'] as const
type CliCommand = (typeof commands)[number]
const commandValueSchema = z.enum(commands)

const commandSchema = z.array(z.string()).transform((positionals, context): CliCommand => {
  if (positionals.length > 1) {
    context.addIssue({
      code: 'custom',
      message: `Argumentos posicionais inesperados: ${positionals.slice(1).join(' ')}`
    })
    return '' as CliCommand
  }

  const requested = positionals.at(0) ?? 'desc'
  const command = commandValueSchema.safeParse(requested)
  if (!command.success) {
    context.addIssue({ code: 'custom', message: `Comando desconhecido: ${requested}` })
    return '' as CliCommand
  }
  return command.data
})

const providerInputSchema = z.string().transform((value, context): ProviderName => {
  const provider = providerSchema.safeParse(value)
  if (!provider.success) {
    context.addIssue({
      code: 'custom',
      message: `Provider inválido: ${value}. Use codex, opencode ou openai-compatible.`
    })
    return '' as ProviderName
  }
  return provider.data
})

const targetSchema = z.string().superRefine((target, context) => {
  const valid = target === 'dev' || target === 'sprint' || target.startsWith('sprint/')
  if (!valid) {
    context.addIssue({
      code: 'custom',
      message: `Target inválido: ${target}. Use dev, sprint ou sprint/<número>.`
    })
  }
})

const cliSyntaxSchema = z
  .object({
    positionals: commandSchema,
    provider: providerInputSchema.optional(),
    create: z.boolean().default(false),
    'no-create': z.boolean().default(false),
    target: z.array(targetSchema).default([])
  })
  .superRefine((input, context) => {
    if (input.create && input['no-create']) {
      context.addIssue({
        code: 'custom',
        path: ['create'],
        message: '--create e --no-create não podem ser usados juntos.'
      })
    }
  })

export type CliSyntaxInput = {
  positionals: string[]
  provider?: string
  target?: string[]
  create?: boolean
  noCreate?: boolean
}

export type CliSyntax = Pick<CliOptions, 'command' | 'provider' | 'targets' | 'create' | 'noCreate'>

export function validateCliSyntax(input: CliSyntaxInput): CliSyntax {
  const result = cliSyntaxSchema.safeParse({
    positionals: input.positionals,
    provider: input.provider,
    create: input.create,
    'no-create': input.noCreate,
    target: input.target
  })
  if (!result.success) {
    const issue = result.error.issues.at(0)
    if (!issue) throw new Error('Argumentos inválidos.')
    throw new Error(issue.message)
  }
  return {
    command: result.data.positionals,
    provider: result.data.provider,
    targets: result.data.target,
    create: result.data.create,
    noCreate: result.data['no-create']
  }
}
