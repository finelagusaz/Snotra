//! governance:check の共有基盤。1 つの検査しか使わない helper はここへ置かず、
//! その検査のファイルへ一緒に移す（`scripts/governance/checks/`）。ここに在るのは、
//! 複数の検査ファイルが使うものか、facade 自身が直接使うもの（母集団・0 件検知・evidence の算出など）。
//! 依存は Node 標準モジュールのみ（`governance-check.mjs` の契約を継承する）。
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

/** 走査から除外するディレクトリ。名前ベース（任意の深さの生成物）とルート相対パス
 *  （untracked バッファ）を分ける——`ui/src/workspace/` のような将来の同名ソースを気づかれないまま
 *  落とさないため、PATHS 側はルート錨止めにする
 *  **PATHS の照合は `rel` の完全一致である**——一致したディレクトリへ降りないので配下ごと落ちる。
 *  `docs/.superpowers` も `.superpowers-extra` も `rel` が一致しないので巻き込まない（#728）
 *  `.superpowers/` は SDD（subagent-driven-development）の作業バッファで、gitignore 済みゆえ CI の
 *  チェックアウトには存在しない——走査に含めると同じコマンドが手元と CI で別の母集団を見る（#722）。 */
const WALK_EXCLUDE_NAMES = new Set([".git", "node_modules", "target", "dist"]);
const WALK_EXCLUDE_PATHS = ["workspace", ".claude/worktrees", ".superpowers"];

/** リポジトリを歩いて snapshot（files: "/" 区切り相対パス一覧, read(rel)）を作る。
 *  列挙は fs 自身に問う（`git ls-files` の pathspec `**` 意味論の罠を避ける・health-check Check 1 注記） */
export function makeSnapshot(root) {
  const files = [];
  const walk = (dir) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const rel = path.relative(root, path.join(dir, e.name)).replaceAll("\\", "/");
      if (e.isDirectory()) {
        if (!WALK_EXCLUDE_NAMES.has(e.name) && !WALK_EXCLUDE_PATHS.includes(rel)) walk(path.join(dir, e.name));
      } else {
        files.push(rel);
      }
    }
  };
  walk(root);
  return {
    files,
    read: (rel) => {
      try {
        return fs.readFileSync(path.join(root, rel), "utf8");
      } catch {
        return null;
      }
    },
  };
}

