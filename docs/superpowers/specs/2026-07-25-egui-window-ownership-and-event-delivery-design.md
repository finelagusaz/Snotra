# egui 窓所有と配送経路の設計（#671 / #673 / #652 機構整備）

- 日付: 2026-07-25
- 対象 issue: #671（従属窓の回避策と ctx 複製）/ #673（毎フレーム config 読み・level-triggered wake）/ #652 の機構整備
- 対象 crate: `src-tauri/`（主）・`snotra-egui-runtime/`（PR D のみ）
- 前提サイクル: #532（softbuffer 移行）完了後・#646（2 窓化）PR1/PR2 マージ後

## 1. この spec が答えている問い

#646 PR2 で 2 つ目の egui 窓（`results`・`focusable(false)` の従属窓）が入った結果、shell に 2 種の回避策が生まれた（#671）。同時に #673 が毎フレームの重複 config lock と level-triggered wake を検出した。これらの共通の背景は「**窓が 1 つ、描画箇所が 1 つのうちは無くて済んでいた所有権の所在が、2 つ目で表面化した**」ことである。

本 spec は、その所有権をどこに置くかを決める。

## 2. 却下した案と、その根拠（否定の知識）

**当初案 I-1: runtime に `WindowKind{Primary, Subordinate}` と `EguiWindowHandle{show, hide, set_topmost, wake}` を導入し、両窓の可視性を runtime 管理下に置く。** 5 レンズのレビュー（tao 越境 / 対称性 / 配送 / スコープ / 敵対）と実測により**取り下げる**。根拠は以下であり、いずれも実測で確定している。

### 2.1 `EguiWindow.visible` は製品コードで恒真 true である

`RuntimeFrame::hide_window()` / `close_window()` の呼び出し元をリポジトリ全体で grep した結果、**製品コードにゼロ**（`snotra-egui-mvp/src/main.rs:185` の `close_window()` のみ。当 crate は #660 で削除予定）。`view.rs:1006` は `drag_window()` のみを使う。

したがって `snotra-egui-runtime/src/runtime.rs:276-278` の `if !visible { return }` と `runtime.rs:265-271` の `Focused(true)` による `visible` 復帰は、**今日 production では到達しない**。

**帰結**: I-1 が「構造的に閉じる」と称した codex #4（空白窓）は今日発生しない。I-1 は恒真だったフラグを状態機械に変え、そのうえで新たに生じる穴を塞ぐ設計だった。

### 2.2 `visible = true` は paint を許可するが惹起しない

