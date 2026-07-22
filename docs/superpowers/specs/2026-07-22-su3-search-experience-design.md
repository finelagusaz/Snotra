# SU3 設計 — 検索体験（#532 Phase 2）

- 種別: サブユニット設計（spec）。実装計画は本 spec 承認後に別途 writing-plans で作る
- 日付: 2026-07-22
- 親: `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md`（SU3）／#532
- 前段: SU1（`2026-07-21-su1-softbuffer-runtime-design.md`・完了）／SU2（`2026-07-22-su2-window-shell-design.md`・完了）
- 主参照: SPEC §4, §4.7, §4.8, §6, §15, §19。flip 基準（roadmap 52–59 行）
- 履歴: 本設計は brainstorm（2026-07-22）で 6 つの分岐を確定して得た。(1) 1 spec + 段階実装、(2) スコープ= core+folder+instant+slash・tool(§18) は defer（独立 SU3.5）、(3) 並行機構は同期直 Engine で崩壊させ移植しない、(4) 状態は src-tauri の純粋 `SearchState`、(5) folder async の staleness token を `SearchState` 内に、(6) 単一ウィンドウ維持（2 ウィンドウは flip 後へ）。spec レビューで (7) debounce を最初から入れる（leading+trailing・決定7）を追加確定。各分岐の否定の知識は本 spec 末尾に残す

## 目的

製品 `src-tauri` の egui メインウィンドウ（SU2 の placeholder view）を、**IPC を通さず直 `Engine` を呼ぶ**実検索 UI へ置き換える。SolidJS フロント（`ui/src`）が WebView2 経路で実現している検索体験——クエリ入力 + IME・インクリメンタル検索・結果リスト/行・キーボードナビ・選択・起動・フォルダ展開・スラッシュコマンド・インスタントコマンド——を egui immediate-mode で parity 再現する。SU2 の外殻（show/hide・blur・位置永続・フラグ選択）はそのまま使い、SU3 は `SearchWindowView::update` の内側を作る。

## アプローチ（要石）

### functional core + imperative shell

egui view は egui/Win32/AppHandle に依存しユニットテストが難しい。だから状態と遷移ロジックを純粋な `SearchState`（functional core）に閉じ、`SearchWindowView` は薄い driver（imperative shell）に保つ。SU2 が `plan_hotkey`/`blur_should_hide` を `lifecycle.rs` の純粋核へ分離したのと同じ設計を、検索状態機械へ拡張する。

```
SearchWindowView (driver / imperative shell)  ── egui/Win32/AppHandle 依存・テスト難
  ・egui TextEdit / ScrollArea 描画。TextEdit の Response.changed() で入力受付
  ・Engine 呼び（search / recent_history / instant / launch_item_core / capture_folder_list_context）を実行
  ・結果を core へ注入（set_results / apply_folder_result）
  ・window.set_size / show / hide、folder 読みの thread spawn + channel poll、request_repaint

SearchState (functional core)  ── egui/Win32 非依存・ユニットテスト
  ・query / selected / results / stack(folder) / folder_gen を所有
  ・view_kind() / interp() を毎フレーム純粋導出（reactive memo は不要＝immediate mode が毎フレーム再計算）
  ・on_escape / on_arrow_{up,down,left,right} / begin_folder_request / apply_folder_result(token) を純関数遷移
```

**核は IO を一切せず「状態遷移」と「次にやるべき要求」を返すだけ**。driver が Engine・Win32 を叩き、結果を core へ戻す。SolidJS の `stores/search.ts` の「調停役 vs 描画コンポーネント」の分離を Rust で再現し、遷移ロジック全体を egui 抜きでテストできる。src-tauri ルール「view/Win32 はユニットテスト前提にしない」とも整合する（テスト対象は純粋核へ寄る）。

### 純粋核へ移植する関数（SolidJS からの移植）

以下は Win32/egui 非依存の純関数。`interpretQuery.ts` / `folderNav.ts` / `windowHeight.ts` の Rust 版として `SearchState` と同じ純粋モジュール群に置き、ユニットテストする。

- `interpret(query, prefix, view_kind) -> QueryIntent`（`plain | command | instant{filter_name, instant_query}`）。instant 判定・parse の SSOT。
- `compute_parent_dir(path)` / `clamp_selected_index(len, idx)`（ドライブルート・UNC 対応）。
- `compute_window_height(show_results, max_results, ...)`（結果表示・行数からの論理高さ）。
- `should_run_search(is_burst_start, elapsed)`（debounce の leading/trailing 判定・決定7。clock は driver が注入）。

