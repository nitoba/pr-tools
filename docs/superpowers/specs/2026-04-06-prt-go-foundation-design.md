# PRT Go Foundation — Design Spec

## Problem

The current CLI is Bash-based and functionally proven, but it has structural limitations:

- modularization is difficult to scale without splitting logic across many sourced files
- testing is expensive and limited compared to a compiled language with a mature testing ecosystem
- installation currently depends on downloading multiple files instead of a single artifact
- Windows support depends on Git Bash or WSL, which adds friction for end users
- long-term maintenance becomes harder as feature count and provider integrations grow

The repository already improved the Bash implementation by extracting shared libraries under `apps/cli/src/lib`, but that only partially solves maintainability. Distribution, portability, and testability remain weak points.

## Goal

Establish a new Go-based CLI foundation for `pr-tools` using a single cross-platform binary named `prt`, with a shorter and cleaner command UX:

- `prt desc`
- `prt test`
- `prt init`
- `prt doctor`

This milestone is intentionally not a full Bash feature port. It creates the new architecture, command surface, configuration model, test foundation, and release pipeline needed to migrate behavior safely in later phases.

## Non-Goals

This foundation milestone does not attempt to:

- fully port `create-pr-description`
- fully port `create-test-card`
- preserve flag-for-flag compatibility with the current Bash CLI
- preserve the current installer contract based on `install.sh`
- implement all LLM provider logic and Azure DevOps flows immediately
- maintain legacy command names as first-class entrypoints

## Product Direction

The repository and project identity remain `pr-tools`, but the installed executable becomes `prt`.

The new UX favors short commands over long command names:

- `prt desc` becomes the future home of PR description generation
- `prt test` becomes the future home of Azure DevOps test card generation
- `prt init` bootstraps local configuration
- `prt doctor` validates the local environment and configuration state

This is a reimagined CLI, not a line-by-line rewrite. The Go implementation should carry forward the product purpose, not the accidental shape of the Bash scripts.

## Recommended Approach

Use a single Go binary built around:

- `cobra` for CLI structure, parsing, help output, and command composition
- explicit config code in internal packages instead of `viper`
- `testify` for assertions and test ergonomics
- `goreleaser` for cross-platform build and release automation

This approach is preferred over a stdlib-only CLI because it reduces boilerplate around subcommands and help generation without pulling in a large configuration abstraction layer.

## Architecture

### High-Level Shape

Create a new Go CLI application under a dedicated app directory, separate from the current Bash implementation while migration is in progress.

Suggested structure:

```text
apps/cli-go/
  cmd/prt/
    main.go
  internal/cli/
    root.go
    desc.go
    test.go
    init.go
    doctor.go
  internal/config/
    config.go
    env.go
    paths.go
  internal/doctor/
    doctor.go
  internal/setup/
    bootstrap.go
  internal/platform/
    os.go
  internal/ui/
    output.go
  internal/version/
    version.go
  go.mod
```

The boundaries are intentionally simple:

- `cmd/prt` only boots the app
- `internal/cli` owns command construction and wiring
- `internal/config` owns file paths, env parsing, config loading, and precedence rules
- `internal/doctor` owns diagnostics checks and report generation
- `internal/setup` owns bootstrap behavior used by `prt init`
- `internal/platform` owns raw runtime facts only, such as OS, architecture, and resolved home directory
- `internal/ui` centralizes output formatting and terminal behavior
- `internal/version` exposes build metadata injected at release time

Path resolution belongs to `internal/config`, not `internal/platform`.

### Command Design

The root command is `prt`. The first foundation milestone includes these commands:

- `prt desc`
- `prt test`
- `prt init`
- `prt doctor`

`desc` and `test` are real commands from day one, but for this milestone they only need enough implementation to validate architecture and future extension points. They do not need full provider, git, or Azure parity yet.

`init` and `doctor` are the most important functional commands in the first milestone because they establish the installation and support experience for the new binary.

### Milestone 1 Command Contracts

