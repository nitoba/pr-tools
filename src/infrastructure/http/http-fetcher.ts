export interface HttpFetcher {
  fetch(input: string, init?: RequestInit): Promise<Response>
}