## 決定事項（brainstorm 確定）

1. **supersede/single-flight 機構は移植しない。** `engine.search()` は Mutex 下で同期（`commands/search.rs`）。SolidJS が `searchLane`(supersede)/`activationLane`(single-flight)/`latestRun`/`exclusive` を要したのは IPC 往復で out-of-order/in-flight Promise が生じるからだけ。driver の `update()` から直 Engine を呼べば search/instant-filter/slash-parse は毎フレーム同期になり、supersede も single-flight も消える（二重起動は起動時 hide で消える）。→ **これらの primitive は egui 経路に持ち込まない。** ただし **debounce（`ownedTimer`）は別**——打鍵 coalescing であって IPC 往復とは無関係ゆえ残す（決定7）。
2. **真の async は 2 つだけ。** (a) フォルダ列挙（UNC/ネットワークで遅延・現状 `spawn_blocking`）、(b) SU4 アイコン。egui idiom（thread spawn → `std::sync::mpsc` → 毎フレーム `try_recv` → 到着で `request_repaint`）で扱う。folder は結果集合を**置換**するので staleness token（`folder_gen`）が要る。アイコンは path キーゆえ stale が自然無視される（token 不要）。
3. **状態と遷移は src-tauri の純粋 `SearchState`。** snotra-core へは押し下げない（selected index・view-stack・Escape ラダーは純然たる UI 関心で、core の責務＝インデックス/検索/履歴 と混ざる）。core に置くのは真に汎用な純関数（`interpret` 等）だけ。
4. **単一ウィンドウ維持（§4.7 parity）。** 検索バーと結果は 1 ウィンドウ内に共存する。2 ウィンドウ分割は flip 後の別 spec（→「否定の知識」）。
5. **tool-selection(§18) は defer → 独立 SU3.5。** spine の view-stack は `Vec<ViewFrame>` として「後で tool 種を加算できる」構造にするが、SU3 では `tool` frame を建てない（SU2 の「live 到達不能な状態を建てない」を踏襲）。SU3 の `view_kind()` は `results | folder` のみ到達する。§18 の parity は独立サブユニット **SU3.5**（SU3 完了後・flip 前）で取る。
6. **IPC コマンドは消さない。** `search`/`get_history_results`/`list_folder`/`get_instant_commands`/`execute_instant_command` 等は flag OFF の WebView2 経路が SU7 まで使う。SU3 は egui view に直 Engine 呼びを**足す**だけ。「IPC 撤去」＝「egui 経路が IPC を通らない」の意。**SU2 の G1（flag OFF 完全不変）を破らない。**
7. **debounce は最初から入れる（SolidJS 相当）。** 高速連打の coalescing は同期直 Engine でも価値が残る（毎フレームではなく打鍵時のみ search でも、1 打鍵ごとの全走査は無駄）。SolidJS を踏襲し **search = leading edge（バースト先頭で即時）+ trailing 50ms**、**instant fetch = 30ms trailing のみ（leading なし）**。timing は driver 所有で **`request_repaint_after`**（SU2 の blur 100ms 猶予と同じ egui idiom）。leading/trailing の判定述語（`should_run_search(is_burst_start, elapsed)`）は純関数として `SearchState` 隣接に置きユニットテストする（clock は driver が注入）。**query 状態は毎打鍵で更新（表示・interp 導出のため）、search 実行だけを debounce する**——query とresults が一瞬ずれるのは debounce の正常動作（SolidJS と同じ）。

### 検証済み（この設計の前提・一次証拠）

1. **`engine.search()` は同期。** `commands/search.rs:19` が `state.engine.lock().unwrap().search(&query)` を同期呼びし `Vec<SearchResult>` を即返す。egui view も同じ `AppState.engine` を掴んで同期呼びできる。
2. **runtime は複数ウィンドウを扱えるが、単一で足りる。** `EguiRuntime` は `HashMap<label, EguiWindow>`、`RuntimePlugin.active` は `HashMap<WindowId, ActiveWindow>`（`runtime.rs:68,129`）。2 ウィンドウは技術的に可能だが SU3 は単一を使う（決定4）。**runtime API を SU3 で拡張しない。**
3. **folder 列挙のコンテキスト捕捉は既存。** `engine.capture_folder_list_context()` → ロック外で `ctx.read_dir_entries()` → `engine.finalize_folder_list()`（`commands/search.rs:58-90`）。この 3 段を driver の thread へ移す（IPC async ではなく std::thread + channel で）。
4. **placeholder view は既に focus/Escape/font-first を持つ。** `view.rs` の `SearchWindowView` が egui `TextEdit`・focus 観測・`emit("egui-hide-requested")`・jp_font index 0 を実装済み。SU3 はこの update の内側を実装で満たす。**Escape の内側モード優先**は placeholder のコメント（`view.rs:131`「内側モード優先は SU3」）が申し送った通りここで実装する。

