# Checks — Session persistence for `prt desc`

Profile: light
Plan: `.specs/features/session-persistence/plan.md`

## Checks

### Session lifecycle and resume

**C1** — Starting `prt desc` and reaching the validated Review phase creates one session snapshot with a UUID v4, approved title/body, Work Item, planned targets, repository identity, Git fingerprint, and UTC `created_at`/`updated_at` before the process can exit.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::creates_review_snapshot_before_exit`

Status: Implemented

**C2** — The Review UI displays the session UUID and the four allowed per-target states (`pending`, `attempting_or_uncertain`, `confirmed`, `failed`) for every planned target.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::describe_app::tests::renders_session_id_and_target_states`

Status: Implemented

**C3** — Exiting from Review after edits durably flushes the latest approved title/body, reviewers, Work Item, targets, and target states before `prt desc` returns.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::flushes_latest_session_before_review_exit`

Status: Implemented

**C4** — `prt desc --resume` lists incomplete sessions ordered by `updated_at` descending and shows each session UUID, repository, branch, and target summary without auto-loading any session.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::lists_incomplete_sessions_newest_first_without_loading`

Status: Implemented

**C5** — `prt desc --session <uuid>` loads exactly the selected incomplete session; a missing UUID returns exit code 1 and a complete session is not selectable.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked cli::tests::session_selector_requires_existing_incomplete_uuid`

Status: Implemented

**C6** — Resuming restores the persisted title/body, reviewers, Work Item, planned target order, per-target states, and confirmed PR IDs/URLs byte-for-byte at the review/publish boundary, with no AI/provider generation call.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::resume_restores_snapshot_without_provider_generation`

Status: Implemented

**C7** — Missing, corrupt, or schema-incompatible session data causes a deterministic local error before any remote request and leaves existing snapshots unchanged.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::rejects_missing_corrupt_and_unknown_schema_without_remote_call`

Status: Implemented

**C8** — `--resume` and `--session` reject every content-generation/publish modifier (`--provider`, `--model`, `--temperature`, `--system-prompt`, `--prompt`, `--target`, `--work-item`, `--pr`, `--no-copy`, `--no-create`) with exit code 2.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked cli::tests::resume_flags_conflict_with_generation_and_publish_options`

Status: Implemented

**C9** — Pressing `d` on the selected resumable session removes all of that session's snapshots and it no longer appears in `--resume`; deletion of another session is impossible while its exclusive lock is held.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::discard_removes_all_revisions_and_locked_session_is_not_deletable`

Status: Implemented

**C10** — A session is removed automatically only after every planned target is `confirmed` and the TUI finishes successfully; an incomplete session remains listable after abort or failure.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::successful_completion_removes_session_but_abort_keeps_it`

Status: Implemented

### Per-target publication protocol

**C11** — Before the first remote request, every planned target is persisted as `pending`, and only the target currently being published transitions durably to `attempting_or_uncertain` before its POST begins.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::persists_pending_then_attempting_before_publish_request`

Status: Implemented

**C12** — A successful create response durably records the confirmed target state, PR ID, and PR URL before the next target request can start.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked azure::pull_requests::tests::confirmed_receipt_is_persisted_before_next_target`

Status: Implemented

**C13** — A provider-classified confirmed failure records `failed` with its message and leaves every not-yet-started target as `pending`.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::describe_app::tests::confirmed_failure_persists_failed_and_remaining_pending`

Status: Implemented

**C14** — A transport error, timeout, HTTP 408, HTTP 429, HTTP 5xx, or invalid successful response is persisted as `attempting_or_uncertain` and no automatic second POST is issued.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked features::describe::tests::ambiguous_publish_errors_never_auto_retry`

Status: Implemented

**C15** — Resuming a `confirmed` target skips publication and preserves its recorded PR ID/URL.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::resume_skips_confirmed_target`

Status: Implemented

**C16** — Resuming an `attempting_or_uncertain` target requires candidate reconciliation or an explicit retry decision; loading the session alone never emits a POST.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::describe_app::tests::uncertain_target_requires_reconciliation_before_retry`

Status: Implemented

**C17** — Adopting one matching candidate transitions that target to `confirmed` with the candidate PR ID/URL and issues no create request.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::describe_app::tests::adopting_candidate_confirms_without_create_request`

Status: Implemented

**C18** — When reconciliation finds zero candidates, the UI shows an explicit warning and requires a user-selected retry before any create request.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::describe_app::tests::zero_candidates_require_explicit_retry`

Status: Implemented

**C19** — A multi-target run persists each confirmed receipt and each uncertain/failed outcome so that a later resume publishes only eligible remaining targets.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::partial_multi_target_progress_survives_resume`

Status: Implemented

**C20** — If the `attempting_or_uncertain` snapshot cannot be durably written, the corresponding remote create call is not made.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::publish_is_blocked_when_attempting_snapshot_cannot_flush`

Status: Implemented

**C21** — If a provider create succeeds but the confirmed receipt cannot be durably written, the next load treats that target as `attempting_or_uncertain` and does not auto-retry it.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::successful_create_with_failed_confirmation_write_is_uncertain_on_resume`

Status: Implemented

