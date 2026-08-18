//! G-clippy-disallowed — src-tauri/clippy.toml の禁止集合が実効しているか（#950）。
import { finding, stripTomlComment, tomlLine, lintLevel } from "../lib.mjs";

export const id = "G-clippy-disallowed";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkClippyDisallowed(snapshot);
}

// ---------------------------------------------------------------------------
// G-clippy-disallowed — src-tauri/clippy.toml の禁止集合が実効しているか（#950）。
//
// **守る命題**: この検査が緑 ⇒ `REQUIRED_DISALLOWED_METHODS` が名指す各群の禁止が src-tauri の clippy で
// error として落ちる（何を禁じているかと群ごとの理由は `src-tauri/clippy.toml` が正本）。
// **前提は 4 つあり、どれも緑が含意しない**——(1) clippy.toml と Cargo.toml を正規表現で
// 近似パースする範囲で、(2) member 側の opt-in（`[lints] workspace = true`）は G-workspace-lints が見る、
// (3) 名指しした各パスが解決し続ける（解決しなくなっても文字列は変わらないので沈黙する。
//     群 1 は上流 egui のピン更新、群 2・3 は snotra-core 側の改名が契機になる。**群 3 だけはこの前提が
//     閉じている**——例外地点の `#[expect]` が不履行で赤くなるため。ただしその赤は `-D warnings` に
//     依存する〔#1122〕）、
// (4) DISALLOWED_METHODS_GROUPS が上流の群構成に追随している。**単独の緑を「禁止は生きている」と読んではならない。**
//
// 塞ぐのは **clippy 自身が exit 0 で沈黙する** 次の経路である（clippy 1.94.0 で実測）:
//   内容側 — ファイルの削除 / disallowed-methods の消滅・空配列化 / エントリが 1 行だけ消える /
//            メソッド名・型名の書き損じ（`does not refer to a reachable function` の warning は出るが
//            `-D warnings` でも exit 0）/ crate 名の書き損じ・egui 依存の消滅（診断そのものが出ない）/
//            エントリが `#` でコメントアウトされる
//   レベル側 — ルート [workspace.lints.clippy] の disallowed_methods の消滅・warn への降格・**同じ節の
//            群 allow による打ち消し**（`all = "allow"` を 1 行足すと deny の行を残したまま禁止が消える。
//            clippy 1.94.0 で実測: exit 0・診断 0 件）。この lint は **warn 既定**ゆえ、どの形でも黙る
// **PostToolUse hook は exit code でしか検出しないため、上記の warning はエージェントにも届かない。**
// 沈黙は二重である——それがこの検査を冗長でなくしている性質である（cargo のキャッシュを一切介さない
// Node の静的読み取りなので、6 経路すべてが入力テキストの差分として現れる）。
//
// 射程外（意図的）: reason 文言の変更・`#[allow]` による迂回（lint に内在する性質）・
// disallowed_methods 以外の clippy lint のレベル・clippy.toml と cargo のキャッシュの関係
// （挙動と 2026-08-17 の再測定は clippy.toml 冒頭の「この設定が死ぬ経路」3 が正本）。
//
// 受容する残余:
// - member 側の opt-in（src-tauri の `[lints] workspace = true`）は **G-workspace-lints が全 member について
//   見る**ため重ねない。**deny が実効するのはその opt-in が在る間だけ**であり、両検査は組で 1 つの命題を守る。
// - ルート Cargo.toml が読めない事実は G-workspace-lints でも鳴る（1 事実 2 件）。**沈黙させない側へ倒す**
//   ——黙って skip すれば、それ自体が新しい沈黙経路になる。
// - disallowed_methods を**ハイフン**（`disallowed-methods`）で書いた形は非実効と判定する＝**赤に倒れる**。
//   向きが赤（沈黙しない）なので受容するが、**次の人の最も安い直し方が「検査を緩める」にならない**よう
//   直し方を書いておく: Cargo の lints テーブルは lint 名をそのまま書くのでアンダースコアにする
//   （ハイフンなのは clippy.toml 側のキーだけである）。
// ---------------------------------------------------------------------------

