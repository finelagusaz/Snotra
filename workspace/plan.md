# plan — issue #1300

`/plan-review` Step 1 項目 7 を道具化し、高リスク計画の既定を Step 2b へ寄せる試行を始める。調査と敵対的調査の採否は `workspace/research.md`。

## 目的と受け入れ条件

1. `npm run prose:sweep -- [--plan <path>] <語>...` が、渡した語（識別子・概念ラベル）を含む行を**切り詰めず全件**出す。行の種類（コメントか否か）で落とさない
   - 語ごとの件数を出し、**0 件の語も印字**する（0 件はラベルが外れているか物理改行を跨いでいる合図）
   - ヒットのあったファイルを面で分類する（常時ロード規範 / skills・rules・hooks / 生きた文書 / 凍結された歴史 / コード）
   - `--plan` を渡すと、パス文字列がその plan に現れないファイルへ `★未掲載` を付け、未掲載ファイル数を出す
   - 語が無い・未知のフラグ・`--plan` が読めない → 使用法を出して exit 2。`git grep` 自体の失敗（exit ≥ 2）→ exit 1。**どちらも「0 件」として印字しない**（壊れたとき緑で推移しない）
2. `/plan-review` Step 1 項目 7 がこの道具を打つ形に変わり、`allowed-tools` が宣言と一致する
3. `/plan-review`「リスク判定」で、高リスクの既定が Step 2b になり、Step 2 は「issue に書かれていない人間の裁定が計画の前提にある」場合に限られる
4. `/start-issue` 5b のセルフレビュー雛形に、道具の件数と試行（#1300）の記録欄があり、欄の隣に撤去条件がある
5. 本 PR は `Refs #1300`（閉じない。#1300 を試行の台帳として開けたまま・ユーザー裁定 2026-10-25）。`--deep` は触らない（同）

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/prose-sweep.mjs`（新規） | 純関数 `parseArgs` / `parseGrepZ` / `surfaceOf` / `buildReport` と、`git grep` を打つ薄い `main`。冒頭コメントに契約（race-boundaries の殻: 依存ゼロ・決定的・0 件の明示・exit 2）と、`isCommentLine` 相当を持たない理由（3b 所見 4: 行の種類で絞ると道具の中に切り詰めを再導入する）を書く |
| `scripts/prose-sweep.test.mjs`（新規） | 下の「テスト方針」 |
| `package.json` | `"prose:sweep": "node scripts/prose-sweep.mjs"` |
| `docs/build-commands.md`「Windows/macOS/Linux で実行可能」 | コマンド 1 行と、出力仕様の SSOT が `scripts/prose-sweep.mjs`・読み方は `/plan-review` が持つ旨の bullet 1 つ |
| `.claude/skills/plan-review/SKILL.md` | frontmatter `allowed-tools` に `Bash(npm run prose:sweep *)`。Step 1 項目 7 を道具の呼び出しへ書き換え（★未掲載を 1 件ずつ裁く・0 件の語は言い換えて打ち直す）。「リスク判定」末尾の「Step 2 または Step 2b の**どちらか一方**」の文を既定 2b ＋ Step 2 の条件へ。Step 2 冒頭の「計画の分解を前提に検証する方が適切な場合に使う」を条件の参照へ。見出し「Step 2b — 独立導出による網羅性レビュー」は**変えない**（`AGENTS.md`・`governance-docs.md`・ADR 等からの参照の着地） |
| `.claude/skills/start-issue/SKILL.md` 5b | 雛形へ 2 行（`- 散文の全件列挙: 語 <n> / <総件数> 件 / ★未掲載 <m>（計画へ足した <k>）` と `- 試行（#1300・高リスクのみ）: 方式 <Step 2 / Step 2b> / 設計変更 <n> / 主の見落とし <n>`）と注記（列の意味は #1300 本文の表に従う・**#1300 が閉じたら試行の行と注記を消す**・散文の欄が空なら項目 7 は実施されていない） |

触らない: `docs/adr/ADR-risk-tiered-plan-review.md`（凍結された歴史。「観点を限定した1体」は当時の決定として真）、`start-issue` 5a の `--deep` 文、plan-review Step 3 の方式欄（両方式を並べるだけで偽にならない）、`race-boundaries.mjs`（共通化しない: race 側は `.md` を除外し続ける・母集団が差分）。

## 実装順序

1. 道具（テスト先行）→ package.json → build-commands
2. 道具を本 PR 自身へ当てる（下の「自己適用」）
3. SKILL.md 2 本

## 不変条件と異常系

