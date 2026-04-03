# Create Test Card Test QA Prompt Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ask the user whether to update the parent work item to `Test QA` after a `Test Case` is created successfully.

**Architecture:** Extend the existing post-create flow in `bin/create-test-card` with one additional interactive confirmation step that runs only after successful `Test Case` creation. Keep non-interactive, `--raw`, `--dry-run`, and `--no-create` behavior safe and unchanged outside that success path.

**Tech Stack:** Bash, curl, jq, Azure DevOps REST APIs.

---

### Task 1: Add post-create confirmation flow

**Files:**

- Modify: `bin/create-test-card`

- [ ] **Step 1: Write a failing test for the new prompt helper behavior**
- [ ] **Step 2: Run the failing test and verify it fails for the missing helper**
- [ ] **Step 3: Implement a helper that asks whether to move the parent work item to `Test QA`**
- [ ] **Step 4: Ensure it only runs after successful `Test Case` creation and only in interactive mode**
- [ ] **Step 5: Ensure `--raw`, `--dry-run`, and `--no-create` do not trigger the prompt**

### Task 2: Add Azure DevOps state update

**Files:**

- Modify: `bin/create-test-card`

- [ ] **Step 1: Write a failing test for the parent work item update payload/path**
- [ ] **Step 2: Run the failing test and verify it fails correctly**
- [ ] **Step 3: Implement the Azure DevOps PATCH to set `System.State = Test QA` on the parent work item**
- [ ] **Step 4: Add success and failure messaging around the state transition**
- [ ] **Step 5: Keep the existing `Test Case` success output intact**

### Task 3: Verify behavior

**Files:**

- Modify: `bin/create-test-card`

- [ ] **Step 1: Run syntax verification**

```bash
bash -n bin/create-test-card
```

- [ ] **Step 2: Run the new local tests**
- [ ] **Step 3: Run a non-interactive path and verify the new prompt is skipped**
- [ ] **Step 4: If safe, run an interactive/manual flow and confirm the prompt appears after successful create**