wake 経路は `RepaintScheduler` スレッド → `proxy.send_event` → イベントループの非同期 3 段であり、`SW_SHOW` が先に勝つ。同期 paint への退避も不可能——listener / イベントループコールバック内ではメッセージポンプが停止している（`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」の既存不変条件）。「show が同期で `visible` を立てるから安全」という I-1 の正当化はこの経路では成立しない。

### 2.3 `hide()` の public 化は hide に括り付いた 4 副作用を剥がす

現在 main の `window.hide()` は `hide_egui_main`（`src-tauri/src/egui_shell/mod.rs:436-461`）内の**私的な 1 行**（`:444`）であり、構造的に次と不可分:

1. `hotkey_generation.fetch_add`（保留中の alt 解放待ち show を無効化・`mod.rs:440`）
2. `save_placement_relative`（save-on-hide・`mod.rs:443`）
3. `main_visible.store(false)`（`mod.rs:452`）
4. `trim_idle_working_set`（`mod.rs:459`）

`EguiWindowHandle::hide()` はどこからでも呼べる public API になり、この 4 つは付いてこない。とくに (1) が抜けると `main.rs:411-425` の alt 解放待ちスレッドが「隠したはずの窓」を後から表示する。

**今日の安全性は `.hide()` が private な 1 行であることに由来しており、規約に由来していない。** I-1 はその構造的保証を規約へ格下げする方向を向いていた。これは本リポジトリの選好（文書契約より「表現不能にする」構造）の逆である。

### 2.4 初回フレームのウォームアップを失う

両窓の builder は `.visible(false)`（`mod.rs:186`, `:202`）だが、`EguiWindow::new` は無条件に `visible: true`（`runtime.rs:253`）。この食い違いのため `attach_pending_windows` の `scheduler.request(ZERO)`（`runtime.rs:219`）が**非表示のまま初回フレームを走らせ**、font atlas とレイアウトを温めている。`visible` を正本にして `false` で種を播くと、この初回描画がホットキー → show のレイテンシ敏感な経路へ移動する。

### 2.5 `Focused(true)` arm は現行の repaint トリガである

`show_egui_main`（`mod.rs:347-432`）は `request_repaint` を一切呼ばない。main の show 後の初フレームは `set_focus()`（`mod.rs:384`）→ `Focused(true)` → `on_window_event` が true → `scheduler.request(ZERO)`（`runtime.rs:161-165`）が唯一の起動源である。

この arm については 2 つの性質を区別すること: `self.visible = true` の代入は §2.1 より no-op だが、**同じイベントが起こす repaint 要求は live** である。arm を「dead code だから」と外してはならない。

### 2.6 目的（家訓の撤廃）は、どの案でも達成できない

`app.get_window("results").hide()` は I-1 でも後述の A′ でもコンパイルが通り、実行時に黙って no-op する（tao の `WindowFlags::VISIBLE` が raw show 後も false のままであるため——#671 本文が根本原因として記述している当のもの）。`tauri::Manager::get_window` は `AppHandle` を持つ誰からでも呼べる。

**footgun は表現不能にできない。crate 境界の向こうへ移動するだけである。** したがって I-1 と shell 内 newtype の差は安全性ではなく費用だけになる。

### 2.7 検証手段が無い

`scripts/smoke-egui.ps1` は Alt+Q と Escape のみを注入し**文字を 1 つも打たない**ため、results 窓は全行程で一度も表示されない。results の show / hide / topmost / フォーカス非奪取の自動被覆は**ゼロ**。加えて `egui_show:done` は `show_egui_main` 末尾で必ず出るため、**描画が止まって白紙になっても smoke は緑**である。

`egui_shell` にヘッドレステスト基盤は無い（`egui_kittest` は `snotra-settings` のみ・#440 で導入済み）。実機 GUI smoke は #532 SU7 以降未実施。

**#673 項目 2（level→edge 化）を「検証手段が無いから」外すなら、同じ理由は I-1 により強く当てはまる**（触る面積がより広い）。この非対称は I-1 の取り下げで解消する。

### 2.8 その他の却下

- **listener テーブル化**: `.listen()` は 7 箇所（`egui_shell` 4 + `main.rs:375` / `:442` / `:478`）。テーブル化すると 3 行が純粋・2 行がエスケープハッチ・3 本が圏外となる**部分被覆**になる。#652 の gap は「行が無い」形だったため、テーブルはこの事故クラスに無力。しかも移した先の規約は「新イベントを足したらテーブルにも足す」であり、#652 で守られなかった規約と同型である。
- **`wake_after`**: `request_repaint_after` の呼び出しは `view.rs:407` / `:1127` / `:1186` / `:1346` / `:1482` の 5 箇所で、いずれも view 自身が `ui.ctx()` に対して呼ぶ。**外部からの遅延 wake の caller は現在ゼロ**。
- **`set_topmost` の runtime 移管**: 呼び出し元は設定サイドカーの起動 / 終了の 2 対のみ（`commands/window.rs:94` / `:99` / `:140` / `:143`）。持ち上げる利得が最も薄い。

## 3. 決定

### 決定 1: 可視性の runtime 移管は行わない

§2 のとおり。`snotra-egui-runtime` の `visible` / `Focused(true)` arm / `EguiWindow` 構造には触れない（PR D の Context 保持を除く）。

### 決定 2: `ResultsWindow` newtype が生 Win32 3 点セットと可視フラグを同時に所有する（PR A′）

`src-tauri/src/egui_shell/` に `pub(crate) struct ResultsWindow` を置く。

```rust
pub(crate) struct ResultsWindow {
    window: tauri::Window,        // 非公開。Deref は実装しない
    visible: std::sync::atomic::AtomicBool,
}
```

`Cell` ではなく `AtomicBool` である理由: managed state は `Send + Sync` を要求し、かつ `commands/window.rs:143` の topmost 復帰は **spawn したポーリングスレッド**から呼ばれる。可視性の読み書きはイベントループスレッドに閉じない。

- `Deref<Target = tauri::Window>` は**実装しない**（実装すると `.hide()` が生えて元の footgun が復活する）。
- 現行の `show_results_no_activate` / `hide_results` / `set_results_topmost`（`mod.rs:247-322`）を inherent method へ移す。`#[cfg(not(windows))]` フォールバックはそのまま移設する。
- サイズ・位置など results に必要な操作（`set_size` / `set_position`）も本型のメソッドとして公開し、shell が `get_window("results")` を書く箇所を `create()` 内の 1 回に減らす。
- `create()` が構築し、managed state へ載せる。

