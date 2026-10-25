# plan-review Step 2 (#1300) 網羅性と緑推移

対象: workspace/plan.md。前提の裁定（Refs #1300・--deep 不触）は覆さない。grep は切り詰めず、workspace/ を除外して実走（docs/adr・docs/superpowers・docs/design は凍結扱い）。

## 要対処

1. **`git grep` が実バイナリに当たると `-z` 解析が壊れる（母集団の取り違え）。** `git grep --untracked -n -z -F -e 5b -- .` は `src-tauri/icons/*.png` / `icon.icns` で `Binary file src-tauri/icons/icon.png matches` という **NUL も行番号も無い 1 行**を返す（実測）。計画の母集団コマンド（plan.md:39）に `-I` が無く、`parseGrepZ` の入力モデル（path NUL line NUL text）に合わない。レコードを捨てれば「0 件」へ、誤って拾えば偽ファイル名になる。対処: コマンドへ `-I` を足す（`-I` 付きで 独立導出 は 70 行・binary 行 0 を実測）。`parseGrepZ` のテストに「`Binary file … matches` 行は母集団に入らない（または throw）」を足す。また `Cargo.lock` などは「5b」のような短い語で大量ヒットするので、`-I` でもロック・生成物のノイズは残る（落ちはしないが面分類で「コード/その他」に混ぜない）。
2. **`--plan` の「パス文字列が plan に現れるか」を素の部分文字列一致にすると、掲載済みと偽って緑になる。** ルート直下の `CLAUDE.md` / `AGENTS.md` / `SPEC.md` は `snotra-core/CLAUDE.md` や `src-tauri/CLAUDE.md` の部分文字列である。plan.md:26 など plan 内に `snotra-core/CLAUDE.md` 系の記述が 1 つでもあれば、ルート `CLAUDE.md` が ★未掲載にならない（`docs/x.md` も `docs/x.md.bak` に当たる）。この道具の存在理由（掲載漏れの検出）が静かに空になる経路。対処: パスの直前・直後がパス文字（`[A-Za-z0-9_./-]`）でない位置の一致だけを掲載とする。`buildReport` のテストへ「plan に `snotra-core/CLAUDE.md` だけがあるとき、ルート `CLAUDE.md` に ★未掲載が付く」を足し、変異（照合を `includes` へ戻す）で落ちることを確かめる。
3. **`git grep` の失敗判定が exit code だけだと、spawn 側の失敗が「0 件」になりうる。** 計画 plan.md:11,38 は「exit ≥ 2 → 失敗」と書くが、`spawnSync` では (a) maxBuffer 超過（既定 1MB。短い語で Cargo.lock 等が当たれば届く）は `status === null` + `r.error`、(b) シグナル終了も `status === null`、(c) git 不在は `r.error`（ENOENT）である。`status !== 0 && status !== 1` を失敗とする書き方でも `null` は両方に当たらず、`status === 1` 比較だけを見ると見逃す書き方になりやすい。対処: 「status が 0 か 1 以外（null 含む）、または `r.error` あり → throw・exit 1」と plan.md の不変条件に書き、`maxBuffer` を明示し、テスト方針の「status 2」に加え `status:null` / `error` の注入ケースを足す。実測: 不正 pathspec は exit 128、`.git` の外は exit 128、一致なしは exit 1（いずれも想定どおり区別できる）。
4. **cwd がリポジトリルートでないと母集団が静かに縮む。** `git grep … -- . ':!workspace'` は cwd 相対。`src-tauri` を cwd にすると 独立導出 は 71→2 行になった（実測）。`npm run` は package.json のあるディレクトリで動くが、`node scripts/prose-sweep.mjs` の直打ち・別 cwd からの呼び出しで「ヒット少数」が正常に見える。対処: `git rev-parse --show-toplevel` の結果（失敗なら exit 1）または `import.meta.url` からルートを導いて `cwd` に固定する。plan.md:39 の不変条件へ一行足す。
5. **空文字の語・`-` 始まりの語の扱いが未定義。** `git grep -F -e ""` は全行に一致する（exit 0 を確認、全件が当たる）。`parseArgs` が空語を弾かないと件数が爆発して「全件出した」ことになる。`--deep` のような語を検索したい場合は `--` 終端が要る（未知フラグ → exit 2 の規則と衝突）。対処: テスト方針の `parseArgs` に「空語 → エラー」「`--` 以降は語として扱う」を足す。

## 軽微

