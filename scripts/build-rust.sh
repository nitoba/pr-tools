#!/usr/bin/env bash
# Build da CLI `prt` (Rust) no repositório `pr-tools`.
#
#   ./scripts/build-rust.sh [alvo] [--no-verify]
#
# Alvos: linux-x64, linux-arm64, macos-arm64, windows-x64.
# O alvo precisa ser o host atual (sem cross por padrão).
# Saída: apps/rust/dist/prt-rust-<alvo>[.exe]
#
# Etapas: cargo fmt --check, clippy (-D correctness), test, build --release.
# Use --no-verify para pular fmt/clippy/test (build puro).
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../apps/rust" && pwd)"
cd "$APP_DIR"

usage() {
  echo "Uso: ./scripts/build-rust.sh [alvo] [--no-verify]" >&2
  echo "Alvos: linux-x64, linux-arm64, macos-arm64, windows-x64" >&2
  exit 2
}

native_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux)
      case "$arch" in
        x86_64) echo "linux-x64" ;;
        aarch64|arm64) echo "linux-arm64" ;;
        *) echo "unsupported" ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        arm64) echo "macos-arm64" ;;
        *) echo "unsupported" ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      echo "windows-x64"
      ;;
    *) echo "unsupported" ;;
  esac
}

TARGET=""
VERIFY=1
for arg in "$@"; do
  case "$arg" in
    -h|--help) usage ;;
    --no-verify) VERIFY=0 ;;
    linux-x64|linux-arm64|macos-arm64|windows-x64)
      if [ -n "$TARGET" ]; then usage; fi
      TARGET="$arg"
      ;;
    *) usage ;;
  esac
done

NATIVE="$(native_target)"
if [ "$NATIVE" = "unsupported" ]; then
  echo "Host não suportado: $(uname -s)/$(uname -m)." >&2
  exit 2
fi
if [ -z "$TARGET" ]; then
  TARGET="$NATIVE"
fi
if [ "$TARGET" != "$NATIVE" ]; then
  echo "O alvo $TARGET exige um host $TARGET; o host atual é $NATIVE." >&2
  exit 2
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo não encontrado no PATH. Instale via https://rustup.rs" >&2
  exit 1
fi

if [ "$VERIFY" = "1" ]; then
  echo "==> cargo fmt --check"
  cargo fmt --all -- --check
  echo "==> cargo clippy"
  cargo clippy --locked --all-targets -- -D clippy::correctness
  echo "==> cargo test"
  cargo test --locked
fi

echo "==> cargo build --release"
cargo build --locked --release

EXE="target/release/prt"
if [ "$TARGET" = "windows-x64" ]; then
  # No host Windows o binário sai com .exe; no Unix simulamos o nome.
  if [ -f "target/release/prt.exe" ]; then
    EXE="target/release/prt.exe"
  fi
fi
if [ ! -f "$EXE" ]; then
  echo "Binário não encontrado em $EXE após o build." >&2
  exit 1
fi

mkdir -p dist
OUT="dist/prt-rust-$TARGET"
if [ "$TARGET" = "windows-x64" ]; then
  OUT="$OUT.exe"
fi
cp -f "$EXE" "$OUT"
chmod +x "$OUT" 2>/dev/null || true

echo "Binário criado em apps/rust/$OUT"
ls -lh "$OUT"
