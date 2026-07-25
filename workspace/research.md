# research: #663 — /race-check スキルを egui worker 並行モデルへ全面改訂する

## 1. issue の要約

`/race-check`（`.claude/skills/race-check/SKILL.md`）は SolidJS フロント前提で書かれている。#532 SU7 のフロント撤去でスキルが参照する対象（`dispatchQueryInput`・`searchLane`/`activationLane`・`latestRun.isStale()`・SolidJS シグナル）は**すべて消滅した**。PR #662 で冒頭に読み替え注記（現行 SKILL.md:11）を入れただけの暫定状態にある。本 issue はスキル本文を現行の並行モデル（Rust worker スレッド + channel + フレーム drain）へ書き直す。

**陳腐化ではなく構造的な機能停止である。** スキルの Step 1 は「関数内の各 `await` 地点を列挙する」から始まるが、対象コードベースの `.await` は **src-tauri 全体で 2 箇所しかない**（実測、下記 3.5）。UI 層の並行性は `std::thread::spawn` + `std::sync::mpsc` + `update()` 内 drain で構成されており、Step 1 が空集合を返して以降の Step 2〜5 が一つも発火しない。

エージェント設定（skill）の変更ゆえ、ルート `CLAUDE.md` 最重要ルール 3 および issue 本文により**実装前に合意が必要**。本 `/start-issue` は plan までで停止する。

## 2. 現行の並行モデル（改訂後スキルが対象とすべき機構）

すべて実測（`file:symbol` で示す。行番号は挿入でずれるためシンボル名を正とする）。

### 2.1 実行文脈は 2 種類だけ

| 文脈 | 実体 | UI 状態への触り方 |
|---|---|---|
| イベントループスレッド | 各 view の `update()`・Tauri `listen` コールバック・`Window::on_window_event` | 直接読み書き（`&mut self`） |
| worker スレッド | `std::thread::spawn`（folder / launch / icon）・`tauri::async_runtime::spawn`（updater 2 箇所） | **直接触らない。channel send か managed state の `Mutex` 経由** |

窓は main / results の 2 つで、**両方とも同一イベントループスレッドで `update()` が走る**（`snotra-egui-runtime/src/runtime.rs` が窓ごと状態を `HashMap` 管理）。ゆえに 2 窓間の共有（`ResultsShared`）の `Mutex` は同一スレッドの順次アクセスが基本で、真の同時アクセスは worker 経路にしか無い。

### 2.2 worker → UI の経路（**表 2.2a が channel 経由の 4 経路、2.2b が channel を経由しない経路**）

> **訂正（plan-review Step 2 の実測による）**: 初稿は「worker → UI は 4 経路だけ」と書いたが**不成立**。`app.listen` のコールバックは **emit した呼び出し元スレッド上で同期実行される**（一次資料: `tauri-2.11.4/src/event/listener.rs` の `emit_filter` が別スレッドへ dispatch せず `(callback)(...)` を直接呼ぶ）。ゆえに Win32 メッセージループスレッド・config_watcher（notify）スレッド・index build スレッドからの emit は、**そのままそのスレッドで UI/managed state を触る**。2.2b がその一覧。

#### 2.2a channel 経由の 4 経路

| 経路 | 送信側 | 受信側（drain） | staleness 機構 |
|---|---|---|---|
| folder ナビ | `view.rs::spawn_folder_load`（per-nav spawn・`folder_tx` は **view 寿命の共有 channel**） | `view.rs::update()` の `folder_rx.try_recv()` while ループ | **世代 token**（`SearchState::folder_gen` / `accept_folder_result`）。滞留を全 drain し、token 一致の**後着で上書き**して最新のみ適用 |
| 起動（launch/tool/instant） | `view.rs::start_launch` の `std::thread::spawn`（**per-launch channel**） | `view.rs::drain_launch` | **チャネル所有権**。`LaunchInFlight` が `rx` を所有し、`launching = None` で `Receiver` ごと drop → 遅着 send は `Err` で自然消滅。**token を持たない**（`view.rs` の `LaunchInFlight` doc が「folder のパターンをコピーするな」と明示） |
| アイコン抽出 | `results_view.rs::spawn_icon_load`（共有 channel `icon_tx`） | `results_view.rs::update()` の `icon_rx.try_recv()` while ループ | **なし（path キーで無害）**。代わりに `icon_pending: HashSet` が**重複 spawn ガード**（thread pileup 対策） |
| updater（唯一の async） | `mod.rs::spawn_update_check` / `view.rs::spawn_install`（`tauri::async_runtime::spawn`・`.await` あり） | なし（`UpdaterUiState` の `Mutex` を worker が直接書き、view が毎フレーム読む＝**level-triggered**） | なし（phase 上書き） |

