# 計画 — issue #1443 入力の無い trailing を発火させない

分類: 性能の改善（refactor/perf）。`SPEC.md` に debounce の発火回数の記述は無いので変えない。調査は `workspace/research.md`。

## 目的と受け入れ条件

- leading の後に入力が無かったバーストでは、trailing を発火しない（lodash の `debounce({leading: true, trailing: true})` と同じ契約）
- 受け入れ:
  1. 孤立した 1 打鍵で、worker へ送る検索が 1 回になる。smoke のパスクエリ打鍵のうち `:` と `\` の 2 打鍵（どちらも前の打鍵から 120 ms 後）で、本ブランチの発行数が 2 になる（main は 4。下の計測）
  2. 連打の最後のクエリは、今までどおり trailing で検索される（`Debouncer` の純粋核テスト）
  3. `Debouncer` のテストに、leading だけで終わるバーストでは trailing が発火しないケースがある
  4. `/state-check`（計画段階）と `/race-check`（実装段階）を通す。#1038 の flush-on-Enter の条件を崩さない

## 設計

`Debouncer` にフィールド `input_after_leading: bool`（leading の後に入力があったか）を足す。

- `on_input`: leading を発火する（`leading && !armed`）なら `input_after_leading = false`。発火しないなら `input_after_leading = true`。どちらでも `armed = true`
- `poll`: `armed && elapsed >= interval` なら `armed = false` にし、`input_after_leading` を返す（返したら `false` に戻す）。条件を満たさなければ `false`
- `cancel`: `armed = false` と `input_after_leading = false`
- `new`: 両方 `false`
- leading=false のモードでは `on_input` が常に「発火しない」側に入るので、`input_after_leading = true` になり、従来どおり trailing だけが発火する
- `armed` の意味（`on_input` で立ち、`poll` / `cancel` で落ちる）は変えない。`is_armed` も変えない。ゆえに `is_unsettled` / `input_idle` / `unsettled_at_entry` / repaint の再要求の式は触らない
- `run_search_with` に「同じクエリなら飛ばす」を入れない（issue の指示）

## 変更するファイルと対象シンボル

1. `src-tauri/src/egui_shell/layout.rs`
   - `Debouncer`（フィールドと struct の doc の箇条「poll: …」）、`new`・`on_input`（doc「いずれにせよ armed にして trailing を予約する」）・`poll`（doc と `#[must_use]` の文言）・`cancel`（doc「予約済み trailing」）・`is_armed`（doc「armed のまま（trailing 未発火）か」）
   - テスト: 既存 3 本（`leading_fires_on_burst_start_then_trailing` は名前どおり「leading の後に入力がある」ケースとして残す。中の 2 回目の `on_input` がその入力である）に、新テスト `leading_only_burst_does_not_fire_trailing`（`on_input` 1 回 → 期限前は `is_armed()` が真 → `poll(50ms)` が偽・かつ `is_armed()` が偽に落ちる → 次の `on_input` は leading。「偽を返すが armed を落とさない」実装は repaint の永久再要求と `is_unsettled` の固着になるので、`is_armed()` の偽を必ず表明する）と `cancel_clears_input_after_leading`（leading → 入力 → cancel → 次の leading だけのバーストで trailing が出ない）を足す
2. `src-tauri/src/egui_shell/launcher_controller/search_flow.rs`
   - `poll_search_debounce` の doc（「trailing debounce の poll と再 arm」）とコメント「連打が収まって interval 経過したら最終クエリで検索し直す」「armed のまま = trailing 未発火」を、新契約（leading の後に入力があったときだけ再検索・armed のままは「窓がまだ開いている」）に合わせる
3. `src-tauri/src/egui_shell/search_state.rs`
   - `FolderFrame::unsettled_at_entry` の doc（「`poll` の trailing 発火で…落ちる」→「`poll` が窓の終わりで落とす」）
   - `is_unsettled` の doc（「解けるのは trailing が発火するか」→「窓が閉じるか」）。「trailing 発火の直後は必ず…」の段は真のままなので残す
   - テスト doc（L1191 付近「debounce が予約を持ったまま（trailing 未発火で」）は直さない——「予約」は armed を指し、armed の意味は変わらないので真のまま（3b の ⚠️ を自分で読み直して判断）
4. `src-tauri/src/egui_shell/search_dispatch.rs` — `Settled` の doc「打鍵起点には 50 ms の trailing debounce 待ちが必ず入る」→「入りうる（trailing で発行した要求のとき）」
5. `snotra-core/tests/search_frame_cost.rs` の `//!`「egui 経路は trailing debounce 1 回ごとに」→「debounce が検索を発行するたびに」
6. `docs/architecture.md` の sequence 図の「Debouncer（leading 発火 + trailing 50ms を arm）」を「Debouncer（leading 発火 + 50ms の窓を arm。窓の中で打鍵が続いたときだけ trailing）」へ（Step 2b の導出。「trailing を arm」は毎回 trailing が出ると読める）
7. `PERFORMANCE.md` — 新しい記録を 1 節足す（採用: 入力の無い trailing を省く・#1443）。中身は smoke の発行数（本ブランチ・main の参考値と揺れ）と、`since_key_us` が孤立打鍵では trailing の待ち（約 50 ms）を含まなくなり、変更前の記録と比べられないこと