/** コードフェンス（``` 行）の内側を落として [lineNo, text] を返す（誤検出源: SPEC.md の TOML コメント等） */
export function linesOutsideFences(text) {
  const out = [];
  let inFence = false;
  text.split("\n").forEach((line, i) => {
    if (/^\s*```/.test(line)) {
      inFence = !inFence;
      return;
    }
    if (!inFence) out.push([i + 1, line]);
  });
  return out;
}

export const finding = (file, line, message) => ({ file, line, message });

/** `git check-ignore` は**ファイルの存在に依らずパス名だけで判定する**（2026-08-14 実測: 不在の
 *  `test-results/never-created.json` が当たり、`docs/nonexistent-typo.md` は当たらない）——これが
 *  「CI に存在しない生成物の名前を散文へバッククォートで書けない」という表記の歪みを解く（#1088）。
 *  **読むのはチェックアウト内の `.gitignore` だけではない**——`.git/info/exclude` と、ユーザ全体の
 *  除外ファイル（`core.excludesFile`。未設定なら `$XDG_CONFIG_HOME/git/ignore`、既定
 *  `~/.config/git/ignore`）も読む。これらはどちらもチェックアウトの外（機体ごとのローカル状態）に
 *  あり CI のチェックアウトには存在しないので、**免除の面は「手元 ⊇ CI」になりうる**——手元だけで
 *  免除されるパスがあれば、その回は「手元で緑・CI で赤」が起こる（逆は起きない。CI が手元より
 *  広く免除することは無い）。実例: `.claude/agent-registry.json` は追跡された `.gitignore` に無いが
 *  `.git/info/exclude` にあり、この機体では免除される（2026-08-14 実測）。
 *  **exit 1 は「該当なし」であって失敗ではない**（失敗は 128）。git が無い・repo でない場合、
 *  および**候補のいずれか 1 件でもリポジトリ外パス（絶対パス・`..` でツリー外へ出る相対パス）で
 *  status が 128 になった場合**は空集合を返す——後者は batch 単位の判定ゆえ、1 件の汚染が
 *  同じ回の他の候補の免除も道連れに落とす（向きは赤側＝安全。何も免除しない側へ倒す）。
 *  **決定的性**: 同一チェックアウト・同一機体では再現する（ネットワーク・時刻・環境変数に依らない）。
 *  機体をまたぐ決定性は無い（上記のとおり `.git/info/exclude` 等が機体ごとに違う）。 */
export function gitIgnoredPaths(paths, root = process.cwd()) {
  if (paths.length === 0) return new Set();
  const r = spawnSync("git", ["check-ignore", "--stdin", "-z"], {
    cwd: root,
    input: paths.join("\0"),
    encoding: "utf8",
  });
  if (r.error || r.status === 128) return new Set();
  return new Set(r.stdout.split("\0").filter(Boolean));
}

/** 参照先になりうる位置（ATX 見出し / 番号付きリスト項目 / 太字リード） */
export function collectAnchors(text) {
  const out = [];
  for (const m of text.matchAll(/^#{1,6}\s+(.+?)\s*$/gm)) out.push(m[1]);
  for (const m of text.matchAll(/^\s*\d+[.)]\s+(.+?)\s*$/gm)) out.push(m[1]);
  for (const m of text.matchAll(/^\s*(?:[-*]|\d+[.)])\s+\*\*(.+?)\*\*/gm)) out.push(m[1]);
  return out;
}

export const normAnchor = (s) => s.replace(/[`*「」\s]/g, "");

/** 参照文字列 → リポジトリ内パス。解決できなければ null */
export function resolveRefTarget(snapshot, doc, target) {
  if (/^\/[a-z0-9-]+$/.test(target)) {
    const p = `.claude/skills/${target.slice(1)}/SKILL.md`;
    return snapshot.files.includes(p) ? p : null;
  }
  if (!target.endsWith(".md")) return null;
  const norm = (p) => path.posix.normalize(p);
  const rel = norm(path.posix.join(path.posix.dirname(doc), target)); // 文書ディレクトリ基準を優先
  if (snapshot.files.includes(rel)) return rel;
  if (snapshot.files.includes(norm(target))) return norm(target);
  const suffix = `/${norm(target)}`;
  if (suffix.includes("..")) return null;
  const hit = snapshot.files.filter((f) => f.endsWith(suffix));
  return hit.length === 1 ? hit[0] : null;
}

/** G-references/G-spec-sections の対象文書（ガバナンス文書群）。docs/superpowers/ は歴史資料（#589 で非規範化）ゆえ除外 */
/** G-references / G-spec-sections の走査元。`docs/adr/` を除くのは**凍結された歴史**の契約
 *  （`ADR-adr-frozen-history`）——ADR 本文は決定日時点の世界の記述であり、そこから外への参照
 *  （パス・SPEC 節）は生きた層の改名・移動に追随させない。守るのは実在の辺だけ
 *  （生きた層 → ADR と ADR → ADR の短縮引用 = `adrCitationDocs` が明示的に持つ）
 *
 *  **保証は狭い**: 3 検査が照合するのは、ここが返した文書の中に書かれた参照だけである
 *  （G-adr-citations は `adrCitationDocs` で入力を足す）。ここに入らない層——**ルート直下へ新設した
 *  文書**など——に書いた実在しない参照・`SPEC §N`・ADR 引用は素通りする（2026-08-09 実測・#1008）。
 *  **リポジトリ全体を見る検査ではない。**
 *  なお crate 名の正規表現は `MODULE_INDEX_CRATES` と同じ一覧を独立に持つ 2 本目であり、
 *  真の母集団はどちらもルート `Cargo.toml` の `[workspace] members` である。**crate を増やしたときの
 *  この正規表現の更新漏れは `governance-check.test.mjs` の母集団カナリア（#701）が `npm test` で
 *  捕まえる**（詳細は `MODULE_INDEX_CRATES` の doc）——**カナリアが見ないのはルート文書の配列の側**
 *  であり、そこへの足し忘れは今も沈黙する。 */
export function governanceDocs(snapshot) {
  return snapshot.files.filter(
    (f) =>
      ["CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "SPEC.md"].includes(f) ||
      (f.startsWith("docs/") && f.endsWith(".md") && !f.startsWith("docs/superpowers/") && !f.startsWith("docs/adr/")) ||
      /^(snotra-core|snotra-egui-runtime|src-tauri|snotra-settings)\/CLAUDE\.md$/.test(f) ||
      /^\.claude\/rules\/[^/]+\.md$/.test(f) ||
      /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f),
  );
}

/**
 * G-heading-refs / G-near-heading-refs の走査元のうち **md の腕**。見出し参照はガバナンス文書の外
 * （`PERFORMANCE.md`・`.claude/agents/`）にも書かれ、実際にそこで腐っていた
 * （`PERFORMANCE.md` が src-tauri の WebView2 期の節を指したまま残っていた。**消えた節の名前は
 * 正準形で書かない**ので散文にしてある——書けばこの検査が自分のコメントを赤にする）ため母集団を広く取る。
 * 除外は履歴資料（`docs/superpowers/`）・作業バッファ（`workspace/`・`/implement` が削除する）・
 * 凍結された歴史（`docs/adr/`・`ADR-adr-frozen-history`。実測で照合 203 件中 86 件が
 * ADR 内＝歴史の研磨だった）。**除外が絞るのは走査元だけである**——参照先のアンカー解決は
 * `snapshot.files` 全体に対して行われるため、生きた層から ADR の見出しを指す参照は除外後も照合される。
 * **ソースの腕（`.rs`）は `headingRefSourceDocs` が別に持つ。** 束ねて 1 本にしないのは、
 * `runAll` の 0 件検知が**母集団ごとに 1 本ずつ**要るからである（`staleDocs` / `staleGuides` と
 * 同型——和にすると md 側の長さが `.rs` の消滅を埋めて永久に沈黙する）。
 */
export function headingRefDocs(snapshot) {
  return snapshot.files.filter(
    (f) =>
      f.endsWith(".md") &&
      !f.startsWith("docs/superpowers/") &&
      !f.startsWith("workspace/") &&
      !f.startsWith("docs/adr/"),
  );
}

/**
 * 同じ走査元の **ソースの腕**（`.rs`）。#921 で `SPEC.md` の節の中身を移したとき、`.rs` 側の参照は
 * 手で直す必要があり検査は緑のままだった。`.rs` のコメントには正準形の参照が 27 件あり（#925 実測）、
 * そのすべてが参照先の改題・移動・削除に対して沈黙していた。
 *
 * **Rust のテストコードを外さない。** `adrCitationDocs` が `*.test.mjs` を外すのは「フィクスチャが
 * 赤経路を測るため意図的に実在しない名前を持つ」からであって、テストだからではない。Rust の
 * テストコメントに書かれた規範への参照は本物であり、腐れば同じ害になる——#925 が見つけた腐り 1 件は
 * 現に `#[cfg(test)]` の内側にあった（`snotra-settings/src/tabs/visual.rs`）。`productionOnly` 相当を
 * 「G-stale-identifiers との対称性の完成」として後から入れてはならない（その非対称は意図である）。
 *
 * **`.mjs` / `.ps1` は入れない**（#925 の裁定）。実測した finding 9 件の内訳は、6 件が
 * `governance-check.test.mjs` のフィクスチャ（赤経路を測るため意図的に実在しない名前を持つ）、
 * 残り 3 件が**本ファイル自身のコメント**（正準形の例示 1・`…` で切り詰めた表記 1・本物の腐り 1）。
 * 入れれば検出器の説明が検出器を赤にする（`docs/adr/` を全検査の走査元から外したのと同クラスの理由）。
 * **腐り 1 件は #925 で直した**——母集団に入れなくても、直せるものは直す。
 *
 * **md の腕が持つ除外接頭辞を共有しない。** `docs/adr/` の除外は「ADR **本文**は決定日時点の世界の
 * 記述として凍結する」という散文についての契約であり（`ADR-adr-frozen-history`）、`docs/superpowers/`
 * も #589 で非規範化された文書である。どちらもコードについては何も決めていない——決まっていない契約を
 * 述語で主張しない（該当する `.rs` は 0 件ゆえ挙動の差も無い）。
 *
 * **受容する残余**: rustdoc のコードフェンス（`///` に続く ``` 行）は `linesOutsideFences` の
 * `/^\s*```/` に当たらないため、rustdoc の例の中に書かれた参照も照合される（今日の影響は 0 件）。
 */
export function headingRefSourceDocs(snapshot) {
  return snapshot.files.filter((f) => f.endsWith(".rs"));
}

/** 語彙源ではなく検査対象になる、`.claude/**` の外の**固定パス**文書
 *  （意図の SSOT・常時ロードの規範・設定 UI のデザイン規約）。
 *  **静的リテラルであること自体が fail-closed である**——読めなければ `scanStaleIdentifiers` が
 *  「母集団の欠落」を出すので、グロブ由来の母集団（`staleIdentifierGuideDocs`）と違って
 *  `runAll` 側の 0 件検知を別に置く必要がない。
 *  **保証は狭い**——「意図の SSOT」級の文書を新設してここへ足さなければ、その文書の腐り識別子は
 *  一度も照合されない（2026-08-09 実測: ルート直下に新設した文書へ実在しない識別子を 3 形置いても
 *  照合件数が動かなかった・#1008）。 */
export const STALE_EXTRA_DOCS = ["SPEC.md", "CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"];

/** 規範の散文。skills / rules / agents の md。
 *  **検査対象の全体ではない**——`staleIdentifierTargets` と分けてあるのは、`runAll` の
 *  「対象 md が 0 件（母集団の欠落）」が `.claude/**` の消滅で鳴り続けるためである
 *  （`STALE_EXTRA_DOCS` を混ぜると長さが常に 1 以上になり、その検知が永久に沈黙する） */
export function staleIdentifierDocs(snapshot) {
  return snapshot.files.filter((f) => /^\.claude\/(skills\/.*|rules\/[^/]+|agents\/[^/]+)\.md$/.test(f));
}

/** G-stale-identifiers の母集団のうち、**グロブ由来**の開発ガイド（`docs/**`）。
 *  除くのは `docs/superpowers/`（#589 で非規範化された当時の設計）と `docs/adr/`（却下案＝
 *  **もう存在しない案**を書く場所）。**基準は「日付を持つか」ではなく「もう成り立たないことを書く場所か」である**
 *  ——`docs/design/` は `status: Agreed` で `docs/architecture.md` が現在形で指す先ゆえ含める。
 *  `docs/adr/` の除外は #893 当時この検査だけの非対称だったが、`ADR-adr-frozen-history` で
 *  全検査（G-references / G-heading-refs 等の走査元）へ揃った。
 *  **静的リテラルと違い空になっても自分では鳴れない**ので `runAll` が 0 件検知を持つ */
export function staleIdentifierGuideDocs(snapshot) {
  return snapshot.files.filter(
    (f) => f.startsWith("docs/") && f.endsWith(".md") && !f.startsWith("docs/superpowers/") && !f.startsWith("docs/adr/"),
  );
}

/** G-stale-identifiers の検査対象。規範の散文 + 開発ガイド + 固定パスの文書。
 *  `STALE_EXTRA_DOCS` は実在を問わず加える——読めなければ `scanStaleIdentifiers` が母集団の欠落として鳴る */
export function staleIdentifierTargets(snapshot) {
  return [...staleIdentifierDocs(snapshot), ...staleIdentifierGuideDocs(snapshot), ...STALE_EXTRA_DOCS];
}
