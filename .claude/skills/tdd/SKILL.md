---
name: tdd
description: Strict TDD workflow - write a failing test first, then iterate implementation until all tests pass
disable-model-invocation: true
argument-hint: "[task description]"
allowed-tools:
  - Bash(cargo test *)
  - Bash(cargo clippy *)
  - Bash(npx vite build)
  - Read
  - Edit
  - Write
  - Grep
  - Glob
---

Work in strict TDD mode on this task. Do not ask me questions--figure it out from the code.

Task: $ARGUMENTS

## Step 1 -- Write a failing test

- Identify the appropriate test module in `snotra-core/src/` based on the task
- Add a `#[test]` function that captures the expected behavior
- Run `cargo test -p snotra-core` and confirm the new test **FAILS**
- If it already passes, the test is not specific enough--rewrite it

## Step 2 -- Implement the minimal fix/feature

- Make the smallest code change that should make the test pass
- Keep implementation in `snotra-core` where possible (per project KISS/DRY principles)
- Run `cargo test -p snotra-core` again

## Step 3 -- Iterate (max 5 attempts)

- If tests fail, read the error output carefully, adjust the implementation, and re-run
- Each attempt should be informed by the previous error--do not repeat the same approach
- If after 5 attempts tests still fail, stop and write a diagnostic summary:
  - What was tried
  - What error persists
  - What the likely root cause is

## Step 4 -- Once all tests pass

- Run `cargo check -p snotra-core -p snotra` to verify full compilation
- Run `cargo clippy -p snotra-core -p snotra -- -D warnings` and fix any warnings
- If the task involved TypeScript changes, also run `npx vite build`

## Output

Show me:
1. The failing test output (Step 1)
2. The final green test output (Step 2/3)
3. The clippy output (Step 4)
4. The diff of all changes
