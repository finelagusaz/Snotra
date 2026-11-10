// 経緯形コメントの候補を列挙し、台帳の雛形を作る（#1409 の一時的な足場。workspace/ ごと撤去する）。
// usage: node candidates.mjs <rev> [--ledger <dir>]   rev のツリー（git show）を読む
//   stdout: 候補行（file:line: text）。--ledger を付けると <dir>/<file>.md に近接ブロック（3 行以内）ごとの表を書く
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const PAT = new RegExp([
  '移設', '以前は', '以前の', 'かつて', 'もともと', '元は', '元々', '当初', '当時', '従来', '名残', '時期',
  '旧[・:：]', '旧実装', '旧判定', '旧経路', '旧 ', '旧-', '旧表示', '旧色', '旧\\s*view',
  'PR [A-Z]', 'PR\\d', 'codex #', '#\\d+\\s*(F\\d|D\\d|段|発見|PR|サイクル|follow-up|タスク|SU\\d|決定)',
  'SU\\d', 'Task \\d', '決定 \\d', '移した', '移った', 'になった', 'worker 化', '以降は', '以前から', '移設前', '移設後',
  '撤去', '廃止', '持ち越し', 'レビュー指摘', '格上げ', '後継', 'ていた', 'だった', 'した結果',
].join('|'));

const rev = process.argv[2];
const ledgerIdx = process.argv.indexOf('--ledger');
const ledgerDir = ledgerIdx >= 0 ? process.argv[ledgerIdx + 1] : null;
const root = process.cwd();
const files = execFileSync('git', ['ls-tree', '-r', '--name-only', rev, 'src-tauri/src/egui_shell'], { cwd: root, encoding: 'utf8' })
  .trim().split('\n').filter((f) => f.endsWith('.rs'));

const commentOf = (l) => { const i = l.indexOf('//'); return i >= 0 ? l.slice(i) : null; };
const out = [];
const byFile = {};
for (const f of files) {
  const lines = execFileSync('git', ['show', `${rev}:${f}`], { cwd: root, encoding: 'utf8', maxBuffer: 64 << 20 }).split(/\r?\n/);
  const hits = [];
  lines.forEach((l, i) => {
    const c = commentOf(l);
    if (!c) return;
    // 行をまたぐ作業単位名（行末の PR / SU と次行の続き）も拾うため、次のコメント行を連結して照合する
    const next = commentOf(lines[i + 1] ?? '') ?? '';
    const joined = c + next.replace(/^\/\/[\/!]?\s*/, '');
    if (PAT.test(c) || (/(PR|SU|#\d+)\s*$/.test(c.trim()) && PAT.test(joined))) hits.push(i + 1);
  });
  for (const h of hits) out.push(`${f}:${h}: ${lines[h - 1].trim()}`);
  byFile[f] = hits.length;
  if (ledgerDir && hits.length) {
    const blocks = [];
    for (const h of hits) {
      const last = blocks[blocks.length - 1];
      if (last && h - last.end <= 3) last.end = h; else blocks.push({ start: h, end: h });
    }
    const rows = blocks.map((b, k) => {
      const text = lines.slice(b.start - 1, b.end).map((l) => l.trim()).join(' ⏎ ').replace(/\|/g, '\\|');
      return `| B${k + 1} | ${b.start}-${b.end} | ${text} | | | | |`;
    });
    const name = f.replace('src-tauri/src/egui_shell/', '').replace(/\//g, '__');
    mkdirSync(ledgerDir, { recursive: true });
    writeFileSync(join(ledgerDir, `${name}.md`),
      `# 台帳: ${f}（基準 ${rev}・候補 ${hits.length} 行・${blocks.length} ブロック）\n\n` +
      '分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。\n\n' +
      '| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |\n|---|---|---|---|---|---|---|\n' +
      rows.join('\n') + '\n');
  }
}
console.log(out.join('\n'));
console.error(Object.entries(byFile).filter(([, n]) => n).sort((a, b) => b[1] - a[1]).map(([f, n]) => `${n}\t${f}`).join('\n'));
console.error(`total ${out.length}`);
