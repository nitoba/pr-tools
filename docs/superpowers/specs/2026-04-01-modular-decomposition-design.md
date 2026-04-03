# Modular Decomposition of pr-tools Scripts — Design Spec

## Problem

`bin/create-pr-description` has 2401 lines and `bin/create-test-card` has 1864 lines. Both are monolithic bash scripts with ~40% duplicated code (logging, prompts, config, env persistence, provider key testing, Azure remote parsing, LLM provider config).

## Goal

Decompose into reusable modules following the "moderate" approach: shared `lib/` files sourced by thin orchestrator scripts.

## Architecture

```
pr-tools/
├── lib/
│   ├── common.sh    (~250 lines) — logging, colors, prompts, env, config, validation, update
│   ├── llm.sh       (~450 lines) — provider config, payload, API calls, streaming, SSE, fallback
│   └── azure.sh     (~250 lines) — remote parsing, repo ID cache, PR links, PR creation API
├── bin/
│   ├── create-pr-description  (~700 lines) — orchestrator + script-specific logic
│   └── create-test-card       (~1400 lines) — updated to source lib/common.sh
└── install.sh                 — updated to install lib/ files
```

### Module Sourcing

Each script resolves `LIB_DIR` relative to its own location:

```bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_DIR="${SCRIPT_DIR}/../lib/pr-tools"
```

When installed to `~/.local/bin/`, libs live at `~/.local/lib/pr-tools/`. In the repo, the relative path `../lib/pr-tools` doesn't work (scripts are in `bin/`, libs in `lib/`), so we use a fallback:

```bash
if [[ ! -d "$LIB_DIR" ]]; then
  LIB_DIR="${SCRIPT_DIR}/../lib"
fi
```

This way both installed (`~/.local/bin/` + `~/.local/lib/pr-tools/`) and dev (`bin/` + `lib/`) layouts work.

### Guard Against Double-Sourcing

Each lib file uses a guard variable:

```bash
[[ -n "${_PR_TOOLS_COMMON_SH:-}" ]] && return 0
_PR_TOOLS_COMMON_SH=1
```

## Module Breakdown

### lib/common.sh

**From create-pr-description (canonical version unless noted):**

| Function                   | Source Lines | Notes                                                               |
| -------------------------- | ------------ | ------------------------------------------------------------------- |
| Color variables            | 22-29        | Add NO_COLOR support from create-test-card                          |
| `log_error()`              | 54-56        |                                                                     |
| `log_warn()`               | 58-60        |                                                                     |
| `log_info()`               | 62-64        | Add RAW_OUTPUT conditional from create-test-card                    |
| `log_success()`            | 66-68        |                                                                     |
| `debug_log()`              | —            | From create-test-card 186-190                                       |
| `set_env_var()`            | —            | Use create-test-card's version (226-255), more robust (mkdir+touch) |
| `ensure_env_key_comment()` | —            | From create-test-card 257-265                                       |
| `confirm_overwrite()`      | 375-386      |                                                                     |
| `prompt_value()`           | 390-420      |                                                                     |
| `prompt_yn()`              | 423-437      |                                                                     |
| `prompt_choice()`          | 441-459      |                                                                     |
| `test_provider_key()`      | 462-518      |                                                                     |
| `test_azure_pat()`         | 521-533      |                                                                     |
| `validate_dependencies()`  | 840-851      |                                                                     |
| `load_config()`            | 900-943      | Parameterize STREAM_MODE handling                                   |
| `do_update()`              | 215-260      | Parameterize: takes script_name and repo_url                        |

### lib/llm.sh

**From create-pr-description only (create-test-card keeps its simpler LLM code):**

| Function                                     | Source Lines |
| -------------------------------------------- | ------------ |
| `get_provider_config()`                      | 1301-1333    |
| `build_openai_compatible_payload()`          | 1335-1378    |
| `execute_openai_compatible_request()`        | 1380-1401    |
| `normalize_llm_content()`                    | 1407-1409    |
| `parse_openai_sse_stream()`                  | 1413-1459    |
| `parse_gemini_sse_stream()`                  | 1461-1496    |
| `execute_openai_compatible_stream_request()` | 1498-1539    |
| `execute_gemini_stream_request()`            | 1541-1570    |
| `is_groq_reasoning_format_retryable_error()` | 1572-1591    |
| `call_llm_api()`                             | 1593-1733    |
| `call_gemini_api()`                          | 1735-1857    |
| `call_with_fallback()`                       | 1859-1900    |

