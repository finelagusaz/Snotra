# plan — #1140 節の中身が変わったら、依存している参照を編集直後に報告する

## 目的

見出しの名前が変わらないまま本文が入れ替わったとき、その節に依存している参照を**編集した本人へその場で**知らせる。合否は出さない——判定は人が持つ（`ADR-retire-area-budget` が面積で採ったのと同型）。

## 受け入れ条件

1. `.md` の節を書き換えたとき、PostToolUse hook が「この節に依存する箇所が N 件」を WARN として出す
2. **純粋な追記（旧側 0 行の hunk）では出さない**——既存の参照を偽にしないため（実測: 発火が 41〜46% から 24〜25% へ半減する）
3. 依存が 0 件のときは何も出さない。かつ**その沈黙が「依存は無い」と読まれない**よう、文書側で不在の意味を明示する
4. 判定ロジックは `scripts/governance/` 側に置き、hook はそれを呼ぶだけにする（`isSourceFileWrite` の doc が示す「判定の SSOT を hook に再実装しない」に従う）
5. 既存の PostToolUse の契約（沈黙 = 合格・失敗時のみ出力・`selectChecks` が割り当ての SSOT）を壊さない

## 設計判断（研究の実測にもとづく）

| 論点 | 決定 | 根拠 |
|---|---|---|
| 判定の置き場所 | `scripts/governance/dependents.mjs` を新設し、**hook が subprocess で呼ぶ**（`runCheck` と同じ経路） | **独立レビューで import 案が棄却された。** 静的 import は `try { main() } catch`（`post-edit.mjs:476-484`）の**外**で走るため、解決に失敗すると JSON エンベロープを出さずにプロセスごと落ち、**`.rs` の fmt / clippy / test まで含めて全編集で hook が沈黙する**（レビュア側が使い捨て repo で再現）。さらに相対 import は**importer の所在**＝`${CLAUDE_PROJECT_DIR}` 基準で解決するので、`resolveRoot` が求める「編集されたファイルのツリー」とずれる（この非対称は `docs/hooks.md:103` が既に明記していた）。subprocess なら `root` 基準で起動でき、失敗も既存の経路で扱える。**代償は node 起動 109〜130 ms**（`.md` 編集 1 回あたり合計 250 ms 前後） |
| 基準（何と比べるか） | `git diff HEAD -- <file>` の hunk | `Write` は `old_string` を持たないので payload からは差分を作れない |
| 純追記の扱い | **外す** | 発火 41〜46% → 24〜25%。消えるのは小さな発火ばかり（件数は 26% しか減らない）。**この 24〜25% は実装後に 55% へ訂正された**（下記） |
| 出力量 | 件数 + 上位 3 件 + 全件を見るコマンド | 純追記除外時の 1 回あたり中央 9〜10 件・最大 21〜31 件 |
| 発火の母集団 | `.md` の編集のみ | `resolveRefTarget` は必ず `.md` へ解決する（敵対レビューで壊せなかった） |

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/governance/dependents.mjs`（新規） | `buildDependentIndex(snapshot)` / `sectionsOf(snapshot, path)` / `dependentsOfChangedLines(snapshot, path, hunks)` |
| `scripts/governance/dependents.test.mjs`（新規） | 上記のユニットテスト（合成スナップショット） |
| `.claude/hooks/post-edit.mjs` | `changedHunks(root, rel)`（`git diff HEAD` の解析）と、`main()` 内での WARN 発行 |
| `.claude/hooks/post-edit.test.mjs` | 発火条件・純追記で出ないこと・出力形の固定 |
| `docs/hooks.md` | PostToolUse の**散文**へ追加し、**不在の意味**を明記。**発火一覧の表には行を足さない**——この reminder は `selectChecks` が発行する検査 id を持たない（`isSourceFileWrite` の WARN と同じ形）。表の母集団照合は id を持つものだけを見る |
| ルート `CLAUDE.md`「フック」 | 「`*.md` 全般…の沈黙は『何も走らなかった』である」が**偽になる**ため訂正 |
| `docs/build-commands.md`（2 か所: L32・L167） | 同じ主張の写し。**同時に直す** |
| `docs/hooks.md`（L59） | **発火一覧の「空集合の行」自身**が「何も走らない——沈黙は『合格』ではない」と言う。`G-hook-fires` が見るのは id 列だけなので散文の修正は安全（`G-hook-fires.mjs:118-127`） |
| `.claude/hooks/post-edit.mjs`（L11-12） | **hook 自身の冒頭の契約コメント**が当の主張を持つ。Phase 2 で変える当のファイルである |
| `.claude/rules/governance-docs.md`（L23） | 「PostToolUse hook **検査**は走らない」は厳密には真のまま（reminder は検査ではない）。**姉妹文書だけ直してここを残すと不整合に見える**ので、reminder が在ることを 1 句だけ足す |
| `.claude/hooks/post-edit.test.mjs`（L33・L171） | コメントが「何も走らない」を前提にしている。Phase 2 のテスト追加と同時に見る |
| `docs/adr/ADR-dependents-reminder-at-edit-time.md`（新規） | 却下した案と実測値（未確定 3 の結論） |
| `snotra-core/CLAUDE.md` ほかモジュール索引 | 変更なし（`.rs` を足さない） |

**散文の母集団の取り方（この計画自身が 1 度失敗している）**: 最初に「4 か所」と数えたが、実際は上表のとおり 6 か所以上だった。**単独の grep はどれも全部を出さない**——

- 1 本目（`--include=*.md` + 「PostToolUse hook 検査は走らない\|md 全般\|何も走らなかった」）は `.mjs` を**除外句で落とし**、`docs/hooks.md:59` を**語形の違い**（「何も走らない」）で落とした
- 2 本目（除外なし + 「何も走らな\|沈黙は合格ではない」）は `.claude/rules/governance-docs.md:23` を**別表現**で落とした

ゆえに **2 本の和集合**を母集団とし、Phase 3 の最後に**両方を再実行**して直し漏れが 0 件であることを確かめる（#977・#1056 は**写しを直す当のコミットが 1 枚落とす**型）。`grep -r` を除外なしで打つと `target/` を舐めて 120 秒で切れるので、ripgrep（`.gitignore` を尊重する）を使う。**PR 本文も数え上げの母集団に含める**（squash で main の commit message になるがファイルの grep には入らない）。

## 実装順序

### Phase 1 — 判定を `scripts/governance/` に置く

- [x] `dependents.mjs` に逆引き索引（`(対象パス, 正規化ラベル) → 参照箇所[]`）を作る。参照の抽出は `refScanLines` と `G-heading-refs.mjs` の `HEADING_REF` を再利用し、**正規表現を 2 本目として持たない**
- [x] 節の行範囲を返す関数を置く（未確定 1 の結論に従う）
- [x] 変更行 → 該当する節 → 依存箇所 を返す関数を置く
- [x] `dependents.test.mjs`: 節を書き換えたら依存が出る／別の節なら出ない／依存 0 なら空／前方一致で複数節に当たる場合の扱い
- [x] `npm test` が緑

### Phase 2 — hook から呼ぶ（subprocess）

- [x] `dependents.mjs` に CLI 入口を置く（`node scripts/governance/dependents.mjs <rel>` で、その編集に対する報告を stdout へ）。**`git diff HEAD -U0` の実行もこの中に置く**——hook 側に判定を持たせない
- [x] `post-edit.mjs` から `root` 基準で spawn する（`runCheck` と同じ経路。**静的 import を足さない**——`try { main() } catch` の外で落ちる経路を作らないため）
- [x] スクリプトが**無いツリー**（この機構より前に凍結された worktree 等）では、hook を落とさず静かに何もしない。ただし**その静けさが沈黙 = 合格を壊さない**ことをテストで固定する
- [x] `post-edit.test.mjs`: 発火する／純追記では発火しない／`.rs` では発火しない／依存 0 では発火しない／**スクリプト不在でも hook が JSON エンベロープを返す**
- [x] `npm test` が緑

### Phase 3 — 文書

- [x] `docs/hooks.md` の散文へ追加し、**「この reminder の不在は『依存が無い』を意味しない」**を明記（発火一覧の表には行を足さない）
- [x] 偽になる散文 4 か所を直す（ルート `CLAUDE.md` L29・`docs/build-commands.md` L32/L167・`.claude/rules/governance-docs.md` L23）
- [x] `docs/adr/ADR-dependents-reminder-at-edit-time.md` を書く（却下した案と実測値）
- [x] **同じ概念で grep し直し、直し漏れが 0 件であることを確かめる**
- [x] `npm run governance:check` が緑

### Phase 4 — 効きの実測

- [ ] 変異注入（worktree の複製・`node_modules` は junction）で、次の 3 種がそれぞれ落ちることを実測する: (a) 純追記フィルタを外す (b) 逆引き索引の母集団を 1 本の腕に減らす (c) hook からの呼び出しを外す
- [ ] 実測値（発火した節・件数・所要時間）を PR 本文へ載せる

## 不変条件と異常系

- **`git diff` が失敗しても hook を落とさない**——リポジトリでない・HEAD が無い（初回コミット前）場合は静かに何も出さない。**ただし「静かに」が沈黙 = 合格を壊す経路にならないこと**を Phase 2 のテストで固定する
- **索引の構築費用は編集ごとに掛かる**（実測 156 ms・git 抜き）。未確定 2 で「安く抜ける前段判定」を決める
- **worktree で動くこと**——`post-edit.mjs` は `resolveRoot` でツリー根を求める。`scripts/governance/` はそのツリー内に在る
- WARN は `errors` ではなく `warnings` へ積む（exit code を動かさない）

## テスト方針と検証コマンド

- `npm test`（vitest。`scripts/governance/dependents.test.mjs` と `.claude/hooks/post-edit.test.mjs`）
- `npm run governance:check`（Phase 3）
- 変異注入は worktree の複製へ当てる（`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）

