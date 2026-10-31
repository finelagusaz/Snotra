# 計画 — #1312 `/merge-pr` の確認と事後検証を道具へ移す（#1304 を吸収）

## 目的

`/merge-pr` が散文の手順で守っている落とし穴を、1 本の道具 `scripts/merge-pr.mjs`（`npm run merge:pr`）の構造へ移す。あわせて #1304 の照合（道具の要約の sha から PR の head までに `workspace/` の外の変更が無いこと）をマージ前に行う。ユーザー裁定は issue #1312 のコメント（2026-10-31）にある。

## 受け入れ条件

1. 道具の使い方は `npm run merge:pr -- <PR> --closes <N[,N...]|none> --body-file <path>` の 1 通りだけである。件名（`--subject`）・`--auto`・`--delete-branch` を渡す口は持たない
2. **マージ前**に次をすべて確かめ、1 つでも満たさなければマージせず exit 1（理由と復帰手順を出す）。順序は書き込みの前に読みを全部終える形にする
   - a. PR が `OPEN` で、`mergeStateStatus` が `CLEAN` である（`CLEAN` 以外ではマージを試みない。`gh pr merge --help` は merge queue の文脈で「必須チェック未通過なら auto-merge を有効にする」と書いており、`--auto` を使わない規範を道具が黙って破る経路を前提条件で断つ）
   - b. `closingIssuesReferences`（`owner/repo#N` で正規化）が `--closes` の宣言と**完全一致**する。宣言に無いものが在れば「本文を直して再実行」を出す。本文の編集は道具が行わない（裁定）
   - c. PR タイトルが、closing keyword の型名（`fix` / `close` / `resolve` 系 9 形）の直後に `#N` を置いていない
   - d. `--body-file` の本文に closing keyword の 9 形（コロンの有無・`owner/repo#N`・issue URL を含む）が無い
   - e. head ブランチを base にする open PR が無い（`delete_branch_on_merge: true` ゆえフラグに依らず検査する）
   - f. 道具の要約の照合を、hook と共有する `verifiedSummaryState`（下表）に `branch: headRefName`・`target: headRefOid` で当てる。head の commit が手元に無ければ、道具の側で `git fetch origin refs/pull/<N>/head` してから当てる（fetch は共有関数の外。hook の 4 秒の git 予算に載せない）。要約が無いときは止め、迂回口は作らない。出力には復帰手順（`gh pr checkout <N>` → `npm run verify:tree -- run --sha HEAD ...` → 再実行）を書く。Dependabot の PR もこの手順で復帰でき、道具を経ずに Web UI でマージすれば残余になる。GitHub の "Update branch" で main を取り込んだ head も、取り込みが `workspace/` の外を動かしていれば止まる（fail-closed の向き。復帰は同じ手順）
3. マージは `gh pr merge <N> --squash --body-file <path> --match-head-commit <headRefOid>` の 1 回だけ。gh は `shell: false` で呼ぶ
4. マージを試みたら、gh の終了コードに依らず `gh pr view --json state,mergedAt` で成否を確定する。`MERGED` でなければ、gh pr merge が非 0 なら exit 1、0 なら反映の遅れでありうるので exit 3（委譲レビュー W-2 で改めた）
5. **マージ後**の検証。ここで落ちたら exit 3 を返し、出力の最初の行で「マージ済み」と名乗る（成否そのものを確かめられないときは「成否を確かめられない」と名乗る・委譲レビュー M-3）
   - (1) 宣言した issue を番号ごとに `gh issue view --json state` で引き、すべて `CLOSED` か
   - (3) `mergedAt` が ISO 8601（`^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$`）であることを検めてから、`gh issue list --state closed --search "closed:>=<mergedAt>" -L <上限> --json number` を打つ。件数が上限に達したら exit 3（検算不能）。宣言外の番号があれば報告して exit 3。並行セッションの可能性を併記し、reopen はしない
   - 検算: `gh pr list --state merged --search "merged:>=<mergedAt>"` に当の PR が現れること（時刻の形が効いている証拠）。現れなければ exit 3。宣言した issue が (3) に現れなければ短く再試行し、それでも現れなければ「検索が未反映」として exit 3。**0 件を「該当なし」として 0 で返す経路を持たない**
   - (2)「残すと決めた issue が OPEN か」は、b の完全一致（予防）と (3) の宣言外の検出（事後）が覆う