**この表が示す軸**: 旧スキルは「世代カウンタ検証漏れ」という 1 つの型しか語彙を持たなかったが、現行には**世代 token / チャネル所有権 / 重複 spawn ガード / level-triggered 状態**の 4 型がある。どれを使うかは「channel が共有か per-request か」で決まる（`LaunchInFlight` doc の判定）。**ただし「4 型で網羅」ではない**——5 型目として**アプリ全体スコープの単調カウンタをフレーム毎に diff する**型がある（`AppState.index_generation` ↔ `view` の `last_seen_index_generation` + 純粋述語 `needs_index_refresh`。channel を持たない）。改訂稿では「代表 4 型 + 網羅ではない旨」で書く。

#### 2.2b channel を経由しない worker → UI 経路（初稿の脱落・実測で発見）

| 経路 | 実行スレッド | 触るもの | 備考 |
|---|---|---|---|
| `hotkey-pressed` listener（`main.rs`） | **Win32 メッセージループスレッド**（`platform/mod.rs` が emit） | `EguiShellState.hotkey_generation` の bump・`show_egui_main` / `hide_egui_main` を**直接呼ぶ** | `HotkeyPlan::ShowAfterAltRelease` はさらに `std::thread::spawn` し、その中でも `show_egui_main` を直接呼ぶ（世代一致チェック付き） |
| `hotkey-registration-failed` listener（`egui_shell/mod.rs`） | **notify（config_watcher）スレッド** | `pending_hotkey_failure`（`Mutex`）へ書き込み | wake しない（`config-applied` に委ねる意図的な非対称） |
| `initial-hotkey-failed` listener | **Win32 メッセージループスレッド** | 同 `Mutex` + `show_egui_main` + `wake_main` | 「格納 → show → wake」の順序不変条件 |
| `config-applied` / `indexing-started` / `indexing-complete` listener | config_watcher スレッド / index build スレッド | `wake_main` のみ（値を運ばない） | 「値を運ばない」が benign 性の load-bearing 前提 |
| 設定サイドカー監視スレッド（`commands/window.rs`） | 専用 `std::thread::spawn` | `main.set_always_on_top(true)` / `results.set_topmost(true)` を**直接呼ぶ** | channel も drain も経由しない |
| 背景再スキャンスレッド（`main.rs`・`snotra-index-rescan`） | 専用 spawn | `icon::invalidate_icon_cache`（managed state）を直接呼ぶ | channel も repaint も経由しない |

`ResultsWindow.visible` が `Cell` ではなく `AtomicBool`（`SeqCst`）なのはこの cross-thread 前提ゆえ（`results_window.rs` の doc が「別スレッドの hide と競っても」と明記）。

### 2.3 wake 義務（送信は次フレームを生まない）

ランタイムはイベント駆動（`repaint.rs`・`RedrawRequested` 待ち）で、**通常フレームは勝手に回らない**。ゆえに:

- **worker は送信の後に `egui_ctx.request_repaint()` で誰かが次フレームを起こす**（folder / launch / icon の 3 経路すべてが実施）。**「送信のたびに」ではない**（訂正）——`spawn_icon_load` は複数 `send` をループで撃ち、`request_repaint()` は**ループの外で 1 回**。最後の repaint が全件ぶんを起こすため実害は無いが、全称主張としては不成立。改訂稿は**「送信後に次フレームを起こす者がいるか」**という形で書く（回数ではなく到達性が要件）
- **フレームの paint より後・遅延 dispatch・クリックハンドラで状態を変えたら同じく repaint が要る**（`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」・toast dismiss で実測されたバグ = PR #647 e746826）
- 外部スレッド・別窓からは `egui_shell::wake_main` / `wake_results`（`WindowWaker`）。**managed state に `egui::Context` の clone を置いてはならない**（`snotra-egui-runtime/CLAUDE.md` 不変条件・#671 PR D で実在した破れ）

### 2.4 hidden 中は `update()` が走らない

SU5 の要石（実測）。帰結:

- `request_repaint_after` による時限処理（`LAUNCH_TIMEOUT` の 4 秒・notice 期限）は**可視中しか効かない**
- hide を跨ぐ in-flight 状態は **reset-on-show の backstop とセット**で設計する（`view.rs::update()` の `reset_pending` 消費ブロックが `state.reset()` / `folder_cache` / `folder_error` / `instant_rows_query` / `search_debounce` / `launching = None` / `notice.clear()` / results サイズガードをクリアする）
- **ただし「in-flight はすべて reset-on-show でクリアされる」は不成立**（訂正）。例外が 2 つある: (i) **アイコンの in-flight**（`icon_pending` / `icon_textures` / `icon_missing`）は `ResultsView` 側にあり `reset_pending` を消費しない——明示クリアではなく「次に rows が非空になったフレームでの自然収束」に依存する。(ii) `pending_hotkey_failure` は**意図的に対象外**（hidden 中の失敗を次 show で見せるため）。**「クリアされる」ではなく「クリアされるか、されない理由が書かれているか」を問うのが正しい検査**
- managed state（`ResultsShared` の `snapshot` / `clicked`）も reset-on-show の視野に**入っていない**——reset ブロックは view-local だけを一掃する
- **drain と reset の順序が不変条件**: `drain_launch` は `reset_pending` 消費の**後**に呼ぶ（前だと show 直後フレームで stale `Ok` が reset より先に処理され、再 show した窓を `emit_hide` で撃つ）

### 2.5 同一フレーム内の live-read 規律（#673）

`read_visual` が返す `VisualSnapshot` を `update()` 冒頭で 1 回だけ取り、以降はそれを読む。**同じ値を後段で config から読み直さない**——間に `config_watcher` の適用が挟まると同一フレーム内で新旧が混ざる（`request_icons_for_results` の doc が「枠は描いたのに抽出は走らない 1 フレーム」を明記）。**snapshot を `self.` へ保持してもならない**（毎フレーム live-read が config 変更の反映経路そのもの）。

これは狭義のデータ競合ではないが、**「読みの時点がずれることで状態が食い違う」という点で drain 窓と同型**であり、race-check の守備範囲に入れるのが妥当。

### 2.6 2 窓間の一方向フロー + クリック逆流

- main の `update()` が `RowsSnapshot`（rows / selected / generation / settled）を `ResultsShared.snapshot`（`Mutex`）へ**差分時のみ**書き、`wake_results` する（edge-triggered）
- results の `update()` はそれを描くだけ。行クリックは `ResultsShared.clicked`（`Mutex<Option<usize>>`・last-wins）へ積んで `wake_main`
- main は次フレームで `clicked.lock().take()` して起動処理（**遅延 dispatch**）

`snapshot_generation` は「結果の総入れ替え」を results 側の scroll gate へ伝えるための世代（`selected` の値だけでは総入れ替えを検出できない）。

### 2.7 その他の共有状態

- `AppState.main_visible`（`AtomicBool`）: hide/show の順序不変条件を持つ（`hide_egui_main` は results.hide() の**前**に false、`show_egui_main` は `show()` の**後**に true。どちらも「main が可視でない期間に true と読ませない」向き）
- `AppState.index_generation`（`AtomicU64`）: view が `last_seen_index_generation` と比較して再検索（bool エッジでないのは started/complete が 1 フレームに合流するとパルスが見えないため）
- `EguiShellState.hotkey_generation`（`AtomicU64`）: alt 解放待ち show の世代。hide が bump して保留 show を無効化
- `EguiShellState.hide_pending` / `reset_pending`（`AtomicBool`）、`pending_hotkey_failure`（`Mutex`）: listener が立て view が消費（**格納 → show → wake の順序不変条件**あり）
- `Engine`（`Mutex`）: worker がロックを取る。**ロックは I/O をまたいで保持しない**（`spawn_folder_load` の capture → ロック外 read_dir → ロック内 finalize の 3 段）

## 3. 関連コード（実在確認済み）

| ファイル | 本 issue での役割 |
|---|---|
| `.claude/skills/race-check/SKILL.md` | **改訂対象**（唯一のコード変更対象） |
| `AGENTS.md`「条件別チェック」表（`async 関数を追加/変更` の行） | トリガー述語を変えるなら同 PR で更新 |
| ルート `CLAUDE.md`「利用できるスキル」表（`/race-check` 行） | 同上（説明文 + 呼び出し例） |
| `.claude/skills/start-issue/SKILL.md` Step 5a の表 | 同上（`計画が async 関数の追加・変更を含む`） |
| `.claude/skills/retrospective/SKILL.md` | スキル名の列挙のみ（述語を書いていない → **変更不要**） |
| `.claude/skills/implement/SKILL.md` | `AGENTS.md` の表へ委譲（述語を複製していない → **変更不要**） |
| `.claude/rules/src-tauri.md`「トリガー → 検査」節 | race-check の行が**無い**（`snotra-core.md` には `/cache-check` の行がある）。rules は自動配送される唯一の機構的起動経路 → 決定 4 |
| `.claude/agents/code-reviewer.md`（2d・Phase 3） | 同種の SolidJS 残骸（`await` / `.then()` / `onCleanup` / `createMemo`）。スキル名を含まず概念でしか拾えない → **スコープ外・follow-up** |
| `src-tauri/src/egui_shell/view.rs` | folder / launch worker・drain・snapshot 発行（例示の出典） |
| `src-tauri/src/egui_shell/results_view.rs` | icon worker・`ResultsShared`・クリック逆流 |
| `src-tauri/src/egui_shell/mod.rs` | wake 経路・show/hide 順序・updater async |
| `src-tauri/src/egui_shell/search_state.rs` | `folder_gen` / `accept_folder_result` / `needs_index_refresh`（純粋核） |
| `src-tauri/src/egui_shell/results_window.rs` | `visible` が `AtomicBool`（cross-thread 前提の証拠） |
| `src-tauri/src/main.rs` | `hotkey-pressed` listener（Win32 スレッドから show/hide を直接呼ぶ）・背景再スキャンスレッド |
| `src-tauri/src/platform/mod.rs` | hotkey / initial-hotkey-failed の emit 元（Win32 メッセージループスレッド） |
| `src-tauri/src/config_watcher.rs` | `hotkey-registration-failed` / `config-applied` の emit 元（notify スレッド） |
| `src-tauri/src/commands/window.rs` | 設定サイドカー監視スレッド（`set_always_on_top` / `set_topmost` を直接呼ぶ） |
| `src-tauri/src/state.rs` | `index_generation`（5 型目の staleness 機構） |
| `snotra-egui-runtime/{CLAUDE.md,src/repaint.rs}` | `WindowWaker` と repaint 配送の不変条件 |
| `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」節 | wake 義務・hidden 中 `update()` 不走行の正本 |
| `.claude/rules/safety-nets.md` | 本改訂に適用される検証手順（規範のフォールトインジェクション） |
| `docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md` | 「#663 未了ゆえ `/race-check` は参照しない」と明記（本改訂の動機の裏付け） |

