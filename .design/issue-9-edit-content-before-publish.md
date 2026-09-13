# Edição de título e Markdown antes da publicação

> Plan this with **tlc-plan** (`.agents/skills/tlc-plan/SKILL.md`).
> Decisions below carry the literal shape - copy them, do not re-derive them.

## Situation

- Project: in active construction.
- Decision: committed by the issue author (@nitoba), 2026-09-12, in [GitHub issue #9](https://github.com/nitoba/pr-tools/issues/9); the requester confirmed the remaining product choices during discovery.
- In flight: `feat/issue-9-allow-edit-title-and-description-before-publish-pr-and-test-cases` - the branch currently contains only the TLC skill additions; the issue implementation has not started. The change touches `apps/rust/src/tui/live.rs`, `apps/rust/src/tui/describe_app.rs`, `apps/rust/src/tui/test_flow.rs`, the shared TUI module, and the existing Azure boundaries.
- At stake: expensive and externally visible if wrong - a wrong title/body can be sent to Azure, and a multi-target publish can create several PRs or Test Cases before the mistake is noticed.

## Problem

Users of `prt desc` and `prt test` cannot correct a generated title or Markdown body inside the existing review flow. They instead copy the body to another editor, abandon the flow, or correct Azure after publication. Small factual corrections therefore either cost a manual detour or reach the remote system too late; the generated content is reviewable but not fully approvable.

## Evidence

- 12 acceptance criteria in issue #9 - the requested behavior covers both flows, save/cancel, exact clipboard and payload use, Unicode/multiline input, and failure recovery.
- No current metric records how often users leave the TUI to correct generated content - measuring that would require an explicit telemetry event or support/workflow data, neither of which is in scope for this issue.
- `DescribeApp.desc` already separates generated title/body, while `TestApp` already stores `title` and `body` separately - repository inspection.
- The current copy contract is body-only in `features::describe::copy_to_clipboard` and the README - repository convention, retained by decision below.

## Journey

1. The command generates title and Markdown body.
2. The user reviews the rendered preview and chooses `Editar conteúdo`.
3. A draft editor opens with title and body loaded from the active content. The title is single-line; the body supports multiple lines, Unicode, paste, horizontal and vertical navigation.
4. `Ctrl+S` validates and commits the draft. The editor closes and the preview immediately renders the committed title/body. `Esc` discards the draft and leaves the previous content unchanged.
5. Before any remote call, the user may copy the body or continue to reviewer/metadata confirmation.
6. At the first remote publish/create call, the approved title/body is snapshotted and frozen for every target in that attempt.
7. On success, the remote payload is the frozen approved content. On failure, the content remains visible and retry reuses the same snapshot without regeneration.
8. For multi-target PR publication, targets still pending use the same frozen content as targets already sent. Recovery may change reviewers/settings only; it does not silently change content.

States that matter:

- Empty title: cannot be saved as publishable content; the user remains in the editor with a validation error. This applies to PRs and Test Cases.
- Empty body: preserve each flow's existing rule. PR keeps its current body rule and maximum-length validation; Test Case keeps `validate_card`, which requires a non-empty body.
- Cancelled edit: the active content and preview remain byte-for-byte unchanged.
- Invalid PR body: the existing `< 4000` character rule remains enforced before publication; edited content is not passed through the generator normalizer.
- Azure/auth/remote failure: existing error and recovery behavior remains, with the frozen approved content available for retry.
- Abandoned review: quitting before a remote call leaves no persisted draft and creates no remote record.

## Verdict

Already committed - see Situation. The cost of leaving the gap is a manual correction path or an incorrect remote record; the committed change is limited to an in-session editor and source-of-truth wiring. Confirmed by the requester, 2026-09-12.

Cheaper paths considered: keeping the review read-only was rejected because it preserves the manual detour; regenerating with IA was rejected because it cannot guarantee a factual correction and is outside the issue; updating Azure after publication was rejected because it is too late for pre-publication approval and is explicitly out of scope.

## Success

- Worked if: in both `prt desc` and `prt test`, the user can save a title/body edit and the preview, body-only clipboard, remote payload, and retry all use the same approved content - by the first release containing issue #9.
- Early signal: a manual smoke run completes the generate → edit → save → preview → copy/create path for both commands within the first validation cycle; the bet is going wrong if preview text differs from the copied body/payload or retry regenerates content.
- Review: first release containing the feature, triggered after one successful and one failed/retried path; the maintainer reviews the smoke run and the automated tests.
- No product metric exists today; the observable proxy is exact equality between the approved in-memory content and every body/title value passed to clipboard or Azure.

## Boundary

In: editing the generated title and Markdown body during review for PRs and Test Cases; multiline Unicode-safe editing; save/cancel; per-flow validation; body-only clipboard; exact content in initial publish/create, retry, and pending multi-target operations; unit, flow, and snapshot tests needed to prove those behaviors.

Out: partial AI regeneration, rewrite instructions, edit history/versioning, cross-machine drafts, persistence between executions, updating already-published PRs, IDE-grade editor features, and changes to existing reviewer or Test Case metadata editing - these are separate problems or already-working flows.

## Shape

Use one lightweight draft/editor model shared by both TUI flows, while retaining each flow's existing active-content representation. A saved draft replaces the active title/body exactly as entered; a cancelled draft is discarded. The first remote attempt stores a frozen `PrDescription` snapshot, so a recovery or pending target cannot accidentally observe a later edit. This pays for a small shared editor now and avoids duplicated key handling and divergent source-of-truth logic.

### Adds

- `apps/rust/src/tui/content_editor.rs` with `TextEditor`, `ContentField`, and `ContentEditState`; `TextEditor` owns Unicode-safe cursor movement, multiline insertion/deletion, viewport movement, and consumed-key handling.
- `tui::content_editor` module registration in `apps/rust/src/tui/mod.rs`.
- `DescribeApp.content_edit: Option<ContentEditState>` and `TestApp.content_edit: Option<ContentEditState>` as draft-only state.
- `DescribeApp.frozen_publish_content: Option<PrDescription>` and `TestApp.frozen_create_content: Option<PrDescription>` as per-attempt snapshots.

### Changes

- `DescribeApp.desc` → remains the canonical approved PR content after save; preview reads it, `on_copy_key` continues copying only `desc.body`, and publish/recovery reads the frozen snapshot once an attempt has started.
- `TestApp.title` and `TestApp.body` → remain the canonical approved Test Case content after save; settings editing stays separate and `copy_body` continues copying only `body`.
- `handle_key_event` in `live.rs` and `handle_test_phase_key` in `test_flow.rs` → dispatch content-editor keys before global review shortcuts while `content_edit` is active. `q`, `j`, `k`, `Tab`, and similar keys are consumed as text/navigation according to the active editor field.
- `start_publish`/`publish_task` → snapshot approved title/body before the first remote call and use that snapshot for every target and recovery retry. Content editing is unavailable after a remote attempt begins.
- `start_create` and Test Case recovery → use the approved content snapshot after the first create attempt; no retry regenerates or rereads a mutable draft.
- PR/Test Case validation boundaries → reject an empty title, preserve the PR body limit, preserve the Test Case non-empty-body rule, and never run edited text through `normalize_description`.
- TUI footer, renderers, and snapshots → expose the explicit edit action, editor focus, save/cancel controls, validation errors, and frozen-recovery state.

### Leaves

- Existing Ratatui/crossterm TUI architecture and Markdown preview renderer.
- Existing reviewer editing and Test Case metadata/settings editing.
- Existing Azure validation, authentication, candidate lookup, partial-success, and recovery mechanisms except for the content snapshot they receive.
- Body-only clipboard semantics.
- All functionality listed in Boundary Out.

The heavier alternative is a full text-area/document-editor dependency with selection, undo/redo, mouse support, search, and IDE-like behavior; it only wins if a later requirement makes those capabilities necessary or the content size/interaction model outgrows this bounded review editor, which issue #9 does not establish.

## Roadmap

| Block | Delivers | Clarity |
|---|---|---|
| Shared content editor | A tested Unicode-safe multiline editor and draft state with save/cancel, title/body focus, viewport navigation, and global-key isolation | clear |
| PR review integration | Explicit edit action, canonical save/cancel behavior, body-only copy, frozen publish/retry content, and PR-specific validation | clear |
| Test Case review integration | Content editing independent from settings, canonical create/retry content, body-only copy, and Test Case-specific validation | clear |
| Boundary and UI verification | Unit tests, flow tests, Azure payload tests, failure/retry tests, and updated TUI snapshots for both commands | clear |

## Decisions

| Decision | Choice | Why this | Alternative, and what would make it win | Reversibility |
|---|---|---|---|---|
| Approved-content source | Draft-only `ContentEditState`; after save, PR uses `DescribeApp.desc` and Test Case uses `TestApp.title`/`body` as canonical active content | Preview, clipboard, and remote calls can read one active representation per flow | Keep generated, displayed, and published copies separate; only wins if independent asynchronous versions become a real requirement | costly |
| Clipboard payload | Copy body only in both flows | Preserves the existing `copy_to_clipboard` contract and README behavior | Copy title plus body; wins only as a separately requested clipboard UX change | reversible |
| Edit lifecycle | Explicit `Editar conteúdo`; title single-line, body multiline; `Tab` switches fields, `Enter` inserts a body newline, `Ctrl+S` saves, `Esc` cancels | Fits the existing TUI's explicit focus model and prevents review shortcuts from mutating the draft | Enter-to-save or a separate full-screen editor; wins if hands-on terminal testing shows the default bindings are unusable | reversible |
| Save behavior | Validate without normalizing; preserve the exact entered title/body on successful save | Human corrections must not be silently rewritten by generator cleanup | Re-run normalization; wins only for generated content before the user approves it, not for edited content | reversible |
| Empty title | Never publish/create with `title.trim().is_empty()` | Both remote record types require a usable title | Allow Azure to reject it remotely; wins only if Azure's error UX is intentionally preferred | reversible |
| Empty body | PR keeps the current body rule and length limit; Test Case keeps `validate_card` and rejects empty body | The issue explicitly requires flow-specific behavior rather than applying PR rules to Test Cases | Enforce one shared non-empty rule; wins only if product deliberately changes both flows | reversible |
| Remote-attempt freeze | Set `frozen_publish_content`/`frozen_create_content` before the first remote call; disable content edit during publishing and recovery; retries and pending targets reuse the snapshot | Prevents mixed versions and preserves exact-title recovery semantics | Permit editing after failure; wins only with a separate explicit version/attempt model and a new confirmation for every affected target | costly |
| Editor implementation | Shared in-repository editor using existing TUI dependencies; no new text-area dependency | The repository already has char-indexed single-line editors and the issue excludes IDE features | Full text-area dependency; wins when richer editing requirements or scale justify its cost | reversible |