/** src-tauri/clippy.toml に在ることを要求する禁止メソッド。**名指しは意図的である**——「配列が非空」だけでは
 *  1 行だけ消えた形も、メソッド名を書き損じた形も緑を通る（どちらも clippy 側は exit 0・実測）。
 *  **含めなかったメソッドと、その除外理由の正本は src-tauri/clippy.toml 冒頭のコメントである**——
 *  ここは「消えたら困る識別子」の写しだけを持つ（先例は REQUIRED_RUSTDOC_LINTS）。 */
export const REQUIRED_DISALLOWED_METHODS = [
  "egui::Context::set_visuals",
  "egui::Context::set_visuals_of",
  "egui::Context::style_mut_of",
  "egui::Context::set_style_of",
  "egui::Context::global_style_mut",
  "egui::Context::set_global_style",
  "egui::Context::all_styles_mut",
  // 群 2（#1067）: 計測ハーネス専用の観測口。製品が読んで分岐してはならない。
  "snotra_core::engine::Engine::sorted_by_path",
  // 群 3（#1122）: engine 錠越しの config の live-read。例外は expect 属性が分類を記録して開ける。
  "snotra_core::engine::Engine::config",
];

const CLIPPY_TOML = "src-tauri/clippy.toml";
const SRC_TAURI_MANIFEST = "src-tauri/Cargo.toml";

/** disallowed-methods 配列の path 値を**全件**返す。配列そのものが無ければ `null`（「空」と区別する）。
 *  **全域 match である**——per-line の単発 match は 1 行形（インラインテーブルを並べた配列）で先頭 1 件しか
 *  拾わない。**コメント除去を先に通す**——通さないと `#` でコメントアウトされたエントリを「在る」と数え、
 *  `disallowed-methods = []` との組み合わせが緑を通る（実測。あのファイルはコメントで長く説明する様式ゆえ、
 *  一時的な無効化はこの形で起きるのが最も自然である）。
 *  配列の終端は最初の `]` とする。reason に `]` を書くと途中で切れてカナリアが欠け**赤に倒れる**。 */
export function disallowedMethodPaths(text) {
  const body = text.split("\n").map(stripTomlComment).join("\n");
  const array = body.match(/disallowed-methods\s*=\s*\[([\s\S]*?)\]/);
  if (array == null) return null;
  return [...array[1].matchAll(/path\s*=\s*"([^"]+)"/g)].map((m) => m[1]);
}

/** src-tauri が egui を**通常の依存として**宣言しているか。**字面ではなく構文的位置で判定する**——
 *  `snotra-egui-runtime = { path = "../snotra-egui-runtime" }` が部分文字列で誤爆するためで、
 *  hasWorkspaceLintsOptIn と同じ理由である。実際の宣言形は dotted 形（`egui.workspace = true`）。
 *
 *  **節は `[dependencies]` と `[target.<cfg>.dependencies]` に限る。** `dependencies]` で終わる節を広く
 *  受けると 3 つが紛れ込み、どれも実害を持つ: `[dev-dependencies]` だけに egui が在る形は **bin/lib で
 *  パスが解決しない**のに緑になり（clippy は診断そのものを出さない）、`[build-dependencies]` も同じ。
 *  ルートの `[workspace.dependencies]` は egui を宣言しているので、**checkClippyDisallowed が 3 つの
 *  同型な読み取り（clippy.toml / src-tauri の Cargo.toml / ルート Cargo.toml）を取り違えても緑を通す**。
 *  節を絞ることで、その取り違えは赤として現れる（#950 の対称性検査で発見）。
 *
 *  ルート直下の dotted 形（`dependencies.egui = …`）は cargo 上有効だが非実効と判定する＝**赤に倒れる**。
 *  向きが赤なので受容する。直し方: `[dependencies]` テーブルで書く。
 *  **`[target.<cfg>.dependencies]` を受けるのは非対称な残余である**——cfg がビルド対象で偽なら egui は依存に
 *  入らず、禁止パスは解決せず clippy は無診断で沈黙する。現構成では到達不能（実データの egui は素の
 *  `[dependencies]`・CI は Windows ジョブ）ゆえ受容する。 */
