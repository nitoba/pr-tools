# Issue 10 Verification

**Verdict**: PASS
**Profile**: light (no profile declaration found; default applied)
**Diff range**: 646bf2a..56f3a8b
**Round**: 1 - full
**Verifier**: independent verifier; implementation was not modified

## Scope and binding artifacts

Read `.checks/issue-10-update-existing-pr.md`, `.tasks/issue-10-update-existing-pr.md`, `.design/issue-10-update-existing-pr.md`, repository `AGENTS.md`, `.agents/skills/tlc-implement/SKILL.md`, and `.agents/skills/tlc-implement/references/verify.md`. The design and task agree on an isolated `desc --pr` update journey, exact remote refs, minimal JSON `PATCH`, pre/post reconciliation, and no creation publisher reuse. No contradiction was found. The complete diff was inspected across all 15 changed files: 3,279 insertions and 25 deletions.

## Checks

Every proof used `cargo test --manifest-path apps/rust/Cargo.toml --locked <filter>` without `--exact`; each output reported the named test as `... ok` and exit code `0`. C19's second proof is the required duplicate Azure route proof and was rerun.

| Check | Evidence (test and assertion) | Exit |
|---|---|---:|
| C1 | `apps/rust/src/cli.rs:642-646`; command is `Desc` and PR `42` is asserted | 0 |
| C2 | `apps/rust/src/cli.rs:650-655`; `assert_eq!(...command, Command::Update)` | 0 |
| C3 | `apps/rust/src/cli.rs:662-670`; invalid values assert exit `2` and existing message | 0 |
| C4 | `apps/rust/src/azure/pull_requests.rs:873-895`; ID/status/repository/source/target/title/description and exact route assertions | 0 |
| C5 | `apps/rust/src/features/update_pull_request.rs:673-693`; `PR #42 não encontrado`, GET `1`, PATCH `0`, collection `0` | 0 |
| C6 | `apps/rust/src/features/update_pull_request.rs:647-657`; non-active rejection before generation/write | 0 |
| C7 | `apps/rust/src/features/update_pull_request.rs:660-670`; incompatible repository error assertion | 0 |
| C8 | `apps/rust/src/git/mod.rs:484-490`; missing ref error and no fallback refs | 0 |
| C9 | `apps/rust/src/git/mod.rs:426-481`; captured commands assert `target...source` and `target..source` | 0 |
| C10 | `apps/rust/src/main.rs:499-516`; noninteractive rejection and zero provider/writer effects | 0 |
| C11 | `apps/rust/src/features/update_pull_request.rs:734-770`; prompt fields and invalid proposal/zero writes | 0 |
| C12 | `apps/rust/src/tui/update_flow.rs:899-907`; current/proposal separation and proposal-only editor | 0 |
| C13 | `apps/rust/src/tui/update_flow.rs:910-925`; exact edited proposal and unchanged current | 0 |
| C14 | `apps/rust/src/tui/update_flow.rs:929-987`; title/body bounds, empty body, review phase | 0 |
| C15 | `apps/rust/src/tui/update_flow.rs:991-1005`; draft discarded, current/proposal unchanged, no write | 0 |
| C16 | `apps/rust/src/tui/update_flow.rs:1049-1094`; frozen content, phases, editor disabled, one PATCH | 0 |
| C17 | `apps/rust/src/features/update_pull_request.rs:813-838`; every changed snapshot field conflicts; PATCH `0` | 0 |
| C18 | `apps/rust/src/features/update_pull_request.rs:840-850`; `UpdateOutcome::NoOp`, PATCH `0` | 0 |
| C19 | `apps/rust/src/features/update_pull_request.rs:854-869`; same PR, one patch, exact title/body; route rerun at `873-895` | 0 |
| C20 | `apps/rust/src/features/update_pull_request.rs:872-905`; exact post-GET updates, divergent GET conflicts | 0 |
| C21 | `apps/rust/src/features/update_pull_request.rs:909-1006`; uncertain outcomes and `GET/PATCH/GET` order | 0 |
| C22 | `apps/rust/src/features/update_pull_request.rs:1010-1034`; PAT/permission/`prt doctor` guidance, PATCH `0` | 0 |
| C23 | `apps/rust/src/features/update_pull_request.rs:1038-1096`; isolated update, one patch, no creation path | 0 |
| C24 | Full gates below; snapshots ran with `INSTA_UPDATE=no` | 0 |

## C24 gates

| Command | Result | Exit |
|---|---|---:|
| `INSTA_UPDATE=no cargo test --manifest-path apps/rust/Cargo.toml --locked` | 222 passed, 0 failed, 1 ignored; doctests passed | 0 |
| `cargo fmt --manifest-path apps/rust/Cargo.toml -- --check` | clean | 0 |
| `cargo clippy --manifest-path apps/rust/Cargo.toml --locked --all-targets -- -D clippy::correctness` | completed; warnings only | 0 |
| `cargo build --manifest-path apps/rust/Cargo.toml --locked --all-targets` | completed | 0 |
| `git diff --check` | clean | 0 |

## Fault injection

Not run: no `standard`/`ui` profile declaration was found; the applicable default profile is `light`, which does not require mutation testing.

## Final worktree note

No implementation, checklist, task/design, or snapshot files were modified. No commit was created. This report is the only artifact written by this verification pass.