## SPEC.md・関連文書の更新要否

- `SPEC.md`: **不要**（製品の挙動を変えない・開発機構である）
- `docs/hooks.md` / ルート `CLAUDE.md`: Phase 3 で更新
- ADR: **要否を未確定 3 で決める**

## 未確定（実装前に潰す）

- [x] **節の切り出しに `sectionOf` を使うか、独自のアンカー行判定を置くか** — **`sectionOf` は使えない**。契約が「見出しの正規表現を 1 本渡して**その 1 節の body を返す**」形であり、複数一致は finding になる（`lib.mjs:289`）。全節の列挙という用途に合わない。**アンカーは `collectAnchors` と同じ 3 種・フェンスをマスクしない形で列挙する**。実測: 参照の対象になっている 28 文書のアンカー行 1371 本のうちフェンス内は **27 本（2.0%）**で、`.claude/skills/plan-review/SKILL.md` と `retrospective/SKILL.md` のテンプレート塊に集中する。マスクしない方を採るのは、**着地判定（`collectAnchors`）と境界判定を同じ規則に揃えるため**——参照が実際に着地した相手を節として扱う。**受容する残余**: テンプレート塊が節を分断するので、その直下の散文を編集しても報告が出ない（沈黙側の誤り）
- [x] **索引の構築費用を毎回払うか** — **払う。前段は置かない。** 実測（プロセス内・git 呼び出し無し・5 回）: 中央 **129 ms**（最小 114 / 最大 146）。うち `makeSnapshot` の fs walk は 11 ms で、残りは 251 文書の読み込みと走査。**安い前段は作れない**——「対象文書か」を知るには参照側を全部読む必要があり、全文を読んで `「` の有無だけ見る前段でも 88 ms 掛かる（差は 41 ms しかない）。hook の node プロセスは既に起動済みなので、起動費用（実測 109〜130 ms）は掛からない
- [x] **ADR を書くか** — **書く**。却下した案がいずれも実測値を伴う否定の知識を持つ: subprocess 化（node 起動 109〜130 ms）・純追記を外さない（発火 41〜46% 対 24〜25%）・安定アンカー ID（序数参照と同じ失敗様式）・機構ごとの廃止（main の履歴で週 2 回の追随）。`docs/adr/ADR-dependents-reminder-at-edit-time.md` を Phase 3 で書く

