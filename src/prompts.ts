import { spawnSync } from 'node:child_process'
import { createInterface } from 'node:readline'
import {
  confirm as clackConfirm,
  password as clackPassword,
  text as clackText
} from '@clack/prompts'
import type { PromptValidator } from './validation'

type TextOptions = {
  message: string
  initialValue?: string
  defaultValue?: string
  placeholder?: string
  validate?: PromptValidator
}

type PasswordOptions = {
  message: string
  validate?: PromptValidator
}

type SelectOption<Value extends string> = {
  value: Value
  label: string
  hint?: string
  disabled?: boolean
}

type SelectOptions<Value extends string> = {
  message: string
  options: SelectOption<Value>[]
  initialValue?: Value
}

type ConfirmOptions = {
  message: string
  initialValue?: boolean
  active?: string
  inactive?: string
}

const nativeBinary = process.versions.bun === undefined
let terminalEchoDisabled = false

if (nativeBinary) {
  process.on('SIGINT', () => {
    if (terminalEchoDisabled) setTerminalEcho(true)
    process.exit(130)
  })
}

export async function text(options: TextOptions): Promise<string | undefined> {
  if (!nativeBinary) {
    while (true) {
      const value = await clackText({
        message: options.message,
        initialValue: options.initialValue,
        defaultValue: options.defaultValue,
        placeholder: options.placeholder
      })
      if (typeof value !== 'string') return undefined
      const error = options.validate?.(value)
      if (!error) return value
      console.log(error)
    }
  }
  return askLine(options.message, options.initialValue ?? options.defaultValue, options)
}

export async function password(options: PasswordOptions): Promise<string | undefined> {
  if (!nativeBinary) {
    while (true) {
      const value = await clackPassword({ message: options.message })
      if (typeof value !== 'string') return undefined
      const error = options.validate?.(value)
      if (!error) return value
      console.log(error)
    }
  }
  if (process.platform === 'win32') return askWindowsPassword(options)

  setTerminalEcho(false)
  try {
    return await askLine(options.message, undefined, options, true)
  } finally {
    setTerminalEcho(true)
  }
}

export async function select<Value extends string>(
  options: SelectOptions<Value>
): Promise<Value | undefined> {
  const available = options.options.filter((option) => !option.disabled)
  if (available.length === 0) return undefined
  const initialIndex = Math.max(
    0,
    available.findIndex((option) => option.value === options.initialValue)
  )

  while (true) {
    console.log(`◆ ${options.message}`)
    available.forEach((option, index) => {
      const hint = option.hint ? ` — ${option.hint}` : ''
      console.log(`  ${index + 1}) ${option.label ?? option.value}${hint}`)
    })
    const answer = await askLine(`Escolha [${initialIndex + 1}]`, undefined)
    if (answer === undefined) return undefined
    if (!answer.trim()) return available[initialIndex]?.value as Value | undefined

    const selected = Number(answer.trim()) - 1
    if (Number.isInteger(selected) && selected >= 0 && selected < available.length) {
      return available[selected]?.value as Value | undefined
    }
    console.log(`Escolha um número entre 1 e ${available.length}.`)
  }
}

export async function confirm(options: ConfirmOptions): Promise<boolean | undefined> {
  if (!nativeBinary) {
    const value = await clackConfirm({
      message: options.message,
      initialValue: options.initialValue,
      active: options.active,
      inactive: options.inactive
    })
    return typeof value === 'boolean' ? value : undefined
  }

  const initialValue = options.initialValue ?? true
  const active = options.active ?? 'Sim'
  const inactive = options.inactive ?? 'Não'
  while (true) {
    const answer = await askLine(`◆ ${options.message} [${active}/${inactive}]`, undefined)
    if (answer === undefined) return undefined
    if (!answer.trim()) return initialValue
    const normalized = answer.trim().toLowerCase()
    if (normalized === 's' || normalized === 'sim' || normalized === 'y' || normalized === 'yes')
      return true
    if (normalized === 'n' || normalized === 'não' || normalized === 'nao' || normalized === 'no')
      return false
    console.log(`Responda ${active.toLowerCase()} ou ${inactive.toLowerCase()}.`)
  }
}

function askLine(
  message: string,
  fallback?: string,
  options?: { placeholder?: string; validate?: PromptValidator },
  secret = false
): Promise<string | undefined> {
  const suffix = fallback
    ? ` [${fallback}]`
    : options?.placeholder
      ? ` (${options.placeholder})`
      : ''
  return new Promise((resolve) => {
    const readline = createInterface({ input: process.stdin, output: process.stdout })
    let settled = false

    const finish = (value: string | undefined): void => {
      if (settled) return
      settled = true
      readline.close()
      resolve(value)
    }

    const ask = (): void => {
      readline.question(`◆ ${message}${suffix}: `, (answer) => {
        if (secret) process.stdout.write('\n')
        const value = answer === '' && fallback !== undefined ? fallback : answer
        const error = options?.validate?.(value)
        if (error) {
          console.log(error)
          ask()
          return
        }
        finish(value)
      })
    }

    readline.on('close', () => finish(undefined))
    ask()
  })
}

function setTerminalEcho(enabled: boolean): void {
  const result = spawnSync('stty', [enabled ? 'echo' : '-echo'], { stdio: 'inherit' })
  if (result.status !== 0)
    throw new Error('Não foi possível configurar a entrada segura do terminal.')
  terminalEchoDisabled = !enabled
}

function askWindowsPassword(options: PasswordOptions): string | undefined {
  const escapedMessage = options.message.replace(/'/gu, "''")
  const command = `$secure = Read-Host -Prompt '${escapedMessage}' -AsSecureString; $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure); try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }`
  for (const executable of ['powershell.exe', 'pwsh']) {
    const result = spawnSync(executable, ['-NoProfile', '-Command', command], {
      encoding: 'utf8',
      stdio: ['inherit', 'pipe', 'inherit']
    })
    if (result.status !== 0 || result.error) continue
    const value = (result.stdout ?? '').replace(/\r?\n$/u, '')
    const error = options.validate?.(value)
    if (!error) return value
    console.log(error)
  }
  throw new Error(
    'Não foi possível ler a senha com segurança neste Windows. Use uma variável de ambiente.'
  )
}
