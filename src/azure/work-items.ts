import { AzureDevOpsClient, pathSegment, withApiVersion } from './client'
import type { AzureWorkItem, CreateTestCaseInput } from './types'

export function workItemField(item: AzureWorkItem, key: string): string {
  const value = item.fields[key]
  return typeof value === 'string' ? value : ''
}

export function workItemTitle(item: AzureWorkItem): string {
  return workItemField(item, 'System.Title')
}

export function workItemType(item: AzureWorkItem): string {
  return workItemField(item, 'System.WorkItemType')
}

export function workItemDescription(item: AzureWorkItem): string {
  return workItemField(item, 'System.Description')
}

export function workItemSprint(item: AzureWorkItem): string {
  const path = workItemField(item, 'System.IterationPath')
  const tokens = path.split(/[\\/ ]/u).filter(Boolean)
  return [...tokens].reverse().find((value) => /^\d+$/u.test(value)) ?? ''
}

export class AzureWorkItemClient {
  constructor(private readonly client: AzureDevOpsClient) {}

  async get(project: string, id: number, signal?: AbortSignal): Promise<AzureWorkItem> {
    return (await this.client.request(
      withApiVersion(`/${pathSegment(project)}/_apis/wit/workitems/${id}`),
      { signal }
    )) as AzureWorkItem
  }

  async query(project: string, wiql: string, signal?: AbortSignal): Promise<number[]> {
    const result = (await this.client.request(
      withApiVersion(`/${pathSegment(project)}/_apis/wit/wiql`),
      { method: 'POST', body: { query: wiql }, signal }
    )) as { workItems: Array<{ id: number }> }
    return result.workItems.map((item) => item.id)
  }

  async updateState(
    project: string,
    id: number,
    state: string,
    signal?: AbortSignal
  ): Promise<void> {
    await this.client.request(withApiVersion(`/${pathSegment(project)}/_apis/wit/workitems/${id}`), {
      method: 'PATCH',
      contentType: 'application/json-patch+json',
      body: [{ op: 'add', path: '/fields/System.State', value: state }],
      signal
    })
  }

  async updateToTestQA(
    project: string,
    id: number,
    effort?: number,
    realEffort?: number,
    signal?: AbortSignal
  ): Promise<void> {
    const body: Array<{ op: 'add'; path: string; value: string | number }> = [
      { op: 'add', path: '/fields/System.State', value: 'Test QA' }
    ]
    if (effort !== undefined)
      body.push({ op: 'add', path: '/fields/Microsoft.VSTS.Scheduling.Effort', value: effort })
    if (realEffort !== undefined)
      body.push({ op: 'add', path: '/fields/Custom.RealEffort', value: realEffort })
    await this.client.request(withApiVersion(`/${pathSegment(project)}/_apis/wit/workitems/${id}`), {
      method: 'PATCH',
      contentType: 'application/json-patch+json',
      body,
      signal
    })
  }

  async createTestCase(
    project: string,
    input: CreateTestCaseInput,
    signal?: AbortSignal
  ): Promise<AzureWorkItem> {
    const body: Array<{ op: 'add'; path: string; value: unknown }> = [
      { op: 'add', path: '/fields/System.Title', value: input.title }
    ]
    const fields: Array<[string, unknown]> = [
      ['/fields/System.Description', input.descriptionHtml],
      ['/fields/Microsoft.VSTS.TCM.Steps', input.stepsXml],
      ['/fields/System.AreaPath', input.areaPath],
      ['/fields/System.IterationPath', input.iterationPath],
      ['/fields/Microsoft.VSTS.Common.Priority', input.priority],
      ['/fields/Custom.Team', input.team],
      ['/fields/Custom.ProgramasAgrotrace', input.program],
      ['/fields/System.AssignedTo', input.assignedTo]
    ]
    for (const [path, value] of fields) {
      if (value !== undefined && value !== '') body.push({ op: 'add', path, value })
    }
    if (input.parentId !== undefined && input.parentId > 0) {
      body.push({
        op: 'add',
        path: '/relations/-',
        value: {
          rel: 'System.LinkTypes.Hierarchy-Reverse',
          url: this.client.url(`/_apis/wit/workitems/${input.parentId}`)
        }
      })
    }
    return (await this.client.request(
      withApiVersion(`/${pathSegment(project)}/_apis/wit/workitems/${pathSegment('$Test Case')}`),
      { method: 'POST', contentType: 'application/json-patch+json', body, signal }
    )) as AzureWorkItem
  }
}
