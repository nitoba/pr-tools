# Documentation Design — pr-tools docs site

**Date:** 2026-04-03  
**Scope:** Fill all 13 placeholder `.mdx` files in `apps/docs/content/docs/`

## Context

The `apps/docs` site is built with Fumadocs + TanStack Start. All 13 MDX files currently contain only `"Conteúdo em breve."`. The content structure and navigation (`meta.json`) are already defined. This spec covers what to write in each file.

## Decisions

- **Language:** Portuguese Brazilian throughout
- **Audience:** Mixed — didactic first-steps, technical reference sections
- **Scope:** All 13 files in one pass; changelog stays as placeholder
- **Approach:** Each file is self-contained and covers exactly what its title promises. No duplication — cross-reference via links. Content sourced from README and CLI scripts.

## File-by-file plan

### Primeiros Passos

**`getting-started/introduction.mdx`**
- What pr-tools is and what problem it solves
- The two tools: `create-pr-description` and `create-test-card` — when to use each
- Supported AI providers
- OS requirements (macOS, Linux, Windows WSL/Git Bash)

**`getting-started/installation.mdx`**
- curl install command (one-liner)
- Variants: specific version, bleeding-edge main
- Prerequisites: `git`, `curl`, `jq`, Bash 4+, at least one API key
- How to update (`--update` flag)

**`getting-started/quickstart.mdx`**
- Minimal sequence: install → `--init` → run `create-pr-description` on a feature branch
- Expected output walkthrough
- Link to configuration for advanced setup

**`getting-started/configuration.mdx`**
- Interactive wizard vs `--init` manual run
- The `~/.config/pr-tools/.env` file: all variables explained
- Configuration precedence: CLI flags > env vars > .env > internal defaults
- Default reviewers for PR creation

### Comandos

**`commands/create-pr-description.mdx`**
- Description of the command
- Full flags table with description and example for each
- "How it works" section (9-step flow from README)
- Usage examples per major flag
- Expected output

**`commands/create-test-card.mdx`**
- Description of the command
- Full flags table
- Auto-detection flow for PR and work item from current branch
- Full output example (formatted block from README)
- Fallback note when Azure DevOps process rules block auto-creation

### Guias

**`guides/azure-devops.mdx`**
- How to create a PAT (minimum required permissions)
- How to configure `AZURE_PAT`
- How the script extracts org/project/repo from git remote
- Notes on automatic PR and Test Case creation

**`guides/ai-providers.mdx`**
- Comparison table of 4 providers: OpenRouter, Groq, Gemini, Ollama
- Default models and how to change them
- How automatic fallback works
- Per-provider configuration

**`guides/markdown-rendering.mdx`**
- Options: `glow`, `bat`/`batcat`, plain text (`--raw`)
- How to install each renderer
- Auto-detection behavior (priority order)

**`guides/advanced-examples.mdx`**
- Combined flag examples: PR for specific branch with work item, test card without Azure creation, dry-run to inspect prompt, model override via env var

### Referência

**`reference/environment-variables.mdx`**
- Full table of all variables: name, description, default value, example
- Grouped by category: providers, Azure DevOps, models, test card defaults

**`reference/troubleshooting.mdx`**
- Common problems: invalid API key, Azure PAT missing permissions, `jq` not installed, clipboard not working, provider fallback behavior
- Each issue: symptom → cause → solution

**`reference/changelog.mdx`**
- Placeholder pointing to CHANGELOG.md on GitHub and Releases page

## Out of scope

- Adding new MDX files or changing navigation structure
- Modifying `meta.json` files
- Any changes to the Fumadocs app code
- English translation