6. 終了コード: 0 = マージ済みで事後検証もすべて緑・1 = マージ前に止めた（マージしていない）・2 = 走らなかった（使用法・環境・linked worktree・マージ前の gh/git の失敗。マージしていない）・3 = **マージを試みたあと**の要確認（事後検証が赤・検算不能・成否を確かめられない。reopen などの判断が要る）。0〜2 は既存の道具と同じ意味で、3 を足すのは「マージしたか」を終了コードだけで読めるようにするため
7. root は `scripts/verify-tree.mjs` と同じ導き方で決める（git-dir と common-dir が一致しなければ exit 2）。linked worktree から打たれた場合に、hook の `findUp` 型の取り違えを起こさない
8. hook（`pre-bash.mjs`）の挙動は変えない。照合を切り出して共有したあとも、既存の hook テストは件数・結果とも変わらない（新しいテストは件数に別勘定で足す）
9. `/merge-pr` は user 起動専用のままにする。道具を経ない `gh pr merge`（Web UI・手打ち）は `--no-verify` と同格の人間専用の迂回として、残余に明記する

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `.claude/hooks/pre-bash.mjs` | `readVerifyState` から、要約の読み込み → `judgeSummary` → 2 端点の diff → `workspace/` の外の抽出を、`export function verifiedSummaryState({ root, branch, target, summaryPathFor })` へ切り出す。`readVerifyState` は root とブランチを導いたうえで `target: "HEAD"` を渡して呼ぶだけにする。挙動は変えない。JSDoc に消費者が 2 つ（hook と `scripts/merge-pr.mjs`）あることを書く。拒否文言（`VERIFY_REMEDY`）はそのままにし、道具はマージ時の復帰手順を自分で足す |
| `.claude/hooks/pre-bash.test.mjs` | `verifiedSummaryState` に `target` として HEAD 以外の sha を渡すケースを足す（要約の sha と target の間に `workspace/` の外の変更がある・無い・要約の branch が違う）。既存ケースは変更しない |
| `scripts/verify-tree.mjs` | `mainContext` から root の導出（linked worktree の拒否まで）を `export function mainRoot(cwd)` へ切り出し、`mainContext` はそれを呼ぶ。挙動は不変 |
| `scripts/merge-pr.mjs`（新規） | 道具本体。判定は入力注入の純関数（実装での名前: `parseArgs`・`findClosingKeywords`・`titleClosesIssue`・`normalizeRef`・`diffClosing`・`isIsoTimestamp`）と、`gh` / `git` / 要約の照合を注入する `run(argv, cwd, deps)`（計画時の仮名から改めた・委譲レビュー L-3） |
| `scripts/merge-pr.test.mjs`（新規） | 純関数のフィクスチャと、偽の実行器による経路のテスト（下の「テスト方針」） |
| `package.json` | `"merge:pr": "node scripts/merge-pr.mjs"` |
| `.claude/skills/merge-pr/SKILL.md` | 手順を「道具を走らせて出力を読む」へ縮める。起動形は Git Bash では `npm run merge:pr -- ...`、PowerShell では `node scripts/merge-pr.mjs ...`（`npm.ps1` が最初の `--` を食う・`docs/build-commands.md` の `prose:sweep` の注記と同じ。実装時に実測して決める）。終了コードごとに次の一手を書く。exit 1 で人へ返されたときの本文の直し方（9 形の注意）・件名は PR タイトルで直すこと・タイトルの型名の直後に番号を置かない注意・reopen・`--auto` を使わないこと・道具を経ないマージは残余であること、は残す。「唯一の防御」の宣言は偽になるので消す。「`--delete-branch` を付ける前に」の節は、道具がフラグに依らず検査することを書いて縮める。経緯（`--subject` の実測・`ConvertFrom-Json` の実測）は ADR の追記へ移す。frontmatter の `description` と `allowed-tools`（`Bash(npm *)` / `Bash(node *)` を足す）も直す |
| `docs/adr/ADR-squash-merge-issue-autoclose.md` | 本文は書き換えず、追記節を足す（issue の指定どおり経緯をここへ寄せる・新しい ADR は起こさない）。追記の中身: 決定 2 の「手順で守る」を道具へ移したこと、却下した案（hook で `gh pr merge` を守る〔既存の却下理由 3 点を継承〕・道具が本文を編集する〔裁定〕・要約が無いときの迂回フラグ・宣言集合を持たず事後検索だけで守る〔予防にならない〕・照合の相手を手元の HEAD にする〔マージされるのは PR の head〕）、SKILL から移す実測 2 件 |
| `docs/hooks.md` | 「要約ゲート」の行へ、同じ照合（`verifiedSummaryState`）を `merge:pr` が PR の head へも当てることを 1 句足す。`ADR-pr-gate-verified-sha` の受容した残余「PR を作ったあとに足したコミットは射程外」は凍結層ゆえ書き換えず、生きた層のこの 1 句で現状を示す |
| ルート `CLAUDE.md`（規範文書・セーフティネット） | 3 か所。「フック」の「`merge` / `close` の残余は `/merge-pr` の手順が持つ」を、道具（`npm run merge:pr`）が持つ形にし、道具を経ないマージは人間専用の迂回であることを書く。「利用できるスキル」表の `/merge-pr` 行は「本文編集」を外し、道具を走らせる形の要約にする。「Git/GitHub 運用」の `/merge-pr` 行は道具を名指す。**人間レビューの承認でこの書き換えにも合意をいただく** |
| `docs/build-commands.md` | 「Windows/macOS/Linux で実行可能」のフェンスへ `npm run merge:pr` の 1 行（使い方の正本は `scripts/merge-pr.mjs` を指す） |