- 0 件と失敗を区別する: `git grep` の status 0 は一致あり・1 は一致なし。**`error` がある（git 不在）・status が `null`（maxBuffer 超過・シグナル）・0/1 以外 → 失敗として exit 1**。`maxBuffer` を明示する（plan-review 要対処 3）
- 母集団: `git grep -I --untracked -n -z -F --no-color -e <語> -- . ':!workspace'`、**cwd は `git rev-parse --show-toplevel` に固定**（`src-tauri` を cwd にすると 71 → 2 行・plan-review 要対処 4）。`-I` でバイナリを外す（無いと `Binary file X matches` が NUL 無しの 1 行で混ざる・実測・要対処 1）。`.gitignore` 尊重・追跡＋未追跡・`workspace/` を除く（計画自身が語を含む）。`-z` で `:` を含む本文と日本語パスの quote を避ける
- `parseArgs`: 空語を弾く（`-e ""` は全行に一致する）。`--` 以降はフラグに見えても語として扱う（要対処 5）
- `--plan` の掲載判定は**パス境界つき**: パスの前が行頭か英数字・`/`・`.`・`-`・`_` 以外であること（ルート `CLAUDE.md` が `snotra-core/CLAUDE.md` の部分文字列として偽の掲載になるのを防ぐ・要対処 2）
- 総件数は (file, line) の重複を除いた数。語ごとの件数は重複を除かない
- 出力行の本文を切り詰めない

## テスト方針と検証コマンド

`scripts/prose-sweep.test.mjs`（vitest・入力注入の純関数）:

- `parseArgs`: 語なし → 使用法エラー／空語 → エラー／未知フラグ → エラー／`--plan` と語の混在／`--` 以降の `-x` は語
- `parseGrepZ`: 本文に `:` を含む行・日本語パス
- `surfaceOf`: 5 面それぞれの代表パス（`AGENTS.md` / `.claude/skills/x/SKILL.md` / `docs/x.md` / `docs/adr/ADR-x.md` / `src-tauri/src/x.rs`）と、既定分岐へ落ちるもの（`docs/design/x.md`・`PERFORMANCE.md`・`package.json`）が出力から消えないこと
- `buildReport`: ①未掲載ファイルに `★未掲載` が付き件数が合う ②語の 1 つが 0 件でも印字される ③コード行だけのヒットが落ちない ④2 語が同じ行に当たると総件数は 1 ⑤plan に `snotra-core/CLAUDE.md` だけがあるとき、ルート `CLAUDE.md` に `★未掲載` が付く
- grep 失敗の扱い: 注入した runner が status 2・status `null`・`error` を返したら throw し、0 件の報告を作らない

検証: `npm test`（scripts）／`npm run governance:check`（F）／`npm run prose:sweep` の実走（自己適用）。変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う（候補: grep 失敗を一致なし扱いにする・`status null` を一致なし扱いにする・0 件の語を印字から落とす・`--plan` 照合を素の `includes` に戻す・`-I` を外す）。

## 自己適用（項目 7 を本 PR へ）

道具ができた時点で `npm run prose:sweep -- --plan workspace/plan.md "どちらか一方" "計画準拠" "項目 7" "セルフレビュー" "Step 2 または" "plan-review --deep"` を打つ。手の `git grep` では生きた面のヒットが plan-review/SKILL.md と start-issue/SKILL.md に限られた（2026-10-25）。名指しの検算対象: `.claude/skills/start-issue/SKILL.md` が出力に現れ、本計画に掲載済みとして印されること。あわせて、空白入りの語（`"Step 2 または"`）と日本語の語が `npm run` 経由で argv へ 1 語のまま届くことを、出力の語ごとの件数欄で確かめる（plan-review 未検証欄）。★未掲載が出たら 1 件ずつ裁き、件数をセルフレビュー節へ書く。

## SPEC.md・関連文書

- `SPEC.md`: 不要（プロダクト挙動に触れない）
- `docs/build-commands.md`: 要（上表）
- ADR: 不要（却下した案は生じていない。試行の結果で Step 2 を畳むなら、そのとき書く）

## 作業項目

### Phase 1 — 道具

- [x] `scripts/prose-sweep.test.mjs` を書き、落ちることを確認する（スタブに当てて 30/32 落ち。通った 2 件はスタブの固定値が期待と一致したもの）
- [x] `scripts/prose-sweep.mjs` を実装して通す（32 passed）
- [x] `package.json` に `prose:sweep` を足す
- [x] `docs/build-commands.md` にコマンドと bullet を足す
- [x] 自己適用を打ち、★未掲載を裁いてセルフレビュー節へ件数を書く
- [x] （計画外の発見）PowerShell の `npm.ps1` が最初の `--` を食い `--plan` が黙って消えることを実測した。道具の内側からは検知できない（`npm_config_plan` も渡らない・実測）ので、`build-commands.md` と SKILL.md 項目 7 に「Bash で打つ／PowerShell では node を直接」を書く

### Phase 2 — スキル

