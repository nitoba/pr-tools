export type Spinner = {
  start(message: string): void
  message(message: string): void
  stop(message: string): void
  error(message: string): void
}

export const log = {
  success(message: string): void {
    console.log(paint('green', `✓ ${message}`))
  },
  warn(message: string): void {
    console.log(paint('yellow', `⚠ ${message}`))
  },
  error(message: string): void {
    console.error(paint('red', `✗ ${message}`))
  },
  info(message: string): void {
    console.log(paint('cyan', `• ${message}`))
  }
}

export function intro(message: string): void {
  console.log(`${paint('magenta', `┌ ${message}`)}\n│`)
}

export function outro(message: string): void {
  console.log(paint('magenta', `└ ${message}`))
}

export function note(message: string, title?: string): void {
  if (title) console.log(paint(['blue', 'bold'], `◆ ${title}`))
  console.log(message)
}

export function cancel(message: string): void {
  console.log(paint('yellow', `✗ ${message}`))
}

export function spinner(): Spinner {
  let active = false
  return {
    start(message) {
      active = true
      console.log(paint('cyan', `… ${message}`))
    },
    message(message) {
      if (active) console.log(paint('cyan', `… ${message}`))
    },
    stop(message) {
      active = false
      console.log(paint('green', `✓ ${message}`))
    },
    error(message) {
      active = false
      console.error(paint('red', `✗ ${message}`))
    }
  }
}

export type StyleFormat = string | readonly string[]

export function setFormatter(formatter: (format: StyleFormat, message: string) => string): void {
  currentFormatter = formatter
}

const ansiCodes: Record<string, string> = {
  blue: '34',
  bold: '1',
  cyan: '36',
  green: '32',
  magenta: '35',
  red: '31',
  yellow: '33'
}

let currentFormatter = (format: StyleFormat, message: string): string => {
  if (!process.stdout.isTTY || process.env.NO_COLOR !== undefined) return message
  const formats: string[] = Array.isArray(format) ? [...format] : [format]
  const codes = formats.map((item) => ansiCodes[item]).filter(Boolean)
  return codes.length > 0 ? `\u001b[${codes.join(';')}m${message}\u001b[0m` : message
}

function paint(format: StyleFormat, message: string): string {
  return currentFormatter(format, message)
}

export function color(format: StyleFormat, message: string): string {
  return paint(format, message)
}