export function declaresEguiDependency(text) {
  let section = "";
  for (const raw of text.split("\n")) {
    const line = tomlLine(raw);
    if (/^\[.*\]$/.test(line)) {
      section = line;
      continue;
    }
    if (!/^\[(?:target\.[^\]]+\.)?dependencies\]$/.test(section)) continue;
    if (/^egui\s*=/.test(line) || /^egui\.[A-Za-z0-9_-]+\s*=/.test(line)) return true;
  }
  return false;
}

/** disallowed_methods を含む lint group。**名指しは意図的である**——群を allow にする兄弟が 1 行在るだけで、
 *  `disallowed_methods = "deny"` はそのままに禁止が黙って消える（clippy 1.94.0 で実測: exit 0・診断 0 件）。
 *  この 2 つは `clippy-driver -W help` の群一覧から disallowed-methods を含むものを数え上げた結果である
 *  ——**上流が 3 つ目の群へ入れたら、この配列が更新されるまで沈黙する**（受容する残余）。 */
const DISALLOWED_METHODS_GROUPS = ["all", "style"];

/** TOML の整数リテラル。**数値区切りの `_` を落とす**——落とさないと `1_0`（TOML では 10）から 1 だけを
 *  読み、群の allow が実際より小さい priority に見えて緑へ倒れる（#950 のレビューで実測）。 */
const tomlInt = (text) => Number((String(text).match(/-?[0-9_]+/)?.[0] ?? "0").replaceAll("_", ""));

/** 同じく priority。文字列形は既定の 0。**priority が大きいほど後に当たる**ので、群の allow が個別 lint の
 *  deny と同じか大きい priority を持つと禁止が消える（#950 で実測）。 */
const lintPriority = (value) => (value.startsWith("{") ? tomlInt(value.match(/priority\s*=\s*([^,}]+)/)?.[1] ?? "0") : 0);

/** ルート [workspace.lints.clippy] の disallowed_methods が deny/forbid で、**かつ後から allow で
 *  打ち消されていない**か。level と priority の 2 形は lintLevel / lintPriority が受ける
 *  （前者は rustdocLintsAreDenied と共有）。
 *
 *  **「deny の行が在る」だけでは足りない**——同じ節の `all = "allow"` 1 行で禁止は完全に消える（実測）。
 *  **エントリは 3 つの綴りで書ける**（インライン形・dotted 形・サブテーブル形）。TOML 上は等価で
 *  clippy の挙動も同じなので、**3 つとも読む**——1 つでも落とすとその綴りだけが緑を通る（実測）。
 *  priority で向きが決まり、群が**同じか大きい** priority を持つときだけ打ち消す（`priority = -1` で
 *  群を先に当てる形は禁止が生き残ることを実測したので、緑に倒す）。**`>=` は `all` で測った境界に
 *  合わせた保守的な規則である**——同 priority の `style` は実測では打ち消さないが、ここでは赤に倒れる
 *  （fail-closed。直し方は `priority = -1` で群を先に当てること）。
 *  隣の rustdocLintsAreDenied が「節内の全エントリが deny」という ∀ で同型の穴を塞いでいるのに対し、
 *  こちらは節に allow を書く正当な用途を残すため、**打ち消しうる群だけを名指しして**塞ぐ。
 *  **節が無い形が最も起きやすい欠落である**（2 行消すだけで起きる）。 */