1. **（偽になる散文・漏れ）plan の変更ファイル一覧の網羅性は概ね妥当。** 実走した grep（`どちらか一方` / `Step 2 または` / `計画準拠` / `項目 7` / `独立レビュー1体` / `plan-review --deep` / `Step 2b` / `plan-review` 全件）で、本変更により偽・曖昧になる生きた面は plan-review/SKILL.md（:40 `どちらか一方`、:42 Step 2 冒頭）と start-issue/SKILL.md（:122-124 の `/plan-review`・`--deep` 説明、:134 の雛形）に限られた。`項目 7` の語は生きた面にヒット 0 件（SKILL.md 内は「7.」の番号形で、道具化は Step 1 の書き換えで足りる）。
2. ⚠️ **start-issue/SKILL.md:122-124 の「それ以外は `/plan-review` を使う」** は、既定が 2b になったあとも偽ではないが、`--deep` が何を足すかの読み手の理解（2b と --deep の差）が曖昧になりうる。`--deep` は触らない裁定なので、書き換え不要か 1 行の注記かは判断に委ねる。
3. ⚠️ **start-issue/SKILL.md:175**（引き渡し記録「リスク判定、plan-review方式、起動したエージェント数」）と plan-review/SKILL.md:79（Step 3 の方式欄）に試行（#1300）の「設計変更 / 主の見落とし」を載せる先が無い。5b 雛形の新欄に載せる設計なので矛盾はしないが、5b 欄と Step 3 の方式欄が二重に方式を持つ（写しの増加）。plan が Step 3 は触らないと明記済みなので、5b の欄は「Step 3 の方式欄を参照」とすると写しが減る。
4. **plan-review/SKILL.md の Step 2b は「Step 2 と同じく…」で始まる（:61）。** Step 2 が条件付きになっても偽にはならないが、既定側の節が従属的な書き方になる。見出しは変えない方針なので本文 1 語の整えで足りる。
5. **docs/adr/ADR-risk-tiered-plan-review.md ほか**（`独立導出` のヒットの大半は docs/adr・docs/superpowers・docs/design 配下の 60 行超）は凍結された歴史。plan の「触らない」と一致。`ADR-plan-document-lens-not-permanent.md:22` は「高リスクは計画準拠レビューか独立導出の 1 体」と現行を描写するが、日付つきの歴史記述なので書き換え不要。
6. **生きた面で Step 2b を名指す参照**（`.claude/rules/governance-docs.md:18`、`AGENTS.md:64`、`docs/development-principles.md:228`、`src-tauri/src/egui_shell/search_state.rs:977`、`scripts/governance/checks/G-heading-refs.test.mjs:45-46`）は見出し「Step 2b — 独立導出による網羅性レビュー」を指す。見出し不変の方針により着地は保たれる。
7. `surfaceOf` のテスト代表パスは 5 面だが、実在する面でテストが無いもの（`docs/design/`・`docs/superpowers/`＝凍結／`SPEC.md`・`PERFORMANCE.md`＝生きた文書／`package.json`・`scripts/*.mjs`・`.github/`・`Cargo.lock`＝コード）が既定分岐に落ちる。どれも「面の既定値に落ちて出力から消えない」ことを 1 ケースで固定すると安全。
8. `docs/build-commands.md`・`package.json` の書き込み先（race:boundaries と同じ節・同じ並び）は上記 grep（`race:boundaries` の全出現: package.json:13、build-commands.md:185/188、race-check/SKILL.md:20）に照らして妥当。他に npm script 名を列挙する生きた面は見つからなかった。`.claude/settings.json` に `npm run` の許可リストは無い（grep 0 件）。

## 未検証

- 道具 `scripts/prose-sweep.mjs` は未実装のため、実装後の挙動（`parseGrepZ` の実入力・exit code の配線）は測っていない。上の git grep 挙動は `git` を直接 spawn する形で実測した（`:!workspace` は shell なしの `spawnSync('git', [...])` でも workspace/ 2 ファイルを正しく除外、npm script 経由の shell 展開は道具が引数配列で git を呼ぶ限り無関係）。
- ⚠️ **PowerShell / cmd 経由の `npm run prose:sweep -- <語>` で、日本語・空白入り・`"` を含む語が argv に損なわず届くか**は未測。道具に直接 `node scripts/prose-sweep.mjs "plan-review --deep"` を渡す形では届く想定だが、cmd.exe 経由（npm の既定 shell）のクォート挙動は実走していない。計画の自己適用（plan.md:57）で `"項目 7"` `"Step 2 または"` のような空白入りの語を Windows の npm 経由で打つ段で確かめること。
- 変異注入（plan.md:53）の候補に上記 要対処 1〜3 の変異（`-I` を外す・`includes` 照合・`status null` を一致なし扱い）を足すことは未検討。
