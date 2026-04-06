# Changelog

## 3.1.0 — 2026-04-06


### Bug Fixes

- Fix remaining errcheck in cli package (`86622aa`)

- Fix errcheck in ui package (`cdc6f12`)

- Fix lint issues in wizard and ui packages (`b838c4b`)


### Features

- Add UI, full prompts, and interactive flows for prt desc/test (`38219ba`)

- Add interactive setup wizard for prt init (`566813e`)


## 3.0.0 — 2026-04-06


### Bug Fixes

- Extract git-cliff to /tmp to avoid dirty git state (`fde6174`)

- Fix remaining errcheck lint issues in test.go and llm providers (`342c8b3`)

- Resolve all golangci-lint errors (`c902a43`)

- Remove unused codeLang assignment in markdown parser (`de01cac`)

- Update release.sh and auto-tag.yml for Go-only CLI (`4b9bd1b`)

- Switch ollama to cloud API (https://ollama.com/v1/chat/completions) (`f54a3e4`)

- Replace @vercel/og with satori + @resvg/resvg-wasm (`07f3b25`)

- Update bun.lock with @vercel/og dependency (`a222d73`)

- Add missing Steps, Step, Tabs, Tab imports to MDX files (`fb4551f`)

- Use @tailwindcss/vite plugin (correct TanStack Start setup) (`3da7de5`)

- Add postcss-import to resolve CSS @import ordering with Tailwind v4 (`1d861f8`)

- Fix PostCSS ordering by importing fumadocs CSS from JS instead of CSS chain (`b556c37`)

- Align TanStack Start setup with official fumadocs guide (`1f1e11f`)

- Use target vite in postinstall (bun target not available in installed version) (`f5d0367`)

- Add postinstall script to generate .source/ for fumadocs-mdx Vite (`0c1a65e`)

- Pin fumadocs deps versions and add docs scripts to monorepo root (`f037cff`)

- Add trailing newlines and commit bun.lock for apps/www (`3445b25`)

- Remove broken schema path from .oxfmtrc.json (`4e46191`)


### CI/CD

- Fix golangci-lint version to v2.11.4 (`cedffb0`)

- Upgrade all GitHub Actions to latest major versions (`12ad48e`)

- Upgrade golangci-lint-action to v9 (`a301473`)

- Pin golangci-lint to v2 for config schema compatibility (`8caa6a2`)

- Update golangci-lint configuration (`9650481`)

- Add path filters for cli app to workflow triggers (`7d0bfc6`)

- Specify config path for wrangler deploy (`ab09595`)

- Switch deployment from Cloudflare Pages to Workers (`8622b04`)

- Add Cloudflare deployment for docs and www apps (`960a44b`)


### Chores

- Bump version to v3.0.0 (`626416e`)

- Simplify pipeline — goreleaser only, fix monorepo build paths (`6ba3df8`)

- Remove Bash CLI and its CI workflow (`4277dc7`)

- Add Go binaries to gitignore (`a07cfa1`)

- Update generated route tree with /api/og route (`e047474`)

- Replace vite-tsconfig-paths with Vite native tsconfigPaths (`a771876`)

- Update bun.lock after full workspace install (`2492fd6`)

- Update release paths to apps/cli (`85c438c`)

- Migrate cli code to apps/cli (`89ba9cd`)

- Add oxlint and oxfmt to monorepo root (`cd2c872`)

- Initialize bun monorepo workspace (`540ccd7`)


### Documentation

- Rewrite README for prt Go CLI as primary (`b3383c9`)

- Add Phase 3 implementation plan (`6df9713`)

- Add Phase 3 migration design spec (`ad4701d`)

- Write changelog placeholder page (`baac2d1`)

- Write troubleshooting reference page (`414770b`)

- Write environment-variables reference page (`d88353f`)

- Write advanced-examples guide page (`34d42c3`)

- Write markdown-rendering guide page (`31341aa`)

- Write ai-providers guide page (`025ccea`)

- Write azure-devops guide page (`f32caa1`)

- Write create-test-card command page (`1fcdeaf`)

- Write create-pr-description command page (`933887d`)

- Write configuration page (`825e8d7`)

- Write quickstart page (`5554d4d`)

- Write installation page (`067dbf9`)

- Write introduction page (`435fd70`)

- Add documentation implementation plan (`3bcabe5`)

- Add documentation design spec for pr-tools docs site (`0240173`)

- Update plan and spec to Fumadocs Vite (replace Mintlify) (`e21420f`)

- Update monorepo foundation plan and design spec to use Fumadocs instead of Mintlify (`42a7b33`)

- Rewrite plan to use CLI scaffolding + correct oxfmt package (`f601fd7`)

- Add monorepo foundation implementation plan (`a572e96`)

- Scope spec to foundation only, defer content implementation (`c5aabce`)

- Add monorepo design spec (www + docs + newsletter) (`5480bf9`)


### Features

- Add Windows PowerShell install script (`8272474`)

- Add Linux/macOS install script (`dab3349`)

- Implement prt test command (`8775a80`)

- Implement prt desc command (`78b28fc`)

- Add cross-platform clipboard support (`6147437`)

- Add Azure DevOps client (`1bce868`)

- Add LLM client interface and implementations (`9c49c5e`)

- Add git context package (`31a42af`)

- Add PR/Test config keys (`ad59341`)

- Add prt CLI foundation (`7f53eec`)

- Add golang skills and go cli foundation (`ad7deed`)

- Add dynamic OG image generation with @vercel/og (`5664e90`)

- Add per-page SEO meta tags based on frontmatter (`48681f9`)

- Add SEO meta tags and favicons for better search ranking (`08e1cbb`)

- Externalize docs link in navbar (`68ba4c8`)

- Implement landing page and newsletter automation (`7a36faf`)

- Migrate from vinext/Next.js to TanStack Start with fumadocs-ui (`015f2f5`)

- Migrate to Next.js App Router with vinext (`96a730b`)

- Monorepo foundation — apps/cli, apps/www (Astro), apps/docs (Fumadocs Vite) (`e842fb2`)

- Scaffold apps/docs with Fumadocs Vite and MDX stubs (`2791c5d`)

- Scaffold apps/docs with Fumadocs Vite and MDX stubs (`97e478c`)

- Scaffold apps/www with Astro 5, Tailwind CSS 4 and React (`dff0541`)


### Refactoring

- Move Go config to apps/cli-go/ (`b5a4bf9`)

- Update Cloudflare deployment workflows to direct wrangler execution (`ed29f6f`)

- Update workflow paths to apps/cli and integrate Node build steps (`abf8bea`)

- Remove custom theme color variables from app.css (`5731f48`)


### Tests

- Add testing script and switch email template to dark mode (`28ffae6`)


### Build

- Add tailwindcss dependency (`2207c95`)


### Redesign

- Refactor UI with emerald palette and premium design (`b6a3a9d`)


### Style

- Standardize code formatting and update development configurations (`a3d5985`)


## 2.9.8 — 2026-04-03


### Bug Fixes

- Add workflow_dispatch trigger to release workflow (`eb49f0f`)


### Chores

- Bump version to v2.9.8 (`a4b8450`)


## 2.9.7 — 2026-04-03


### Bug Fixes

- Explicitly trigger Release workflow after auto-tag creation (`eef62f0`)


### Chores

- Bump version to v2.9.7 (`77c8cf9`)


## 2.9.6 — 2026-04-03


### Chores

- Bump version to v2.9.6 (`85c4bbf`)


## 2.9.5 — 2026-04-03


### Chores

- Bump version to v2.9.5 (`9fae868`)

- Bump version to v2.9.6 (`e9adf71`)


### Features

- Switch release flow to PR-based workflow with auto-tagging (`6716c87`)


## 2.9.4 — 2026-04-03


### Bug Fixes

- Include OLLAMA_API_KEY in validate_api_keys and validate_provider_keys (`df04b6d`)


### Chores

- Bump version to v2.9.4 (`4a7004b`)

- Bump VERSION file to v2.9.4 (`d020ff6`)

- Bump version to v2.9.3 (`93bdef5`)


### Documentation

- Add Ollama Cloud provider implementation plan (`d9f58c5`)

- Add Ollama Cloud provider design spec (`e88dbce`)


### Features

- Add ollama provider support to create-test-card config (`a5003ac`)

- Add ollama provider support to create-pr-description wizard and config (`f69aac8`)

- Add ollama case to call_with_fallback in test-card-llm.sh (`78671ed`)

- Add ollama case to get_provider_config in llm.sh (`9b69bd4`)

- Add ollama to test_provider_key and load_config in common.sh (`9d6fa25`)


## 2.9.2 — 2026-04-03


### Bug Fixes

- Package release assets into a single zip archive (`c574fdd`)


### Chores

- Bump version to v2.9.2 (`1ef8ace`)


## 2.9.1 — 2026-04-03


### Bug Fixes

- Add .shellcheckrc and fix SC2064 trap quoting (`8e535a5`)

- Prevent subshells from killing parent's spinner process (`165ec64`)

- Remove spinner from Test QA update step to allow interactive prompts (`f6cdcf5`)

- Align hierarchy connector after title icon position (`4ac32d8`)

- Title text color to orange (#c15f3c dim) instead of gray (`7e76a3f`)

- Reduce spacing between hierarchy connector and status icons (`58fa22d`)

- Rewrite spinner UI to use single background process (`43aad2b`)

- Correct Portuguese diacritical marks in scripts and docs (`e98e8ca`)

- Correct Portuguese language errors in log messages and prompts (`8033031`)

- Show branch name after git context collection (`7c6a3c5`)

- Auto-download libs on first run after update from monolithic version (`15f93df`)

- Ensure .env creation on init and disable streaming by default (`13d69aa`)

- Prompt for Effort when empty before Test QA transition (`57db64f`)

- Add PATCH method and error message extraction (`e56c11f`)

- Populate Azure DevOps test case steps (`0a422ca`)

- Refine test card creation flow (`83df2ff`)

- Normaliza \n literais no conteudo retornado pelos providers LLM (`b91c490`)

- Normaliza \n literais emitidos por alguns modelos LLM (ex: qwen) (`cf7541e`)

- Corrige parsing de titulo/descricao e retorno de conteudo LLM (`4db9fd4`)

- Retry groq requests when reasoning_format is unsupported (`c44ed54`)

- Set PR reviewers as required instead of optional (`6394648`)

- Use sprint branch as diff base instead of dev (`a37670d`)

- Use api-version 7.0-preview.1 for IdentityPicker API (`58d1b7d`)

- --init now shows each missing provider individually (`574caed`)

- Use #number instead of AB#number for work item linking (`f6db6fd`)

- Suppress reasoning/thinking output from LLM responses (`a820bf5`)

- Use origin/dev over local dev, and temp files for JSON payload (`1ba053a`)

- Use temp file for git diff instead of variable capture (`9c203b9`)

- Patch 3 silent failures found by set -euo pipefail audit (`1822529`)

- Robust git diff with proper error handling and diagnostics (`6af379f`)

- Use origin/dev as fallback when local dev branch doesn't exist (`9c861b8`)

- Prompt functions now display correctly in interactive wizard (`ac0f6f9`)


### CI/CD

- Add release workflow with git-cliff changelog generation (`26d6823`)

- Add CI workflow with shellcheck, syntax check, and smoke tests (`c789be7`)

- Add opencode workflow (`4baa452`)


### Chores

- Bump version to v2.9.1 (`82d6f0a`)

- Localize release script messages to Portuguese with accents (`7d0dde2`)

- Add VERSION file as single source of truth (`c871bcb`)

- Add git-cliff configuration for conventional commits (`171eabc`)

- Add .worktrees to gitignore (`9a837ce`)

- Bump versions for spinner UI fix (`75b675f`)

- Bump versions for sparkle title UI (`9f485cf`)

- Bump versions for spinner UI feature (`cc15fa3`)

- Add ui.sh to download, install, and source lists (`d64307d`)

- Bump create-test-card to v0.1.7 (`e4871f4`)

- Bump script versions (`194f687`)

- Bump script versions (`d4030aa`)

- Bump create-test-card version para 0.1.3 (`331eb2e`)

- Switch linear mcp to remote configuration (`975d4f5`)

- Bump to v2.2.3 to force update delivery (`478fa61`)

- Bump version to 1.1.0 (`4de5fe9`)


### Documentation

- Document versioned installation and release process (`92196d9`)

- Generate initial changelog from git history (`f31b4ed`)

- Fix Portuguese spelling and markdown table formatting (`00fd2d4`)

- Add spinner UI implementation plan (`9e5655d`)

- Add spinner UI design spec (`c559260`)

- Add decomposition spec and implementation plan (`861c293`)

- Add groq reasoning fallback spec and plan (`a5089ff`)

- Update README with Gemini support, auto PR creation, and all CLI flags (`626bcc0`)


### Features

- Add automated release script and deployment documentation (`baa7c48`)

- Read version from VERSION file with hardcoded fallback (`8e9362c`)

- Support installing from specific versions via INSTALL_VERSION env var (`7168662`)

- Apply consistent UI style to post-generation output sections (`3628429`)

- Align log_info/log_warn/log_error with hierarchy UI when title active (`ea931ba`)

- Orange sparkle title, gray hierarchy connector, improved alignment (`b5779f4`)

- Sparkle icon animation on title header (`4ad0a97`)

- Add pulsing title header and hierarchy connectors to spinner UI (`648cc56`)

- Integrate spinner UI into create-test-card (`61d06a1`)

- Integrate spinner UI into create-pr-description (`f500621`)

- Add spinner UI library (src/lib/ui.sh) (`e0c3986`)

- Prompt for Real Effort before Test QA state update (`767b0cc`)

- Prompt for Test QA transition (`beed3c0`)

- Add Azure DevOps test card generator (`cc6a9aa`)

- Make streaming the default behavior (`670fffe`)

- Add streaming responses for LLM API calls (`7a4273e`)

- Add --source flag to override PR source branch (`6840a38`)

- Render PR description with Markdown syntax highlight in terminal (`5ab44bb`)

- Add Linear skill for issue and project management (`c11d905`)

- Support NUMERO/descricao branch pattern for work item detection (`08827a5`)

- Add Google Gemini as LLM provider (`4ba9e03`)

- --init now only asks about missing config, preserves existing .env (`b6fc8bd`)

- Create PRs directly via Azure DevOps API with reviewers (`122c81a`)

- Add wl-copy support for Wayland clipboard on Linux (`b3b6af5`)

- Add work item ID detection and linking (`26e30c8`)

- Add PR title generation, --dry-run and --update flags (`56dd80d`)

- Add interactive setup wizard and --set-*-model flags (`62418c6`)

- Initial release - PR description generator with LLM support (`a16b688`)


### Refactoring

- Move bin/ and lib/ under src/ directory (`a107475`)

- Extract Azure and LLM modules from create-test-card into lib/ (`244af63`)

- Use lib/common.sh for shared utilities in create-test-card (`1995a72`)

- Install lib/ modules alongside scripts (`9fd4bcd`)

- Rewrite create-pr-description as orchestrator sourcing lib modules (`95587e2`)

- Extract Azure DevOps integration into lib/azure.sh (`b3a7e15`)

- Extract LLM provider logic into lib/llm.sh (`e7b68e7`)

- Extract shared utilities into lib/common.sh (`011e294`)

- Improve test card generation to focus on observable behavior (`d5e987a`)


