# Issue Action Modal & Ship Flow

## Overview

Replace the current "M on issue = silently create two worktrees" behavior with an intentional **Issue Action Modal** and a complete issue → branch → implement → PR → merge flow.

Only available when the project has a GitHub remote.

---

## New Flow

### Trigger
Select an issue in the Issues tab of `NewWorktreeModal` → press `M` → opens **Issue Action Modal** (same style as MagicModal).

### Options

| Option | Key | Git | End behavior |
|--------|-----|-----|--------------|
| **Investigate** | `I` | unchanged (local `git worktree add`) | None — same as today |
| **Plan** | `P` | unchanged | None — same as today |
| **Implement** | `M` | `gh issue develop` | Changed files view + "Create PR" button → user clicks → PR created → "Merge PR" button |
| **Ship** | `S` | `gh issue develop` | Changed files view → PR auto-created → "Merge PR" button |

Investigate and Plan: **no git or PR changes**, they behave exactly as today.

---

## UI Detail: Implement & Ship end state

When Claude finishes (session goes idle after implement/ship):

**Summary area** (where plan/approve currently lives):
- List of changed files with `+green / -red` line stats, each file clickable (opens diff or file)
- This replaces the approve/yolo buttons for these two modes

**Implement mode:** "Create PR" button
- User clicks → calls `gh pr create` (existing `create_pr_with_ai_content` command)
- After PR created → new message in session with **"Merge PR"** button

**Ship mode:** No button — PR is created automatically when Claude finishes
- Same changed files view shown
- After PR created → **"Merge PR"** button appears

**Merge PR button** → calls `gh pr merge` (new, see backend section).

> Future: show CI check status alongside the Merge PR button before allowing merge.

---

## Branch Creation Change (Implement & Ship only)

Replace `git worktree add -b issue-{n}-{slug} <path> <base>` with:

```
gh issue develop {number} --base {base_branch} --name issue-{number}-{slug}
git fetch origin issue-{number}-{slug}
git worktree add <path> issue-{number}-{slug}
```

This links the branch to the issue on GitHub. Gate: only when project has a GitHub remote.

---

## Files Affected

### Frontend

- `src/components/worktree/NewWorktreeModal.tsx`
  - `M` on issue now opens IssueActionModal instead of calling `handleSelectIssueAndInvestigate` directly
  - Remove the silent double-worktree creation behavior

- `src/components/worktree/IssueActionModal.tsx` *(new)*
  - MagicModal-style component with 4 options: Investigate, Plan, Implement, Ship
  - Keyboard shortcuts: I / P / M / S

- `src/store/ui-store.ts`
  - Add `pendingIssueAction: 'investigate' | 'plan' | 'implement' | 'ship' | null`
  - Add setter + consumer (same pattern as `pendingInvestigateType`)

- `src/components/chat/ChatWindow.tsx`
  - Consume `pendingIssueAction` on session mount
  - Route to appropriate handler

- `src/components/chat/hooks/useInvestigateHandlers.ts`
  - Add `handleImplement()` and `handleShip()` — send implementation prompt with issue context
  - Ship mode: after session goes idle, auto-call `handleOpenPr()`
  - After PR created (both modes): show "Merge PR" message/button in session

- `src/components/chat/hooks/useGitOperations.ts`
  - Add `handleMergePr()` — calls new `merge_pr` Tauri command
  - `handleOpenPr()` already exists; hook into its success path to show merge button

- New component or inline in ChatWindow: **changed files summary view**
  - Reads `git diff --stat HEAD~1` or uses cached diff data
  - Renders file list with `+/-` counts, clickable

### Backend

- `src-tauri/src/projects/git.rs`
  - Add `gh_issue_develop(repo_path, issue_number, branch_name, base_branch, gh_binary)` function
  - Add `gh_pr_merge(repo_path, pr_number, merge_type, gh_binary)` function

- `src-tauri/src/projects/commands.rs`
  - In `create_worktree`: when `issue_context` is present AND project has GitHub remote → use `gh_issue_develop` instead of `git worktree add -b`
  - Add `merge_pr` Tauri command: calls `gh_pr_merge`, then archives/removes worktree

- `src-tauri/src/lib.rs`
  - Register `merge_pr` command

---

## What Gets Removed

- The "two simultaneous worktrees" behavior when pressing M on an issue — gone entirely
- The blank `Session 1 - idle` worktree that was created alongside the investigation worktree

---

## Out of Scope (future tasks)

- CI check status before merge button
- Auto-PR / auto-merge global config (separate task)
- Per-project merge strategy config