To keep scope explicit, milestone 1 defines the minimum behavior of each command.

- `prt`: prints help and exits `0`
- `prt init`: creates or updates the config foundation, prints what it changed, exits `0` on success
- `prt doctor`: runs foundation-level diagnostics, prints a readable report, exits `0` when no blocking issue is found and non-zero when blocking issues are present
- `prt desc`: foundation command only; prints a clear `not implemented yet` message that points to the migration status, exits `2`
- `prt test`: foundation command only; prints a clear `not implemented yet` message that points to the migration status, exits `2`

For milestone 1, `desc` and `test` are intentionally stubbed commands. Their purpose is to lock the future UX and command structure without forcing a premature port of Bash behavior.

### Configuration Model

Keep the existing user-facing config location to minimize migration friction:

- config dir: `~/.config/pr-tools/`
- config file: `~/.config/pr-tools/.env`

Windows rule for milestone 1:

- resolve the same logical location under the user's home directory for parity, i.e. `%USERPROFILE%/.config/pr-tools/.env`
- a native Windows-specific config relocation is deferred until a later migration phase

Configuration precedence remains explicit:

1. CLI flags
2. shell environment variables
3. values from `.env`
4. internal defaults

The Go implementation should not depend on hidden framework precedence. It should load configuration in a controlled sequence so each layer is easy to reason about and test.

### Config Compatibility Rule

The Go CLI reuses `~/.config/pr-tools/.env`, but it should not inherit Bash behavior implicitly.

Rules:

- Go reads and writes only a documented Go-owned subset of keys needed by the foundation milestone
- unknown keys are ignored, not treated as errors
- Bash-only keys remain untouched in the file
- `prt init` may add Go-relevant defaults/comments, but it must not delete existing user keys
- later migration phases may expand the supported key set explicitly, never implicitly

For milestone 1, Go-owned keys should use a `PRT_` prefix. Existing non-prefixed keys from the Bash CLI are preserved but ignored unless explicitly adopted in a later migration phase.

Milestone 1 key subset:

- `PRT_CONFIG_VERSION=1`
- `PRT_NO_COLOR=false`
- `PRT_DEBUG=false`

Milestone 1 `.env` parsing rules:

- accept blank lines and comment lines beginning with `#`
- accept `KEY=value` and `export KEY=value`
- accept single-quoted, double-quoted, and unquoted values
- unknown keys are ignored after parsing
- when duplicate keys appear, the last valid occurrence wins
- malformed lines are reported by `doctor` as parse issues for Go-owned keys; malformed unknown lines are preserved and ignored for milestone 1

This preserves user data while preventing accidental coupling between the new Go CLI and the old Bash implementation.

### Setup Flow

`prt init` is responsible for foundation-level setup only. It should:

- create the config directory when missing
- create or update `.env`
- write safe default keys/comments for future features
- prepare the config foundation for later `desc` and `test` expansion

The initial setup should stay intentionally small. The current Bash wizard has useful behavior, but the Go CLI should avoid porting it wholesale before the new command model and internal boundaries are proven.

Milestone 1 idempotency rules:

- repeated runs must not remove unknown keys
- repeated runs must not duplicate Go-owned keys
- when a Go-owned key already exists, `init` leaves the user value in place unless an explicit overwrite mode is added in a later phase
- unknown lines must be preserved verbatim
- relative order of non-Go-owned lines must be preserved
- `init` uses an append-only/update-in-place strategy for Go-owned keys rather than reserializing the whole file
- missing Go-owned keys are appended in one dedicated managed block
- the resulting file must remain stable across repeated runs with no logical changes

Managed block contract:

- block start marker: `# --- PRT managed block start ---`
- block end marker: `# --- PRT managed block end ---`
- the block is appended at end of file when first created
- Go-owned keys should live inside the managed block in milestone 1
- if a Go-owned key already exists outside the managed block, `init` must preserve it and must not duplicate that key inside the managed block

Malformed managed-block recovery rules:

- a valid file should contain at most one managed block
- if multiple managed blocks are found, `init` must fail with exit `1` and report the ambiguity instead of attempting automatic repair
- if managed block markers are partially corrupted or unmatched, `init` must fail with exit `1` and report the problem
- if Go-owned keys exist both inside and outside a valid managed block, `init` must keep the existing user-owned values and avoid creating duplicates

### Diagnostics Flow

`prt doctor` should give fast, actionable feedback about:

- config file presence
- foundation config parseability
- current version/build metadata

It should diagnose, not mutate, except where a later explicitly interactive repair flow is added.

### Milestone 1 Doctor Contract

`prt doctor` must report these checks explicitly:

- config directory exists or is creatable
- config file exists or is missing
- `.env` is readable
- supported foundation keys can be parsed
- current executable version/build metadata is available
- OS and architecture are detected

Milestone 1 `doctor` is intentionally limited to the Go foundation. It must not fail based on Bash-era external tools, provider CLIs, or future migration dependencies.

For local development builds without ldflags, `doctor` should report version metadata using safe fallback values such as `dev` and `unknown` and must not fail for that reason.

`creatable` must be determined without mutating state, using checks such as parent-directory existence and write permission evaluation.

Blocking matrix for milestone 1:

- missing config directory but creatable: non-blocking, exit `0`
- missing `.env`: non-blocking, exit `0`
- unreadable `.env`: blocking, exit `1`
- invalid syntax affecting Go-owned keys: blocking, exit `1`
- malformed or unknown non-Go-owned lines: non-blocking, exit `0`
- missing ldflags metadata in local dev builds: non-blocking, exit `0`

## Testing Strategy

Use `testify` as the default testing toolkit.

Testing guidelines:

- use `require` for prerequisites and fatal checks
- use `assert` for follow-up validations
- prefer table-driven tests for parsing, precedence, and path resolution
- use `suite` only where shared setup materially improves clarity
- avoid mocking by default; prefer small interfaces and temp-directory based tests

Foundation milestone test coverage should include:

- config loading from `.env`
- environment variable override behavior
- default value resolution
- path handling across Linux/macOS/Windows runtime facts
- command registration and help behavior
- `init` filesystem behavior using temp directories
- `doctor` result generation for common scenarios
- version/build metadata injection points

The test suite should prioritize deterministic unit tests first, then add a thin layer of CLI integration tests around command execution.

Path resolution logic must be testable without depending on the host runtime OS. Resolvers should accept explicit runtime facts such as OS name and home directory during tests.

### Required Test Bar for Milestone 1

Minimum required verification:

- unit tests for config precedence and parsing
- unit tests for path resolution on Linux/macOS/Windows path rules where logic is platform-conditional
- unit tests for `init` idempotency using temp directories
- CLI tests proving `prt`, `prt init`, `prt doctor`, `prt desc`, and `prt test` are registered and return the expected exit behavior
- CI must run `go test ./...` from `apps/cli-go`
- CI must build the Go binary from `apps/cli-go` at least on the host runner used by the workflow

Cross-compilation for release targets is required in the release pipeline. Full runtime smoke execution on every target OS is not required for milestone 1.

CLI testing contract:

- command-level unit tests assert command wiring and returned errors
- subprocess-style integration tests assert final process exit codes `0`, `1`, and `2` where applicable

## Build and Release

The new distribution model is one of the main reasons for the rewrite, so release automation is part of the foundation milestone.

Use `goreleaser` to produce a single executable named `prt` for:

- Linux amd64/arm64
- macOS amd64/arm64
- Windows amd64/arm64

The release pipeline should:

- build cross-platform artifacts
- inject version metadata through ldflags
- publish GitHub release assets
- generate checksums
- run `prt --help` on the host runner as the required smoke test for the release workflow

Artifact contract for milestone 1:

- Linux and macOS artifacts are published as `.tar.gz`
- Windows artifacts are published as `.zip`
- each archive contains a single executable named `prt` or `prt.exe` on Windows
- asset names follow a platform-explicit pattern such as `prt_<version>_<os>_<arch>.<ext>`
- a checksum file is published alongside the release assets

