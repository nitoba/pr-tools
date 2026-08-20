import { describe, expect, test } from 'bun:test'
import { parseArgs } from '../src/cli'
import { parseAzureRemote } from '../src/infrastructure/git/git-context-service'
import { normalizeDescription } from '../src/infrastructure/ai/ai-description-generator'
import { buildCreateTestCaseInput, selectParentWorkItem } from '../src/features/test-card'
import {
  optionalEmailPromptSchema,
  parseReasoningLevel,
  validateOptionalEmail
} from '../src/infrastructure/config/config-validation'
import { parseWorkItemId, validateWorkItemId } from '../src/shared/validation/work-item'
import {
  parseExamplesCount,
  parsePositiveDecimal,
  validatePositiveDecimal
} from '../src/features/test-card/test-card.validation'
import type { AzureWorkItem } from '../src/azure'

describe('pr-tools core parsing', () => {
  test('uses the native CLI parser for repeated targets', () => {
    expect(
      parseArgs(['desc', '--provider', 'codex', '--target', 'dev', '--target', 'sprint'])
    ).toMatchObject({
      command: 'desc',
      provider: 'codex',
      targets: ['dev', 'sprint']
    })
  })

  test('accepts explicit Azure PR creation', () => {
    expect(parseArgs(['desc', '--create'])).toMatchObject({ create: true })
  })

  test('accepts the test card command and options', () => {
    expect(
      parseArgs([
        'test',
        '--work-item',
        '42',
        '--pr',
        '99',
        '--area-path',
        'Project\\QA',
        '--no-create'
      ])
    ).toMatchObject({
      command: 'test',
      workItem: '42',
      pr: '99',
      areaPath: 'Project\\QA',
      noCreate: true
    })
  })

  test('accepts the doctor command', () => {
    expect(parseArgs(['doctor'])).toMatchObject({ command: 'doctor' })
  })

  test('selects a non-Test Case parent before linked Test Cases', () => {
    const items: AzureWorkItem[] = [
      { id: 300, fields: { 'System.WorkItemType': 'Test Case' } },
      { id: 120, fields: { 'System.WorkItemType': 'Bug' } },
      { id: 80, fields: { 'System.WorkItemType': 'User Story' } }
    ]
    expect(selectParentWorkItem(items)).toBe(80)
  })

  test('maps generated content and Azure fields into a Test Case request', () => {
    expect(
      buildCreateTestCaseInput(
        {
          areaPath: 'Project\\QA',
          assignedTo: 'qa@example.com',
          iterationPath: 'Project\\Sprint 98',
          priority: 2,
          team: 'DevOps',
          program: 'Agrotrace'
        },
        42,
        'Teste login',
        '## Objetivo\nValidar login.'
      )
    ).toEqual({
      title: 'Teste login',
      descriptionHtml: '## Objetivo\nValidar login.',
      areaPath: 'Project\\QA',
      parentId: 42,
      iterationPath: 'Project\\Sprint 98',
      priority: 2,
      team: 'DevOps',
      program: 'Agrotrace',
      assignedTo: 'qa@example.com'
    })
  })

  test('accepts the local OpenCode provider', () => {
    expect(parseArgs(['desc', '--provider', 'opencode'])).toMatchObject({
      provider: 'opencode'
    })
  })

  test('validates CLI values with reusable Zod schemas', () => {
    expect(parseWorkItemId(' 42 ', 'Work Item')).toBe(42)
    expect(parseExamplesCount('5')).toBe(5)
    expect(parsePositiveDecimal('1,5', 2, '--priority')).toBe(1.5)
    expect(() => parseExamplesCount('6')).toThrow()
    expect(() => parsePositiveDecimal('0', 2, '--priority')).toThrow()
  })

  test('validates provider thinking levels and review emails', () => {
    expect(parseReasoningLevel('high', 'medium')).toBe('high')
    expect(parseReasoningLevel(undefined, 'provider-default')).toBe('provider-default')
    expect(optionalEmailPromptSchema.safeParse('reviewer@example.com').success).toBe(true)
    expect(optionalEmailPromptSchema.safeParse('').success).toBe(true)
    expect(optionalEmailPromptSchema.safeParse('not-an-email').success).toBe(false)
    expect(() => parseReasoningLevel('turbo', 'medium')).toThrow()
  })

  test('keeps native prompt validation messages', () => {
    expect(validateOptionalEmail('invalid')).toBe('Informe um email válido ou deixe vazio.')
    expect(validateOptionalEmail('')).toBeUndefined()
    expect(validatePositiveDecimal('0')).toBe('Informe um número positivo.')
    expect(validateWorkItemId('abc')).toBe('Use um ID numérico.')
  })

  test('extracts modern Azure DevOps remotes', () => {
    expect(parseAzureRemote('https://dev.azure.com/acme/My%20Project/_git/repo.git')).toEqual({
      isAzureDevOps: true,
      azureOrg: 'acme',
      azureProject: 'My Project',
      azureRepo: 'repo'
    })
  })

  test('extracts Azure DevOps SSH remotes', () => {
    expect(parseAzureRemote('git@ssh.dev.azure.com:v3/acme/My%20Project/repo.git')).toEqual({
      isAzureDevOps: true,
      azureOrg: 'acme',
      azureProject: 'My Project',
      azureRepo: 'repo'
    })
  })

  test('extracts legacy Azure DevOps remotes', () => {
    expect(parseAzureRemote('https://acme.visualstudio.com/My%20Project/_git/repo.git')).toEqual({
      isAzureDevOps: true,
      azureOrg: 'acme',
      azureProject: 'My Project',
      azureRepo: 'repo'
    })
  })

  test('keeps compatibility with the legacy model response format', () => {
    expect(
      normalizeDescription(
        undefined,
        'TÍTULO: Ajusta login\n\n## Descrição\nCorrige fluxo.',
        'feature/1-login'
      )
    ).toEqual({
      title: 'Ajusta login',
      body: '## Descrição\nCorrige fluxo.'
    })
  })

  test('normalizes JSON returned by local CLI providers', () => {
    expect(
      normalizeDescription(
        undefined,
        '{"title":"Adiciona filtro","body":"## Descrição\\nInclui CPF."}',
        'feature/1-filtro'
      )
    ).toEqual({
      title: 'Adiciona filtro',
      body: '## Descrição\nInclui CPF.'
    })
  })
})