## 実装順序

1. 切り出し 2 件（hook・verify-tree）。挙動不変を既存テストの件数と結果で測る
2. 道具の純関数とそのテスト（Red → Green）
3. 実行器を注入した `run` と経路テスト
4. `package.json`・`docs/build-commands.md`
5. SKILL・ADR の追記・`docs/hooks.md`・ルート `CLAUDE.md`

## 不変条件と異常系

- **書き込み（マージ）の前に、読みの検査をすべて終える。** 検査の途中で gh が失敗したら、マージせず exit 2
- **マージ後の失敗は exit 3 だけで返す。** 0〜2 が出たら、マージはしていない（0 を除く）。マージ後に検証の gh が失敗したときも 3 であって、2 ではない
- ガバナンス検査 G-stale-identifiers: SKILL・`docs/` のバッククォートに単独で書く camelCase（`headRefOid`・`mergedAt`・`mergeStateStatus`・`verifiedSummaryState`）は、テストでないソースの非コメント本文に在る必要がある。道具の中で文字列リテラル・識別子として使うので満たすが、`npm run governance:check` で確かめる
- `gh` / `git` は `shell: false`。`>` を含む検索語を壊さない
- `--match-head-commit` により、確認からマージまでのあいだに head が動けば GitHub が拒否する。本文の変化は凍結できないので残余として SKILL に書く（現行の「手順 1 の一覧が閉じる issue のすべてになる条件」と同じ）
- 要約の sha は `judgeSummary` が 40 桁の 16 進に限る。`headRefOid` も同じ形を検めてから git へ渡す

## テスト方針と検証コマンド