- [x] `.claude/skills/plan-review/SKILL.md` の `allowed-tools`・項目 7・リスク判定・Step 2 冒頭を書き換える
- [x] `.claude/skills/start-issue/SKILL.md` 5b の雛形と注記を足す（他を序数「項目 7」で指していたのを正準形の見出し参照へ直した・`governance-docs.md`）
- [x] `npm test` と `npm run governance:check` を通す（704 passed／全検査 passed）

### Step 3 — レビュー巡 1（`58765a39`・`workspace/review-58765a39.md`・道具 `workspace/verify-58765a39.json` green）

- [x] M1: ★未掲載の意味を「計画が一度も名指していない」と doc と SKILL に明記（照合範囲は絞らない——「触らない」と名指したのも裁いた記録である）
- [x] L1: 見出しの母集団を `GREP_FLAGS` / `GREP_PATHSPEC` から作る
- [x] L2: doc の測定値「71 → 2」を帰結へ
- [x] L3: `\` 区切りもパス境界に（テスト追加・修正前で赤を確認）
- [x] L4: 行番号欄が空なら throw（テスト追加・修正前で赤を確認）
- [x] L6: 0 件の原因の断定をやめ「言い換えて打ち直す」と行動を書く（道具と SKILL）
- [x] W1: PowerShell の件を全称から「#1300 の実測では」へ弱める
- 受容: L5（`.delegate/` が委譲先の木でだけ母集団に入る。主の木には存在しない）

### Step 3 — レビュー巡 2（`c7ca8a55`・`workspace/review-c7ca8a55.md`・道具 green）

- [x] 前巡の M1・L1〜L4・L6・W1 はレビュアー自身の検算で解消、L5 の受容に同意
- [x] N1: build-commands の「Git Bash では起きない」を過去形の実測へ
- [x] N2: 末尾 lookahead の `\` を赤にするテストを追加（変異 g2 を複製へ当て、diff で変異の適用を確かめてから assert の赤を確認。初回は sed が当たらず変異 0 件のまま緑だったので証拠に数えなかった）

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-10-25 / 問い: "`workspace/plan.md` に注釈を書き込むか、承認のお言葉をいただけますか。" / 回答: "承認します"

## plan-review 結果

- リスク: 高（skills を変更）
- レビュー方式: 計画準拠レビュー1体（`workspace/plan-review-step2-1300.md`）。選んだ理由: issue に書かれていない人間の裁定（`Refs #1300`・`--deep` 不変）が計画の前提にあり、本 PR が導入する Step 2 の条件そのものに当たる（試行の 0 件目のデータ）
- エージェント数: 1

### 要対処（5 件・すべて計画へ反映）
1. `-I` 欠落でバイナリの `Binary file X matches` が `-z` 解析へ混ざる — 再照合: `5b` で `src-tauri/icons/*.png` 等 3 件が出て、`-I` 付きで 0 件（実測）
2. `--plan` の部分文字列一致でルート `CLAUDE.md` が偽の掲載になる — 再照合: `"snotra-core/CLAUDE.md".includes("CLAUDE.md")` は真
3. `spawnSync` の `error` / `status null` が失敗として定義されていない — 再照合: 計画の不変条件が exit 1 と「それ以外の非ゼロ」しか書いていなかった（不正 pathspec が 128 になるかは私の打ち方では再現せず、結論はそれに依存しない）
4. cwd がルートでないと母集団が縮む — 再照合: pathspec `.` は cwd 相対
5. 空語・`-` 始まりの語が未定義 — 再照合: `-e ""` は全行一致

### 軽微
- 生きた面で偽になるのは plan-review :40・:42、start-issue :122-124・:134 に限られた（計画の一覧と一致）
- 5b の試行欄の「方式」は Step 3 の方式欄と重なる。採らない: Step 3 は会話への報告で、`plan.md` に残る面は 5b だけ（PR head から集計する経路）
- Step 2b 本文の「Step 2 と同じく」は偽にならない。触らない

### 未検証
- `npm run` 経由の argv（空白・日本語）— 自己適用で測る

### 判断
- 実装着手: 可

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（計画準拠）
- エージェント数: 1（ほか 3b の敵対的調査 1 体）
- 要対処: 5 件・すべて不変条件とテスト方針へ反映
- 未検証: `npm run` 経由の argv — Phase 1 の自己適用で測る
- 散文の全件列挙: 語 6 / 46 件 / ★未掲載 17（計画へ足した 0）— 17 件は凍結された歴史 12・無関係な「どちらか一方」4（`symmetric-check/SKILL.md`・`architecture.md`・`index_tree.rs`・`egui_shell/mod.rs`）・`docs/design/` の「項目 7」1（別文脈）。名指しの検算対象 `start-issue/SKILL.md` は掲載済みで出た。空白入り・日本語の語は Bash・PowerShell とも 1 語で届いた（件数欄で確認）
