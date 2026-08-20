export type StyleFormat = string | readonly string[]

const ANSI_CODES: Record<string, string> = {
  blue: '34',
  bold: '1',
  cyan: '36',
  green: '32',
  magenta: '35',
  red: '31',
  yellow: '33'
}

let formatter = (format: StyleFormat, message: string): string => {
  if (!process.stdout.isTTY || process.env.NO_COLOR !== undefined) return message
  const formats = Array.isArray(format) ? [...format] : [format]
  const codes = formats.map((item) => ANSI_CODES[String(item)]).filter(Boolean)
  return codes.length > 0 ? `\u001b[${codes.join(';')}m${message}\u001b[0m` : message
}

export function setFormatter(next: (format: StyleFormat, message: string) => string): void {
  formatter = next
}

export function color(format: StyleFormat, message: string): string {
  return formatter(format, message)
}
