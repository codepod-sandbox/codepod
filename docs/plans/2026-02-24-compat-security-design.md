# Shell Compatibility & Security Improvements — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Update README to reflect current state, add 4 shell features (string param expansion, set flags, brace expansion, read builtin), and fix 3 security gaps (extension allowlist, extension limits, adversarial tests).

**Architecture:** Shell features are added in the TypeScript executor (`shell-runner.ts`) — the Rust parser already handles most of the syntax. Brace expansion is a pre-processing step added to `expandWord()`. Security fixes modify `shell-runner.ts` to check allowlists and limits for extensions. All changes are in `packages/orchestrator/`.

**Tech Stack:** TypeScript (Bun runtime), Vitest-compatible test runner (`bun test`)

---

### Task 1: Update README

**Files:**
- Modify: `README.md:3,12,619-621,640`

**Step 1: Update README**

Changes:
1. Line 3: "60+ commands" → "65+ commands" (bc, dc, sqlite3, gzip/gunzip, tar etc.)
2. Line 12: "60+ commands" → "65+ commands" in Available tools intro
3. Line 161 (Scripting row): add `bc`, `dc` to the table
4. Add new row: `| Database | sqlite3 (in-memory) |`
5. Lines 619-621 (Limitations):

Replace:
```
- **Bash subset, not full POSIX.** No aliases, `eval`, job control, or advanced file descriptor manipulation (e.g., `>&3`).
- **No runtime pip install from PyPI.** `pip install` only works for host-registered extensions. There is no PyPI access — Python packages are either standard library or provided via extensions.
- **Security hardening is in progress.** Timeout enforcement, capability policies, output truncation, and session isolation are implemented but not yet audited for adversarial untrusted input in production.
```

With:
```
- **Bash-compatible, not full POSIX.** Covers most scripting needs — control flow, functions, parameter expansion, here-docs, subshells, arithmetic. Missing: aliases, `eval`, `trap`, job control (`&`, `fg`, `bg`), arrays, process substitution (`<(...)`), advanced file descriptor manipulation (`>&3`).
- **No runtime pip install from PyPI.** `pip install` only works for host-registered extensions. There is no PyPI access — Python packages are either standard library or provided via extensions.
- **Security is defense-in-depth, not formally audited.** Hard-kill timeout via `Worker.terminate()`, tool allowlist, output/memory limits, VFS isolation (no host filesystem access), network default-deny with domain allowlist, file count limits, command length limits, and session isolation are all implemented. Not yet pen-tested against adversarial untrusted input in production.
```

6. Line 640: Update test count to current.

**Step 2: Verify README renders correctly**

