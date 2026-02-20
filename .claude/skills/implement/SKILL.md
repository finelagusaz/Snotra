---
name: implement
description: "Autonomous full-cycle feature development: investigate, plan, implement, verify, commit"
disable-model-invocation: true
argument-hint: "[feature description]"
allowed-tools:
  - Bash(cargo *)
  - Bash(npx vite build)
  - Bash(git *)
  - Read
  - Edit
  - Write
  - Grep
  - Glob
---

Autonomous full-cycle development. Do not ask me questions--figure it out from the code and docs.

Task: $ARGUMENTS

## Step 1 -- INVESTIGATE

- Read `SPEC.md` and relevant `CLAUDE.md` files to understand intent and architecture
- Identify entry points and related modules from `$ARGUMENTS`
- Search for existing code that overlaps with the requested feature
- Note any constraints from the 3-layer model (intent in SPEC.md, implementation in code)

## Step 2 -- PLAN

- Summarize the change plan in a short list (which files to create/modify and why)
- Print the plan to the conversation--do not create external plan files
- Follow project principles: logic in `snotra-core`, thin wrappers in `commands.rs` (KISS/DRY/YAGNI)

## Step 3 -- IMPLEMENT

- Make the changes following the plan
- Add `#[cfg(test)]` unit tests in `snotra-core` for any new pure logic
- If the change affects behavior described in `SPEC.md`, update `SPEC.md` accordingly

## Step 4 -- VERIFY (max 5 retry cycles)

Run these checks in order. If any step fails, fix and re-run from the failing step:

1. `cargo check -p snotra-core -p snotra`
2. `cargo clippy -p snotra-core -p snotra -- -D warnings`
3. `cargo test -p snotra-core`
4. `npx vite build` (only if TypeScript/frontend files were changed)

If after 5 cycles errors persist, stop and write a diagnostic summary:
- What was tried
- What error persists
- What the likely root cause is

## Step 5 -- COMMIT

- Stage only the files changed in this task
- Create a conventional commit (e.g. `feat:`, `fix:`, `refactor:`)
- Include a concise description of what was implemented and why

## Output

Show me:
1. The investigation findings (Step 1)
2. The change plan (Step 2)
3. The final verify output--check, clippy, test results (Step 4)
4. The commit hash and message (Step 5)
5. The diff of all changes
