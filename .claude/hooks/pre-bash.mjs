// PreToolUse (Bash|PowerShell) ガード。`gh pr create` が (1) 空 PR を作るのを防ぎ、
// (2) workspace/plan.md に未チェックの作業項目を残したまま PR になるのを防ぐ（#749）。
// さらに (3) コマンドの形だけで判定できる規範 5 件を判定する（#768。`judgeCommandShape`)。
// (3) は常時ロード規範から降ろしたもので、**全コマンドで走る**（(1)(2) は `gh pr create` 検出後のみ）。
//
// 設計: payload 全体を grep するのではなく tool_input.command **だけ**を読み、
// 「コマンド位置」に現れる `gh pr create` を検出して、それが走る瞬間にコミットが
// remote に存在するかを判定する。真になる経路は 2 つある:
//   1. 静的 — 鎖の中で `git push` が `&&` で先行する（`&&` が前段の成功を保証する）
//   2. 動的 — upstream が設定済みで、未 push コミットが無い
//
// 契約: **exit 2 だけがツール呼び出しをブロックする。** exit 0 は許可であり、
// それ以外の非ゼロ（Node が未捕捉例外で返す 1 を含む）は「非ブロッキングエラー」で
// コマンドはそのまま実行される。ゆえに fail-closed とは
// 「既定の exitCode を 2 に置き、許可が確定した経路だけが 0 を書く」ことである。
//
// この領域は hook にしか見えない。`gh pr create` はリポジトリを触らないため git hook は
// 鳴らず、push もしないので pre-push も鳴らない。GitHub ruleset も空 PR を防げない。
//
// 受容する未対応リスク（いずれも「gh がコマンド位置に現れない」形。意図的迂回であり事故モードではない。
// `--no-verify` と同格に人間専用として扱う）:
//   - `sh -c 'gh pr create'` / `eval` / バッククォート / `$(...)` の内側
//   - ラッパ経由（`timeout 5 gh pr create` / `nohup` / `xargs gh pr create`）
// 検出を shell パーサ相当まで広げると、payload 全体 grep の誤爆（#482 の D2）を作り直すことになる。
//
// 詳細と実測の根拠は issue #482。

// 実測（#482 Phase 2 のフォールトインジェクション V2）: Windows の PowerShell tool の tool_name は
// 文字列 "PowerShell" である。matcher が取りこぼすと hook は気づかれないまま素通りするため
// （それがこの issue の D1 そのもの）、ここは推測ではなく実測値で固定している。

