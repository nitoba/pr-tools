# AGENTS.md

## Purpose
- This repository is a Bash-first CLI project for generating Azure DevOps PR descriptions with LLM providers.
- Main runtime code lives in `bin/create-pr-description`; installation/bootstrap logic lives in `install.sh`.
- There is no formal build system; agents should optimize for safe shell edits, syntax checks, and targeted manual validation.

## Repository Map
- `bin/create-pr-description` - main CLI, provider calls, git context collection, Azure DevOps integration, clipboard handling.
- `install.sh` - installer that downloads the CLI into `~/.local/bin` and optionally bootstraps config.
- `README.md` - user-facing installation and usage docs in Brazilian Portuguese.
- `opencode.json` - local agent tooling config.
- `docs/superpowers/` - planning/spec artifacts; useful for historical intent, not runtime behavior.

## Commands
### Setup / install
- Install from remote: `curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash`
- Run local installer from repo: `bash install.sh`
- Initialize local config only: `bin/create-pr-description --init`

### Build
- There is no compile/build step.
- The closest equivalent is shell syntax validation:
- `bash -n bin/create-pr-description`
- `bash -n install.sh`

### Lint / format
- No repo-local linter or formatter config was found.
- Optional checks if available in the environment:
- `shellcheck bin/create-pr-description install.sh`
- `shfmt -w bin/create-pr-description install.sh`
- Do not assume `shellcheck` or `shfmt` are installed unless you verify first.

### Test
- No automated test suite, test runner, or fixture directory exists in this repository.
- No `package.json`, `pyproject.toml`, `go.mod`, `Cargo.toml`, `Makefile`, `justfile`, `.bats` suite, or `tests/` tree was found.
- Use syntax checks plus focused CLI/manual verification.

### Single-test equivalent
- There is no true single-test command because no automated harness exists.
- Use the narrowest possible check for the code you changed:
- `bash -n bin/create-pr-description`
- `bash -n install.sh`
- `bin/create-pr-description --help`
- `bin/create-pr-description --version`
- `bin/create-pr-description --dry-run`

### Recommended validation flows
- CLI-only change: `bash -n bin/create-pr-description && bin/create-pr-description --help`
- Installer-only change: `bash -n install.sh`
- Prompt/output change: `bash -n bin/create-pr-description && bin/create-pr-description --dry-run`
- Config/init change: `bash -n bin/create-pr-description && bin/create-pr-description --init`

## Tooling Findings
- Language: Bash
- Shebang style: `#!/usr/bin/env bash`
- Strict mode is expected: `set -euo pipefail`
- Core external dependencies: `git`, `curl`, `jq`
- Optional clipboard tools: `pbcopy`, `wl-copy`, `xclip`, `xsel`
- Optional services: OpenRouter, Groq, Google Gemini, Azure DevOps APIs

## Rules Files
- No `.cursor/rules/` directory was found.
- No `.cursorrules` file was found.
- No `.github/copilot-instructions.md` file was found.
- This file therefore serves as the repository-specific operating guide for coding agents.

## Code Style
### General
- Preserve the Bash-first approach; do not introduce another language or framework for small changes.
- Match existing script structure before inventing abstractions.
- Keep scripts portable across macOS, Linux, and WSL/Git Bash where practical.
- Prefer simple procedural flow with focused helper functions over clever metaprogramming.

### Formatting
- Use 2-space indentation inside functions, loops, and conditionals.
- Keep one logical step per block and separate major sections with blank lines.
- Existing scripts use section banners like `# ---- Validation ----`; keep that style for new major sections.
- Long heredocs and help text are acceptable when needed.

### Dependencies and external tools
- There are no import statements in Bash, but dependency usage should stay explicit and minimal.
- Validate required external commands before use when they are core to execution.
- Prefer existing dependencies (`git`, `curl`, `jq`) over adding new ones.
- If a new dependency is unavoidable, document it in `README.md` and validate it in the script.