**`last_results_visible` を本型が吸収する。** 現在 results の hide は 2 経路（`mod.rs:449` の `hide_egui_main` と `view.rs:712` の `drive_results_window`）あるが、**フラグを更新するのは後者だけ**である。この非対称は今 reset-on-show（`view.rs:1039`）が後始末して閉じており、`view.rs:1035-1038` のコメントが事故の形（stale なまま残ると再 show 後に「既に visible」と誤認し show を skip し続ける）を明記している。

型が窓とフラグを同じ物として持てば、**2 つの hide 経路が同じオブジェクトを通る**ためこの非対称は構造的に消える。したがって `view.rs:1039` の `last_results_visible = false` は撤去する。

**`view.rs:1040-1041` の `last_results_height` / `last_results_width` は view に残す。これは意図的な分割である**——サイズガードは「冗長な `set_size` を避ける」性能上の都合であり、可視性は correctness のフラグである。同じ表層形（リセットされる 3 行）が 2 つの概念を担っているため、概念で分ける（`/simplify` が再統合しないよう本節を根拠とする）。

**得られないもの**: §2.6 のとおり `app.get_window("results").hide()` は依然書ける。本決定は表現不能化ではなく、**正しい経路を 1 つにし、誤った経路を書く動機を消す**ことを目的とする。

#### 追補（PR A′ 実装後・実機目視で発見）: 消した非対称は荷重を持っていた

上の「この非対称は構造的に消える」は**代償を伴った**。PR A′ を入れた実機で「Escape で main を閉じても results が残る」が再現した（実装者のセルフレビューでも独立レビューでも同じ機構を指摘・後者は Important 1）。

機構: main を hide しても `state.results()` は消えない（reset は show 側の `reset_pending` 消費でしか起きない）ため `show_results` は hidden 中も true のまま残る。hidden 中でも repaint 要求は飛ぶ（`config-applied` / `indexing-*` / updater 完了の `wake_view` は main の可視性を見ない）。その 1 フレームが `drive_results_window` を通ると results だけが最前面に取り残される。

**PR A′ 以前にこれを防いでいたのは、まさに本決定が「非対称」と呼んだ当のもの**である——view-local の可視フラグは `hide_egui_main` から到達できず stale な true のまま残り、結果として show を skip していた。**意図されない保護だったが、実効的な保護だった。**

したがって本決定の主張は次に訂正する:

> `ResultsWindow` は「**誰が** raw 操作を撃つか」を一点に集める。「**撃ってよい状況か**」は判定しない。後者は show 述語側のゲート（`layout::results_should_show` が `AppState.main_visible` を合流させる）が担う。

`hide()` の無条件化ではこの穴は閉じない（フラグが false になった後に drive が `show()` を撃つ構図が変わらないため）。`hide_egui_main` は `main_visible = false` を `results.hide()` の**前**に置く（後ろだと隙間のフレームが素通りする）。

**残余（狭めただけで閉じていない）**: `hide_egui_main` は `hotkey-pressed` listener 経由で platform スレッドから走るため、イベントループ側が `results_should_show` の判定を通過した後・`show()` の前に hide が挟まると、`SW_SHOWNOACTIVATE` が hide の後に撃たれうる。閉じるには show 経路をイベントループへ marshalling する必要があり、§7-5 の未決事項と同根である。

**教訓（機構の側）**: この回帰を `smoke:egui` の presence 検査は**素通りさせた**——orphan でも `egui_results:hide` は出るためである。trace の presence は窓の状態ではない。PR A′ で「最後の `egui_hide:done` より後ろに `egui_results:show` が出ないこと」を smoke に追加した。