### 実装初手で確定させる検証ゲート（崩れると設計が反転する）

- **G-RESIZE（単一ウィンドウ動的リサイズの目視品質）**: M1 で softbuffer の CPU present 下、結果の展開/折りたたみ時に reflow/ちらつき/位置ずれが目に見えて悪くないことを実機で目視確認する。**悪ければまず present タイミングを単一ウィンドウ内で直す**（2 ウィンドウ化は最後の手段・flip 後）。SU2 が申し送った「高さリセット→位置→show の結合」がここで初めて動的高さと絡む。
- **G-SYNC（同期 search のフレーム内コスト）**: debounce（決定7）は最初から入れる前提で、大インデックス（実データ規模）で trailing 発火 1 回の `engine.search()` がフレームを詰まらせないことを実測する。詰まるなら debounce 間隔（50ms）を調整するか、`spawn_blocking` 化を検討（folder と同じ token パターン）。**debounce 導入は G-SYNC の結果待ちではない**——連打 coalescing の価値は独立に確定済み。
- **G1 再確認（flag OFF 完全不変）**: SU3 の egui 経路の直 Engine 呼び追加が、WebView2 経路・IPC コマンド・E2E 注入を一行も触らないこと（`SNOTRA_EGUI_MAIN` 未設定で既存テスト + `smoke:startup` + `e2e:tauri` 緑）。

## 状態モデル（背骨）

検索ウィンドウの「モード」は単一 enum ではなく、SolidJS と同じ 2 軸 + overlay を `SearchState` から純粋導出する（SPEC §8.6 状態図と一対一）。immediate-mode では毎フレーム再計算されるため、SolidJS の memo（`createMemo` の等価最適化）は不要——素の関数でよい。

- **軸1 `view_kind() -> ViewKind`**（`results | folder`。将来 `tool` 加算）: モーダルビュースタック頂点の種類。`stack.last().map(kind).unwrap_or(Results)`。
- **軸2 `interp() -> QueryIntent`**（`plain | command | instant`）: 入力の意味。`interpret(query, prefix, view_kind())` の純粋導出。`view_kind() != results` のときは常に plain（folder 中は非 plain 化しない）。
- **overlay**: `indexing`（AppState 由来 bool）/ `launching`（起動中 bool）。軸ではなくどのモードにも重なる。

### Escape 優先度ラダー（SU2 placeholder の無条件 emit を SU3 が先取り）

`SearchState::on_escape() -> EscapeOutcome` が純粋に分岐する（driver が Outcome を実行）:

1. **folder 中** → 展開開始前の検索状態へ一気に復帰（§6.4: 元の候補・選択位置・クエリ）。`RestoreSearch`。
2. **instant / command 中**（view_kind==results かつ interp!=plain）→ モード解除（query の prefix/`/` を消してモード脱出）。`ClearMode`。
3. **top-level**（results + plain）→ `Hide`（driver が `emit("egui-hide-requested")`＝SU2 の hide 合流点へ）。

これにより「入力欄に focus があっても ctx から Escape を先取り」する現 placeholder の挙動を、モード階層に沿って正す。

### view-stack（モーダルビュー）

`stack: Vec<ViewFrame>` は退避/復元の単位。SU3 では `ViewFrame::Folder{ restore_query, restore_results, restore_selected, current_dir, filter }` の 1 種のみ push される。`save`（展開前状態を frame に退避）→ `restore`（Escape/親ルート到達で復元）→ `pop`（1 段戻す）の規律は SolidJS の ViewStack（`saveView`/`restoreView`/`popView`）を Rust へ移す。**tool 種は SU3 では加えない**（決定5）。

## データフロー（毎フレーム dispatch）

driver の `update()` は毎フレーム:

