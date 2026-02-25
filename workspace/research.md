# 検索ウィンドウ表示直後の履歴リスト不具合 構造調査

## 調査日時
- 2026-02-25
- 手法: 静的解析（`ui` / `src-tauri` / `snotra-core` のコード追跡）

## 調査目的
- 「検索ウィンドウ表示後に出る直近選択アイテム（空クエリ履歴）表示・クリック周辺」で不具合が多発する理由を、個別バグではなく**構造要因**として整理する。

## 対象コード
- `ui/src/App.tsx`
- `ui/src/stores/search.ts`
- `ui/src/components/SearchWindow.tsx`
- `ui/src/components/ResultsWindow.tsx`
- `src-tauri/src/main.rs`
- `src-tauri/src/commands.rs`
- `snotra-core/src/search.rs`
- `snotra-core/src/history.rs`

---

## 1. 表示直後フロー（空クエリ履歴）で起きていること

1. Rust 側 `show_main_and_emit()` が `window-shown` を emit（`src-tauri/src/main.rs:65-87`）。
2. main 側 `App.tsx` が `window-shown` を受け `resetForShow()` 実行（`ui/src/App.tsx:51-57`）。
3. `resetForShow()` は `query=""` にし `refreshResults()` を即実行（`ui/src/stores/search.ts:335-342`）。
4. `query` 変更の `createEffect` でも debounce 経由で `refreshResults()` が走り得る（`ui/src/stores/search.ts:166-191`）。
5. `refreshResults()` は空クエリなら `getHistoryResults()` を呼び、`results-updated` / `results-count-changed` を emit（`ui/src/stores/search.ts:148-163`）。
6. `results` ウィンドウは別 WebView で、イベント受信して独自 state を更新し行クリック時に index を返す（`ui/src/components/ResultsWindow.tsx:69-110`）。
7. click は Rust command を経由して `result-clicked(index)` として main に戻る（`src-tauri/src/commands.rs:288-295` → `ui/src/App.tsx:253-261`）。

---

## 2. 不具合が増えやすい構造要因

## C1. 2つの WebView 間で state をイベント同期している（単一状態源が壊れやすい）
- `main` と `results` は JS コンテキストを共有しないため、`results-updated` イベントを介して擬似同期している。
- `main` 側 store state と `results` 側 local state が、非同期到着順に依存してズレる可能性がある。
- 表示直後は `window-shown`, `refresh`, `count-changed`, `render-done`, `click` が短時間に重なるため、ズレが顕在化しやすい。

## C2. クリック確定プロトコルが `index` ベース（アイテム同一性を失う）
- `ResultRow` click は `notifyResultClicked(idx)` で index のみ送る（`ui/src/components/ResultsWindow.tsx:108`）。
- main 側で受ける時点の配列が変化していると、同じ index が別アイテムを指す。
- 現在は `searchResults()[index]` をスナップショットして改善したが、根本的には「path/id を送る」より脆い構造のまま。

## C3. 表示直後に `refresh` が多重発火する設計
- `resetForShow()` の即時 `refreshResults()` と、`setQuery("")` による effect 側 debounce refresh が共存。
- これに `SearchWindow.onMount()` の `refreshResults()`、`indexing-complete` refresh も重なる。
- 表示直後は「複数 request が同一 UI 領域を更新」する前提になっており、クリックタイミングとの競合が起きやすい。

## C4. UI制御（show/hide）をデータイベント（count-changed）に強く結合
- `results` ウィンドウの表示制御は `results-count-changed` に依存（`ui/src/App.tsx:169-251`）。
- 本来はウィンドウ状態機械（Visible/Hidden）で扱うべき責務が、検索結果件数イベントに埋め込まれている。
- そのため stale request / 競合時に「データ更新の遅延」が「ウィンドウ残留・再表示」に直結する。

## C5. requestId 管理が層ごとに分散
- `search.ts`（データ）、`App.tsx`（ウィンドウ）、`ResultsWindow.tsx`（描画）でそれぞれ requestId を管理。
- 各層で stale 判定はあるが、全体として単一の state machine ではない。
- 1箇所のガード漏れが残ると、他層が正しくても不整合が再発する構造。

