# research — issue #1143（`.claude/rules/` の paths が `scripts/governance/**` を覆わない）

## 1. issue の要約

`.claude/rules/safety-nets.md` と `.claude/rules/governance-docs.md` の `paths` は
`scripts/*.mjs` / `scripts/*.ps1` / `scripts/lib/**` を持つが、`scripts/governance/**` を持たない。
`*` は `/` を跨がないので、#1093 の per-check 分割で `scripts/governance/` へ移った判定 51 ファイル
（26 本の実装 + テスト）へルールが配送されない。`scripts/governance-check.mjs`（facade）は今も
`scripts/*.mjs` に当たるため、**`G-rules-globs`（マッチ 0 件の検知）は緑のまま**であり glob は壊れて見えない。

issue が決めることとして挙げるのは 3 点: (1) `scripts/governance/**` を足すか (2) `scripts/lib/**` との
関係（ディレクトリごとか `scripts/**` か） (3) この漏れの検出器を置くか、置けないなら死角として宣言するか。

## 2. 一次証拠（本セッションで実測したもの）

### 2a. 配送の A/B — harness 自身に問うた（2026-08-19）

**#837 が「受容する残余」として残した「配送そのものは実測できていない」を埋めた。**
本セッションはそれまで rules にマッチするファイルを Read しておらず（auto mode で `cat` を使っていた——
後述 2e）、once-per-session の配送予算が両 rule とも未消費だった。

| 手順 | 読んだファイル | 現行 glob | 観測 |
|---|---|---|---|
| A | `scripts/governance/registry.mjs` | 当たらない | **配送なし**（system-reminder が出ない） |
| 対照 | `scripts/governance-check.mjs` | `scripts/*.mjs` に当たる | **`safety-nets.md` と `governance-docs.md` の両方が全文配送された** |

対照が陽性なので、A の沈黙は「このセッションで配送機構が死んでいる」では説明が付かない。
**issue の主張（配送されない）は harness の実挙動として確定した。**

**射程の限定**: 証明したのは「現行 glob は `scripts/governance/` 配下へ配送しない」ことだけである。
`scripts/**` が 2 階層目（`scripts/governance/checks/`）まで届くかは、この 2 手では未証明 → 2a-2 で測った。

### 2a-2. `**` はディレクトリ階層を跨ぐか — サブエージェントで測った（2026-08-19）

配送予算は 1 セッション 1 回のため、本セッションでは B 側を測れない。**配送予算が未消費のサブエージェント**へ、
最初のツール呼び出しとして Read を 1 回だけ行わせた。

| 読ませたファイル | マッチする glob | 階層 | 観測 |
|---|---|---|---|
| `.claude/skills/health-check/references/mechanized-checks.md` | `.claude/skills/**` | **3 段下** | **`safety-nets.md` が配送された**（本文先頭行を逐語で確認） |

**harness の `**` は少なくとも 3 段のディレクトリを跨ぐ。** ゆえに `scripts/**` は
`scripts/governance/checks/G-*.mjs`（2 段下）を覆う。**「ディレクトリごとに 2 行足す」案の根拠は消えた。**

**ただしサブエージェントへの配送は 2 体で挙動が割れた（未解明・§8 の [採用] UNSURE-1）。**
上の 1 体では配送されたが、3b の 1 体は `scripts/governance-check.mjs`（`scripts/*.mjs` に当たる）を
Read しても配送されなかった——本人の履歴で「それ以前に paths にマッチするファイルを Read しておらず、
セッション中一度も rules 本文の system-reminder を受けていない」ことを確認済みで、**予算消費では説明が付かない**。
**「サブエージェントにも必ず配送される」とは書けない。**

### 2b. 被覆集合の差分 — 判定関数 `globToRegex` に問うた

`scripts/**` は現行 3 glob の和の**真の上位集合**であり、等号ではない。

```
scripts/ 配下の総数: 84
現行 3 glob の被覆: 32 件 / scripts/** の被覆: 84 件
scripts/** だけが覆う: scripts/governance/ 配下 51 件 + scripts/run-codex.sh 1 件
現行だけが覆う（scripts/** で失うもの）: 0 件
```

