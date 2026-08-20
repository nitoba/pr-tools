export class CliExit extends Error {
  constructor(
    readonly code: number,
    readonly output?: string
  ) {
    super(output ?? '')
    this.name = 'CliExit'
  }
}
