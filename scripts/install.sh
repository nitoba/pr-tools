#!/usr/bin/env bash

set -euo pipefail

platform="$(uname -s)"
architecture="$(uname -m)"
version="${PR_TOOLS_VERSION:-latest}"
repository="${PR_TOOLS_REPOSITORY:-nitoba/pr-tools}"

if [[ -z "${PR_TOOLS_BINARY:-}" && ( -z "$repository" || ! "$repository" =~ ^[^/]+/[^/]+$ ) ]]; then
  printf 'Não foi possível determinar o repositório GitHub. Use PR_TOOLS_REPOSITORY=owner/repo.\n' >&2
  exit 1
fi

if [[ -n "${PR_TOOLS_BINARY:-}" ]]; then
  binary_path="$PR_TOOLS_BINARY"
else
  case "$platform:$architecture" in
    Linux:x86_64|Linux:amd64)
      asset_name='prt-linux-x64'
      ;;
    Linux:aarch64|Linux:arm64)
      asset_name='prt-linux-arm64'
      ;;
    Darwin:arm64)
      asset_name='prt-macos-arm64'
      ;;
    *)
      printf 'Plataforma não suportada pelo instalador Bash: %s/%s.\n' "$platform" "$architecture" >&2
      exit 1
      ;;
  esac
fi

readonly install_dir="${PR_TOOLS_INSTALL_DIR:-${XDG_BIN_HOME:-$HOME/.local/bin}}"
readonly target_path="$install_dir/prt"
temporary_dir=""

if [[ -z "${PR_TOOLS_BINARY:-}" ]]; then
  if ! command -v curl >/dev/null 2>&1; then
    printf 'Comando necessário não encontrado: curl.\n' >&2
    exit 1
  fi
  temporary_dir="$(mktemp -d)"
  trap 'rm -rf "$temporary_dir"' EXIT
  if [[ "$version" == 'latest' ]]; then
    download_url="https://github.com/$repository/releases/latest/download/$asset_name"
  else
    release_tag="${version#v}"
    download_url="https://github.com/$repository/releases/download/v$release_tag/$asset_name"
  fi
  binary_path="$temporary_dir/$asset_name"
  curl_args=(--fail --silent --show-error --location)
  if [[ -n "${PR_TOOLS_GITHUB_TOKEN:-}" ]]; then
    curl_args+=(--header "Authorization: Bearer $PR_TOOLS_GITHUB_TOKEN")
  fi
  curl "${curl_args[@]}" "$download_url" --output "$binary_path"
  chmod 0755 "$binary_path"
fi

if [[ ! -f "$binary_path" ]]; then
  printf 'Binário não encontrado em %s.\n' "$binary_path" >&2
  exit 1
fi

if [[ ! -x "$binary_path" ]]; then
  printf 'O arquivo %s não é executável.\n' "$binary_path" >&2
  exit 1
fi

mkdir -p "$install_dir"
install -m 0755 "$binary_path" "$target_path"

append_path_entry() {
  local file="$1"
  local escaped_directory="$(printf '%q' "$install_dir")"
  local path_line="export PATH=$escaped_directory:\$PATH"
  mkdir -p "$(dirname "$file")"
  if [[ ! -f "$file" ]] || ! grep -Fqx "$path_line" "$file"; then
    {
      printf '\n# Added by prt installer\n'
      printf '%s\n' "$path_line"
    } >> "$file"
    printf 'PATH atualizado em %s\n' "$file"
  fi
}

append_fish_path_entry() {
  local file="$1"
  local escaped_directory
  if command -v fish >/dev/null 2>&1; then
    escaped_directory="$(fish -c 'string escape -- "$argv[1]"' -- "$install_dir")"
  else
    escaped_directory="$(printf '%q' "$install_dir")"
  fi
  local path_line="fish_add_path --prepend $escaped_directory"
  mkdir -p "$(dirname "$file")"
  if [[ ! -f "$file" ]] || ! grep -Fqx "$path_line" "$file"; then
    {
      printf '\n# Added by prt installer\n'
      printf '%s\n' "$path_line"
    } >> "$file"
    printf 'PATH atualizado em %s\n' "$file"
  fi
}

if [[ ":${PATH:-}:" != *:"$install_dir":* ]]; then
  shell_name="${SHELL##*/}"
  case "$shell_name" in
    fish) append_fish_path_entry "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish" ;;
    bash) append_path_entry "$HOME/.profile"; append_path_entry "$HOME/.bashrc" ;;
    zsh) append_path_entry "$HOME/.profile"; append_path_entry "$HOME/.zprofile" ;;
    *) append_path_entry "$HOME/.profile" ;;
  esac
fi

printf 'prt instalado em %s\n' "$target_path"
case ":${PATH:-}:" in
  *:"$install_dir":*) ;;
  *) printf 'Abra um novo terminal para usar `prt` diretamente.\n' ;;
esac
