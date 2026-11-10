// ソーステキスト検査が探す needle の出現数を、基準 sha と作業ツリーで比べる（#1409 の一時的な足場。workspace/ ごと撤去する）。
// usage: node needles.mjs <base-sha>
// needle の出所:
//   view.rs の assert_read_once_in_this_file の 2 呼び出し（include_str!("view.rs")）
//   icon_textures.rs の include_str! 検査（view.rs・results_view.rs への contains）
//   launcher_controller/activation/tests.rs の sources()（launcher_controller/*.rs 直下）への owners_of_all / contains と entry_points
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const LC = 'src-tauri/src/egui_shell/launcher_controller/';
const GROUPS = [
  { files: ['src-tauri/src/egui_shell/view.rs'], needles: ['.controller.indexing()', 'read_visible_rows(', 'let input_idle = !self.controller.is_search_armed();'] },
  { files: ['src-tauri/src/egui_shell/results_view.rs'], needles: ['if snapshot.input_idle {'] },
  {
    dir: LC,
    needles: ['self.indexing()', 'read_visible_rows(', 'read_config(', 'plain_results_hidden(', 'results_area_collapsed(',
      'if crate::egui_shell::should_flush_on_enter(', 'fn on_enter(', 'fn activate_or_execute(', 'fn shift_activate(', 'forbidden('],
  },
];

const base = process.argv[2];
const root = process.cwd();
const count = (s, n) => s.split(n).length - 1;
let diffs = 0;
let checked = 0;
for (const g of GROUPS) {
  let files = g.files;
  if (g.dir) {
    files = execFileSync('git', ['ls-tree', '--name-only', `${base}:${g.dir}`], { cwd: root, encoding: 'utf8' })
      .trim().split('\n').filter((f) => f.endsWith('.rs')).map((f) => g.dir + f);
  }
  for (const f of files) {
    const before = execFileSync('git', ['show', `${base}:${f}`], { cwd: root, encoding: 'utf8', maxBuffer: 64 << 20 });
    const after = readFileSync(join(root, f), 'utf8');
    for (const n of g.needles) {
      checked++;
      const a = count(before, n);
      const b = count(after, n);
      if (a !== b) { diffs++; console.log(`DIFF ${f} \`${n}\`: ${a} -> ${b}`); }
    }
  }
}
console.log(`${checked} (file, needle) pairs, ${diffs} differ`);
process.exit(diffs ? 1 : 0);