- 純関数: closing keyword の 9 形 × コロンの有無 × `#N` / `owner/repo#N` / URL、`chore:` / `refactor:` のタイトルは通ること、宣言と一覧の差（過不足・他リポジトリ）、ISO と非 ISO（`08/10/2026 10:26:23`・空文字）
- 経路（偽の実行器）
  - 各マージ前検査で落ちたときに `gh pr merge` が**呼ばれない**こと（`mergeStateStatus` が `CLEAN` でない場合を含む）
  - gh の非 0 終了でも `state` で成否を確定すること
  - `--match-head-commit` の不一致で拒まれたら exit 1 になること（gh の stderr の実物は実装時に一度観測してからフィクスチャにする）
  - 事後の宣言外 close・検算の失敗・上限到達・検索が未反映で、それぞれ exit 3 になること
  - 要約の branch が headRefName と違うとき（`/` と `-` が潰れて同じファイル名になる場合）に止まること
- 切り出しの挙動不変: `.claude/hooks/pre-bash.test.mjs` と `scripts/verify-tree.test.mjs` の件数と結果を、切り出しの前後で比べる（新しいテストは別に数える）。fail-closed のカナリア（`pre-bash.test.mjs` の動的 import 配線）の形も変わらないこと
- 変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う（主エージェントの作業項目にしない）
- 検証カテゴリ: E（`.claude/hooks/**`・`scripts/**`）と F（ガバナンス文書）。A〜D は該当しない

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（製品の挙動に触れない）
- 上の表の文書: 要

## フェーズ 1 — 共有のための切り出し

- [x] `pre-bash.mjs` の `verifiedSummaryState` を切り出し、既存の hook テストが前後で同件数・全緑であることを確かめ、HEAD 以外の target のテストを足す — 216 → 216（既存）+ 5（新規）= 221、全緑。target を HEAD へ戻す変異で新規 2 件が赤（Red 確認）
- [x] `verify-tree.mjs` の `mainRoot` を切り出し、verify-tree テストが前後で同件数・全緑であることを確かめる — 32 → 32、全緑

## フェーズ 2 — 道具

- [x] 純関数とテストを書く（Red を確かめてから Green） — スタブで 44 件赤 → 実装で 56 件緑
- [x] `run` と経路テストを書く — no-op の run で 28 件赤・3 件が終了コードしか見ておらず通ったので呼び出しの assert を足した → 87 件緑。実物の gh でマージ済み #1311 に当てて読みの経路を確認（exit 1 で止まり書き込みに届かない）
- [x] `package.json` へ `merge:pr` を登録し、`docs/build-commands.md` へ 1 行足す

## フェーズ 3 — 文書

- [x] `/merge-pr` の SKILL を道具中心へ書き直す
- [x] `ADR-squash-merge-issue-autoclose` へ追記節を足す
- [x] `docs/hooks.md` の「要約ゲート」へ 1 句足す
- [x] ルート `CLAUDE.md` の 3 か所を直す

## 未確定（実装前に潰す）

- [x] `prose:sweep` で、変更で偽になる散文の★未掲載を裁く — 語 8（`唯一の防御` `/merge-pr` `readVerifyState` `mainContext` `--delete-branch` `closingIssuesReferences` `手順が持つ` `射程外`）/ 総件数 166 行 / ★未掲載 48。裁定: `docs/hooks.md` の「要約ゲート」は判定の正本を `readVerifyState` と名指しており、切り出し後も真だが、共有の事実を書く先として計画へ足した（+1）。`.claude/skills/retrospective/SKILL.md:69` は #488 の手順を `/merge-pr` へ移した歴史の記述で、偽にならない。残りの 46 は `docs/superpowers/`・`docs/adr/`・`docs/design/` の凍結された歴史と、無関係な文脈の「射程外」（`PERFORMANCE.md`・`G-module-linkage.mjs` など）で、偽にならない

## plan-review 結果

- リスク: 高（hook・skills・規範文書を変更する）
- レビュー方式: 独立導出 1 体（Step 2b。ユーザー裁定 2 点は issue コメントへ書いてから渡したので、Step 2 を選ぶ条件に当たらない）
- エージェント数: 1（全文は `workspace/plan-review-1312-derive.md`）

