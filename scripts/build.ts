import { spawnSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

type Target = 'native' | 'linux-x64' | 'linux-arm64' | 'windows-x64' | 'macos-arm64'

const projectDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const outputDir = resolve(projectDir, 'dist')

function usage(): void {
  console.error(`Uso:
  bun run build
  bun run build -- linux-x64
  bun run build -- linux-arm64
  bun run build -- windows-x64
  bun run build -- macos-arm64

Linux e Windows usam Zig para cross-compilação. macOS arm64 deve ser
compilado em um Mac Apple Silicon com as Xcode Command Line Tools.`)
}

function requireZig(): void {
  const result = spawnSync('zig', ['version'], { stdio: 'ignore' })
  if (result.error || result.status !== 0) throw new Error('Comando necessário não encontrado: zig')
}

function build(output: string, target?: string): void {
  const environment = { ...process.env }
  if (target) {
    requireZig()
    environment.SCRIPTC_CC = 'zigcc'
    environment.SCRIPTC_TARGET = target
  } else if (process.platform === 'linux') {
    environment.PATH = `/usr/bin:${environment.PATH ?? ''}`
  }

  const result = spawnSync(
    process.execPath,
    [
      'x',
      '--no-install',
      'scriptc',
      'build',
      'src/bin.ts',
      '--dynamic',
      '--no-keep-c',
      '-o',
      `dist/${output}`
    ],
    { cwd: projectDir, env: environment, stdio: 'inherit' }
  )
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}

function buildNative(): void {
  if (process.platform === 'linux' && process.arch === 'x64') {
    build('pr-tools-linux-x64')
    return
  }
  if (process.platform === 'linux' && process.arch === 'arm64') {
    build('pr-tools-linux-arm64')
    return
  }
  if (process.platform === 'darwin' && process.arch === 'arm64') {
    build('pr-tools-macos-arm64')
    return
  }
  if (process.platform === 'win32' && process.arch === 'x64') {
    build('pr-tools-windows-x64.exe', 'x86_64-windows-gnu')
    return
  }
  throw new Error(`Não há um alvo nativo configurado para ${process.platform}/${process.arch}.`)
}

function main(): void {
  const target = (process.argv[2] ?? 'native') as Target
  mkdirSync(outputDir, { recursive: true })

  switch (target) {
    case 'native':
      buildNative()
      return
    case 'linux-x64':
      build('pr-tools-linux-x64', 'x86_64-linux-gnu.2.36')
      return
    case 'linux-arm64':
      build('pr-tools-linux-arm64', 'aarch64-linux-gnu.2.36')
      return
    case 'windows-x64':
      build('pr-tools-windows-x64.exe', 'x86_64-windows-gnu')
      return
    case 'macos-arm64':
      if (process.platform !== 'darwin' || process.arch !== 'arm64')
        throw new Error('O alvo macos-arm64 requer um Mac Apple Silicon.')
      build('pr-tools-macos-arm64')
      return
    default:
      usage()
      process.exit(2)
  }
}

try {
  main()
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error))
  process.exit(1)
}