## 実装中に判明した訂正（2026-08-19）

- [x] **発火率の見積もり 24〜25% は誤りで、実測 55% である。** 原因は計画段階のプロトタイプが節を
  入れ子にしておらず、`## 節` が最初の箇条書きの手前で切れていたこと。実物（`CLAUDE.md`「フック」節の
  箇条書きを編集）で走らせて初めて出た——**合成フィクスチャは見出し直下の行しか触っていなかった**。
  修正後の実測（実装そのもので測定・`.md` を触った直近 80 コミット）: **44/80（55%）・重複を除いた
  参照は中央 6 件・最大 37 件**。**コミット単位の値なので、編集 1 回あたりの発火はこれより低い**
- [x] **ソースに NUL バイトが混入していた**（索引の鍵の区切り）。`grep` がファイルを binary と判定して
  以後の走査から落ちる。明示のエスケープで書ける区切りへ替えた

## plan-review 結果

- リスク: **高**（hook を変更する）
- レビュー方式: 計画準拠レビュー 1 体
- エージェント数: 2（3b の敵対調査 1 体 + 本レビュー 1 体）

### 要対処（4 件・すべて一次証拠で再照合済み・計画へ反映済み）

- **静的 import は `try { main() } catch` の外で落ちる** — `post-edit.mjs:476-484` を読んで確認。全編集で hook が沈黙する経路になる → **subprocess へ設計変更**
- **相対 import は `${CLAUDE_PROJECT_DIR}` 基準で解決し、`resolveRoot` のツリーとずれる** — `docs/hooks.md:103` が既に明記していた非対称。計画の「そのツリー内に在る」は 2 つのツリーを混同していた → **subprocess で `root` 基準の起動へ**
- **`docs/hooks.md:59`（空集合の行）が偽になる主張を持つ** — 修正対象へ追加。`G-hook-fires` は id 列しか見ないので散文の修正は安全（`G-hook-fires.mjs:118-127`）
- **`post-edit.mjs:11-12`（hook 自身の契約コメント）が偽になる** — 修正対象へ追加。**私の grep が `--include=*.md` で落としていた**

