/**
 * Standalone utility functions used by the shell runner.
 * These have no dependency on ShellRunner class state.
 */

/**
 * Parse a chmod mode argument. Returns the new octal mode or null if invalid.
 *
 * Supports:
 *   - Octal: "755", "644", "0755"
 *   - Symbolic: "+x", "-w", "u+x", "go-w", "a+rx"
 */
export function parseChmodMode(modeArg: string, currentMode: number): number | null {
  // Try octal first
  if (/^0?[0-7]{3}$/.test(modeArg)) {
    return parseInt(modeArg, 8);
  }

  // Symbolic mode: [ugoa]*[+-][rwx]+
  const match = modeArg.match(/^([ugoa]*)([+-])([rwx]+)$/);
  if (!match) return null;

  const [, whoStr, op, permsStr] = match;
  const who = whoStr === '' || whoStr === 'a' ? 'ugo' : whoStr;

  // Build the bit mask for the specified permissions
  let mask = 0;
  for (const w of who) {
    const shift = w === 'u' ? 6 : w === 'g' ? 3 : 0;
    for (const p of permsStr) {
      const bit = p === 'r' ? 4 : p === 'w' ? 2 : 1;
      mask |= bit << shift;
    }
  }

  return op === '+' ? currentMode | mask : currentMode & ~mask;
}

/**
 * Parse a shebang line and return the interpreter base name, or null.
 *
 * Handles:
 *   #!/usr/bin/env python3  → "python3"
 *   #!/usr/bin/python3      → "python3"
 *   #!/bin/sh               → "sh"
 *   #!/bin/bash              → "bash"
 *   (no shebang)            → null
 */
export function parseShebang(firstLine: string): string | null {
  if (!firstLine.startsWith('#!')) return null;

  const rest = firstLine.slice(2).trim();
  const parts = rest.split(/\s+/);

  // #!/usr/bin/env <interpreter> — use the second word
  if (parts.length >= 2 && parts[0].endsWith('/env')) {
    return parts[1];
  }

  // #!/path/to/interpreter — use the basename
  if (parts.length >= 1) {
    const slash = parts[0].lastIndexOf('/');
    return slash >= 0 ? parts[0].slice(slash + 1) : parts[0];
  }

  return null;
}

/**
 * Normalize an absolute path by resolving `.` and `..` segments.
 * E.g. "/home/user/.." → "/home", "/home/./user" → "/home/user".
 */
export function normalizePath(path: string): string {
  const parts = path.split('/');
  const resolved: string[] = [];
  for (const part of parts) {
    if (part === '' || part === '.') continue;
    if (part === '..') {
      resolved.pop();
    } else {
      resolved.push(part);
    }
  }
  return '/' + resolved.join('/');
}

/** Simple strftime-like date formatter. Supports common % tokens. */
export function formatDate(d: Date, format: string): string {
  const pad = (n: number, w = 2) => String(n).padStart(w, '0');
  const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

  return format.replace(/%([YmdHMSaAbBpZsnT%])/g, (_, code: string) => {
    switch (code) {
      case 'Y': return String(d.getUTCFullYear());
      case 'm': return pad(d.getUTCMonth() + 1);
      case 'd': return pad(d.getUTCDate());
      case 'H': return pad(d.getUTCHours());
      case 'M': return pad(d.getUTCMinutes());
      case 'S': return pad(d.getUTCSeconds());
      case 'a': return days[d.getUTCDay()];
      case 'A': return ['Sunday','Monday','Tuesday','Wednesday','Thursday','Friday','Saturday'][d.getUTCDay()];
      case 'b': return months[d.getUTCMonth()];
      case 'B': return ['January','February','March','April','May','June','July','August','September','October','November','December'][d.getUTCMonth()];
      case 'p': return d.getUTCHours() < 12 ? 'AM' : 'PM';
      case 'Z': return 'UTC';
      case 's': return String(Math.floor(d.getTime() / 1000));
      case 'n': return '\n';
      case 'T': return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
      case '%': return '%';
      default: return `%${code}`;
    }
  });
}

/**
 * Safe arithmetic evaluator using recursive descent.
 * Supports: +, -, *, /, %, parentheses, comparisons (==, !=, <, >, <=, >=).
 */
export function safeEvalArithmetic(expr: string): number {
  const tokens: string[] = [];
  let i = 0;
  while (i < expr.length) {
    if (expr[i] === ' ' || expr[i] === '\t') { i++; continue; }
    if ('0123456789'.includes(expr[i])) {
      let num = '';
      while (i < expr.length && '0123456789'.includes(expr[i])) { num += expr[i++]; }
      tokens.push(num);
    } else if ('+-*/%()'.includes(expr[i])) {
      tokens.push(expr[i++]);
    } else if (expr[i] === '<' || expr[i] === '>' || expr[i] === '=' || expr[i] === '!') {
      let op = expr[i++];
      if (i < expr.length && expr[i] === '=') { op += expr[i++]; }
      tokens.push(op);
    } else {
      i++; // skip unknown
    }
  }
  let pos = 0;
  function peek(): string | undefined { return tokens[pos]; }
  function next(): string { return tokens[pos++]; }
  function parseExpr(): number { return parseComparison(); }
  function parseComparison(): number {
    let left = parseAddSub();
    while (peek() === '==' || peek() === '!=' || peek() === '<' || peek() === '>' || peek() === '<=' || peek() === '>=') {
      const op = next();
      const right = parseAddSub();
      switch (op) {
        case '==': left = left === right ? 1 : 0; break;
        case '!=': left = left !== right ? 1 : 0; break;
        case '<': left = left < right ? 1 : 0; break;
        case '>': left = left > right ? 1 : 0; break;
        case '<=': left = left <= right ? 1 : 0; break;
        case '>=': left = left >= right ? 1 : 0; break;
      }
    }
    return left;
  }
  function parseAddSub(): number {
    let left = parseMulDiv();
    while (peek() === '+' || peek() === '-') {
      const op = next();
      const right = parseMulDiv();
      left = op === '+' ? left + right : left - right;
    }
    return left;
  }
  function parseMulDiv(): number {
    let left = parseUnary();
    while (peek() === '*' || peek() === '/' || peek() === '%') {
      const op = next();
      const right = parseUnary();
      if (op === '*') left = left * right;
      else if (op === '/') left = right !== 0 ? Math.trunc(left / right) : 0;
      else left = right !== 0 ? left % right : 0;
    }
    return left;
  }
  function parseUnary(): number {
    if (peek() === '-') { next(); return -parsePrimary(); }
    if (peek() === '+') { next(); return parsePrimary(); }
    return parsePrimary();
  }
  function parsePrimary(): number {
    if (peek() === '(') {
      next(); // skip (
      const val = parseExpr();
      if (peek() === ')') next();
      return val;
    }
    const tok = next();
    return tok !== undefined ? parseInt(tok, 10) || 0 : 0;
  }
  return parseExpr();
}

export function concatBytes(a: Uint8Array, b: Uint8Array): Uint8Array {
  const result = new Uint8Array(a.byteLength + b.byteLength);
  result.set(a, 0);
  result.set(b, a.byteLength);
  return result;
}
