// コメントを落とした Rust のトークン列を、基準 sha と作業ツリーで比べる（#1409 の一時的な足場。workspace/ ごと撤去する）。
// usage: node rust-tokens.mjs <base-sha> [--root <dir>] [--self-test]
// コメント（`//` `///` `//!` と入れ子の `/* */`）は空白として扱う。文字列・raw 文字列・byte 文字列・char は逐語のまま残す。
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export function normalize(src) {
  let out = '';
  let i = 0;
  const n = src.length;
  let pendingSpace = false;
  const emit = (s) => {
    if (pendingSpace && out.length > 0) out += ' ';
    pendingSpace = false;
    out += s;
  };
  const isIdent = (c) => /[A-Za-z0-9_]/.test(c);
  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (/\s/.test(c)) { pendingSpace = true; i++; continue; }
    if (c === '/' && d === '/') {
      while (i < n && src[i] !== '\n') i++;
      pendingSpace = true;
      continue;
    }
    if (c === '/' && d === '*') {
      let depth = 1; i += 2;
      while (i < n && depth > 0) {
        if (src[i] === '/' && src[i + 1] === '*') { depth++; i += 2; }
        else if (src[i] === '*' && src[i + 1] === '/') { depth--; i += 2; }
        else i++;
      }
      if (depth > 0) throw new Error('unterminated block comment');
      pendingSpace = true;
      continue;
    }
    // raw string: r"..", r#".."#, br"..", cr".."
    const raw = src.slice(i).match(/^(?:b|c)?r(#*)"/);
    if (raw && (i === 0 || !isIdent(src[i - 1]))) {
      const hashes = raw[1];
      const close = '"' + hashes;
      const start = i;
      i += raw[0].length;
      const end = src.indexOf(close, i);
      if (end < 0) throw new Error('unterminated raw string');
      i = end + close.length;
      emit(src.slice(start, i));
      continue;
    }
    // string: "..", b"..", c".."
    if (c === '"' || ((c === 'b' || c === 'c') && d === '"' && (i === 0 || !isIdent(src[i - 1])))) {
      const start = i;
      i += c === '"' ? 1 : 2;
      while (i < n && src[i] !== '"') { if (src[i] === '\\') i += 2; else i++; }
      if (i >= n) throw new Error('unterminated string');
      i++;
      emit(src.slice(start, i));
      continue;
    }
    // char / byte char / lifetime
    if (c === '\'' || (c === 'b' && d === '\'' && (i === 0 || !isIdent(src[i - 1])))) {
      const q = c === '\'' ? i : i + 1;
      const rest = src.slice(q);
      const ch = rest.match(/^'(?:\\(?:u\{[0-9A-Fa-f_]+\}|x[0-9A-Fa-f]{2}|.)|[^\\'\n])'/u);
      if (ch) {
        const start = i;
        i = q + ch[0].length;
        emit(src.slice(start, i));
        continue;
      }
      // lifetime or label
      const lt = rest.match(/^'[A-Za-z_][A-Za-z0-9_]*/);
      if (lt && c === '\'') { emit(lt[0]); i += lt[0].length; continue; }
    }
    if (isIdent(c)) {
      const m = src.slice(i).match(/^[A-Za-z0-9_]+/);
      emit(m[0]); i += m[0].length; continue;
    }
    // punctuation: one char per token, but keep adjacency information via no space
    if (pendingSpace && out.length > 0) out += ' ';
    pendingSpace = false;
    out += c;
    i++;
  }
  return out;
}

function selfTest() {
  const cases = [
    // [name, before, after, expectDiff]
    ['line comment', 'let a = 1; // x\n', 'let a = 1; // y\n', false],
    ['doc comment', '/// x\nfn f() {}', '/// y y\nfn f() {}', false],
    ['inner doc', '//! a\nfn f() {}', '//! b\n//! c\nfn f() {}', false],
    ['nested block', 'a /* x /* y */ z */ b', 'a /* q */ b', false],
    ['remove comment line', 'a;\n// x\nb;', 'a;\nb;', false],
    ['string continuation', 'let s = "abc\\\n// x";', 'let s = "abc\\\n// y";', true],
    ['code deletion', 'a;\nb;\nc;', 'a;\nc;', true],
    ['raw string', 'let s = r#"// a"#;', 'let s = r#"// b"#;', true],
    ['char literal', "let c = '/';", "let c = '*';", true],
    ['lifetime kept', "fn f<'a>(x: &'a str) {}", "fn f<'b>(x: &'b str) {}", true],
    ['doc attribute', '#[doc = "a"]\nfn f() {}', '#[doc = "b"]\nfn f() {}', true],
    ['string with //', 'let u = "http://x";', 'let u = "http://y";', true],
    ['whitespace only', 'a  +  b', 'a + b', false],
    ['doc to line comment', '/// x\nfn f() {}', '// x\nfn f() {}', false],
  ];
  let fail = 0;
  for (const [name, a, b, expect] of cases) {
    const diff = normalize(a) !== normalize(b);
    const ok = diff === expect;
    if (!ok) fail++;
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}: diff=${diff} expect=${expect}`);
  }
  return fail;
}

const args = process.argv.slice(2);
if (args.includes('--self-test')) {
  process.exit(selfTest() ? 1 : 0);
}
const base = args[0];
const rootIdx = args.indexOf('--root');
const root = rootIdx >= 0 ? args[rootIdx + 1] : process.cwd();
const files = execFileSync('git', ['ls-tree', '-r', '--name-only', base, 'src-tauri/src/egui_shell'], { cwd: root, encoding: 'utf8' })
  .trim().split('\n').filter((f) => f.endsWith('.rs'));
let diffs = 0;
for (const f of files) {
  const before = normalize(execFileSync('git', ['show', `${base}:${f}`], { cwd: root, encoding: 'utf8', maxBuffer: 64 << 20 }));
  const after = normalize(readFileSync(join(root, f), 'utf8'));
  if (before !== after) {
    diffs++;
    let k = 0;
    while (k < before.length && before[k] === after[k]) k++;
    console.log(`DIFF ${f} @${k}\n  base:  ${before.slice(Math.max(0, k - 60), k + 60)}\n  after: ${after.slice(Math.max(0, k - 60), k + 60)}`);
  }
}
console.log(`${files.length} files, ${diffs} differ`);
process.exit(diffs ? 1 : 0);