### 3.5 実測した数値

```
grep -rn "\.await" src-tauri/src --include=*.rs | wc -l   → 2
（内訳: egui_shell/mod.rs::spawn_update_check、egui_shell/view.rs::spawn_install。
  どちらも tauri::async_runtime::spawn(async move { ... }) の中）
```

→ **await 軸は削除ではなく降格**（1 セクションに縮小して残す）。消すと updater / install の並行性がスキルの視界から落ちる。

他 crate の worker（スキルの適用範囲に含めるか要判断）:
- `snotra-settings/src/tabs/{backup.rs,common.rs}`: `std::thread::spawn` + rfd ファイルピッカー（別バイナリ・egui 直）
- `snotra-core/src/indexer.rs`: **spawn しない**（タスクとして返し所有者 `src-tauri` が spawn する）

## 4. 既存パターン（再利用できるもの）

- **姉妹スキルの構成**: `/state-check`・`/cache-check`・`/symmetric-check` は「背景 → Step 群 → 判定マトリクス → 出力（根拠の規律）」の型を共有する。改訂後もこの外形は保つ（読者の期待とスキル間の一貫性）
- **`.claude/rules/src-tauri.md` の #588 規律**: 位置は「ファイル名・行で断定せず**見出し名・シンボル名で grep**」。旧スキルが死んだ原因（消えた識別子のハードコード）に対する既定の答えが repo 内に既にある
- **AGENTS.md「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」**: wake 義務・hidden 不走行の正本は `src-tauri/CLAUDE.md`。スキルは**検査手順**を持ち、事実は参照する