### 決定 3: `platform-event` 袋の解体（PR C）

`platform-event`（`platform/mod.rs:290` で emit・payload 内 `event` フィールドで種別分岐）の内側種別は**今日 `initial-hotkey-failed` の 1 種のみ**。この袋詰めは TS フロントが汎用チャネルを 1 本持っていた時代の遺物である。

`initial-hotkey-failed` を独立イベント名へ昇格し、`serde_json::Value` の手動フィルタ（`mod.rs:597-616`）を消す。これにより grep 可能性が回復する——**#652 の gap 特定が構造的に不可能だった原因そのもの**である。

あわせて 9 種のイベント名を定数化し、emit 側と listen 側の双方が同一 path を参照する。

**効能の限定**: 定数化が防ぐのは綴り不一致のみであり、現状 9/9 が一致しているため今この誤りは存在しない。#652 の実形は綴り不一致でも受け口消失でもなく、**新しい UI 経路（egui）を並走させたときに旧経路（TS）にしかない受け口を複製し忘れた coverage gap** である（`git show 15933af^:ui/src/MainApp.tsx` に `listen("platform-event", ...)` が実在したことを確認済み）。定数化ではこれを防げない。袋の解体のほうが効果が大きい。

### 決定 4: `read_visual` 合成アクセサ（PR B）

`egui_shell/mod.rs` に `read_visual(app) -> VisualSnapshot` を置き、1 lock で必要な値を読み切る。main / results の双方が使う。

必須条件と制約:

- **`results_view.rs:124`（`request_icons_for_results` 冒頭の `show_icons`）を引数化して潰す。** `ResultsView::update()` の config lock は 4 回ではなく **5 回**であり、1 つでも残すと「束ねた読み」と「はぐれた読み」が 1 フレーム内で食い違う——修正が新しいフレーム内不整合を作る。
- **clone 削減の実体は「束ねること」ではなく「guard 内で hex → `Color32` を parse すること」**である。現行 `row_theme` は `String` を 3 本 clone してから lock 外で parse している。guard 内で parse すれば 3 本の確保が完全に消える（`Color32` は `Copy`）。`font_family` は比較を guard 内で行い、変化フレームのみ clone する。
- **guard 内に確保の重い処理・I/O を置かない**（制約として明記。無ければ将来この guard は肥る）。
- **`VisualSnapshot` の寿命は 1 フレーム。`self.` フィールドに保持しない**（毎フレーム live-read 方針・#576 / #646 決定 2 の保護）。
- **`read_metrics` は独立した projection として残す。** `show_egui_main`（`mod.rs:373`）が呼ぶため、show 経路が不要な色 parse を払う形にしない。
- `VisualSnapshot` は main と results の要求の和集合になる（main: `background` / `input_bg` / `font_family` / metrics、results: `text` / `hint` / `sel` / `show_icons` / metrics）。**和集合を許容する**（窓ごとの projection には分けない——分けると導出式が再び 2 箇所になる）。

**効果の限定**: 4→1 lock が閉じるのは「フレーム内で `font_size` が `row_theme` と `read_metrics` で別々に読まれ、間に `config_watcher` の `update_config` が挟まると新フォントサイズを旧行高で描く 1 フレームが生じる」という窓であり、次フレームで自然に直る **cosmetic** である。correctness の不変条件ではない。

### 決定 5: `#673` 項目 2（level → edge 化）は行わず、理由を記録する

`drive_results_window` 末尾の無条件 `wake_results`（`view.rs:733`）は**削ってはならない**。理由は「検証手段が無い」ではなく、**correctness regression になる**ためである:

- `register_config_wake_listeners`（`mod.rs:562-569`）は `wake_view`（main）しか呼ばない。**results は config 系イベントを一切 listen していない。**
- results 可視中に visual-only の config 変更（`font_size` / `row_padding` / 各色 / `show_icons`）が入ると、`RowsSnapshot` は不変（rows / selected / generation / settled すべて同じ）ゆえ差分 wake（`view.rs:1552`）も発火しない。
- したがって **results が新しい値を描く唯一の経路が、この無条件 wake である。**

