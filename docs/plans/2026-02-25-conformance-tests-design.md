# Conformance Test Suite Design

## Goal

Import ~180 conformance tests across awk, sed, find, and shell to discover
gaps in our WASM implementations. Tests are LLM-generated from man page
knowledge and spot-checked against real tools.

## Structure

```
packages/orchestrator/src/shell/__tests__/conformance/
  awk.test.ts      (~50 tests)
  sed.test.ts      (~40 tests)
  find.test.ts     (~30 tests)
  shell.test.ts    (~60 tests)
```

Each file follows the existing test pattern: `ShellRunner` + `VFS` + `ProcessManager`,
organized by feature area in `describe` blocks. Tests that exercise unimplemented
features are marked `it.skip` with a comment noting what's missing.

## Test Categories

### awk (~50 tests)
- Field splitting (default, -F, -F with regex)
- Print variants ($0, $NF, formatted)
- Patterns and conditions (regex, comparison, range)
- BEGIN/END blocks
- Built-in functions (length, substr, gsub, sub, split, sprintf, tolower/toupper)
- Built-in variables (NR, NF, FS, OFS, ORS, FILENAME)
- Arrays (associative, for-in, delete)
- Multiple rules and actions
- printf formatting
- Pipes and getline

### sed (~40 tests)
- Substitution (basic, global, nth occurrence, case-insensitive)
- Address types (line number, regex, range, last line $)
- Commands (d, p, i, a, c, y, q, w)
- Multiple -e expressions
- Hold/pattern space (h, g, x)
- Labels and branching (b, t)
- Character classes and regex features
- Newline handling

### find (~30 tests)
- Name matching (-name, -iname, wildcards)
- Type filtering (-type f/d/l)
- Depth control (-maxdepth, -mindepth)
- Size predicates (-size +/-/exact)
- Logical operators (-and, -or, -not, ! grouping)
- Actions (-exec, -print, -delete)
- Multiple predicates combined
- Empty files/dirs (-empty)
- Path matching (-path, -wholename)

### shell (~60 tests)
- Parameter expansion (${var:-default}, ${var:+alt}, ${#var}, ${var%pattern})
- Arithmetic expansion ($((expr)))
- Here documents and here strings
- Process substitution (if supported)
- Functions (definition, local vars, return values)
- Arrays (indexed, ${arr[@]}, ${#arr[@]})
- Test/conditional expressions ([[ ]], [ ])
- Brace expansion ({a,b,c}, {1..5})
- Glob patterns (*, ?, [charset])
- Quoting edge cases
- Exit status and error propagation
- Subshell variable isolation
- Nested command substitution
- eval and indirect expansion

## Verification

For non-obvious expected outputs, verify against real tools:
```bash
echo 'test input' | awk '{print $1}'  # verify expected output
```