## C6. 履歴順序の時刻解像度が秒単位（同秒 launch の並びが不安定）
- `record_launch()` は `as_secs()` で秒単位保存（`snotra-core/src/history.rs:88-97`）。
- `recent_launches()` は `last_launched` 降順のみで、同値 tie-break がない（`snotra-core/src/history.rs:158-169`）。
- 連続起動が同一秒に収まると表示順が確定せず、表示更新ごとに体感的な「並び揺れ」が起こり得る。
- 表示揺れは index クリック設計（C2）と組み合わさると誤選択/無反応に見えやすい。

## C7. 主要 listener 登録が config 取得成功に依存
- `result-clicked` / `results-count-changed` listener は `if (label === "main" && config)` 内で登録（`ui/src/App.tsx:73-271`）。
- `getConfig()` 失敗時に listener 未登録となり、表示やクリック経路が部分停止する。
- 一見「たまにクリックが効かない」症状に見える潜在構造。

## C8. launch 成否が UI に返らない fire-and-forget
- `launch_item` はスレッド起動後に即復帰し、実行成否を返さない（`src-tauri/src/commands.rs:46-83`）。
- UI は「呼び出し成功」を「実際に開いた成功」と区別できない。
- 失敗時はユーザー視点で「クリックしたのに開かない」に見えるが、UI 側は検知不能。

---

## 3. なぜ「表示直後 + 履歴リスト + クリック」で特に壊れやすいか

- 表示直後は state 初期化・履歴取得・ウィンドウ再配置・focus 連動が同時進行する。
- そのタイミングの履歴リストは「空クエリによる自動更新」の対象で、ユーザーのクリックと並行して再計算される。
- クリック確定は index 伝搬であるため、更新中のリストに対して同一性保証が弱い。
- 結果として、同じ不具合に見える症状でも、実体は「配列ズレ」「表示順揺れ」「listener未登録」「launch失敗」の複合で起きる。

---

## 4. あるべき構造（To-Be）

## T1. クリック確定は index ではなく item identity（path または内部ID）で伝搬
- `results -> main` は `index` ではなく `path` を送る。
- main 側で `path` を直接起動対象にし、配列変化の影響を受けないようにする。

## T2. 表示直後の refresh を単一入口に統一
- `window-shown` で実行する refresh を1本化し、同一フェーズで多重発火させない。
- `resetForShow()` と query effect refresh の重複を整理する。

## T3. データ更新とウィンドウ表示制御を分離
- `count-changed` で window show/hide を直接決める方式から、明示状態機械へ分離。
- 例: `Standby -> ShowingHistory -> SearchReady -> Activating -> Closing`

## T4. requestId を UI全体で一元化
- 層ごとの独立カウンタではなく、共通 generation を配布する。
- stale 判定ルールを共通化してガード漏れ面積を減らす。

## T5. 履歴順序の tie-break を定義
- `last_launched` 同値時は `path` などで安定ソートする。
- 可能なら timestamp 解像度を ms/ns へ上げる。

## T6. listener は config 取得成否と分離して登録
- core なイベント経路（クリック・結果表示制御）は常に有効化する。
- config 必須処理だけを条件分岐へ分離する。

## T7. launch 結果を UI へ返す
- fire-and-forget ではなく、最低限の成功/失敗結果を返し、失敗時にユーザーへ理由を表示する。

---

## 5. 優先度（構造改善の順序）

1. `index` 伝搬の廃止（T1）
2. 表示直後 refresh の一本化（T2）
3. listener 登録条件の分離（T6）
4. 履歴順序の安定化（T5）
5. launch 成否返却（T7）
6. requestId 一元化・状態機械化（T3, T4）

---

## 6. 結論

- 不具合多発の主因は、個別ロジックよりも「**分散状態 + index クリック + 多重非同期更新**」という構造の組み合わせ。
- 直近の修正で一部レースは軽減されたが、構造的には同種の不具合を再生産しやすい設計が残っている。
- 改善効果が最も高いのは「クリック同一性を path/id 化」し、「表示直後 refresh を単一入口化」すること。