The existing Bash release flow remains in place during migration. The Go release path is additive at first, not a destructive replacement.

### Distribution During Migration

For milestone 1, the primary distribution path for the Go CLI is GitHub release assets.

Rules:

- `apps/cli/install.sh` remains the installer for the Bash CLI only
- the Go CLI is installed by downloading the appropriate `prt` release artifact
- updating the primary one-line install path is deferred until the Go CLI becomes the primary product path
- Go and Bash assets are published on the same GitHub release/tag during migration, with non-overlapping asset names

Release coordination rule during migration:

- Go asset publication must be gated so a failed Go publish does not silently produce a misleading mixed release
- all semver tags used for releases publish Go assets during milestone 1
- the workflow must fail loudly when expected Go artifacts are missing from the release

### Release Versioning During Migration

During migration, the Go CLI shares the repository tag stream with the project, but release assets must distinguish Bash and Go deliverables clearly.

Rules:

- repository tags remain the single version source for both implementations during transition
- Go artifacts are published with `prt` in the asset names
- existing Bash assets keep their current naming until deprecation is planned
- release notes and documentation must make it explicit which assets belong to Bash and which belong to Go

This avoids introducing two independent version systems while still keeping migration artifacts understandable.

## CLI Execution Contract

Command handlers should return errors upward instead of calling `os.Exit` directly. Root command execution owns final exit-code mapping.

Milestone 1 mapping:

- success: `0`
- blocking validation or runtime error: `1`
- intentionally unimplemented command (`desc`, `test`): `2`

This keeps command behavior testable and makes exit semantics consistent across the CLI.

## Migration Strategy

The migration should be staged.

### Phase 1: Foundation

- add `apps/cli-go`
- create `prt` root command and foundation subcommands
- implement config/env handling
- implement `init` and `doctor`
- add tests with `testify`
- add cross-platform release automation

### Phase 2: Functional Port by Capability

- move PR description generation into `prt desc`
- move test card generation into `prt test`
- port integrations incrementally instead of porting the scripts wholesale

### Phase 3: Migration and Deprecation

- document the new CLI as primary
- decide whether Bash commands remain supported temporarily, become wrappers, or are removed
- simplify installation and release documentation around the Go binary

This phased approach reduces risk and avoids mixing architecture work with a full behavioral rewrite in one pass.

## Risks

### Scope Creep

Trying to port all Bash behavior inside the foundation milestone would delay the architectural reset and likely recreate old coupling inside Go.

### Over-Abstraction Too Early

Introducing too many domain layers, provider abstractions, or mock-heavy interfaces before real Go features land would create maintenance overhead without validated value.

### Installer Ambiguity During Migration

For a period, the repository will contain both the Bash CLI and the Go CLI. Documentation and release naming must clearly distinguish the two until the migration is complete.

### Behavioral Drift

Because this is a reimagining rather than strict parity, later ports must intentionally decide which Bash behaviors are still product requirements and which are implementation artifacts worth dropping.

## Success Criteria

The foundation milestone is successful when:

- the repository contains a new Go CLI app under `apps/cli-go`
- `prt`, `prt init`, `prt doctor`, `prt desc`, and `prt test` exist and run
- configuration loading is explicit, testable, and documented
- tests run with `go test ./...` in the Go CLI app and use `testify`
- release automation can generate cross-platform `prt` binaries
- the project has a clean base for incremental migration of `desc` and `test`

## Open Decisions Deferred to Implementation Planning

These decisions are intentionally deferred until the implementation plan:

- exact module path naming for `go.mod`
- whether `doctor` prints plain text, structured sections, or status tables

## Summary

The right first step is not a literal Bash port. It is a Go-first foundation that introduces a single binary, short commands, explicit config, a real test base, and cross-platform release automation. That foundation should be small, opinionated, and deliberately incomplete so later migration work lands on stable boundaries instead of carrying Bash complexity into a new language.