## 5. 技術的制約

- **Win32 の部分的非同期**: `SetForegroundWindow`/`SendInput` は非同期側面を持ち、`show_egui_main` は `SendMessageTimeoutW(WM_NULL, ..., 100)` で同期待ちしてから IME オフ・Alt-up を撃つ（`src-tauri/CLAUDE.md` に記録済み・スキルからは参照で足りる）
- **`.md` 編集には PostToolUse hook 検査が割り当てられていない**（ルート `CLAUDE.md` フック節・#497）。沈黙は「何も走らなかった」であって合格ではない。`npm run governance:check`（スキル表・参照実在）を明示的に回す必要がある
- **skill の frontmatter `allowed-tools` はインライン実行を拘束しない**（実測・ルート `CLAUDE.md`）。`Agent` が無いことを根拠に「委譲しない」と推論できない
- 規範（ドキュメント・スキル）は実行して測れない → `safety-nets.md` の「回避しようとする読者」によるフォールトインジェクションが唯一の検証手段。**読者は 2 クラス必要**（手を抜く読者 / 規則を全部守る読者・#488）、**1 巡では終わらない**（#489）、**停止条件を先に決める**

## 6. 未解決の疑問（plan.md で決める / 合意を要する）

1. **トリガー述語をどこまで広げるか**（要合意・最大の設計判断）。現行 `async 関数を追加/変更` は発火しない述語である。候補は (A)「worker spawn・channel・共有状態を追加/変更したとき」へ全面置換、(B) A に加えて async/await も明示的に列挙して残す。→ plan.md で推奨とともに提示
2. **適用範囲を `src-tauri/egui_shell` に限るか、`snotra-settings` の worker も含めるか**
3. **例示の耐久性**: 現行シンボル（`folder_rx` / `icon_pending` / `LaunchInFlight` / `snapshot_generation`）を表に書くと、旧スキルと同じ「識別子が消えたら死ぬ」脆さを再生産する。→ 機構カテゴリを名前にし、例示は 1 カテゴリ 1 件・**例示と明示ラベル付き**にする方針を plan.md で確定する
4. **`.claude/rules/src-tauri.md` にトリガー行を足すか**（plan-review Step 2b の指摘・plan.md 決定 4）。rules は**対象ファイル編集で自動配送される唯一の機構的起動経路**であり、行が無い限りスキルは「思い出したときだけ」起動する

### 面積 ratchet の実測（決定の制約条件・`npm run governance:check` の evidence 行）

```
恒久規範 常時ロード 216/216 行・rules 150/173 行
（常時ロード = ルート CLAUDE.md + AGENTS.md の合計行数）
```

→ **常時ロードは余裕ゼロ**。`AGENTS.md` / ルート `CLAUDE.md` は**行数を増やさない置換**（既存行の文言差し替え）に限る。1 行でも増やすなら他所を削るか `LINE_BUDGET` を理由コメント付きで更新する必要がある。**rules は 23 行の余裕があり、決定 4 の 1 行追加は通る**（実測）。
</content>