1. egui `TextEdit` を描画し `Response.changed()` を得る。変化があれば `state.set_query(new)`。
2. `state.set_query` 後、driver が **同期 dispatch**（`interp()` × `view_kind()` × overlay で分岐）:
   - `results + plain + !indexing` → `engine.search(query)` → `state.set_results()`。空クエリは結果クリア（§4.6）。
   - `results + instant` → `engine.get_instant_commands(filter_name)` → `set_results`（アイコンスキップ）。
   - `results + command` → 完全一致なら slash 実行（後述）、部分入力なら候補表示なし。
   - `folder` → ロード済みエントリを同期フィルタ（§6.3・表示名のみ・フォルダ展開時の検索方式）。
3. `state.results` を `ScrollArea` で描画、`state.selected` 行に `scroll_to_me`。
4. キーボード: ↑↓＝`on_arrow_up/down`（clamp）、→←＝`on_arrow_right/left`（folder・後述）、Enter/クリック＝起動、Escape＝ラダー。
5. `compute_window_height(should_show_results, max_results, ...)` を算出、前回と異なれば `window.set_size()`。

**search は debounce する**（決定7）: query 状態は毎打鍵で更新（描画・`interp()`/`view_kind()` 導出）するが、`engine.search()` 実行は leading（バースト先頭で即時）+ trailing 50ms、instant fetch は 30ms trailing のみ。driver が `last_input_at` を持ち trailing 発火のため `request_repaint_after(50ms)` を積む。判定述語は純関数でユニットテスト。

### folder の async（唯一の staleness）

- `on_arrow_right()`：選択中がフォルダなら `state.begin_folder_request(dir) -> FolderToken`（`folder_gen += 1`）を発行し、driver が thread spawn（`capture_folder_list_context` → `read_dir_entries` → `finalize_folder_list`）。結果を `(token, entries)` で channel へ。
- 毎フレーム `try_recv`。届いたら `state.apply_folder_result(token, entries) -> bool`。**token が現行 `folder_gen` と不一致なら false（stale 破棄）**。Escape/←/打鍵でモードが動くと `folder_gen` が進み、遅れて届いた旧フォルダ結果は現ビューを轢かない。
- ロード中 UI: 直前フレームの結果を保持（フリット無し）。列挙失敗は単一エラー行（§6.6・Enter 無効）。
- `on_arrow_left()`：folder 中は親へ（`compute_parent_dir`・ルート/UNC 終端で無反応）。通常検索中の ← は選択項目の親を展開して folder モードへ遷移（§6.1）。どちらも新 `folder_gen` を発行。

### アイコン（SU4 の seam）

結果行はアイコンスロットを持つが、SU3 は名前 + 淡色パス + プレースホルダのみ描く。SU4 が実アイコン抽出 + LRU + 非同期バッチを埋める。`truncatePath`（Canvas 計測）は移植せず egui galley の省略に委ねる（instant モード中はスロット非表示＝§19.5 アイコンスキップ）。

## 描画と IO

- **結果行**（§4.8・`ResultRow.tsx` 相当）: `[アイコンスロット] 名前  ·  淡色パス  [フォルダバッジ]`。ホバーは視覚のみ（selected 不変）、シングルクリック＝起動、ダブルクリック＝選択更新（起動しない）。**選択・クリックは行 index で参照**（パス文字列を使わない・将来 tool でパス非一意・ui ルール踏襲）。
- **キーボードナビ**: `ScrollArea` + `selected` index、選択行 `scroll_to_me`。↑↓ clamp。マウスとキーボードは干渉しない（ホバーで selected を変えない）。
- **動的ウィンドウ高さ**（§4.5・§4.7）: `should_show_results` × `max_results` から `compute_window_height`。結果非表示なら 52px、表示なら 52 + rows×行高 + padding。**show 順序**（高さ確定→`position_on_target_monitor`（クランプは窓サイズ依存）→show）を SU2 の `show_egui_main` と接続——SU2 は 52px 固定だったが SU3 で高さが動くため、show 時の初期高さ確定を driver が行う。
- **IME**: SU1 runtime が preedit/候補/確定を softbuffer 上で処理済み。view は egui `TextEdit` を使うだけ。instant/folder フィルタは確定済みテキストに対して働く。
- **空クエリ**（§4.6）＝結果非表示、**indexing overlay**＝構築中を表示。ただし instant/folder は構築中でも結果表示（§19.7・§4.7）。

## slash / instant モード