`view.rs:733` に上記を doc コメントとして記録する。記録しなければ、将来「毎フレーム wake は無駄」という一見正しい最適化で静かに壊れる。

### 決定 6: `RuntimeFrame::hide_window()` を削除する（PR A）

製品コードの呼び出し元はゼロ（§2.1）。`results_view.rs:6-9` の「この view で `frame.hide_window()` を呼ぶな」という家訓は、**API ごと消せば表現不能になる**。`results` は `focusable(false)` ゆえ `Focused(true)` が原理的に来ず、`visible` が false に固着すると復帰経路が無い——その第 3 の writer を構造的に存在させない。

`close_window()` / `drag_window()` は残す（`drag_window` は `view.rs:1006` が使用中。`close_window` は `snotra-egui-mvp` のみだが #660 の削除まで温存する）。

### 決定 7: trace の置き場（PR A と A′ の衝突の解決）

PR A が追加する results の trace は、**`show_results_no_activate` / `hide_results` の内側ではなく、呼び出し側**（`drive_results_window` と `hide_egui_main`）に置く。

理由: PR A′ はこの 3 関数を `ResultsWindow` の method へ移すため、内側に置くと trace を 2 度書くことになり、PR A の smoke アサーションが 1 PR 後に消える関数から出る event 名を pin することになる。

trace は**要求レベル**（遷移レベルではない）とする。`egui_results:show` / `egui_results:hide` を emit し、既に同じ状態でも出る。smoke は presence のみを assert する。

### 決定 8: setup の manage 順序

PR A′ と PR D の双方が `main.rs` の setup ブロックへ順序制約を追加する。**両者を統合した順序をここで 1 度だけ決める**（片方が後から他方の前提を無効化しないため）。

現状（`main.rs:280-303`）:

```
setup_platform_thread
app.manage(EguiShellState::default())     // :285
egui_shell::create(...)                   // :286
register_hide_listener                    // :288
register_config_wake_listeners            // :292
register_hotkey_failure_listener          // :295
register_platform_event_listener          // :299
app.manage(UpdaterUiState(...))           // :300
app.manage(ResultsShared::default())      // :302
spawn_update_check                        // :303
```

改訂後（**PR D 完了時点の終端形**。A′ 単独では下記「A′ の中間形」に留める）:

```
setup_platform_thread
let handles = egui_shell::create(...)        // ResultsWindow と両窓の wake handle を返す
app.manage(EguiShellState::from(handles))    // create の後。handle は非 Option
app.manage(handles.results_window)           // create の後（A′）
register_* listeners                          // 以降は現行順を維持
app.manage(UpdaterUiState(...))
app.manage(ResultsShared::default())
spawn_update_check
```

**A′ の中間形**（`EguiShellState` は `create()` より前のまま動かさない）:

```
setup_platform_thread
app.manage(EguiShellState::default())        // 現行位置を維持（register_ctx がまだ読むため）
let results = egui_shell::create(...)        // ResultsWindow を返す
app.manage(results)                          // create の直後・listener 登録より前
register_* listeners
...
```

根拠と制約:

- **`EguiShellState` を `create()` の後へ移せるのは、PR D で `register_ctx` を撤去した後である。** 現在 `create()` より前に manage されているのは、`view.setup()`（`view.rs:992`）と `results_view.setup()`（`results_view.rs:378`）が `register_ctx` で managed state を読むためである。`register_ctx` を撤去すると両 `setup()` は `EguiShellState` を一切参照しなくなる。
- **この順序変更をしないと PR D は主張どおりにならない。** `Mutex<Option<EguiWindowHandle>>` × 2 + setter という `egui_ctx` / `results_ctx` と同型のスロットが残り、「未登録＝無害な no-op」という論法も一緒に残る。撤去リストが意味を持つのは非 `Option` 化とセットのときだけである。
- **`ResultsWindow` は hide が起こりうる時点より前に manage されている必要がある。** 現行 `hide_egui_main` は `app.get_window("results")` で到達しており順序依存が無いが、A′ では managed state 経由になるため制約が生じる。listener 登録より前に置くことでこれを満たす。
- したがって **PR A′ は PR D と同じ setup 順序改訂を共有する。** A′ を先に入れる場合は `EguiShellState` の移動を伴わない形（`ResultsWindow` の manage だけを `create()` 直後に追加）に留め、D が残りを行う。