実値: `scripts/*.mjs` → `/^scripts\/[^/]*\.mjs$/`、`scripts/lib/**` → `/^scripts\/lib\/.*$/`、
`scripts/**` → `/^scripts\/.*$/`、`scripts/governance/**` → `/^scripts\/governance\/.*$/`。

`scripts/run-codex.sh` は `.sh` ゆえ現行のどの glob にも当たらない。**`scripts/**` を採るなら
これへの誤配送を新たに 1 件引き受ける**（`codex exec` の起動ラッパ。#837 が `clean-worktrees.mjs`
について引き受けたのと同型で、有害になる相手ではない）。

### 2c. #837 の判別線を同じ土俵で再現した

判別線は「check スキルの判定がそれに依存するか」で、#837 は代理として**規範文書からの参照数**を数えた。
同じ母集団（`AGENTS.md` / `CLAUDE.md` 群 / `docs/`（`docs/superpowers/` を除く） / `.claude/rules/` /
`.claude/skills/`）で数え直した:

| 対象 | 規範からの参照 | #837 の判別線 |
|---|---|---|
| `scripts/governance-check.mjs`（facade） | 14 文書 | ✓（既に paths 内） |
| `scripts/governance/dependents.mjs` | 3 文書 | ✓ |
| `scripts/governance/lib.mjs` / `edit-findings.mjs` | 各 2 文書 | ✓ |
| `scripts/governance/registry.mjs` / `instrument.mjs` | 各 1 文書 | ✓ |
| 検査 ID（例: `G-heading-refs` 11 / `G-module-linkage` 6 / `G-skill-table` 4 / `G-rules-globs` 2） | 2〜11 文書 | ✓ |
| `scripts/governance/evidence.mjs` / `test-helpers.mjs` | 各 0 文書 | 判別線には掛からない（が同ディレクトリ） |
| `scripts/clean-worktrees.mjs`（#837 が ✗ と裁定） | 0 文書 | ✗（誤配送を受容済み） |

**`scripts/governance/` 配下は #837 の判別線に掛かる。** 足す方向が #837 と同じ土俵で正当化される。

### 2d. 検出器の候補を絞る一次証拠（import closure が届かない辺）

「covered なファイルの import 先も covered であること」（import 閉包）は一見よい検出器だが、
**この repo の肝心な辺を 2 つとも見ない**:

- `.claude/hooks/post-edit.mjs:234` は `path.join(root, "scripts", "governance", "edit-findings.mjs")` を
  **spawn する**（静的 import ではない。`post-edit.test.mjs:309` が「静的 import を足していない」を
  逐語で固定している——足すと import 解決失敗で hook が全編集で沈黙するため）
- `scripts/governance/registry.mjs` は `readdirSync` + 動的 `import()` で `checks/` を走査する（静的辺なし）

ゆえに**母集団被覆形**（「repo 内の全 `.mjs` / `.ps1` / `.psm1` が rule の paths に覆われているか」）を採る。
この形に上の 2 つの死角は無く、#1093 の移動を実際に赤くできる。

### 2f. 母集団被覆形の判定ロジックを、実装前に実ツリーで測った（2026-08-19）

計画に書く述語は実装前に代表入力で実行する（`AGENTS.md`「検証の作法」）。`globToRegex` + `makeSnapshot` +
被覆述語をメモリ上で組み、実リポジトリへ当てた（**リポジトリのファイルは変更していない**）。

| 入力 | findings |
|---|---|
| 今日の main の paths（変異なし） | **102 件**（`safety-nets.md` 51 + `governance-docs.md` 51。すべて `scripts/governance/` 配下） |
| paths を `scripts/**` へ畳んだ場合 | **0 件** |
| rule が snapshot に無い / 母集団 0 件（canary） | 発火する |

**この検出器は #1143 の穴そのものを赤にする。** 敵対枠が git 履歴で追試し、`4f5f4d3f`（#1093）の**直前は 0 件・
直後は 45 件**と測った——**検出器は #1093 の移動の瞬間に赤くなっていた**。

### 2e. 副次観測（本 issue の射程外・穴が見えなかった機序）

