import { describe, expect, test } from 'bun:test'
import { Buffer } from 'node:buffer'
import { AzureDevOpsClient } from './client'
import { AzureWorkItemClient } from './work-items'
import { AzurePullRequestPublisher } from '../infrastructure/azure/azure-pull-request-publisher'
import type { GitContext } from '../infrastructure/git/git-context.models'

const context: GitContext = {
  branch: 'feature/123-login',
  sourceRef: 'feature/123-login',
  baseBranch: 'dev',
  sprintBranch: 'sprint/98',
  diff: '',
  diffOriginalLines: 0,
  log: '',
  workItemId: '123',
  isAzureDevOps: true,
  azureOrg: 'acme',
  azureProject: 'My Project',
  azureRepo: 'repo'
}

describe('Azure DevOps REST client', () => {
  test('publishes PRs with PAT auth, refs, reviewer and work item', async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = []
    const client = new AzureDevOpsClient({
      pat: 'test-pat',
      organization: 'acme',
      baseUrl: 'https://azure.test',
      fetcher: async (input, init) => {
        const url = input
        requests.push({ url, init })
        if (url.includes('/repositories/repo?')) return Response.json({ id: 'repo-id' })
        return Response.json({
          pullRequestId: requests.length,
          title: 'A title',
          description: 'A body',
          sourceRefName: 'refs/heads/feature/123-login',
          targetRefName: 'refs/heads/dev',
          url: 'https://azure.test/pr/1'
        })
      }
    })

    const published = await new AzurePullRequestPublisher({
      create: () => client,
      createForOrganization: () => client
    }).publish(
      {} as never,
      context,
      ['dev'],
      { title: 'A title', description: 'A body', workItemRefs: [{ id: '123' }] },
      () => 'reviewer@example.com'
    )
    const result = published[0]
    if (!result) throw new Error('PR não publicado no teste')

    expect(result.pullRequest.pullRequestId).toBe(2)
    expect(requests).toHaveLength(2)
    expect(requests[0]?.url).toBe(
      'https://azure.test/My%20Project/_apis/git/repositories/repo?api-version=7.1'
    )
    expect(requests[1]?.url).toBe(
      'https://azure.test/My%20Project/_apis/git/repositories/repo-id/pullrequests?api-version=7.1'
    )
    expect(new Headers(requests[1]?.init?.headers).get('Authorization')).toBe(
      `Basic ${Buffer.from(':test-pat').toString('base64')}`
    )
    const requestBody = requests[1]?.init?.body
    const bodyText = typeof requestBody === 'string' ? requestBody : JSON.stringify(requestBody)
    expect(JSON.parse(bodyText ?? '{}')).toMatchObject({
      sourceRefName: 'refs/heads/feature/123-login',
      targetRefName: 'refs/heads/dev',
      reviewers: [{ uniqueName: 'reviewer@example.com' }],
      workItemRefs: [{ id: '123' }]
    })
  })

  test('surfaces Azure API status and response body', async () => {
    const client = new AzureDevOpsClient({
      pat: 'test-pat',
      organization: 'acme',
      fetcher: async () => new Response('{"message":"denied"}', { status: 403 })
    })

    try {
      await client.request('/projects')
      throw new Error('A chamada deveria falhar')
    } catch (error) {
      expect(error).toMatchObject({ status: 403, responseBody: '{"message":"denied"}' })
    }
  })

  test('creates a Test Case with JSON Patch and parent relation', async () => {
    let requestUrl = ''
    let requestMethod = ''
    let requestContentType = ''
    let requestBody: unknown
    const client = new AzureDevOpsClient({
      pat: 'test-pat',
      organization: 'acme',
      baseUrl: 'https://azure.test',
      fetcher: async (input, init) => {
        requestUrl = input
        requestMethod = init?.method ?? ''
        requestContentType = new Headers(init?.headers).get('Content-Type') ?? ''
        const body = init?.body
        requestBody = JSON.parse(typeof body === 'string' ? body : JSON.stringify(body))
        return Response.json({ id: 99, fields: { 'System.Title': 'Teste login' } })
      }
    })

    const created = await new AzureWorkItemClient(client).createTestCase('My Project', {
      title: 'Teste login',
      descriptionHtml: '<p>Validar login.</p>',
      areaPath: 'Project\\QA',
      parentId: 42,
      iterationPath: 'Project\\Sprint 98',
      priority: 2,
      team: 'DevOps',
      program: 'Agrotrace',
      assignedTo: 'qa@example.com'
    })

    expect(created.id).toBe(99)
    expect(requestMethod).toBe('POST')
    expect(requestContentType).toBe('application/json-patch+json')
    expect(requestUrl).toBe(
      'https://azure.test/My%20Project/_apis/wit/workitems/%24Test%20Case?api-version=7.1'
    )
    expect(requestBody).toEqual([
      { op: 'add', path: '/fields/System.Title', value: 'Teste login' },
      { op: 'add', path: '/fields/System.Description', value: '<p>Validar login.</p>' },
      { op: 'add', path: '/fields/System.AreaPath', value: 'Project\\QA' },
      {
        op: 'add',
        path: '/fields/System.IterationPath',
        value: 'Project\\Sprint 98'
      },
      { op: 'add', path: '/fields/Microsoft.VSTS.Common.Priority', value: 2 },
      { op: 'add', path: '/fields/Custom.Team', value: 'DevOps' },
      { op: 'add', path: '/fields/Custom.ProgramasAgrotrace', value: 'Agrotrace' },
      { op: 'add', path: '/fields/System.AssignedTo', value: 'qa@example.com' },
      {
        op: 'add',
        path: '/relations/-',
        value: {
          rel: 'System.LinkTypes.Hierarchy-Reverse',
          url: 'https://azure.test/_apis/wit/workitems/42'
        }
      }
    ])
  })

  test('updates a parent Work Item to Test QA with effort fields', async () => {
    let requestUrl = ''
    let requestMethod = ''
    let requestBody: unknown
    const client = new AzureDevOpsClient({
      pat: 'test-pat',
      organization: 'acme',
      baseUrl: 'https://azure.test',
      fetcher: async (input, init) => {
        requestUrl = input
        requestMethod = init?.method ?? ''
        const body = init?.body
        requestBody = JSON.parse(typeof body === 'string' ? body : JSON.stringify(body))
        return new Response('', { status: 200 })
      }
    })

    await new AzureWorkItemClient(client).updateToTestQA('My Project', 42, 0.5, 1)

    expect(requestMethod).toBe('PATCH')
    expect(requestUrl).toBe(
      'https://azure.test/My%20Project/_apis/wit/workitems/42?api-version=7.1'
    )
    expect(requestBody).toEqual([
      { op: 'add', path: '/fields/System.State', value: 'Test QA' },
      { op: 'add', path: '/fields/Microsoft.VSTS.Scheduling.Effort', value: 0.5 },
      { op: 'add', path: '/fields/Custom.RealEffort', value: 1 }
    ])
  })
})