- **slash**（§15）: `interp()==command`。完全一致で即実行（Enter 不要・§15.2）。`/o`＝設定起動、`/s`＝インデックス再構築、`/q`＝終了、`/r`＝直近履歴表示。driver が既存操作へ写像（`/r` は `engine.recent_history()` を注入して**留まる**＝選択・起動可、他は fire-once + クエリクリア）。folder 中は slash 無視で通常フィルタ扱い（§15.4）。
- **instant**（§19）: `@`prefix で `interp()==instant`。`get_instant_commands(filter_name)` フィルタ・`execute_instant_command(name, query)` 実行、アイコンスキップ（§19.5）、←→無効（§19.7）、Shift+Enter=通常 Enter（§19.6）。**tool-selection が defer なので通常結果の Shift+Enter も SU3 では通常 Enter**（tool メニュー無し）。実行後クエリクリア + hide（§19.6）。prefix は config live-read（bootstrap + `instant-prefix-changed` 相当を AppState から都度読む）。

## milestone（1 spec・段階実装、各 green でコミット）

順に積む。背骨（`SearchState` + 2 軸 + view-stack）は M1 で建て、folder/instant/slash が差さる。

- **M1 core**: `SearchState` 骨格（results 軸のみ）+ `interpret`/`compute_window_height`/`should_run_search` 純関数 + 描画（検索バー・結果リスト・行）+ 同期 search + **debounce（leading+trailing 50ms）** + ↑↓ナビ/選択/scroll 追従 + Enter/クリック起動 + hide + 空クエリ + indexing overlay + 動的高さ/show 順序。**G-RESIZE / G-SYNC / G1 をここで接地。**
- **M2 folder**: →←/親ナビ（`compute_parent_dir`）+ Escape 復帰（§6.4）+ async folder 読み（`folder_gen` token staleness）+ folder filter（§6.3）+ Enter で explorer + 列挙失敗行（§6.6）。view-stack に `Folder` 種追加。
- **M3 instant+slash**: `@`instant（filter/exec/skip icon/ガード・**30ms trailing debounce**）+ slash（`/r /o /s /q`）+ Escape ラダー完成（instant/command 解除段）。

## テスト計画

- **純粋核ユニット**（egui/Win32 非依存）:
  - `interpret` 表（plain/command/instant・空 prefix で false・trimStart 規則）。
  - `compute_parent_dir`/`clamp_selected_index`（ドライブルート・UNC 終端）。
  - `compute_window_height`（表示/非表示・行数・トースト有無）。
  - `should_run_search`（debounce 判定・決定7）: バースト先頭で即時（leading）・trailing は elapsed≥50ms で発火・連打中は抑止。
  - `SearchState` 遷移: Escape ラダー 3 段・folder token stale（begin→begin→apply(旧tok)=false）・view_kind/interp 導出・selected clamp・空クエリで結果クリア・instant/command での search 抑止。
- **trace スモーク**（Win32 依存ゆえユニット前提にしない・SU2 と同型）: `SNOTRA_EGUI_MAIN=1` で search→結果→起動・folder 往復（→展開/←親/Escape 復帰）・instant/slash 実行・動的高さ変化・`msedgewebview2.exe` 子孫 0。
- **flag OFF 完全不変（G1）**: `SNOTRA_EGUI_MAIN` 未設定で既存テスト + `smoke:startup` + `e2e:tauri` 緑。IPC 追加が WebView2 経路を触らない。
- **font-first カナリア継承**（SU2）: 実 `SearchWindowView::setup` の jp_font index 0 を維持（結果リストは製品規模の混在テキストゆえ #399/#579 の被覆 AA を製品規模で目視 parity）。

## 受け入れ条件（SU3）

1. インクリメンタル検索の不変条件（§4.2.1）・優先順位（§4.3）・結果表示制御（§4.7）・空クエリ（§4.6）・マウス操作（§4.8）が **IPC なし**（直 Engine）で WebView2 経路と一致。
2. フォルダ展開（§6: 右/左/親/Escape 復帰/Enter/列挙失敗）が folder token staleness 込みで一致。
3. スラッシュコマンド（§15: `/o /s /q /r`・完全一致即実行・folder 中無視）が一致。
4. インスタントコマンド（§19: `@`検出・前方一致・実行・アイコンスキップ・←→無効・Shift+Enter=Enter）が一致。
5. **flag OFF で WebView2 挙動・E2E 注入が完全不変（G1）**。IPC コマンドを削除しない。
6. 状態遷移・純関数（`interpret`/folderNav/height/Escape ラダー/folder token）がユニットテストされ、view は薄い driver に保たれる。
7. `cargo clippy -p snotra --all-targets` 緑・`src-tauri` テスト緑・`msedgewebview2.exe` 子孫 0。動的リサイズが目視で parity（G-RESIZE）。

