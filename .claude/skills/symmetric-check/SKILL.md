---
name: symmetric-check
description: "Given a changed or fixed code path, find symmetric counterparts and verify the same change applies (or explicitly confirm why not)"
disable-model-invocation: true
argument-hint: "[changed code path or bug keyword, e.g. 'result-clicked: added emitSelectionUpdate']"
allowed-tools:
  - Read
  - Grep
  - Glob
---

Search for symmetric code paths related to: $ARGUMENTS

## Background

When a fix or change is applied to one code path, its symmetric counterpart often
needs the same treatment. Missing this is a common source of bugs — the invariant
is violated in one branch while visibly working in another.

## Step 1 — Identify the pattern

From $ARGUMENTS, extract:
- The affected function / event name / invariant keyword
- The type of change (added call, removed call, reordered logic, etc.)

## Step 2 — Search for symmetric counterparts

Grep the codebase for pairs such as:

| Changed | Check |
|---------|-------|
| `*clicked*` | `*double-clicked*` |
| `show` / `open` | `hide` / `close` |
| `enter*` | `exit*` |
| `expand` | `collapse` |
| `mount` / `setup` | `unmount` / `teardown` |
| `register` | `unregister` |

Also search for other call sites of the same function or keyword to find any
code path that was not included in the original change.

## Step 3 — Evaluate each candidate

For each candidate location, read enough context to enumerate all execution cases:

```
Candidate: <file>:<line> — <description>
  Case A: <scenario> → <effect> → OK / PROBLEM: <description>
  Case B: <scenario> → <effect> → OK / PROBLEM: <description>
  Decision: [APPLY] same change needed
            [NOT NEEDED] reason: <explicit per-case justification>
```

A "NOT NEEDED" decision with no case-by-case reasoning is a red flag.
Treat unexplained exclusions as unverified.

## Output

List every candidate with its decision and reasoning.
If no counterparts are found, state that explicitly.