### 要対処（計画を修正した）
- 導出 ∖ plan: hook のテストに HEAD 以外の target を足す — 変更ファイルに `pre-bash.test.mjs` を追加した
- 導出 ∖ plan: 共有の単位は差分だけでなく「要約の読み込み → `judgeSummary` → diff」まで（`{root, branch, target}`）— `verifiedSummaryState` へ改めた。根拠は `pre-bash.mjs` の `readVerifyState`。固定されているのはブランチと `HEAD` の 2 点だけである
- 導出 ∖ plan: SKILL の frontmatter（`description`・`allowed-tools`）— 表へ足した
- 導出 ∖ plan: PowerShell から `npm run` を通すと最初の `--` が食われる — SKILL の起動形を 2 通り書き、実装時に実測する
- ⚠ 導出: 必須チェックが未通過だと gh が auto-merge を有効にしうる — `gh pr merge --help` を自分で読んだ。文脈は merge queue だが、`mergeStateStatus == CLEAN` の前提条件で断つ（受け入れ条件 2a）
- 判断の不一致: 終了コード — 導出の D11（マージ後は 3）を採った。「マージしたか」を終了コードだけで読める
- 判断の不一致: ADR — 導出の D8 を採った。issue 本文が「経緯は ADR-squash-merge-issue-autoclose へ寄せる」と指定していたので、新しい ADR は起こさず追記にする。`ADR-pr-gate-verified-sha` は凍結層なので書き換えず、生きた層の `docs/hooks.md` に現状を書く
- 判断の不一致: ルート `CLAUDE.md` の書き換えは、裁定 2 点の範囲を越える規範変更である — 人間レビューの承認で明示的に合意をいただく

### 軽微（採らなかった・理由つき）
- plan ∖ 導出: `scripts/verify-tree.mjs` の `mainRoot` の切り出し — 導出は変更不要とした。それでも維持する。敵対的調査が、hook の `findUp` 型と `mainContext` 型で root の求め方が食い違うことを実測した。道具が `mainContext` と同じ前提を名乗るには、導出を共有するのが写しを作らない形である
- 導出 D4（事後検証 (2) の母集団として `--keep` と本文の `#N` を足す）— 採らない。(2) は、マージ前の完全一致と、マージ後の宣言外 close の検出で覆える。口を増やさない
- 導出 D10（マージ前に、直近に閉じた issue を対照として控える）— 採らない。時刻の形の検算は自 PR の `merged:>=` で足り、issue 側の索引遅延は番号ごとの状態確認と再試行で覆う
- 導出 D12（道具は `--delete-branch` を受けず、SKILL の節を残す）— 一致。道具は受けず、節は縮めて残す

### 未検証
- `--match-head-commit` で拒まれたときの gh の stderr と終了コード（実装時に一度観測してからフィクスチャにする）
- マージ直後の検索インデックスの遅延（再試行し、それでも現れなければ exit 3 で倒す）
- （実装時に判明）PowerShell から `npm run merge:pr -- ...` が最初の `--` を食うかの実測は、マージを伴う道具の実行として auto mode の分類器に拒否された。SKILL は `prose:sweep` の実測（同じ `npm.ps1` の機序）に倣い、PowerShell では `node scripts/merge-pr.mjs` を直接打つと書いた

### 判断
- 実装着手: 可（人間レビュー承認済み）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに敵対的調査 1 体）
- 要対処: 8 件を計画へ反映（上の「要対処」）
- 未検証: 上の 2 点。いずれも実装時に観測する
- 散文の全件列挙: 語 8 / 総件数 166 行 / ★未掲載 48（計画へ足した 1）
- 試行（#1300・高リスクのみ）: 方式 Step 2b / 設計変更 3（共有の単位・終了コード 3・ADR の置き場） / 主の見落とし 3（`pre-bash.test.mjs`・frontmatter・`mergeStateStatus`）

## 人間レビュー

- [x] 承認済み — 2026-10-31 / 問い: "**この計画を承認いただけますか？** 注釈は `workspace/plan.md` へ直接書いていただいても構いません。" / 回答: "承認するわ、CLAUDE.md の書き換えも含めて進めてちょうだい"
