// コメント中の「…」引用のうち、基準では引用先（同じ語句の別の出現）があったのに、今は無くなったものを出す（#1409 の一時的な足場）。
// usage: node quotes.mjs <base-sha>
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
const base = process.argv[2];
const files = execFileSync('git', ['ls-tree', '-r', '--name-only', base, 'src-tauri/src/egui_shell'], { encoding: 'utf8' }).trim().split('\n');
const extra = ['docs', 'SPEC.md', 'src-tauri/CLAUDE.md', 'src-tauri/src'];
const read = (rev, f) => rev ? execFileSync('git', ['show', `${rev}:${f}`], { encoding: 'utf8', maxBuffer: 64 << 20 }) : readFileSync(f, 'utf8');
const corpus = (rev) => {
  const list = execFileSync('git', ['ls-tree', '-r', '--name-only', rev ?? 'HEAD', ...extra], { encoding: 'utf8' }).trim().split('\n')
    .filter((f) => /\.(rs|md)$/.test(f) && !f.startsWith('docs/superpowers'));
  return list.map((f) => { try { return read(rev, f); } catch { return ''; } }).join('\n');
};
const baseAll = corpus(base);
const headAll = corpus(null);
const count = (s, n) => s.split(n).length - 1;
let n = 0;
for (const f of files) {
  const src = readFileSync(f, 'utf8');
  src.split(/\r?\n/).forEach((l, i) => {
    const c = l.indexOf('//'); if (c < 0) return;
    for (const m of l.slice(c).matchAll(/「([^」]{6,})」/g)) {
      const q = m[1];
      // 自分自身の 1 回を除いた出現数
      const b = count(baseAll, q) - 1, h = count(headAll, q) - 1;
      if (b > 0 && h <= 0) { n++; console.log(`${f}:${i + 1}: 「${q}」 base=${b} head=${h}`); }
    }
  });
}
console.log(`${n} broken quotes`);