### 軽微

- `scripts/governance-check.mjs:6` の関連する主張は字義どおりには真のまま（任意の整合修正）
- `dependents.mjs` 自身の編集には PostToolUse 検査が付かない（`scripts/` の既存の残余であり、本件が作る穴ではない）

### 未検証

- worktree で分離されたエージェントセッションにおける `${CLAUDE_PROJECT_DIR}` の実際の解決（subprocess 化で影響は小さくなるが、消えてはいない）
- exit 1 / 空 stdout の hook クラッシュを harness がどう見せるか
- フェンスをマスクしないアンカー列挙との相互作用（本レビューの観点 2 つの外）

### 判断

- 実装着手: **人間の裁定待ち**（設計が import → subprocess へ変わったため）

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の「hook、CI、rules、skills、ガバナンス文書を変更する」に該当）
- plan-review: 独立レビュー 1 体（`--deep` は使わない——網羅性が要件でもガバナンス文書の移動・圧縮・分割でもない）
- エージェント数: 2（3b の敵対調査 1 体 + plan-review 1 体）
- 自己レビュー 5 点:
  1. **issue の全要件に作業項目が対応する** — #1140 の「決めること」2 点（churn の量・どこで走らせるか）は設計判断の表で決着
  2. **境界条件と検証** — 純追記のみ / 依存 0 件 / `.rs` 編集 / `git diff` 失敗 / worktree の 5 つに Phase 2 のテストを割り当て
  3. **正常/失敗/破棄経路** — 新しいリソースもプロセスも持たない（索引はプロセス内で作って捨てる）
  4. **より単純な既存パターン** — `isSourceFileWrite` と同じ「gate ではない reminder」に揃えた。判定の再実装はしない
  5. **不変条件の検知手段** — 「沈黙 = 合格」を壊さないこと・純追記で出ないことに Phase 4 の変異注入を割り当て
- 未検証: 実運用での体感（発火 24〜25% の見積もりは直近 80 コミットからの近似であり、実際の編集単位ではない）
- 実装時に追加で走らせる: `/dry-check`（新規関数の重複確認。コードが無い今は対象が無い）

## 人間レビュー

- [x] 承認済み — 2026-08-19 / 問い: "`workspace/plan.md` へ注釈を書き込んでいただくか、承認をいただければ Step 6 へ進みます。" / 回答: "承認"
