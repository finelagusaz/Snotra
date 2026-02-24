---
name: dry-check
description: "Given a new or modified function, grep for call sites that hand-write equivalent logic and flag DRY violations"
disable-model-invocation: true
argument-hint: "[function name and key operations, e.g. 'show_main_and_emit: show() + set_focus() + emit(window-shown)']"
allowed-tools:
  - Read
  - Grep
  - Glob
---

Check for DRY violations related to: $ARGUMENTS

## Background

Creating a function and applying it to all existing code that does the same thing
are two separate steps. Hand-written duplicates that predate the function often
remain in place, silently missing the behaviour the function encapsulates
(e.g. an `emit()` call that the manual version never had).

## Step 1 — Parse the function

From $ARGUMENTS, extract:
- The function name
- The key operations it performs (as a list of grep-able patterns)

Example:
```
Function: show_main_and_emit
Key operations:
  - .show()
  - .set_focus()
  - emit("window-shown")
```

If $ARGUMENTS does not list operations explicitly, read the function body first.

## Step 2 — Grep for each key operation

Search the codebase for each operation individually.
Exclude matches inside the function itself.

## Step 3 — Identify hand-written duplicates

A call site is a candidate for replacement when it:
- Performs two or more of the key operations manually, AND
- Does not call the function

Read enough context around each candidate to understand the call site.

## Step 4 — Evaluate each candidate

```
Candidate: <file>:<line> — <description>
  Current: <what it does manually>
  Missing: <what the function adds that the manual version lacks>
  Decision: [REPLACE] with <function call>
            [KEEP]    reason: <why replacement is not appropriate>
```

## Output

List every candidate with its decision and reasoning.
If no violations are found, state: "No hand-written duplicates found."