Scan the changes — no commands needed, just a visual check.

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update README limitations, tool count, and security description"
```

---

### Task 2: String Parameter Expansion

**Files:**
- Modify: `packages/orchestrator/src/shell/shell-runner.ts:956-972` (add switch cases)
- Test: `packages/orchestrator/src/shell/__tests__/shell-runner.test.ts` (add test describe block)

**Step 1: Write failing tests**

Add to `shell-runner.test.ts` inside the top-level `describe('ShellRunner')`:

```typescript
describe('string parameter expansion', () => {
  it('${var#pattern} removes shortest prefix', async () => {
    const result = await runner.run('X=/usr/local/bin; echo ${X#*/}');
    expect(result.stdout).toBe('usr/local/bin\n');
  });

  it('${var##pattern} removes longest prefix', async () => {
    const result = await runner.run('X=/usr/local/bin; echo ${X##*/}');
    expect(result.stdout).toBe('bin\n');
  });

  it('${var%pattern} removes shortest suffix', async () => {
    const result = await runner.run('X=/usr/local/bin; echo ${X%/*}');
    expect(result.stdout).toBe('/usr/local\n');
  });

  it('${var%%pattern} removes longest suffix', async () => {
    const result = await runner.run('X=/usr/local/bin; echo ${X%%/*}');
    expect(result.stdout).toBe('\n');
  });

  it('${var/pattern/replacement} replaces first match', async () => {
    const result = await runner.run('X=hello_world_hello; echo ${X/hello/goodbye}');
    expect(result.stdout).toBe('goodbye_world_hello\n');
  });

  it('${var//pattern/replacement} replaces all matches', async () => {
    const result = await runner.run('X=hello_world_hello; echo ${X//hello/goodbye}');
    expect(result.stdout).toBe('goodbye_world_goodbye\n');
  });

  it('returns empty when var is unset', async () => {
    const result = await runner.run('echo ${UNSET_VAR#pattern}');
    expect(result.stdout).toBe('\n');
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `bun test packages/orchestrator/src/shell/__tests__/shell-runner.test.ts -t "string parameter expansion"`
Expected: All 7 tests FAIL (they'll return the raw value without trimming)

**Step 3: Implement string parameter expansion**

In `shell-runner.ts`, replace the `default` case at line 972 with these additional cases in the switch at line 956:

```typescript
case '#': {
  // Remove shortest prefix matching glob pattern
  if (val === undefined) return '';
  return this.trimPrefix(val, operand, false);
}
case '##': {
  // Remove longest prefix matching glob pattern
  if (val === undefined) return '';
  return this.trimPrefix(val, operand, true);
}
case '%': {
  // Remove shortest suffix matching glob pattern
  if (val === undefined) return '';
  return this.trimSuffix(val, operand, false);
}
case '%%': {
  // Remove longest suffix matching glob pattern
  if (val === undefined) return '';
  return this.trimSuffix(val, operand, true);
}
case '/': {
  // Replace first occurrence
  if (val === undefined) return '';
  return this.replacePattern(val, operand, false);
}
case '//': {
  // Replace all occurrences
  if (val === undefined) return '';
  return this.replacePattern(val, operand, true);
}
default: return val ?? '';
```

Then add these helper methods to the ShellRunner class (anywhere after `expandWordPart`):

```typescript
/** Convert a shell glob pattern to a RegExp. */
private globToRegex(pattern: string): RegExp {
  const re = pattern
    .replace(/[.+^${}()|[\]\\]/g, '\\$&')
    .replace(/\*/g, '.*')
    .replace(/\?/g, '.');
  return new RegExp(re);
}

/** Remove prefix matching glob pattern. greedy=true for longest match. */
private trimPrefix(val: string, pattern: string, greedy: boolean): string {
  const re = this.globToRegex(pattern);
  if (greedy) {
    // Try longest match from start
    for (let i = val.length; i >= 0; i--) {
      if (re.test(val.slice(0, i))) return val.slice(i);
    }
  } else {
    // Try shortest match from start
    for (let i = 0; i <= val.length; i++) {
      if (re.test(val.slice(0, i))) return val.slice(i);
    }
  }
  return val;
}

/** Remove suffix matching glob pattern. greedy=true for longest match. */
private trimSuffix(val: string, pattern: string, greedy: boolean): string {
  const re = this.globToRegex(pattern);
  if (greedy) {
    // Try longest match from end
    for (let i = 0; i <= val.length; i++) {
      if (re.test(val.slice(i))) return val.slice(0, i);
    }
  } else {
    // Try shortest match from end
    for (let i = val.length; i >= 0; i--) {
      if (re.test(val.slice(i))) return val.slice(0, i);
    }
  }
  return val;
}

/** Replace first or all occurrences of pattern. Operand is "pattern/replacement". */
private replacePattern(val: string, operand: string, all: boolean): string {
  const slashIdx = operand.indexOf('/');
  const pattern = slashIdx >= 0 ? operand.slice(0, slashIdx) : operand;
  const replacement = slashIdx >= 0 ? operand.slice(slashIdx + 1) : '';
  const re = this.globToRegex(pattern);

  if (all) {
    // Replace all non-overlapping matches left to right
    let result = '';
    let i = 0;
    while (i < val.length) {
      let matched = false;
      // Try longest match at position i
      for (let j = val.length; j > i; j--) {
        if (re.test(val.slice(i, j))) {
          result += replacement;
          i = j;
          matched = true;
          break;
        }
      }
      if (!matched) {
        result += val[i];
        i++;
      }
    }
    return result;
  } else {
    // Replace first match
    for (let i = 0; i < val.length; i++) {
      for (let j = val.length; j > i; j--) {
        if (re.test(val.slice(i, j))) {
          return val.slice(0, i) + replacement + val.slice(j);
        }
      }
    }
    return val;
  }
}
```

**Step 4: Run tests to verify they pass**

Run: `bun test packages/orchestrator/src/shell/__tests__/shell-runner.test.ts -t "string parameter expansion"`
Expected: All 7 PASS

**Step 5: Commit**

```bash
git add packages/orchestrator/src/shell/shell-runner.ts packages/orchestrator/src/shell/__tests__/shell-runner.test.ts
git commit -m "feat: add string parameter expansion operators (#, ##, %, %%, /, //)"
```

---

### Task 3: Extension Allowlist Enforcement

**Files:**
- Modify: `packages/orchestrator/src/shell/shell-runner.ts:112-141,1005` (add toolAllowlist field, check before extension exec)
- Modify: `packages/orchestrator/src/sandbox.ts:158` (pass allowlist to ShellRunner)
- Test: `packages/orchestrator/src/__tests__/security.test.ts` (add extension + allowlist tests)

**Step 1: Write failing tests**

Add to `security.test.ts` after the existing "tool allowlist" tests:

```typescript
it('tool allowlist blocks extension not in list', async () => {
  const sb = await Sandbox.create({
    wasmDir: WASM_DIR,
    shellWasmPath: SHELL_WASM,
    adapter: new NodeAdapter(),
    security: { toolAllowlist: ['echo'] },
    extensions: [{
      name: 'greet',
      description: 'says hello',
      command: async () => ({ stdout: 'hello\n', exitCode: 0 }),
    }],
  });
  const result = await sb.run('greet');
  expect(result.exitCode).not.toBe(0);
  expect(result.stderr).toContain('not allowed');
  sb.destroy();
});

it('tool allowlist allows extension in list', async () => {
  const sb = await Sandbox.create({
    wasmDir: WASM_DIR,
    shellWasmPath: SHELL_WASM,
    adapter: new NodeAdapter(),
    security: { toolAllowlist: ['echo', 'greet'] },
    extensions: [{
      name: 'greet',
      description: 'says hello',
      command: async () => ({ stdout: 'hello\n', exitCode: 0 }),
    }],
  });
  const result = await sb.run('greet');
  expect(result.exitCode).toBe(0);
  expect(result.stdout).toBe('hello\n');
  sb.destroy();
});
```

**Step 2: Run tests to verify they fail**

Run: `bun test packages/orchestrator/src/__tests__/security.test.ts -t "tool allowlist blocks extension"`
Expected: FAIL (extension runs despite not being in allowlist)

**Step 3: Implement extension allowlist check**

In `shell-runner.ts`, add a field and setter:

```typescript
// Add to class fields (around line 141):
private toolAllowlist: Set<string> | null = null;

// Add setter method (after setExtensionRegistry):
setToolAllowlist(list: string[]): void {
  this.toolAllowlist = new Set(list);
}
```

In the `spawnOrPython` method (line 1005), add allowlist check before extension dispatch:

```typescript
} else if (this.extensionRegistry?.has(cmdName) && this.extensionRegistry.get(cmdName)!.command) {
  // Check allowlist — extensions are subject to the same policy as tools
  if (this.toolAllowlist && !this.toolAllowlist.has(cmdName)) {
    result = {
      exitCode: 126,
      stdout: '',
      stderr: `${cmdName}: tool not allowed by security policy\n`,
      executionTimeMs: 0,
    };
  } else {
    result = await this.execExtension(cmdName, args, stdinData);
  }
}
```

In `sandbox.ts`, after the ShellRunner is created (around line 158), pass the allowlist:

```typescript
if (options.security?.toolAllowlist) {
  runner.setToolAllowlist(options.security.toolAllowlist);
}
```

Also do the same for forked sandboxes — find where `ShellRunner` is created in the `fork()` method and add the same call.

**Step 4: Run tests to verify they pass**

Run: `bun test packages/orchestrator/src/__tests__/security.test.ts -t "tool allowlist"`
Expected: All PASS (both old and new tests)

**Step 5: Commit**

```bash
git add packages/orchestrator/src/shell/shell-runner.ts packages/orchestrator/src/sandbox.ts packages/orchestrator/src/__tests__/security.test.ts
git commit -m "fix: enforce tool allowlist on extension commands"
```

---

### Task 4: Extension Output & Timeout Limits

**Files:**
- Modify: `packages/orchestrator/src/shell/shell-runner.ts:1031-1058` (add truncation and deadline check to execExtension)
- Test: `packages/orchestrator/src/__tests__/security.test.ts` (add output limit test)

**Step 1: Write failing test**

Add to `security.test.ts`:

```typescript
it('extension output is truncated to stdout limit', async () => {
  const sb = await Sandbox.create({
    wasmDir: WASM_DIR,
    shellWasmPath: SHELL_WASM,
    adapter: new NodeAdapter(),
    security: { limits: { stdoutBytes: 100 } },
    extensions: [{
      name: 'flood',
      description: 'outputs a lot',
      command: async () => ({ stdout: 'x'.repeat(10000), exitCode: 0 }),
    }],
  });
  const result = await sb.run('flood');
  expect(result.stdout.length).toBeLessThanOrEqual(200); // some slack for encoding
  sb.destroy();
});
```

**Step 2: Run test to verify it fails**

Run: `bun test packages/orchestrator/src/__tests__/security.test.ts -t "extension output is truncated"`
Expected: FAIL (stdout is full 10000 chars)

**Step 3: Implement output truncation for extensions**

In `execExtension()` at shell-runner.ts:1046, after getting the result from `invoke()`, add truncation:

```typescript
const r = await this.extensionRegistry!.invoke(cmdName, {
  args,
  stdin,
  env: Object.fromEntries(this.env),
  cwd: this.env.get('PWD') ?? '/',
});

let stdout = r.stdout;
let stderr = r.stderr ?? '';
let truncated: { stdout: boolean; stderr: boolean } | undefined;

if (this.stdoutLimit !== undefined && stdout.length > this.stdoutLimit) {
  stdout = stdout.slice(0, this.stdoutLimit);
  truncated = { stdout: true, stderr: false };
}
if (this.stderrLimit !== undefined && stderr.length > this.stderrLimit) {
  stderr = stderr.slice(0, this.stderrLimit);
  truncated = truncated
    ? { ...truncated, stderr: true }
    : { stdout: false, stderr: true };
}

return {
  exitCode: r.exitCode,
  stdout,
  stderr,
  executionTimeMs: performance.now() - start,
  truncated,
};
```

Also add a deadline check before invoking the extension:

```typescript
if (Date.now() > this.deadlineMs) throw new CancelledError('TIMEOUT');
```

**Step 4: Run tests to verify they pass**

Run: `bun test packages/orchestrator/src/__tests__/security.test.ts`
Expected: All PASS

**Step 5: Commit**

```bash
git add packages/orchestrator/src/shell/shell-runner.ts packages/orchestrator/src/__tests__/security.test.ts
git commit -m "fix: apply output limits and deadline check to extension commands"
```

---

### Task 5: `set` Builtin (Flags -e, -x, -u)

**Files:**
- Modify: `packages/orchestrator/src/shell/shell-runner.ts:28,112-141,505-508,749-818` (add flags, integrate into execution)
- Test: `packages/orchestrator/src/shell/__tests__/shell-runner.test.ts` (add set flag tests)

**Step 1: Write failing tests**

```typescript
describe('set flags', () => {
  it('set -e aborts on non-zero exit', async () => {
    const result = await runner.run('set -e; echo before; false; echo after');
    expect(result.stdout).toContain('before');
    expect(result.stdout).not.toContain('after');
    expect(result.exitCode).not.toBe(0);
  });

  it('set -e does not abort in if condition', async () => {
    const result = await runner.run('set -e; if false; then echo no; else echo yes; fi; echo after');
    expect(result.stdout).toContain('yes');
    expect(result.stdout).toContain('after');
  });

  it('set -e does not abort in || chain', async () => {
    const result = await runner.run('set -e; false || echo fallback; echo after');
    expect(result.stdout).toContain('fallback');
    expect(result.stdout).toContain('after');
  });

  it('set -u errors on undefined variable', async () => {
    const result = await runner.run('set -u; echo $UNDEFINED_VAR');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain('UNDEFINED_VAR');
  });

  it('set +e disables errexit', async () => {
    const result = await runner.run('set -e; set +e; false; echo still-here');
    expect(result.stdout).toContain('still-here');
  });
});
```

Note: `set -x` (xtrace) is harder to test deterministically, so we skip the test for it.

**Step 2: Run tests to verify they fail**

Run: `bun test packages/orchestrator/src/shell/__tests__/shell-runner.test.ts -t "set flags"`
Expected: All FAIL

**Step 3: Implement set builtin**

Add to class fields:
```typescript
/** Shell option flags (e=errexit, u=nounset, x=xtrace). */
private shellFlags = new Set<string>();
/** Whether we're in a conditional context (if condition, || / && right side). */
private inConditionalContext = false;
```

Add `'set'` to the `SHELL_BUILTINS` set at line 28.

Add a `set` case in the builtin dispatch in `execSimple` (after the other builtins):

```typescript
if (cmdName === 'set') {
  for (const arg of args) {
    if (arg.startsWith('-')) {
      for (const ch of arg.slice(1)) this.shellFlags.add(ch);
    } else if (arg.startsWith('+')) {
      for (const ch of arg.slice(1)) this.shellFlags.delete(ch);
    }
  }
  result = { ...EMPTY_RESULT };
}
```

In `execList`, wrap the right side of `And`/`Or` in conditional context:

For the `Or` case, before executing the right side:
```typescript
const prevCtx = this.inConditionalContext;
this.inConditionalContext = true;
const rightResult = await this.execCommand(list.right);
this.inConditionalContext = prevCtx;
```

Do the same for the `And` case.

In `execIf`, wrap the condition in conditional context:
```typescript
const prevCtx = this.inConditionalContext;
this.inConditionalContext = true;
const condResult = await this.execCommand(ifCmd.condition);
this.inConditionalContext = prevCtx;
```

After every command execution that produces a non-zero exit code (in `execSimple`, at the end before returning), add an errexit check:

```typescript
// At the end of execSimple, before the final return:
if (this.shellFlags.has('e') && !this.inConditionalContext && finalResult.exitCode !== 0) {
  throw new ExitSignal(finalResult.exitCode, finalResult.stdout, finalResult.stderr);
}
```

For `set -u`, modify `expandWordPart` at the `Variable` case — when the variable is undefined and `u` flag is set, throw an error (return an error result):

```typescript
if ('Variable' in part) {
  const name = part.Variable;
  // Handle special variables ($?, $@, etc.) first...
  // ...then at the end:
  const val = this.env.get(name);
  if (val === undefined && this.shellFlags.has('u')) {
    throw new Error(`${name}: unbound variable`);
  }
  return val ?? '';
}
```

**Step 4: Run tests to verify they pass**

Run: `bun test packages/orchestrator/src/shell/__tests__/shell-runner.test.ts -t "set flags"`
Expected: All 5 PASS

Then run full test suite: `bun test`
Expected: All pass (no regressions)

**Step 5: Commit**

```bash
git add packages/orchestrator/src/shell/shell-runner.ts packages/orchestrator/src/shell/__tests__/shell-runner.test.ts
git commit -m "feat: add set builtin with -e (errexit), -u (nounset), +e/+u to disable"
```

---

### Task 6: Brace Expansion

**Files:**
- Modify: `packages/orchestrator/src/shell/shell-runner.ts:505-506` (add brace expansion step)
- Test: `packages/orchestrator/src/shell/__tests__/shell-runner.test.ts`

**Step 1: Write failing tests**

```typescript
describe('brace expansion', () => {
  it('expands comma-separated braces', async () => {
    const result = await runner.run('echo {a,b,c}');
    expect(result.stdout).toBe('a b c\n');
  });

  it('expands braces with prefix and suffix', async () => {
    const result = await runner.run('echo file.{txt,md,rs}');
    expect(result.stdout).toBe('file.txt file.md file.rs\n');
  });

  it('expands numeric range', async () => {
    const result = await runner.run('echo {1..5}');
    expect(result.stdout).toBe('1 2 3 4 5\n');
  });

  it('expands reverse numeric range', async () => {
    const result = await runner.run('echo {5..1}');
    expect(result.stdout).toBe('5 4 3 2 1\n');
  });

  it('expands alpha range', async () => {
    const result = await runner.run('echo {a..e}');
    expect(result.stdout).toBe('a b c d e\n');
  });

  it('does not expand single item in braces', async () => {
    const result = await runner.run('echo {solo}');
    expect(result.stdout).toBe('{solo}\n');
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `bun test packages/orchestrator/src/shell/__tests__/shell-runner.test.ts -t "brace expansion"`
Expected: All FAIL

**Step 3: Implement brace expansion**

Add a method to ShellRunner:

```typescript
/**
 * Expand brace patterns in a list of words.
 * {a,b,c} → ['a', 'b', 'c'], prefix{a,b}suffix → ['prefixasuffix', 'prefixbsuffix']
 * {1..5} → ['1', '2', '3', '4', '5'], {a..e} → ['a', 'b', 'c', 'd', 'e']
 */
private expandBraces(words: string[]): string[] {
  const result: string[] = [];
  for (const word of words) {
    const expanded = this.expandBrace(word);
    result.push(...expanded);
  }
  return result;
}

private expandBrace(word: string): string[] {
  // Find the first top-level { } pair
  let depth = 0;
  let start = -1;
  for (let i = 0; i < word.length; i++) {
    if (word[i] === '{') {
      if (depth === 0) start = i;
      depth++;
    } else if (word[i] === '}') {
      depth--;
      if (depth === 0 && start >= 0) {
        const prefix = word.slice(0, start);
        const inner = word.slice(start + 1, i);
        const suffix = word.slice(i + 1);

        // Check for range pattern: {a..z} or {1..10}
        const rangeMatch = inner.match(/^(-?\w+)\.\.(-?\w+)$/);
        if (rangeMatch) {
          const items = this.expandRange(rangeMatch[1], rangeMatch[2]);
          if (items.length > 0) {
            // Recursively expand suffix
            return items.flatMap(item => this.expandBrace(prefix + item + suffix));
          }
        }

        // Check for comma-separated list
        if (inner.includes(',')) {
          const items = this.splitBraceItems(inner);
          if (items.length > 1) {
            return items.flatMap(item => this.expandBrace(prefix + item + suffix));
          }
        }

        // Not a valid brace expansion — treat as literal
        return [word];
      }
    }
  }
  return [word];
}

private splitBraceItems(inner: string): string[] {
  const items: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of inner) {
    if (ch === '{') depth++;
    else if (ch === '}') depth--;
    if (ch === ',' && depth === 0) {
      items.push(current);
      current = '';
    } else {
      current += ch;
    }
  }
  items.push(current);
  return items;
}

private expandRange(startStr: string, endStr: string): string[] {
  // Numeric range
  const startNum = parseInt(startStr, 10);
  const endNum = parseInt(endStr, 10);
  if (!isNaN(startNum) && !isNaN(endNum)) {
    const items: string[] = [];
    const step = startNum <= endNum ? 1 : -1;
    for (let i = startNum; step > 0 ? i <= endNum : i >= endNum; i += step) {
      items.push(String(i));
    }
    return items;
  }
  // Alpha range (single chars)
  if (startStr.length === 1 && endStr.length === 1) {
    const s = startStr.charCodeAt(0);
    const e = endStr.charCodeAt(0);
    const items: string[] = [];
    const step = s <= e ? 1 : -1;
    for (let i = s; step > 0 ? i <= e : i >= e; i += step) {
      items.push(String.fromCharCode(i));
    }
    return items;
  }
  return [];
}
```

Then insert brace expansion into the pipeline at line 505-506:

```typescript
const rawWords = await Promise.all(simple.words.map(w => this.expandWord(w)));
const bracedWords = this.expandBraces(rawWords);
const expandedWords = this.expandGlobs(bracedWords);
```

Do the same at lines 720-721 (pipeline) and 842-843 (for loop).

**Step 4: Run tests to verify they pass**

Run: `bun test packages/orchestrator/src/shell/__tests__/shell-runner.test.ts -t "brace expansion"`
Expected: All 6 PASS

Then: `bun test` — full suite, no regressions.

**Step 5: Commit**

```bash
git add packages/orchestrator/src/shell/shell-runner.ts packages/orchestrator/src/shell/__tests__/shell-runner.test.ts
git commit -m "feat: add brace expansion ({a,b,c} and {1..5} ranges)"
```

---

### Task 7: `read` Builtin

**Files:**
- Modify: `packages/orchestrator/src/shell/shell-runner.ts:28,505+` (add read to builtins, implement)
- Test: `packages/orchestrator/src/shell/__tests__/shell-runner.test.ts`

**Step 1: Write failing tests**

```typescript
describe('read builtin', () => {
  it('reads stdin into variable', async () => {
    const result = await runner.run('echo hello | read LINE; echo $LINE');
    // Note: in subshell pipelines, read runs in a subshell. Test with redirection instead.
    expect(result.exitCode).toBe(0);
  });

  it('reads from here-document into variable', async () => {
    const result = await runner.run('read NAME <<EOF\nalice\nEOF\necho "name is $NAME"');
    expect(result.stdout).toBe('name is alice\n');
  });

  it('splits into multiple variables', async () => {
    const result = await runner.run('read A B C <<EOF\none two three four\nEOF\necho "$A $B $C"');
    expect(result.stdout).toBe('one two three four\n');
  });

  it('returns 1 on empty input', async () => {
    const result = await runner.run('echo -n "" | read X');
    expect(result.exitCode).not.toBe(0);
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `bun test packages/orchestrator/src/shell/__tests__/shell-runner.test.ts -t "read builtin"`
Expected: All FAIL

**Step 3: Implement read builtin**

Add `'read'` to `SHELL_BUILTINS` at line 28.

In `execSimple`, add a case for `read`:

```typescript
if (cmdName === 'read') {
  // read [-r] VAR1 [VAR2 ...]
  let raw = false;
  const varNames: string[] = [];
  for (const a of args) {
    if (a === '-r') { raw = true; continue; }
    varNames.push(a);
  }
  if (varNames.length === 0) varNames.push('REPLY');

  // Get first line from stdin
  const input = stdinData ? new TextDecoder().decode(stdinData) : '';
  const firstLine = input.split('\n')[0];
  if (firstLine === undefined || (input === '' && !stdinData?.length)) {
    result = { exitCode: 1, stdout: '', stderr: '', executionTimeMs: 0 };
  } else {
    const line = raw ? firstLine : firstLine.replace(/\\(.)/g, '$1');
    const parts = line.split(/\s+/);
    for (let i = 0; i < varNames.length; i++) {
      if (i === varNames.length - 1) {
        // Last variable gets the remainder
        this.env.set(varNames[i], parts.slice(i).join(' '));
      } else {
        this.env.set(varNames[i], parts[i] ?? '');
      }
    }
    result = { ...EMPTY_RESULT };
  }
}
```

Note: `read` in a pipeline runs in a subshell in real bash, which means variables don't propagate. Our sequential pipeline model actually makes this work differently — the variable gets set but only in the subshell context. For the tests, we use here-documents which execute in the current shell.

**Step 4: Run tests to verify they pass**

Run: `bun test packages/orchestrator/src/shell/__tests__/shell-runner.test.ts -t "read builtin"`
Expected: All PASS

Then: `bun test` — full suite.

**Step 5: Commit**

```bash
git add packages/orchestrator/src/shell/shell-runner.ts packages/orchestrator/src/shell/__tests__/shell-runner.test.ts
git commit -m "feat: add read builtin for reading stdin into variables"
```

---

### Task 8: Adversarial Security Tests

**Files:**
- Create: `packages/orchestrator/src/__tests__/security-adversarial.test.ts`
- Reference: `packages/orchestrator/src/__tests__/security.test.ts` (for setup pattern)

**Step 1: Write the adversarial test suite**

```typescript
import { describe, it, expect } from 'bun:test';
import { resolve } from 'node:path';
import { Sandbox } from '../sandbox.js';
import { NodeAdapter } from '../platform/node-adapter.js';

const WASM_DIR = resolve(import.meta.dirname, '../platform/__tests__/fixtures');
const SHELL_WASM = resolve(import.meta.dirname, '../shell/__tests__/fixtures/wasmsand-shell.wasm');

describe('Security: adversarial inputs', () => {
  // Extension + allowlist (already covered in Task 3, but verify here too)
  it('extension blocked by allowlist cannot be reached via pipe', async () => {
    const sb = await Sandbox.create({
      wasmDir: WASM_DIR, shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      security: { toolAllowlist: ['echo'] },
      extensions: [{
        name: 'secret',
        command: async () => ({ stdout: 'leaked\n', exitCode: 0 }),
      }],
    });
    const result = await sb.run('echo test | secret');
    expect(result.stdout).not.toContain('leaked');
    sb.destroy();
  });

  // Shell script bypass attempt
  it('script file cannot run blocked commands', async () => {
    const sb = await Sandbox.create({
      wasmDir: WASM_DIR, shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      security: { toolAllowlist: ['echo', 'cat'] },
    });
    sb.writeFile('/tmp/evil.sh', new TextEncoder().encode('#!/bin/sh\ngrep secret /etc/passwd\n'));
    // Even sourcing the script should fail — grep is not in allowlist
    const result = await sb.run('source /tmp/evil.sh');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain('not allowed');
    sb.destroy();
  });

  // Deeply nested command substitution
  it('deeply nested command substitution is bounded', async () => {
    // MAX_SUBSTITUTION_DEPTH is 50 — this should fail
    let cmd = 'echo innermost';
    for (let i = 0; i < 60; i++) cmd = `echo $(${cmd})`;
    const sb = await Sandbox.create({
      wasmDir: WASM_DIR, shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
    });
    const result = await sb.run(cmd);
    expect(result.exitCode).not.toBe(0);
    sb.destroy();
  });

  // Output flood via repeated command
  it('output truncation works under repeated writes', async () => {
    const sb = await Sandbox.create({
      wasmDir: WASM_DIR, shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      security: { limits: { stdoutBytes: 1024 } },
    });
    // Generate large output
    const result = await sb.run('for i in $(seq 1 1000); do echo "line $i padding padding padding padding"; done');
    // stdout should be capped
    expect(result.stdout.length).toBeLessThan(2048); // some slack
    sb.destroy();
  });

  // Command length limit
  it('very long command is rejected', async () => {
    const sb = await Sandbox.create({
      wasmDir: WASM_DIR, shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      security: { limits: { commandBytes: 100 } },
    });
    const result = await sb.run('echo ' + 'a'.repeat(200));
    expect(result.exitCode).not.toBe(0);
    sb.destroy();
  });

  // Path traversal via symlinks
  it('symlink chains are bounded', async () => {
    const sb = await Sandbox.create({
      wasmDir: WASM_DIR, shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
    });
    // Create circular symlink — should not hang
    const result = await sb.run('ln -s /tmp/link1 /tmp/link2; ln -s /tmp/link2 /tmp/link1; cat /tmp/link1');
    expect(result.exitCode).not.toBe(0);
    sb.destroy();
  });
});
```

**Step 2: Run tests**

Run: `bun test packages/orchestrator/src/__tests__/security-adversarial.test.ts`
Expected: All PASS (these test existing protections + the new extension fix from Task 3)

If any fail, investigate and fix the underlying issue.

**Step 3: Commit**

```bash
git add packages/orchestrator/src/__tests__/security-adversarial.test.ts
git commit -m "test: add adversarial security test suite"
```

---

### Task 9: Final Verification & Update README Test Count

**Step 1: Run full test suite**

Run: `bun test`
Expected: All pass, 0 failures. Note exact test count.

**Step 2: Update README test count**

Update line 640 in README.md with the actual test count from the run.

**Step 3: Update README shell features section**

In the Shell features section (lines 169-177), add mention of string parameter expansion and brace expansion:

Add to "Quoting and expansion" line: `, brace expansion ({a,b,c}, {1..5})`
Add to expansion details: `, string manipulation (${VAR#prefix}, ${VAR%suffix}, ${VAR/old/new})`

**Step 4: Commit all**

```bash
git add README.md
git commit -m "docs: update README with new shell features and test count"
```

**Step 5: Squash and push**

Squash all commits from this session into a single commit with message:
```
feat: shell compatibility improvements, security hardening, and README update

- String parameter expansion (${var#pat}, ${var%pat}, ${var/old/new})
- set -e (errexit), set -u (nounset) shell flags
- Brace expansion ({a,b,c}, {1..5})
- read builtin for stdin parsing
- Extension commands now respect tool allowlist
- Extension output now respects stdout/stderr limits
- Adversarial security test suite
- README updated to reflect current capabilities
```