### 決定 10: smoke の注入ホットキーは「アプリが実際に登録した値」から導出する（PR A）

`scripts/smoke-egui.ps1` は現在、注入する仮想キーコードを `-HotkeyVks`（既定 `"18,81"` = Alt+Q）で受け取り、コメントで「実 config を持つ実機ではその値を渡す」と**運用者の知識に依存する規範**を置いている。実際 PR A の Task 1+2 の実行で、実機 hotkey が Ctrl+K であることに気づくまで一度躓いた。

**PowerShell 側で `config.toml` を読む案は採らない。** `src-tauri/src/platform/hotkey.rs` の変換は 2 段あり、どちらも表を持つ——`parse_modifier`（`+` 区切りの複合修飾・`ctrl`/`control`・`win`/`super` の別名）と `parse_vk`（`space` / `enter` / F1〜F12 / `home` 等 約 30 の名前付きキー）。さらに `RegisterHotKey` 用の修飾ビット（`MOD_ALT` 等）と `keybd_event` 用の VK（`VK_MENU` = 0x12 等）は**別の値**である。config を読む案は、この 3 種類の対応表の写しを PowerShell 側に作り、Rust 側とドリフトする（AGENTS.md「派生コピー同士の一致を完全性の証拠にしない」）。

代わりに **`hotkey::register` が登録結果を trace し、smoke がそれを読む**。

- trace: `hotkey:registered` / `data = {"modifier", "key", "vks": [注入用 VK の押下順], "ok"}`
- 対応表の SSOT は `hotkey.rs` の新設 `injection_vks` 1 か所に留まる（smoke は表を持たない）
- **「何を要求したか」ではなく「実際に何が効いているか」を読む**ため、config の parse 失敗で既定値へフォールバックした場合や `RegisterHotKey` が失敗した場合にも正しい
- この trace はユーザーの「ホットキーが効かない」報告に対する一次情報でもあり、テスト専用の足場ではない
- `-HotkeyVks` は明示指定の override として残す（trace を出さない旧バイナリの検証用）

### 決定 9: `Primary`（main）は現行のまま tao 経由を維持する

決定 1 の帰結。ただし次の規則を `src-tauri/CLAUDE.md` に**窓単位の層の選択**として明記する（既存の「tao の窓状態を迂回したら同種操作はすべて迂回側へ寄せる」を、窓種の性質として書き直す）:

> ある窓の show / hide / topmost のいずれか 1 つが tao を迂回したら、残り 2 つも必ず迂回側へ寄せる。混在は許されない。
> - main（`Primary`）= 3 操作すべて tao 経由（tauri `show` / `hide` / `set_always_on_top`）
> - results（`Subordinate`）= 3 操作すべて raw（`SW_SHOWNOACTIVATE` / `SW_HIDE` / `SetWindowPos`）
>
> **「main の show だけ raw にして統一する」は禁止。** main の tao `VISIBLE` が stale 化し、`set_always_on_top` が main を消す（`commands/window.rs:94` / `:140` の topmost 対称がその瞬間に凶器になる）。

## 4. PR 分割

| PR | 内容 | 単独 green | 検証 |
|---|---|---|---|
| **A** | smoke 拡張（決定 7 の trace + `smoke-egui.ps1` に 1 文字注入 → `egui_results:show` 観測 → Escape → `egui_results:hide` 観測）／ `RuntimeFrame::hide_window()` 削除（決定 6）／ 文書訂正（§5）／ 注入 hotkey を登録結果から導出（決定 10） | ○ | `smoke:egui` 自身。**後続の網** |
| **A′** | `ResultsWindow` newtype（決定 2）。results の 5 呼び出し点（`view.rs:712` / `:730`、`mod.rs:449`、`commands/window.rs:99` / `:143`）と `get_window("results")` の 5 箇所（`view.rs:700`、`mod.rs:448` / `:541`、`commands/window.rs:96` / `:142`）を移行 | ○ | cargo + A 拡張後の `smoke:egui` + 実機目視 |
| **B** | `read_visual`（決定 4） | ○ | cargo test（純関数部分）+ clippy |
| **C** | `platform-event` 解体 + イベント名定数化（決定 3） | ○ | cargo（compile-fail）+ `smoke:startup` |
| **D** | ctx 複製の解消（#671 項目 2）。`egui_ctx` / `results_ctx` / `register_ctx` / `wake_ctx` / `wake_view` / `wake_results` を撤去し runtime 側の窓ごと Context を使う。setup 順序改訂（決定 8） | ○ | cargo + A 拡張後の `smoke:egui` |