export function clippyMethodsDenied(rootText) {
  const entries = new Map();
  const upsert = (key, patch) => entries.set(key, { level: null, priority: 0, ...entries.get(key), ...patch });
  let section = "";
  for (const raw of rootText.split("\n")) {
    const line = tomlLine(raw);
    if (/^\[.*\]$/.test(line)) {
      section = line;
      continue;
    }
    if (line === "") continue;
    if (section === "[workspace.lints.clippy]") {
      // インライン形（`all = "allow"` / `all = { level = "allow", priority = 1 }`）
      const flat = line.match(/^([A-Za-z0-9_-]+)\s*=\s*(.+)$/);
      if (flat != null) {
        const value = flat[2].trim();
        upsert(flat[1], { level: lintLevel(value), priority: lintPriority(value) });
        continue;
      }
      // dotted 形（`all.level = "allow"`）。**この形を落とすと fail-open になる**——群が Map に現れず
      // 「打ち消し無し」と読んで緑を返す一方、clippy 側は禁止が消えて exit 0 になる（実測）
      const dotted = line.match(/^([A-Za-z0-9_-]+)\.(level|priority)\s*=\s*(.+)$/);
      if (dotted != null) {
        upsert(dotted[1], dotted[2] === "level" ? { level: lintLevel(dotted[3].trim()) } : { priority: tomlInt(dotted[3]) });
      }
      continue;
    }
    // サブテーブル形（`[workspace.lints.clippy.all]` の下に level / priority）。dotted 形と同じ理由で要る
    const sub = section.match(/^\[workspace\.lints\.clippy\.([A-Za-z0-9_-]+)\]$/);
    if (sub == null) continue;
    const kv = line.match(/^(level|priority)\s*=\s*(.+)$/);
    if (kv != null) {
      upsert(sub[1], kv[1] === "level" ? { level: lintLevel(kv[2].trim()) } : { priority: tomlInt(kv[2]) });
    }
  }
  const target = entries.get("disallowed_methods");
  if (target == null || (target.level !== "deny" && target.level !== "forbid")) return false;
  for (const group of DISALLOWED_METHODS_GROUPS) {
    const e = entries.get(group);
    if (e != null && e.level === "allow" && e.priority >= target.priority) return false;
  }
  return true;
}

/** evidence 用の件数。**読めない・配列が無い形は 0 とする**——素直に書くと
 *  「clippy 禁止 undefined 件」になり、この検査が存在する当の失敗ケースで evidence が壊れる。 */
export function clippyDisallowedCount(snapshot) {
  return disallowedMethodPaths(snapshot.read(CLIPPY_TOML) ?? "")?.length ?? 0;
}

export function checkClippyDisallowed(snapshot) {
  const findings = [];
  const toml = snapshot.read(CLIPPY_TOML);
  if (toml == null) {
    findings.push(finding(CLIPPY_TOML, 1, "禁止設定が読めない（G-clippy-disallowed 母集団の欠落）——消しても clippy は沈黙して exit 0 を返す（#950）"));
  } else {
    const paths = disallowedMethodPaths(toml);
    if (paths == null) {
      findings.push(finding(CLIPPY_TOML, 1, "disallowed-methods の配列が無い（#751 の禁止が丸ごと消えている・#950）"));
    } else {
      const missing = REQUIRED_DISALLOWED_METHODS.filter((p) => !paths.includes(p));
      if (missing.length > 0) {
        findings.push(
          finding(CLIPPY_TOML, 1, `disallowed-methods に ${missing.join(" / ")} が無い（行の消失・書き損じ・コメントアウトのいずれでも clippy は exit 0 で沈黙する・#950）`),
        );
      }
    }
  }
  const manifest = snapshot.read(SRC_TAURI_MANIFEST);
  if (manifest == null) {
    findings.push(finding(SRC_TAURI_MANIFEST, 1, "src-tauri の Cargo.toml が読めない（G-clippy-disallowed 母集団の欠落）"));
  } else if (!declaresEguiDependency(manifest)) {
    findings.push(finding(SRC_TAURI_MANIFEST, 1, "egui を依存に宣言していない（禁止パスが解決する前提が消え、clippy は診断そのものを出さなくなる・#950）"));
  }
  const root = snapshot.read("Cargo.toml");
  if (root == null) {
    findings.push(finding("Cargo.toml", 1, "ルート Cargo.toml が読めない（G-clippy-disallowed 母集団の欠落）"));
  } else if (!clippyMethodsDenied(root)) {
    findings.push(
      finding("Cargo.toml", 1, "[workspace.lints.clippy] の disallowed_methods が deny/forbid で無い（warn 既定へ戻り、禁止が -D warnings 依存の助言へ黙って降格する・#950）"),
    );
  }
  return findings;
}