import { readFileSync, existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import path from "node:path";

/** この hook が管轄するツール。matcher が部分一致しても、ここが最終的な境界になる。 */
export const TARGET_TOOLS = Object.freeze(["Bash", "PowerShell"]);

/** git 1 コマンドあたりの上限。hook 全体の timeout に丸投げすると kill された沈黙が
 *  exit≠2 になり fail-open へ倒れる。自分で打ち切って block する。 */
const GIT_TIMEOUT_MS = 10_000;

// コマンド位置 = 文字列先頭、または区切り文字の直後。`&&` の 2 文字目も区切り文字なので
// `&& gh pr create` は一致する。引用の内側までは解釈しない（過剰検出は許容、過小検出は不可）。
const AT_CMD_POS = String.raw`(?:^|[;&|\n\r(){}])\s*`;

// フラグはサブコマンドの前後どちらにも来うる。値がスペース区切り（`--repo o/r`）でも読み飛ばす。
//
// **ダッシュの終わりは一意でなければならない。** 素朴な `-{1,2}\S+` は `--x` を「`-` + `-x`」とも
// 「`--` + `x`」とも解釈でき、どちらも同じ位置で終わる。この曖昧さが `*` の下に入ると、
// 全体が失敗するときに 2^n 通りを探索する——`git --f0=v0 … --f23=v23 x` の 225 字で **747ms**
// かかった（実測）。#768 でこの FLAG が `git` セグメント判定経由で**全コマンド**に乗ったため、
// hook の timeout（exit≠2 = 非ブロッキング）で fail-open する経路になっていた。
// `--?[^-\s]` はダッシュの直後に非ダッシュを要求するので分割は 1 通りに決まる（`--` 単独は別枝）。
const FLAG = String.raw`(?:(?:--?[^-\s]\S*|--)(?:\s+[^-\s]\S*)?\s+)*`;

// `GH_TOKEN=x gh pr create` のような環境変数の前置もコマンド位置と見なす。
const ENV_PREFIX = String.raw`(?:[A-Za-z_]\w*=\S*\s+)*`;

// AT_CMD_POS は先頭の区切り文字を**消費する**。ゆえにコマンド本体をキャプチャし、
// tokenStart() で位置を復元する。match.index をそのまま使うと `&&` の片方の `&` しか
// 「push と gh の間」に入らず、`git push … && gh pr create` を誤って block する。
export const GH_PR_CREATE = new RegExp(
  `${AT_CMD_POS}${ENV_PREFIX}(gh\\s+${FLAG}pr\\s+${FLAG}create\\b)`,
);

// `git -C <tree> push` は意図的に一致させない。別ツリーへ push しても
// このツリーのコミットは remote に載らないため、安全な鎖ではない。
export const GIT_PUSH = new RegExp(`${AT_CMD_POS}(git\\s+(?:-\\S+\\s+)*push\\b)`);

// `git push --dry-run` は何も送信しない。動詞の名前ではなく意味を見る。
const DRY_RUN = /(?:^|\s)(?:--dry-run|-n)(?:\s|$)/;

// 鎖の途中で cwd が変わると、どのリポジトリに PR が作られるか判定できない。
export const CWD_CHANGE = new RegExp(`${AT_CMD_POS}((?:cd|pushd|popd|chdir|Set-Location)\\b)`, "i");

const REMEDY = "`git push -u origin HEAD` してから PR を作るか、`git push … && gh pr create` と `&&` で繋いでください。";

// コマンドの区切り文字。終端探索（segmentEnd）と区切り列の列挙（hasSafeChain）で共有する。
// 同じ文字クラスのリテラルを 3 か所目に作らないため定数へ寄せた（#768 の /dry-check）。
const SEPARATOR_CLASS = String.raw`[;&|\n\r]`;
const SEPARATOR = new RegExp(SEPARATOR_CLASS);
const SEPARATOR_RUN = new RegExp(`${SEPARATOR_CLASS}+`, "g");

/** at（コマンド位置）から次の区切り文字までの終端 index。区切りが無ければ文末。 */
export function segmentEnd(command, at) {
  const rel = command.slice(at).search(SEPARATOR);
  return rel < 0 ? command.length : at + rel;
}

/** マッチしたコマンド本体（キャプチャ 1）の開始位置。区切り文字を含まない。不一致は -1。 */
export function tokenStart(re, command) {
  const m = re.exec(command);
  return m ? m.index + m[0].length - m[1].length : -1;
}

/**
 * `gh pr create` より前で `git push` が走り、両者の間の区切りが**すべて `&&`**か。
 *
 * 区切りが 1 つでも `;` / `||` / `|` / 改行なら、push が失敗しても `gh pr create` が
 * 走りうる。そのときコミットは remote に無く、空 PR になる。判定不能として block する。
 */
export function hasSafeChain(command) {
  const ghAt = tokenStart(GH_PR_CREATE, command);
  if (ghAt < 0) return false;

  const pushAt = tokenStart(GIT_PUSH, command);
  if (pushAt < 0) return false;

  // push セグメントの終端 = 次の区切り文字。フラグは区切りを跨がないのでここで閉じる。
  const pushEnd = segmentEnd(command, pushAt);
  if (pushEnd > ghAt) return false; // push が後ろにある

  // `git push --dry-run` は送信しない。安全な鎖ではない。
  if (DRY_RUN.test(command.slice(pushAt, pushEnd))) return false;

  const separators = command.slice(pushEnd, ghAt).match(SEPARATOR_RUN) ?? [];
  return separators.length >= 1 && separators.every((s) => s === "&&");
}

const ALLOW = Object.freeze({ action: "allow" });
const block = (reason) => ({ action: "block", reason });

// ---------------------------------------------------------------------------
// コマンドの形だけで判定できる規範（#768）。ルート CLAUDE.md の常時ロード面から降ろした。
//
// **これらは全 Bash/PowerShell コマンドで走る**（従来の判定は `gh pr create` 検出の後ろにしか
// 無かった）。ゆえに全域関数でなければならない — throw すれば main() の catch が exit 2 を書いて
// セッションの全コマンドが止まり、hang すれば hook の timeout まで全コマンドが待つ。
// すべて正規表現とハッシュ参照だけで書き、入力長と候補数に対して線形に保つ。
// ---------------------------------------------------------------------------

// heredoc 演算子。`<<<`（herestring）と C++/Rust のシフト演算子を巻き込まないよう、
// 前後の `<` を lookaround で落とし、区切り語は識別子の形に限る。
const HEREDOC_OP = /(?<![<>])<<(?!<)-?[ \t]*(['"]?)([A-Za-z_]\w*)\1/g;
// 演算子と区切り語の直後に来てよいもの: 区切り・リダイレクト・改行・文末。
// **引用の閉じ（`"` / `'`）は入っていない** — これが `grep "x << y" f | head` を誤検出から守る。
// **`)` も入れない** — `node -e "console.log(1<<n)"` のようなシフト演算子を誤爆する（実測）。
// 代償として `(cat <<EOF)` のようなサブシェル内の heredoc はこの枝で拾えないが、終端行の
// 索引が拾うので取りこぼしにはならない。
const HEREDOC_FOLLOW = /^[ \t]*(?:[<>|&;]|\r?\n|$)/;
// 語だけの行 = heredoc 終端行の候補。
const LONE_WORD_LINE = /^[\t ]*([A-Za-z_]\w*)[\t ]*$/gm;

/**
 * bash の heredoc を使っているか。
 *
 * **全候補を回す。** 最初の候補だけを見ると `grep -rn "x << y" src && cat <<EOF…` のように
 * 囮のシフト演算子が先行したときに本物を取り落とす（fail-open。実測で確認した）。
 *
 * 終端行の索引は**最初に必要になったときだけ 1 パスで作る**。候補ごとに全文を走査すると
 * O(候補数 × 長さ) になり、候補 2 万件・21 万字で 1812ms かかった（実測。この形は 4.5ms）。
 * 索引は語ごとに**最後の**出現位置を持つ — 同じ語が演算子の前後どちらにも現れる場合、
 * 後ろの出現が終端行なので、最初の出現を保持すると取り落とす。
 */
export function usesHeredoc(command) {
  let loneWords = null;
  for (const m of command.matchAll(HEREDOC_OP)) {
    const after = m.index + m[0].length;
    if (HEREDOC_FOLLOW.test(command.slice(after))) return true;
    if (loneWords === null) {
      loneWords = new Map();
      for (const line of command.matchAll(LONE_WORD_LINE)) loneWords.set(line[1], line.index);
    }
    const at = loneWords.get(m[2]);
    if (at !== undefined && at >= after) return true;
  }
  return false;
}

// `\` をパス区切りに使った形。**広く `\` を見ない** — 正規表現エスケープ（`grep -E "\d+"`）や
// `find . -exec rm {} \;` を巻き込むためである。ドライブレター絶対パスと、環境変数展開に
// 続く区切りの 3 形に絞る。散文中の `C:\path` も止まるが fail-closed 方向ゆえ受容する。
//
// ドライブレターの形には**前後の条件が要る**。素の `[A-Za-z]:\\` は「語の末尾 1 文字 + `:` +
// 正規表現エスケープ」と区別できず、`rg "version:\s+" src` を止めた（実測）——拒否文言は
// 「`/` で統一せよ」なので、**復帰手順が適用できない拒否**になっていた。
// `(?<!\w)` が語の途中を落とし、`{2,}` が `a:\s` のような 1 文字続きを落とす。
// 代償: `cd C:\`（ドライブルート）と `C:\a` は検出しない（過小検出・受容する）。
const BACKSLASH_PATH = [/(?<!\w)[A-Za-z]:\\[\w.$%~-]{2,}/, /\$env:\w+\\/i, /%\w+%\\/];

/** パス区切りに `\` を使っているか。 */
export function usesBackslashPath(command) {
  return BACKSLASH_PATH.some((re) => re.test(command));
}

const PYTHON_AT_CMD = new RegExp(`${AT_CMD_POS}${ENV_PREFIX}(?:python3?|py)\\b`);
// 非 ASCII を安全に出す設定。どれか 1 つあれば足りる（`-X utf8` も等価な解である）。
const PY_UTF8 = /PYTHONIOENCODING\s*=|PYTHONUTF8\s*=|-X\s*utf8/i;
const NON_ASCII = /[^\x00-\x7F]/;

/**
 * Python を起動し、非 ASCII を含むのに出力エンコーディングを指定していないか。
 *
 * コマンド文字列に非 ASCII が現れない形（`python foo.py` でスクリプト側が出す）は見えない —
 * 過小検出として受容する（コマンドの形からは判定材料が無い）。
 */
export function needsPyEncoding(command) {
  return PYTHON_AT_CMD.test(command) && NON_ASCII.test(command) && !PY_UTF8.test(command);
}

// `git -C <tree> push` を意図的に外す GIT_PUSH とは違い、こちらは**すべての** git 呼び出しを
// 拾う。`--no-verify` はどのツリーに対しても人間専用であり、`-C` の有無で変わらない。
const GIT_AT_CMD = new RegExp(`${AT_CMD_POS}${ENV_PREFIX}(git)\\b`, "g");

/**
 * コマンド位置の `git` ごとに「`git` から次の区切りまで」を切り出す。
 *
 * セグメントに閉じて判定するのが要点である — コマンド全体を見ると
 * `grep -n "--no-verify" CLAUDE.md` のような「言及」で発火する（#482）。
 * 引用内の区切り（`git commit -m "a;b" --no-verify`）でセグメントが切れる過小検出は、
 * `sh -c` と同格の意図的迂回として受容する（引用認識は shell パーサ相当になる）。
 */
export function gitSegments(command) {
  const out = [];
  for (const m of command.matchAll(GIT_AT_CMD)) {
    const at = m.index + m[0].length - m[1].length;
    out.push(command.slice(at, segmentEnd(command, at)));
  }
  return out;
}

const NO_VERIFY = /(?:^|\s)--no-verify(?:\s|$)/;
// commit では `-n` は `--no-verify` そのもの（`-nm` のような結合形も同じ）。
// **push の `-n` は `--dry-run`** なので、短縮形の判定は commit セグメントに限る。
const SHORT_FLAG_N = /(?:^|\s)-[A-Za-z]*n[A-Za-z]*(?:\s|$)/;
// subcommand の前に来るグローバルフラグは FLAG で読み飛ばす。`(?:-\S+\s+)*` では
// `git -C /x commit` のスペース区切りの値を飛ばせず取りこぼした（実測 fail-open）。
const GIT_COMMIT = new RegExp(`^git\\s+${FLAG}commit\\b`);
const GIT_PULL = new RegExp(`^git\\s+${FLAG}pull\\b`);
const FF_ONLY = /(?:^|\s)--ff-only(?:\s|$)/;

/** `.githooks/` を迂回する形を使っているか。 */
export function usesNoVerify(command) {
  return gitSegments(command).some(
    (s) => NO_VERIFY.test(s) || (GIT_COMMIT.test(s) && SHORT_FLAG_N.test(s)),
  );
}

/** `--ff-only` の無い `git pull` か。ブランチは見ない（過剰検出を受容する）。 */
export function pullWithoutFfOnly(command) {
  return gitSegments(command).some((s) => GIT_PULL.test(s) && !FF_ONLY.test(s));
}

/**
 * 拒否文言。**「なぜ止めたか」と「代わりに何をするか」を必ず持つ**（#768）。
 *
 * 規範を常時ロード面から降ろす設計は、この文言がその場で教えることに賭けている。
 * ここが痩せると、規範は機構へ移らずに消えるだけになる。
 */
const SHAPE_REMEDY = Object.freeze({
  heredoc:
    "bash の HEREDOC は Windows で引用境界が壊れ、終端マーカーがコミットメッセージ本文へ漏れる事故が起きています。" +
    "複数行テキストは Write ツールで一時ファイルへ書いて `git commit -F <path>` / `--body-file <path>` で渡すか、" +
    "PowerShell の here-string `@'...'@`（閉じ `'@` は必ず行頭）を使ってください。" +
    "**コマンドに書くパスは、先頭から末尾まで区切りを `/` にしてください**（絶対パスなら `C:/Users/...` の形）。",
  backslashPath:
    "パス区切りの `\\` はエスケープが要るため壊れやすく、Bash では黙って食われて**エラーではなく誤った結果**になります" +
    "（**この判定は Bash / PowerShell の両方に発火します**——ツールを替えても要件は変わりません）。" +
    "`/` で統一してください — PowerShell でも Git / Node / Cargo は `/` を受け付けます。",
  pyEncoding:
    "cp932 コンソールで非 ASCII（`—`・日本語など）を print すると `UnicodeEncodeError` で落ちます。" +
    "`PYTHONIOENCODING=utf-8` を前置してください（`PYTHONUTF8=1` か `python -X utf8` でも同じです）。",
  noVerify:
    "`--no-verify`（commit では `-n` も同義）は `.githooks/` の main 保護を迂回します。**人間専用であり、エージェントは使用してはなりません**。" +
    "hook が拒んだなら、迂回せずその理由を解消してください。main へ入れたい変更は feature ブランチと PR を経由します。",
  pull:
    "非 FF の `git pull` はマージコミットを作り、main では `.githooks/pre-merge-commit` が拒否します。" +
    "`git pull --ff-only` を使ってください（**この判定はブランチを問わず発火します**。FF できないほど発散しているなら、" +
    "feature ブランチでは `git fetch` してから `git rebase` を明示的に打ち、" +
    "main では `.githooks/` が rebase も非 FF merge も拒むので、想定外のコミットが local main に入っていないかを先に確認してください）。",
});

/**
 * コマンドの形だけで決まる判定（#768）。I/O を要さないので git / plan.md を読む前に走る。
 *
 * `platform` を**値で**受けるのは `process.platform` が失敗しないためである — `readGitState` の
 * ような `{ ok: false }` を持つ reader 形にすると、失敗しえないものへ失敗の表現を与えて嘘になる。
 *
 * **`platform` が渡されないときは Windows 専用判定を発火させない。** これは「判定不能」ではなく
 * 「規範の射程外」であり、block へ倒すと非 Windows で false block になる。この状態へ至る経路は
 * 「呼び出し側が第 4 引数を渡さない」1 本だけで、`main()` の配線はソースカナリアと
 * process 級 e2e（`npm test` は ubuntu と windows の両方で走る）が固定する。
 *
 * 一致は最初の 1 件で返す。複数該当しても、直せば次が出るので取りこぼしにはならない。
 */
export function judgeCommandShape(command, platform) {
  if (usesNoVerify(command)) return block(SHAPE_REMEDY.noVerify);
  if (pullWithoutFfOnly(command)) return block(SHAPE_REMEDY.pull);

  // 以下は Windows 固有の失敗様態（cp932 コンソール・`\` を食う shell）にのみ当たる。
  if (platform !== "win32") return null;
  if (usesHeredoc(command)) return block(SHAPE_REMEDY.heredoc);
  if (usesBackslashPath(command)) return block(SHAPE_REMEDY.backslashPath);
  if (needsPyEncoding(command)) return block(SHAPE_REMEDY.pyEncoding);
  return null;
}

/**
 * 判定の SSOT。`readGitState` を注入することで、分岐表全体が git 無しでテストできる。
 *
 * 「管轄外」と「判定不能」を混同しない。前者は allow（対象ツール以外・`gh pr create` 以外）、
 * 後者は block（見えないものは通さない）。
 *
 * `platform` は `judgeCommandShape` へそのまま渡る（省略時の意味はそちらの注釈が SSOT）。
 */
export function decide(payload, readGitState, readPlanState, platform) {
  const tool = payload?.tool_name;
  if (!TARGET_TOOLS.includes(tool)) return ALLOW; // 管轄外

  const command = payload?.tool_input?.command;
  if (typeof command !== "string") {
    return block(`${tool} の tool_input.command を読めませんでした。何が実行されるか判定できません。`);
  }

  // コマンドの形だけで決まる判定（#768）。I/O を要さないのでここに置く。
  const shape = judgeCommandShape(command, platform);
  if (shape) return shape;

  const ghAt = tokenStart(GH_PR_CREATE, command);
  if (ghAt < 0) return ALLOW; // 管轄外

  const cwdAt = tokenStart(CWD_CHANGE, command);
  if (cwdAt >= 0 && cwdAt < ghAt) {
    return block("PR 作成の前に作業ディレクトリを変更しています。どのリポジトリに PR が作られるか判定できません。");
  }

  const plan = readPlanState();
  if (!plan.ok) return block(`${plan.reason}。計画の完了状態を判定できません。`);
  if (plan.exists && plan.unchecked > 0) {
    return block(
      `workspace/plan.md に未チェックの作業項目が ${plan.unchecked} 件残っています。` +
        `項目を完了させて [x] にするか、やらないと決めた項目は計画から外して理由を記録してから、PR を作成してください。`,
    );
  }

  if (hasSafeChain(command)) return ALLOW; // push が `&&` で先行する

  const state = readGitState();
  if (!state.ok) return block(`git の状態を確認できませんでした（${state.reason}）。${REMEDY}`);
  if (!state.upstream) return block(`upstream が未設定です（またはリポジトリ外です）。${REMEDY}`);
  if (state.unpushed) return block(`未 push のコミットがあります。空 PR / \`Closes\` 誤 close を防ぐため止めました。${REMEDY}`);

  return ALLOW;
}

/** upstream の有無と未 push コミットの有無。判定できないときは `{ ok: false }`。 */
export function readGitState(cwd) {
  const git = (args) =>
    spawnSync("git", args, { cwd, encoding: "utf8", shell: false, timeout: GIT_TIMEOUT_MS });

  const upstream = git(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
  if (upstream.error) {
    return { ok: false, reason: `git rev-parse を実行できません: ${upstream.error.code ?? upstream.error.message}` };
  }
  // 非ゼロは「upstream 未設定」または「リポジトリ外」。どちらも block へ倒すため区別しない。
  if (upstream.status !== 0) return { ok: true, upstream: null, unpushed: false };

  const log = git(["log", "@{u}..HEAD", "--oneline"]);
  if (log.error) {
    return { ok: false, reason: `git log を実行できません: ${log.error.code ?? log.error.message}` };
  }
  if (log.status !== 0) return { ok: false, reason: `git log が exit ${log.status} で失敗しました` };

  return { ok: true, upstream: upstream.stdout.trim(), unpushed: log.stdout.trim().length > 0 };
}

/** 行頭（インデント許容）の未チェック項目。plan.md のコードブロック内の一致は過剰検出として受容する。 */
export function countUnchecked(text) {
  return (text.match(/^\s*[-*]\s+\[ \]/gm) ?? []).length;
}

/**
 * 祖先を遡り relTarget を含むディレクトリを返す。見つからなければ null。
 * post-edit.mjs の findUp と同じ実装（重複だが 2 hook 間の import は結合を増やすため許容）。
 */
function findUp(startDir, relTarget) {
  let dir = path.resolve(startDir);
  for (;;) {
    if (existsSync(path.join(dir, relTarget))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

/**
 * リポジトリルート（cwd から最近接の `.git` へ遡って導出）の `workspace/plan.md` の完了状態。
 * cwd がサブディレクトリ（例: src-tauri/）でもゲートが沈黙で外れないようにする（I2）。
 * `.git` が見つからなければ cwd 自身を root とみなす（リポジトリ外での誤爆を作らない）。
 * plan.md が無ければ管轄外、存在するのに読めなければ判定不能。
 */
export function readPlanState(cwd) {
  const root = findUp(cwd, ".git") ?? cwd;
  const file = path.join(root, "workspace", "plan.md");
  if (!existsSync(file)) return { ok: true, exists: false, unchecked: 0 };
  try {
    return { ok: true, exists: true, unchecked: countUnchecked(readFileSync(file, "utf8")) };
  } catch (e) {
    return { ok: false, reason: `workspace/plan.md を読めません: ${e.code ?? e.message}` };
  }
}

/** stderr は exit 2 のときだけ Claude に届く。allow は無出力（沈黙 = 許可）。 */
function emitBlock(reason, toolName) {
  const who = toolName ? ` (tool_name=${toolName})` : "";
  process.stderr.write(`BLOCKED [pre-bash]${who}: ${reason}\n`);
  process.exitCode = 2;
}

function main() {
  let payload;
  try {
    payload = JSON.parse(readFileSync(0, "utf8"));
  } catch (e) {
    return emitBlock(`payload の JSON 解析に失敗しました: ${e.message}`);
  }

  // hook プロセスの cwd ではなく、セッションの作業ディレクトリで git を評価する。
  const cwd = typeof payload?.cwd === "string" && payload.cwd.length > 0 ? payload.cwd : process.cwd();

  // platform は値で渡す（reader 形にしない理由は judgeCommandShape の注釈）。
  // この 1 行が Windows 専用判定の唯一の配線であり、カナリアと e2e が両方でこれを固定する。
  const result = decide(payload, () => readGitState(cwd), () => readPlanState(cwd), process.platform);
  if (result.action === "allow") {
    process.exitCode = 0;
    return;
  }
  emitBlock(result.reason, payload?.tool_name);
}

// テストが import しただけで stdin 読み取りが走らないようにする。
const invokedDirectly =
  Boolean(process.argv[1]) && import.meta.url === pathToFileURL(process.argv[1]).href;

if (invokedDirectly) {
  // 既定は block。allow が確定した経路だけが 0 を書く。
  // Node は未捕捉例外時に process.exitCode を無視して 1 で終了するため、catch は省略できない
  // （exit 1 = 非ブロッキング = コマンドが実行される = fail-open）。
  process.exitCode = 2;
  try {
    main();
  } catch (e) {
    emitBlock(`HOOK ERROR: ${e?.stack ?? String(e)}`);
  }
  // process.exit() は使わない。stdout/stderr がパイプのとき未 flush 出力を切り捨てる。
}