**C22** — A resumed publication never invokes the content-generation provider and never recreates a target already marked `confirmed`, even when other targets remain pending.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked tui::live::tests::resume_has_no_generation_and_no_confirmed_recreation`

Status: Implemented

### Repository and context divergence

**C23** — A repository-root or source-branch mismatch permits session inspection and editing but blocks publication with an explicit divergence error before any remote request.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked git::tests::session_repo_or_source_branch_mismatch_blocks_publish`

Status: Implemented

**C24** — A changed or missing source/target object ID permits inspection and editing, displays a stale-context warning, and requires explicit confirmation before publication.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked git::tests::changed_or_missing_fingerprint_requires_publish_confirmation`

Status: Implemented

**C25** — An exact current Git fingerprint permits publication without an additional divergence confirmation.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked git::tests::exact_session_fingerprint_has_no_extra_publish_gate`

Status: Implemented

**C26** — Failure to capture the current repository fingerprint blocks publication and makes no remote request.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked git::tests::fingerprint_capture_failure_blocks_publish_without_remote_call`

Status: Implemented

### Persistence safety and boundaries

**C27** — Every persisted file validates as schema version `1`, UUID v4, UTC RFC3339 timestamps, and an allowlisted session/target field set containing only safe resume data.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::writes_schema_v1_uuid_v4_utc_rfc3339_allowlisted_data`

Status: Implemented

**C28** — Serialized session data contains no `Config`, PAT, API key, bearer token, secret, prompt, diff, or log content, including nested serialized values.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::serialized_snapshot_contains_no_secrets_or_generation_context`

Status: Implemented

**C29** — Unknown fields, unknown target states, and unsupported schema versions are rejected without overwriting the last valid snapshot or issuing a remote request.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::schema_rejection_preserves_last_valid_snapshot`

Status: Implemented

**C30** — A snapshot is written to a new revision through a temporary file, flushed with `sync_all`, atomically renamed, and old revisions remain recoverable if interruption happens before rename.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::snapshot_revision_is_atomic_and_recovers_previous_file`

Status: Implemented

**C31** — A second process attempting to open the same session fails at exclusive lock acquisition before writing, deleting, or making a remote request.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::second_session_lock_fails_before_side_effects`

Status: Implemented

**C32** — The session directory is created below the existing `config_paths().directory`, and no session file is written outside its `sessions` child directory.

Proof: `cargo test --manifest-path apps/rust/Cargo.toml --locked session::tests::session_files_stay_below_config_directory`

Status: Implemented

## Coverage

| Set | Members | Unproven |
|---|---|---|
| CLI entry routes (3) | `prt desc` — C1; `prt desc --resume` — C4; `prt desc --session <uuid>` — C5 | - |
| Resume selector outcomes (4) | empty list — C4; missing UUID — C5; complete UUID — C5; invalid UUID — C7 | - |
| Generation/publish conflicts (10) | `--provider` — C8; `--model` — C8; `--temperature` — C8; `--system-prompt` — C8; `--prompt` — C8; `--target` — C8; `--work-item` — C8; `--pr` — C8; `--no-copy` — C8; `--no-create` — C8 | - |
| Target states (4) | `pending` — C11; `attempting_or_uncertain` — C14; `confirmed` — C12; `failed` — C13 | - |
| Publish outcomes (6) | confirmed success — C12; confirmed provider failure — C13; transport error — C14; timeout — C14; HTTP 408/429/5xx — C14; invalid 2xx — C14 | - |
| Resume decisions (3) | confirmed skip — C15; candidate adoption — C17; explicit retry after zero candidates — C18 | - |
| Git divergence outcomes (4) | repository/source branch mismatch — C23; changed OID — C24; missing OID — C24; exact fingerprint — C25 | - |
| Durability boundaries (5) | pending-before-request — C11; attempting-before-request — C11; receipt-before-next-target — C12; failed confirmation write — C21; atomic revision — C30 | - |
| Safety inputs (5) | schema v1 — C27; UUID v4 — C27; UTC RFC3339 — C27; allowlist — C27; forbidden secret/context fields — C28 | - |
| Lock/lifecycle outcomes (4) | exclusive lock — C31; second opener — C31; discard — C9; successful cleanup — C10 | - |

## Swept

- Validation: C5, C7, C8, C27, C29, C32
- Failure modes: C7, C13, C14, C20, C21, C23, C24, C26, C29, C30
- Idempotency: C14, C15, C16, C17, C19, C21, C22
- Authorization: existing `Config` reload path supplies current credentials; C28 proves credentials are not persisted
- Concurrency: C9, C31
- Data lifecycle: C3, C4, C9, C10, C30, C32
- Dependency failure: C14, C20, C21, C26
- State transitions: C11, C12, C13, C14, C15, C16, C17, C19, C21, C22
- Observability: C2, C4, C13, C14, C18, C23, C24

## Handoff

- Builder: one builder is sufficient; the measured touched-file union is 425,233 bytes, approximately 106k tokens at 4 bytes/token, below the 150k handoff threshold.
- Boundary: implement `features::session` first, then wire CLI, `main`, `DescribeApp`, `live`, and Git divergence checks; keep provider/config secret types outside the persisted DTO.
- Independent verifier: required after implementation over `<feature base>..HEAD`, with fresh verification context and no builder-owned proof claims.
