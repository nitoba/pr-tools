#!/usr/bin/env python3
"""Generate user-facing release notes through an OpenAI-compatible endpoint."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


def run_git(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return result.stdout.strip()


def find_previous_tag(tag: str) -> str | None:
    try:
        tags = run_git(
            "tag",
            "--merged",
            f"{tag}^",
            "--list",
            "v*.*.*",
            "--sort=-version:refname",
        )
    except subprocess.CalledProcessError:
        return None

    return tags.splitlines()[0] if tags else None


def collect_commits(previous_tag: str | None, tag: str) -> list[dict[str, str]]:
    revision = f"{previous_tag}..{tag}" if previous_tag else tag
    separator = "\x1e"
    field_separator = "\x1f"
    raw = run_git(
        "log",
        "--no-merges",
        f"--format=%H{field_separator}%s{field_separator}%b{separator}",
        revision,
    )

    commits: list[dict[str, str]] = []
    for record in raw.split(separator):
        fields = record.strip().split(field_separator, maxsplit=2)
        if len(fields) < 2 or not fields[1].strip():
            continue
        commits.append(
            {
                "sha": fields[0][:7],
                "subject": fields[1].strip(),
                "body": fields[2].strip() if len(fields) == 3 else "",
            }
        )
    return commits


def build_prompt(
    tag: str,
    previous_tag: str | None,
    commits: list[dict[str, str]],
    diff_stat: str,
    technical_notes: str,
) -> str:
    commit_text = "\n\n".join(
        "\n".join(
            [
                f"[{commit['sha']}] {commit['subject']}",
                commit["body"] or "(sem descrição adicional)",
            ]
        )
        for commit in commits
    )

    return f"""Você escreve notas de versão para usuários do aplicativo CLI `prt`.

Gere notas de release em português do Brasil, com linguagem clara e humanizada.
Baseie-se somente nas mudanças fornecidas; não invente funcionalidades, impactos,
números ou correções que não estejam evidenciados.

A saída deve ser somente Markdown, sem cercas de código e sem uma introdução fora
das seções. Não inclua hashes de commit, nomes internos de funções ou detalhes de
implementação que não sejam úteis para quem usa o `prt`.

Use esta estrutura, omitindo seções vazias:

## Resumo
Uma explicação curta do principal valor desta versão para o usuário.

### Novidades
Funcionalidades e comportamentos novos.

### Correções
Problemas corrigidos e comportamentos que passaram a funcionar melhor.

### Melhorias
Melhorias de desempenho, UX, confiabilidade ou compatibilidade.

### Manutenção e documentação
Mudanças internas, documentação, testes e CI somente quando forem relevantes.

Se houver uma mudança incompatível, destaque-a em uma seção `### Atenção`.
Prefira listas curtas e frases orientadas ao benefício do usuário.

Release: {tag}
Release anterior: {previous_tag or '(não disponível)'}

Commits:
---
{commit_text}
---

Resumo de arquivos alterados:
---
{diff_stat or '(não disponível)'}
---

Notas técnicas geradas pelo changelog automático, usadas apenas como contexto:
---
{technical_notes or '(não disponível)'}
---
"""


def request_notes(prompt: str) -> str:
    api_url = os.environ.get("RELEASE_NOTES_API_URL", "").strip()
    api_key = os.environ.get("RELEASE_NOTES_API_KEY", "").strip()
    model = os.environ.get("RELEASE_NOTES_MODEL", "openai/gpt-oss-120b").strip()
    reasoning_effort = os.environ.get("RELEASE_NOTES_REASONING_EFFORT", "medium").strip()

    if not api_url or not api_key:
        raise RuntimeError(
            "RELEASE_NOTES_API_URL and RELEASE_NOTES_API_KEY are required"
        )

    print(
        f"Requesting release notes from {api_url} "
        f"with model {model} and reasoning effort {reasoning_effort} "
        "(API key present: yes)",
        file=sys.stderr,
    )

    payload = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "reasoning_effort": reasoning_effort,
        "max_completion_tokens": 2500,
        "stream": False,
    }
    request = Request(
        api_url,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Accept": "application/json",
            "Content-Type": "application/json",
            "User-Agent": "prt-release-notes/1.0 (+https://github.com/nitoba/pr-tools)",
        },
        method="POST",
    )

    with urlopen(request, timeout=90) as response:
        response_data = json.loads(response.read().decode("utf-8"))

    content = response_data["choices"][0]["message"].get("content", "")
    if not isinstance(content, str) or not content.strip():
        raise RuntimeError("the model returned an empty release note")

    notes = content.strip()
    if notes.startswith("```") and notes.endswith("```"):
        lines = notes.splitlines()
        notes = "\n".join(lines[1:-1]).strip()
    return notes


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--fallback", type=Path)
    args = parser.parse_args()

    try:
        previous_tag = find_previous_tag(args.tag)
        commits = collect_commits(previous_tag, args.tag)
        if not commits:
            print("No commits found for release notes; keeping technical notes.")
            return 0

        revision = f"{previous_tag}..{args.tag}" if previous_tag else args.tag
        diff_stat = run_git("diff", "--stat", revision)
        technical_notes = (
            args.fallback.read_text(encoding="utf-8")
            if args.fallback and args.fallback.exists()
            else ""
        )
        prompt = build_prompt(
            args.tag,
            previous_tag,
            commits,
            diff_stat,
            technical_notes,
        )
        notes = request_notes(prompt)
        args.output.write_text(f"{notes}\n", encoding="utf-8")
        print(f"Generated humanized release notes at {args.output}")
    except HTTPError as error:
        response_body_bytes = error.read()
        response_body = response_body_bytes.decode("utf-8", errors="replace")
        details = f"HTTP {error.code} {error.reason}"
        print(
            f"::warning::Could not generate humanized release notes: {details}",
            file=sys.stderr,
        )
        print(
            f"HTTP error response body ({len(response_body_bytes)} bytes): "
            f"{response_body!r}",
            file=sys.stderr,
        )
        diagnostic_header_names = {
            "content-type",
            "content-length",
            "cf-ray",
            "cf-error-type",
            "cf-error-origin",
        }
        diagnostic_headers = {
            name.lower(): value
            for name, value in error.headers.items()
            if name.lower() in diagnostic_header_names
        }
        if diagnostic_headers:
            print(f"HTTP error diagnostic headers: {diagnostic_headers}", file=sys.stderr)
        print("Keeping the technical release notes generated by git-cliff.")
        return 0
    except (
        OSError,
        RuntimeError,
        subprocess.CalledProcessError,
        URLError,
        KeyError,
        IndexError,
        json.JSONDecodeError,
    ) as error:
        print(f"::warning::Could not generate humanized release notes: {error}")
        print("Keeping the technical release notes generated by git-cliff.")
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())