触らないもの: `scripts/smoke-egui.ps1`（「trailing を跨がせる」は時刻を跨がせる意味で真・セーフティネット）、`PERFORMANCE.md` の既存の記録（採否ログ）、`docs/superpowers/`（設計メモ）。

## 実装の順序

1. Red: `layout.rs` に新テスト 2 本を足し、現行実装で赤を確認
2. Green: `Debouncer` を直す
3. 散文（2〜5）を直す
4. 検証・smoke の計測・`PERFORMANCE.md`

## 不変条件と異常系

- `armed` の真偽の推移は変更前と同一（`on_input` で真、`poll` の期限・`cancel`・`new` で偽）。検知: 既存テスト `leading_fires_on_burst_start_then_trailing` の「発火後は disarm」と、新テスト `leading_only_burst_does_not_fire_trailing` の `is_armed()` 偽
- trailing が発火するのは、leading の後（または leading=false で）に入力があったときだけ
- 境界条件と検証:
  - 1 打鍵だけのバースト → trailing なし（新テスト）
  - 2 打鍵以上のバースト → trailing あり（既存テスト）
  - leading=false → 1 打鍵でも trailing あり（既存テスト `no_leading_mode_only_trailing`）
  - 入力後に cancel → 次の 1 打鍵バーストで trailing なし（新テスト）
  - `poll` が期限前 → 偽で armed のまま（既存テスト）
- 製品側の異常系（3b で確認済み）: leading が同期クリア（空クエリ・indexing 中・送信失敗）だったバーストは trailing でも同じ同期クリアになるだけなので省いてよい。索引の構築完了は `consume_external_pending` が再検索する

## テスト方針と検証コマンド

- 単体: `cargo test -p snotra layout`（`Debouncer` のテスト）
- 変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: (a) `poll` を `return true`（従来の契約）へ戻す → 新テスト `leading_only_burst_does_not_fire_trailing` が赤、(b) `on_input` の非 leading 側で `input_after_leading` を立てない → 既存 `leading_fires_on_burst_start_then_trailing` が赤、(c) `cancel` で `input_after_leading` を落とさない → `cancel_clears_input_after_leading` が赤、(d) `poll` で `armed = false` を消す → 既存の「発火後は disarm」と新テストの `is_armed()` が赤
- 検証: `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（`.rs`）。カテゴリ C/D は表示・ホットキーに触れないので該当しないが、受け入れ 1 のために `smoke:egui` を手で回す
- smoke の計測（受け入れ 1）: 常駐の Snotra を終了してもらってから、`npm run smoke:egui` を本ブランチで 1 回、main で 1 回走らせる（`SNOTRA_TRACE` の出力は `%TEMP%\snotra_smoke_egui.err`）。数え方は `dispatch_seq` の差である（`SearchDispatch::issue` は発行ごとに seq を 1 進め、`reset` でも作り直さない。`settled + dropped` の件数は worker の `coalesce` が間引いた seq を数えず、leading が 50 ms を超えると dropped に回るので使わない）。`egui_input:changed` の `after_chars == 1`（`c`）の後に観測された最大の `dispatch_seq` と、`after_chars == 3`（`\`）の後に観測された最大の `dispatch_seq` の差が、`:` と `\` の 2 打鍵の発行数である。本ブランチは 2、main は 4。`c` は直前の Backspace から 50 ms 後で窓の境界に乗り、main でも 1〜2 に揺れるので数えない（3b の F3・Step 2b）。計測スクリプトは書かず、ログを読んで数え、PR 本文と `PERFORMANCE.md` に載せる

## 文書の更新

- `SPEC.md`: 不要
- `docs/architecture.md`: 不要（上記）
- `PERFORMANCE.md`: 新しい記録を 1 節（上記 7）
- `docs/architecture.md`: sequence 図の 1 行（上記 6）。`npm run governance:check` はカテゴリ F としてツールが回す
- `src-tauri/CLAUDE.md`: debounce の発火回数を書いていないので不要（3b で確認）

## 作業項目

### Phase 1 — Red
- [x] `layout.rs` に `leading_only_burst_does_not_fire_trailing` と `cancel_clears_input_after_leading` を足し、現行実装で赤を確認

### Phase 2 — Green
- [x] `Debouncer` に `input_after_leading` を足し、`new` / `on_input` / `poll` / `cancel` を直す
- [x] `Debouncer` の struct・`on_input`・`poll`（`#[must_use]` 含む）・`cancel`・`is_armed` の doc を直す

### Phase 3 — 散文
- [x] `search_flow.rs` の `poll_search_debounce` の doc とコメント
- [x] `search_state.rs` の `unsettled_at_entry` / `is_unsettled` / テスト doc
- [x] `search_dispatch.rs` の `Settled` の doc
- [x] `snotra-core/tests/search_frame_cost.rs` の `//!`
- [x] `docs/architecture.md` の sequence 図の 1 行