**A は他のすべての前提条件**である（§2.7 のとおり現在の自動被覆はゼロ）。

PR D は副次的に既存の不変条件違反を解消する: `register_ctx` が `egui::Context` の clone を managed state へ保存するため、`Destroyed`（`runtime.rs:157-159` の `active.remove`）を越えて Context が生き残り、`RepaintScheduler` の `SchedulerInner::drop`（`repaint.rs:100-107`）による stop + join が走らない。`snotra-egui-runtime/CLAUDE.md`「repaint worker は所有型の Drop で停止し join する」は既に破れている。実害はゼロ（`close_window()` の製品呼び出し元が無く `Destroyed` はプロセス終了時のみ）だが、D が直す自然な機会である。

## 5. 併せて訂正する文書（PR A）

レビューで検出された、実装と食い違う記述:

1. **`hide_egui_main` = 「全 hide の唯一の副作用所有点（codex #7）」は現在偽。** `view.rs:712` の results hide はこれを経由しない。所在は**コードコメント 4 箇所**——`mod.rs:434`（`hide_egui_main` の doc）・`mod.rs:454`（trim の唯一の呼び出し元）・`mod.rs:488`（`register_hide_listener` の doc）・`main.rs:287`。`mod.rs:447` のコメントだけは「唯一の**外部** hide 経路」と正しく限定している。`src-tauri/CLAUDE.md` には該当する全称記述は**無い**（grep 済み・当初 CLAUDE.md と誤記していたのを訂正）。4 箇所に「main の」という限定を付ける（AGENTS.md「全称表現は前提条件とセットで書く」）。
2. **削除済み関数 `show_main_and_emit` への参照が 2 箇所残る。** `platform/mod.rs:270` は「`show_main_and_emit()` が main thread で `show()`/`set_focus()` を呼んだ**後**にこのコマンドが配送される」という**機構の説明**に使っており、根拠として成立しない（#532 SU7 で削除済み）。しかも現行 `show_egui_main`（`mod.rs:410-426`）は IME オフを focus 同期の**後**に置くことを意図的な順序制約としているため、コメントが述べる「focus が先に来る narrow timing window」は残存レースではなく設計どおりの順序である。実態に合わせて書き直す。`mod.rs:357` は歴史的経緯の参照（「SU2 の `show_main_and_emit` と同じ制約」）ゆえ、現行の関数名へ言い換える。

## 6. 検証

### cargo test / clippy で守れるもの
- `read_visual` の写像（`VisualConfig → VisualSnapshot`）を純関数として切り出せばユニットテスト可能
- イベント名定数の未定義参照は compile-fail（ただし「emit 側と listen 側が**同じ**定数を使っているか」は守れない。1 モジュールに集約し双方が同一 path を参照して初めて意味を持つ）
- `ResultsWindow` の呼び出し点移行は型で守られる

### `smoke:egui` / `smoke:startup` で守れるもの
- **PR A 適用後**: results が表示される / 隠れる（`egui_results:show` / `:hide` の観測）
- 起動 trace が 1 件以上あり、seed 済み検証用プロファイルで非 first-run 起動すること（`smoke:startup`。汎用的なエラー不在検査は #845 で撤去）
- PR A 以前は **results 経路の自動被覆はゼロ**（§2.7）

### 人間の実機目視でしか守れないもの
- results が 1 文字目で現れ、**かつフォーカスを奪わない**（#646 PR2 が実機スモークでのみ発見したバグの再発検出。smoke の 2 文字目注入で近似はできるが等価ではない）
- 設定サイドカー起動中に results が設定画面の上に浮かないこと / 終了後に topmost が戻ること
- main / results が白紙にならないこと（`egui_show:done` は白紙でも出るため trace では検出できない）
- ドラッグ移動中の results 追従（`Moved` リスナー経路）