auto mode で `cat` / `grep` によりファイルを読むと **rules は配送されない**（本セッションで、Read ツールを
使って初めて配送された）。`paths` の穴が長く見えなかった機序の一部を説明するが、本 issue で直す対象ではない。

## 3. 関連ファイル・シンボル（grep で実在を確認済み）

| パス | 役割 |
|---|---|
| `.claude/rules/safety-nets.md` | frontmatter `paths` 10 行。セーフティネット本体を触ると配送 |
| `.claude/rules/governance-docs.md` | frontmatter `paths` 6 行 |
| `scripts/governance/checks/G-rules-globs.mjs` | `globToRegex` / `checkRulesGlobs`。**マッチ 0 件のみ**を見る |
| `scripts/governance/lib.mjs` | `makeSnapshot`（`files` / `read`）・`WALK_EXCLUDE_PATHS`・`finding` |
| `scripts/governance/registry.mjs` | `checkModulesFrom` — `checks/` の走査から検査を導出（登録行が無い） |
| `scripts/governance-manifest.mjs` | `manifest()` の `checks` 列。検査 ID の増減は PR 本文の宣言と突き合わされる（#1092） |
| `.claude/skills/health-check/references/mechanized-checks.md` | 旧 Check 8 → G-rules-globs の写像表。備考に「マッチ 0 件の検知」 |
| `.claude/hooks/post-edit.mjs` | `dependentsReminder` / edit-findings の spawn 元 |

## 4. 再利用できる既存パターン

- **1 検査 1 ファイル**（#1093）: `scripts/governance/checks/G-<name>.mjs` に `id` / `run(snapshot, ctx)` を
  export し、`checks/` へ置けば `registry.mjs` が自動で拾う。登録行は無い。テストは同名 `.test.mjs`。
- **母集団 0 件を finding にする**（`G-rules-globs` の `rules.length === 0`）: 被覆形の述語は母集団が
  縮む側で沈黙するため、下界を assert する（`docs/development-principles.md`「検証の層と、層と層の隙間」）。
- **フォールトインジェクションは複製へ**（`safety-nets.md`）: `snapshot` はメモリ上のオブジェクトなので、
  paths を欠いた fixture snapshot を組めば稼働中の rules を弱めずに赤を測れる（`test-helpers.mjs` の `snap`）。

## 5. 技術的制約

- **`globToRegex` は harness の配送判定の**近似**である**（当該ファイルのヘッダが明記）。検出器は
  「harness がこう配送する」ではなく「documented な glob 意味論で覆われている」を判定する。
- 検査 ID を増やすと `governance-manifest` の `checks` 列が `+1` になり、PR 本文での宣言が要る（#1092 の設計通り）。
- 新検査の doc 波及は小さい: `.md` で検査数を数える現行文書は無く（`19 検査` の文字列は
  `.superpowers/` と日付付き設計書のみ＝歴史記録）、`mechanized-checks.md` は旧 Check → G の写像表なので
  新規検査は行を持たない。**ただし G-rules-globs を拡張する道を採るなら同表の備考が古びる**。
- 母集団は `makeSnapshot` 由来 ＝ **git 未追跡ファイルも入る**（`WALK_EXCLUDE_PATHS` で
  `node_modules` / `target` / `dist` / `workspace` / `.superpowers` / `.claude/worktrees` は落ちる）。
- 現在 `.mjs` 68 件は `scripts/`(61) / `.claude/hooks/`(6) / `.githooks/`(1) にのみ在り、`.ps1`/`.psm1` 22 件は
  `scripts/`(12) / `scripts/lib/`(10) にのみ在る。`safety-nets.md` の paths は後 2 者を既に覆う。
  `governance-docs.md` は `.claude/hooks/**` / `.githooks/**` を持たない（＝ .mjs 全件を縛るなら射程が広がる）。

## 6. 未解決の疑問（計画の未確定欄へ送るもの）

1. ~~`scripts/**` が 2 階層目まで配送されるか~~ → **2a-2 で解決（3 段下まで届く）**。
2. **検出器を新規検査にするか `G-rules-globs` の拡張にするか。** 新規 = 1 検査 1 ファイルの原則に沿い
   失敗の意味も別（死んだ glob vs 未被覆ファイル）。拡張 = manifest 差分ゼロだが `mechanized-checks.md` の
   備考を同時に直す必要がある。