## リスク

- **被覆 AA テキスト品質**（#399/#579）: 結果リストは製品規模の混在テキスト。font-first(index 0) 維持を実 setup 駆動テストで固定（SU2 継承）。
- **同期 search のフレーム内コスト**（G-SYNC）: debounce（決定7）で連打は束ねるが、trailing 1 回の `engine.search()` が大インデックスでフレームを詰まらせないかは残余リスク。詰まれば間隔調整か `spawn_blocking`（folder と同じ token）へ。
- **動的高さ × 位置クランプの結合**（G-RESIZE・SU2 申し送り）: 展開/折りたたみで位置ずれ・ちらつきしないよう show 順序を M1 で接地。悪ければ present タイミングを単一ウィンドウ内で直す。
- **folder async の stale 破棄**: token 照合を誤ると旧フォルダ結果が現ビューを轢く/正当な結果が捨てられる。`SearchState` ユニットテストで begin/apply の世代照合を固定。

## スコープ外（SU3 では触らない）

- **tool-selection / カスタムオープナー（§18）**: spine の view-stack は加算可能に保つが frame は建てない。**独立サブユニット SU3.5**（SU3 完了後・SU4 と並行しうる）で `tool` 種を view-stack に加算し §18 の parity を取る。flip（SU7）前に SU3.5 を通す。
- アイコン実体（SU4）・updater（SU5）・config 反映/終了保存（SU6）・切替/配布（SU7）。
- **IPC コマンドの削除**（SU7）。SU3 は egui 経路で bypass するだけ。
- ルート `CLAUDE.md`/`AGENTS.md` 等の規範文書。

## 否定の知識（なぜ却下したか）

- **2 ウィンドウ分割（検索窓 + 結果窓）を SU3 では却下**（brainstorm 2026-07-22）。高さ調整は確かに楽になる（検索窓 52px 固定・結果窓は中身に合わせる）が、(a) 文書化された §4.7（単一ウィンドウ）からの乖離＝移行に仕様変更を折り込む、(b) WebView2 ベースラインと綺麗に diff できず parity 検証の軸が消える、(c) softbuffer は CPU present ゆえ 2 枚の present タイミングずれが継ぎ目/隙間/z-order ちらつきを生み「外観維持」flip 基準を難しくする。**移行の内側でウィンドウ・アーキテクチャを再設計しない**が主因。高さの複雑さは消えず「結果窓を検索窓へ糊付けする責務（move/show/hide/モニター変更追従・同幅・直下・非アクティブ化）」へ移動するだけ、という点も効いた。過去の「統合」先例は根拠として弱い——元の統合理由（`is_main_foreground` のプロセス ID 比較）は WebView2 固有で egui/softbuffer(in-process) では再発しないため。**2 ウィンドウは flip（SU7）後の別 spec で merits で再検討する**——egui が唯一経路になり parity 制約が外れ、かつ元の統合理由も消えている、本来の適所。単一ウィンドウ内で動的リサイズが目視で悪い場合のみ SU3 内で再考が開くが（G-RESIZE）、その第一の修正も present タイミングであって 2 枚目ではない。
- **並行 primitive（`searchLane`/`activationLane`/`latestRun`/`exclusive`）の移植を却下**。これらは IPC 往復の out-of-order/in-flight Promise 調停のためだけに存在する。直 Engine の同期呼びでは out-of-order も二重 in-flight も生じない（起動時 hide で二重起動も消える）。移植は不要な機構を egui 経路へ持ち込む。→ 同期 search + folder のみ token staleness。
- **状態を snotra-core へ押し下げるのを却下**。selected index・view-stack・Escape ラダーは純然たる UI 関心で、core の責務（インデックス/検索/履歴）と混ざる。core に置くのは真に汎用な純関数（`interpret` 等）だけ。状態機械は src-tauri の純粋 `SearchState`。
- **spine に `tool` 軸の実体を今建てるのを却下**（SU2 の否定の知識の踏襲）。SU3 で live 到達しない `tool` frame を建てるのは「live 到達不能な状態を建てる」過剰設計。view-stack を `Vec<ViewFrame>` にして加算可能にだけしておく。