### Naming
- Function names use lowercase snake_case, e.g. `collect_git_context`, `detect_sprint`.
- Global constants and global state use uppercase snake_case, e.g. `VERSION`, `CONFIG_DIR`, `PR_LINKS`.
- Function-local variables use `local` and lowercase snake_case.
- Booleans are stored as strings like `true` and `false`; keep existing conventions.
- User-facing option names should remain long-form GNU-style flags like `--dry-run`.

### Variables and quoting
- Quote variable expansions by default: `"$var"`, `"${arr[@]}"`.
- Use braces for clarity in interpolated variables, especially adjacent to text.
- Prefer arrays for lists of args and targets instead of space-delimited strings.
- Use `local` inside functions unless the value is intentionally shared global state.
- Preserve existing global state patterns when touching `main`; avoid accidental shadowing.

### Control flow
- Prefer `[[ ... ]]` over `[ ... ]`.
- Prefer `case` for CLI argument parsing and finite option dispatch.
- Use guard clauses with `exit 1` or `return 1` for invalid state.
- Keep fallback chains explicit and readable rather than compressed.

### Error handling
- Keep `set -euo pipefail` at the top of executable scripts.
- Route user-visible status through helper functions like `log_error`, `log_warn`, `log_info`, `log_success`.
- Fail early on invalid CLI input, missing config, missing dependencies, or unsupported repo state.
- For non-critical network operations, preserve the current graceful-degradation behavior with warnings.
- When a failure is intentionally non-fatal, make that obvious in code and logs.

### Shell command usage
- Use `command -v` for dependency detection.
- Use `jq` for JSON construction/parsing instead of brittle string concatenation.
- Use `mktemp` for large payloads or temporary files and clean them up explicitly.
- The existing code uses `trap` for some temp cleanup; keep cleanup reliable.
- When building `curl` calls, prefer arrays for arguments where feasible.

### Text handling
- Prefer `printf '%s'` when exact text preservation matters.
- Existing code sometimes uses `echo`; follow the surrounding pattern unless there is a correctness reason to switch.
- Use `sed`, `grep`, `cut`, `head`, and `tail` carefully and only where they keep the script clearer.
- Preserve Brazilian Portuguese for user-facing help text, prompts, logs, and README updates.

### Types and data modeling
- Bash has no static types here; represent structured data with clear conventions.
- Arrays are preferred for ordered collections such as `TARGETS` and `PR_LINKS`.
- Keep string formats stable when other functions parse them later, e.g. `target|url` entries.
- Be defensive about empty or missing env values, API fields, and command results.

### API and config changes
- Preserve environment-variable precedence: explicit environment values override `.env` values.
- Keep default provider/model constants centralized near the top of `bin/create-pr-description`.
- When adding a new CLI flag, update both `parse_args` and `show_help` together.
- When adding config keys, update creation, loading, validation, and docs together.

### Documentation expectations
- Update `README.md` whenever install steps, flags, behavior, or dependencies change.
- Keep examples realistic and aligned with the actual CLI.
- Avoid documenting features that are not implemented.

## Agent Workflow Guidance
- Read the relevant script end-to-end before behavior changes; much of the CLI relies on shared global state.
- Prefer small, surgical edits over broad rewrites.
- Preserve backward compatibility for existing flags and output shape unless the task explicitly requires a breaking change.
- Do not remove graceful fallbacks for providers, clipboard tools, or Azure DevOps detection without strong justification.
- If you cannot run a real end-to-end flow because secrets are missing, still run syntax checks and the safest non-network commands.

## Verification Notes
- `--dry-run` is the best built-in verification mode because it avoids LLM calls while exercising argument parsing, git checks, config loading, and prompt generation.
- `--help` and `--version` are the safest smoke tests for quick CLI validation.
- Full PR creation requires valid Azure DevOps credentials and should not be claimed as verified unless actually exercised.
- Provider call paths require real API keys; if unavailable, state that verification was limited to non-network paths.
