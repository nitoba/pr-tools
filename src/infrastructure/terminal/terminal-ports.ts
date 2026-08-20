export interface TerminalOutput {
  write(message: string): void
  writeError(message: string): void
}

export interface ProgressReporter {
  start(message: string): void
  message(message: string): void
  stop(message: string): void
  error(message: string): void
}

export interface PromptPort {
  text(options: {
    message: string
    initialValue?: string
    defaultValue?: string
    placeholder?: string
    validate?: (value: string | undefined) => string | undefined
  }): Promise<string | undefined>
  password(options: {
    message: string
    validate?: (value: string | undefined) => string | undefined
  }): Promise<string | undefined>
  select(options: {
    message: string
    options: Array<{ value: string; label: string; hint?: string; disabled?: boolean }>
    initialValue?: string
  }): Promise<string | undefined>
  confirm(options: { message: string; initialValue?: boolean }): Promise<boolean | undefined>
}