`.claude/rules/src-tauri.md` のとおり、本サイクルはカテゴリ C（`smoke:startup` / `smoke:egui`）のトリガに当たる。**post-edit hook はカテゴリ A しか走らせないため、本サイクルで「沈黙 = 合格」は成立しない。**

適用する skill: `/symmetric-check`（show/hide 対称・A′）、`/dry-check`（`ResultsWindow` 導入後の重複・A′）、`/persistence-check` は非該当。`/race-check` は #663 のとおり旧 SolidJS モデルのままで本サイクルの並行モデルに適合しないため、**参照しない**（スキル本文の改訂は #663・エージェント設定ゆえ合意が必要で本 spec のスコープ外）。

## 7. 受容する残余（未解決・未測定）

**本 spec はこれらを解決しない。記録することで、次のサイクルが「無い」と誤認しないようにする。**

1. **`app.get_window("results").hide()` は依然書けて黙って no-op する。** §2.6。表現不能化は I-1 でも A′ でも達成できない。A′ は正しい経路を 1 つにするに留まる。
2. **「hidden 中は `update()` が走らない」（#532 SU5 の要石）の機構は未同定・未測定である。** §2.1 より `visible` は恒真であるから、runtime のガードではありえない。`register_config_wake_listeners` は可視性に関係なく main を wake するため、hidden 中の `config-applied` は `RepaintScheduler` → proxy → `RequestRedraw` まで届くはずである。抑止しているのは OS / tao 層（hidden HWND に `WM_PAINT` が来ない）と**推測**されるが未確認。**本 spec はこの命題を既定事実として引用しない。** 測るなら `render()` に trace 1 行を足し、hidden 中に `wake_view` を起こして観測する。
   - **errata（2026-07-26・#697 実測）**: 解消済み。送受信 2 計器（`SNOTRA_EGUI_WAKE_TRACE`）で、hidden 中の `config-applied` 刺激に対し worker の送信（SEND=2）・イベントループの受信（RECV=0）を観測——worker は `RequestRedraw` を送信するが、hidden な窓には `RedrawRequested` が配送されない。**抑止は tao/OS 層で確定**（可視区間では SEND=RECV=REPAINT の 1:1 を陽性対照として確認）。
3. **`runtime.rs:411-419` の `hidden_window_is_not_painted` は恒真テストである。** テスト内でローカル定義した `fn should_render(visible: bool) -> bool { visible }` を検査しており、実際の `render()` 早期 return を一切守っていない。本 spec は runtime の `visible` に触れないため修正対象にしないが、**このテストを「守られている根拠」として引用してはならない。**
   - **errata（2026-07-26・#697）**: 同テストは削除した。実 `render()` を検査する形は dev-dependencies ゼロ・実 HWND 要求ゆえ不可能で、接地は残余 2 の実測と `render()` の到達可能性注記が担う（行番号 411-419 は記録当時の値）。
4. **`egui_shell` にヘッドレステスト基盤が無い。** `egui_kittest` は `snotra-settings` のみ（#440 で導入済み・#456 で回転実績あり）。最も複雑な egui サーフェスが最も検証されていない状態は本 spec で解消しない。
5. **handle メソッドの marshalling（どのスレッドで OS 呼びを実行するか）は決めない。** `mod.rs:449` と `commands/window.rs:143`（spawn したポーリングスレッド）は既にイベントループ外から raw Win32 を呼んでおり、`.claude/rules/src-tauri.md`「Win32 API は `PlatformBridge` 経由」とは既に不整合。A′ はこの不整合を**移設するだけで解消しない**。
6. **#673 項目 2 は永続的に「やらない」ではない。** 決定 5 の理由（results が config 系イベントを listen していない）が解消されれば——例えば results 自身に config wake の受け口を足せば——edge 化は再び選択肢になる。本 spec はその設計を行わない。

## 8. 参照

- issue: #671 / #673 / #652 / #646 / #532 / #660 / #663 / #666
- 前サイクルの spec: `docs/superpowers/specs/2026-07-24-646-two-window-ui-design.md`
- 本 spec の根拠となったレビュー: 5 レンズ（tao 越境 / 対称性・ライフサイクル / 状態配送 / スコープ・YAGNI / 敵対的反証）