Reads globals: `STREAM_MODE`, `PR_PROVIDERS`, `OPENROUTER_API_KEY`, `GROQ_API_KEY`, `GEMINI_API_KEY`, `OPENROUTER_MODEL`, `GROQ_MODEL`, `GEMINI_MODEL`.
Writes globals: `USED_PROVIDER`, `USED_MODEL`, `LLM_RESULT`, `PROVIDER_*`.

### lib/azure.sh

**From create-pr-description:**

| Function                | Source Lines |
| ----------------------- | ------------ |
| `parse_azure_remote()`  | 1153-1201    |
| `get_cached_repo_id()`  | 1203-1210    |
| `cache_repo_id()`       | 1212-1221    |
| `fetch_repo_id()`       | 1223-1268    |
| `build_pr_links()`      | 1270-1297    |
| `resolve_reviewer_id()` | 1929-1975    |
| `create_pr_via_api()`   | 1977-2076    |
| `offer_pr_creation()`   | 2078-2124    |

Reads globals: `AZURE_PAT`, `AZURE_ORG`, `AZURE_PROJECT`, `AZURE_REPO`, `AZURE_REPO_ID`, `CACHE_FILE`, `IS_AZURE_DEVOPS`, `TARGETS`, `BRANCH_NAME`, `SPRINT_BRANCH`, `WORK_ITEM_ID`, `PR_TITLE`, `PR_BODY`, `PR_REVIEWER_DEV`, `PR_REVIEWER_SPRINT`.
Writes globals: `AZURE_ORG`, `AZURE_PROJECT`, `AZURE_REPO`, `AZURE_REPO_ID`, `IS_AZURE_DEVOPS`, `PR_LINKS`.

### bin/create-pr-description (orchestrator)

**Keeps script-specific logic:**

| Section    | Functions/Content                                                                                                        |
| ---------- | ------------------------------------------------------------------------------------------------------------------------ |
| Constants  | VERSION, CONFIG_DIR, ENV_FILE, TEMPLATE_FILE, CACHE_FILE, REPO_URL                                                       |
| Defaults   | DEFAULT*PROVIDERS, DEFAULT*\*\_MODEL                                                                                     |
| State      | All global state vars (BRANCH_NAME, GIT_DIFF, etc.)                                                                      |
| Templates  | DEFAULT_TEMPLATE, DEFAULT_ENV                                                                                            |
| CLI        | show_help(), parse_args()                                                                                                |
| Init       | run_setup_wizard(), do_init()                                                                                            |
| Validation | validate_git_repo(), validate_not_base_branch(), validate_config(), validate_api_keys()                                  |
| Git        | collect_git_context(), detect_sprint(), detect_work_item()                                                               |
| Output     | detect_clipboard(), copy_to_clipboard(), detect_md_renderer(), render_markdown(), parse_title_and_body(), print_output() |
| Main       | main()                                                                                                                   |

### bin/create-test-card

- Source `lib/common.sh`
- Remove duplicated functions: log\_\*, set_env_var, ensure_env_key_comment, prompt_value, prompt_yn, test_azure_pat, test_provider_key, validate_dependencies, load_config
- Keep all Azure DevOps API wrappers (azure_get, azure_post_json, etc.) — different from create-pr-description's approach
- Keep its own simpler LLM code (no streaming)

### install.sh

- Download lib files to `~/.local/lib/pr-tools/`
- Create the directory structure

## Constraints

- No behavior changes — pure refactoring
- All existing CLI flags and output must remain identical
- Global variables continue to work as before (sourced files share the same shell scope)
- Both dev (from repo) and installed (from ~/.local/) layouts must work
- `create-test-card` keeps its own LLM and Azure API code (simpler, different patterns) — only shared utilities are extracted

## Testing

- `bash -n` syntax check on all files
- `create-pr-description --help` and `--version` must work
- `create-test-card --help` and `--version` must work
- `create-pr-description --dry-run` in a git repo to verify full pipeline
- `create-test-card --init` to verify shared config functions work