3. **`governance-docs.md` も同じ検出器で縛るか。** #837 は「2 rules の配送対象が一致したことで片方だけが
   古びる形が消えた」に価値を置いた。縛るなら母集団は `scripts/` 部分木へ限るのが最安（.claude/hooks は
   あちらの射程外）。

## 7. 敵対的調査（3b）の反映

`workspace/adversarial-1143.txt` を参照。採否は §8 に記す。

## 8. 敵対的調査の採否

出力: `workspace/adversarial-1143.txt`（183 行）。**壊せた 1 / 壊せなかった 8 / ⚠️ 5**。

### 壊せた項目（1）

- **[BROKEN-1] 「母集団被覆形の検出器は今日の main で緑になる」は偽。** → **採用（重要）。**
  今日の main では**赤**（未被覆 51 件／rule。§2f で自分でも 102 件と実測）。穴がまだ塞がっていないのだから
  当然である。これは `research.md` の主張ではなく、私が敵対枠へ渡した命題 6 の書き方の誤りだが、
  **計画の受け入れ条件を「main で緑」と書けば同じ誤りが実装へ流れた**——受け入れ条件は
  「**paths を広げた後に**緑」と時点を明示する形へ直した。
  さらに敵対枠は git 履歴で追試し、`4f5f4d3f`（#1093）の**直前 0 件・直後 45 件**を測った（§2f に反映）。

### 壊せなかった項目（8）

52 件の差分（governance 51 + `run-codex.sh` 1）／`.mjs` 68 件・`.ps1`/`.psm1` 22 件の所在／import 閉包の
死角 2 辺（`post-edit.mjs:234` の spawn・`registry.mjs` の動的 import）／#837 の判別線の参照数表／
生きた `.md` に検査数の写しが無いこと／frontmatter の行数（10 / 6）／#1093 の移動が検出器を赤にすること／
`makeSnapshot` が git 追跡状態を見ないこと。**いずれも独立に再実行して一致。**

**判別線の表は補強された**: 敵対枠は 4 検査 ID の標本ではなく**19 検査 ID 全部**を数え、全 ID が 1 件以上の
参照を持つと測った（最小 1 件: `G-architecture-table` / `G-check-skill-enumeration`）。

### ⚠️ 確信の持てない所見（5）の採否

- **[UNSURE-1] 自分のセッションで配送を再現できなかった** → **採用（所見のみ・機序は棄却）。**
  所見は真である（§2a-2 に反映）。ただし本人が挙げた機序「サブエージェントには配送が効かない」は、
  **別のサブエージェントで配送を実測した**（§2a-2）ため成り立たない。追加の問い合わせで「予算消費でもない」
  ことまで確定し、**機序は未解明のまま残る**。→ 計画では、配送の実測を**陽性のみ証拠**として扱う。
- **[UNSURE-2] §2a の #1139 セッションの時系列は第三者検証不能** → **採用。** issue 本文の引用であり、
  本調査の結論はそれに依存しない（§2a の A/B で独立に確定させた）。research.md の一次証拠は §2a 以降だけである。
- **[UNSURE-3] `test-helpers.mjs` も 0 参照だが表に無い** → **採用**（§2c の表へ追記した。結論は変わらない）。
- **[UNSURE-4] 「.mjs は 3 か所にしか無い」は物理ディレクトリ数と誤読されうる** → **採用**（§5 は
  `scripts/`(61) / `.claude/hooks/`(6) / `.githooks/`(1) と内訳つきで書いてあるが、「3 か所」は
  ディレクトリ木ではなく**トップレベルの区画**の意味である）。
- **[UNSURE-5] `globToRegex` の「documented 意味論」に repo 内の外部典拠が無い**（自身のヘッダのみ）
  → **採用（射程の宣言として）。** 現行 paths 10 行は bare 名もブレースも使わないため今日の判定には効かない。
  検査の `//!` に「harness の判定の近似である」を宣言する（計画の死角宣言に含む）。
