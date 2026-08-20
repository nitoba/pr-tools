import { Buffer } from 'node:buffer'

type AzureFetcher = (input: string, init?: RequestInit) => Promise<Response>

export type AzureClientOptions = {
  pat: string
  organization: string
  baseUrl?: string
  fetcher?: AzureFetcher
}

export type AzureRequestOptions = {
  method?: string
  body?: unknown
  contentType?: string
  signal?: AbortSignal
}

export class AzureApiError extends Error {
  readonly status: number
  readonly responseBody: string

  constructor(status: number, responseBody: string) {
    super(`Azure DevOps API respondeu ${status}${responseBody ? `: ${responseBody}` : ''}`)
    this.name = 'AzureApiError'
    this.status = status
    this.responseBody = responseBody
  }
}

export class AzureDevOpsClient {
  private readonly pat: string
  private readonly baseUrl: string
  private readonly fetcher: AzureFetcher

  constructor(options: AzureClientOptions) {
    if (!options.organization.trim()) throw new Error('Organização Azure DevOps não informada.')
    if (!options.pat.trim()) throw new Error('PAT do Azure DevOps não configurado.')
    this.pat = options.pat
    this.baseUrl = (
      options.baseUrl ?? `https://dev.azure.com/${encodeURIComponent(options.organization)}`
    ).replace(/\/$/, '')
    this.fetcher = options.fetcher ?? ((input, init) => fetch(input, init))
  }

  url(path: string): string {
    return `${this.baseUrl}/${path.replace(/^\/+/, '')}`
  }

  async request(path: string, options: AzureRequestOptions = {}): Promise<unknown> {
    const headers = {
      Accept: 'application/json',
      Authorization: `Basic ${Buffer.from(`:${this.pat}`).toString('base64')}`,
      'Content-Type': options.contentType ?? 'application/json'
    }
    const response = await this.fetcher(this.url(path), {
      method: options.method ?? 'GET',
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: options.signal
    })
    const responseBody = await response.text()
    if (!response.ok) throw new AzureApiError(response.status, responseBody.trim())
    if (!responseBody.trim()) return undefined
    try {
      return JSON.parse(responseBody)
    } catch {
      return responseBody
    }
  }
}

export function pathSegment(value: string | number): string {
  return encodeURIComponent(String(value))
}

export function withApiVersion(path: string, version = '7.1'): string {
  return `${path}${path.includes('?') ? '&' : '?'}api-version=${encodeURIComponent(version)}`
}