### Phase 4 — 検証と記録
- [x] 変更後の検証チェックリスト（カテゴリ A）を通す
  - ラウンド 1: sha fc14664b / verify:tree green（A・E・F）/ 指摘 High 2（既存テストの「発火後は disarm」が新契約で空洞化〈変異 f が生存〉→ `is_armed()` の偽と次の leading を表明／訳語「窓」が GUI ウィンドウの語義と衝突 → 既存の armed / disarm の語彙へ）・Medium 1（`cancel` のクリアと `take` は等価変異で何も縛らない → 両方を削り、`on_input` が毎回上書きすることを doc に書き、持ち越しを縛るテスト `trailing_reservation_does_not_leak_into_next_burst` に置き換え）・Low 4（`search_dispatch.rs` の 50 ms の写しを削除・`search_frame_cost.rs` の経路の列挙を削除。folder 往復と Phase 4 未了は既知）/ 変異 (a)(b)(d) 検知・(c)(e)(g) は等価・(f) 生存 → 上の修正で検知
  - ラウンド 2: sha 37115878 / verify:tree green（A・E・F）/ 前回の 5 件すべて解消 / 新規 Low 2（Low-5: `input_after_leading` の doc の名指しを intra-doc link へ——採用。Low-6: 「armed にする唯一の経路」は `armed = true` が 1 か所の間だけ真——記録のみ）/ 変異 (d)(f)(n)(a)(b) 検知 / 生き残り 0
- [x] 常駐の Snotra を終了してもらい、smoke を本ブランチと main で走らせて発行数を数える — 2026-11-16・debug・各 1 回・両方 pass。`dispatch_seq` で数えて、smoke 全体で main 18 / 本ブランチ 10、`:` と `\` で main 4 / 本ブランチ 2（計画の見込みどおり）
- [x] `PERFORMANCE.md` に記録を足す

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-11-16 / 問い: "計画を承認なさいますか。それとも `workspace/plan.md` に注釈を書き込まれますか。" / 回答: "承認"

## plan-review 結果

- リスク: 高（`Debouncer` の状態遷移〈trailing の発火条件〉を変える）
- レビュー方式: 独立導出 1 体（Step 2b・成果物 `workspace/plan-review-derive-1443.md`）
- エージェント数: 1

### 要対処
- smoke の数え方 — 計画を修正 — `c` は直前の Backspace から 50 ms 後で窓の境界に乗り、main でも 1〜2 に揺れる。比較は `:` と `\` の 2 打鍵（main 4・本ブランチ 2）に絞った。`dispatch_seq` が `reset` で作り直されないことは `search_dispatch.rs` の `issue` と `SearchState::reset` を読んで確かめた
- `docs/architecture.md` の sequence 図 — 変更ファイルへ追加 — 導出 ∖ plan。「trailing 50ms を arm」は毎回 trailing が出ると読める
- 新テストの表明 — 計画を修正 — 「偽を返すが armed を落とさない」実装がテストを通る落とし穴。`is_armed()` の偽と、次の `on_input` が leading になることを表明する

### 軽微
- worker の死の検知が「trailing の送信」から「次の打鍵の送信」へ遅れる（導出の ⚠️1）。`search_worker.rs` の `spawn_search_worker` の doc「検知は 1 要求ぶん遅れる——次の送信が失敗して初めて現れる」は真のまま。到達しない経路なので受容
- 50 ms 以内の folder 往復で、復帰した古い行が次の打鍵まで残る（導出の ⚠️2・3b の F9）。Enter は `restored_rows_stale` で flush されるので誤起動しない。受容
- trailing の結果着による世代交代（#714 の瞬時スクロール 1 回）が消える（導出の ⚠️4）。改善の向き
- `view.rs:681` の既存の不正確さ（導出の ⚠️8）は本件と無関係。触らない
- 「`poll_search_debounce` で leading のクエリと比べる」代替（導出の ⚠️9）は純粋核の外に状態が増えるので採らない

### 未検証
- smoke の実測（Phase 4・常駐の Snotra を終了してもらう必要がある）

### 判断
- 実装着手: 可（人間の承認待ち）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（Step 2b）
- エージェント数: 1（3b の敵対的調査を含めると 2）
- 要対処: 3 件（上の plan-review 結果のとおり反映済み）
- 未検証: smoke の実測（Phase 4 で行う）
- 散文の全件列挙: 語 3 / 総件数 264 行 / ★未掲載 25（計画へ足した 0：別義の「予約」〈repaint の予約・環境変数の予約〉か、armed を「予約」と呼ぶ記述で armed の意味は変わらず真のまま） / ☆行未掲載 0（計画へ足した 0）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 0 / 主の見落とし 2
- `/state-check`: 状態モデルとの不整合なし（新しい状態は `input_after_leading` だけで、show の作り直し・`cancel` で落ちる。Enter の flush は `is_unsettled(armed)` のまま）
