# research — issue #1194: モーダル移動ループ中の窓位置

作成: 2026-08-27 / ブランチ: `fix/keyboard-move-clamp-1194`

## issue の要約

`clamp_main_into_work_area` の発火を止める条件は `view.rs:1279` の `!ui.input(|i| i.pointer.any_down())` **だけ**である。マウスドラッグはポインタ押下で除外されるが、キーボード移動（`Alt+Space` → `M` → 矢印）にはポインタ押下が伴わないため除外されず、OS のモーダル移動ループ中もクランプが働く。#1173 が対照つきで実測し、PR #1193 が rustdoc と `SPEC.md` §8.2 へ as-built として着地させた。

3 つの問いを 1 度の計装で同時に答える。

- **Q1（答えは出ている・直す対象）**: クランプがキーボード移動を拘束する。移動中に作業領域の下端ちょうど（1020）で止まり、`Enter` の前に戻る。対照（クランプ無効）では 1133〜1134 に留まる
- **Q2（未特定）**: 対照側で `Enter` 後に窓が 1133〜1134 → 1106 と 27〜28 px 上がる（5/5）
- **Q3（未特定）**: クランプ有効側でも移動中に一時的に外へ出る反復がある。「ループ側の `SetWindowPos` と競り合っている」と「フレームが疎で戻りが遅い」を分離できていない

## 分類: バグではなく **仕様変更**

`SPEC.md` §8.2「表示中の作業領域への復帰（#738）」に、**現在の挙動が as-built として明記されている**（`SPEC.md:478`）:

> **キーボードによるウィンドウ移動（`Alt+Space` → `M` → 矢印キー）は、ドラッグと違って拘束される。** ポインタが押されないため復帰が働き続け、移動中に作業領域の外へ出ても内側へ引き戻される——**移動先が作業領域の外になる位置には確定できない**（#1173 実測）。複数モニター環境では、上と同じ理由で隣のモニターへ渡れないと見込まれる

`AGENTS.md`「開発ワークフロー」1 の判定に当てると、**記述に合わせるのではなく記述を変える**ので仕様変更である。ゆえに更新順序は `SPEC.md` → コード → ドキュメント。加えて受け入れ条件が「採らなかった候補と却下理由を ADR へ残している」を要求するため **新規 ADR が要る**。

## 関連ファイル・シンボル（すべて `git grep` で実在確認済み・`.claude/worktrees/` は走査から除外）

| 場所 | 役割 |
|---|---|
| `src-tauri/src/egui_shell/view.rs:1279-1281` | クランプの**唯一の呼び出し点**とガード `!any_down()`。呼び出し側にしか無い制約（`drive_results_window` より前）はここのコメントが正本 |
| `src-tauri/src/egui_shell/window_coordinator.rs:817` | `clamp_main_into_work_area` 本体。`read_bar_anchor` → `WorkArea::clamp` → `set_position`。同値なら撃たない |
| 同 `:618` 付近 | `read_placement_relative`（hide 時の保存）。クランプと**同じ基準モニター**を通す |
| 同 `:704` | `read_bar_anchor`（`outer_position` + `read_frame_geom` + `point_monitor_work_area`） |
| 同 `:262` | `position_on_target_monitor` の `set_position`（show 経路） |
| `src-tauri/src/egui_shell/view.rs:1320` | `egui_frame` trace（`update_us` / `interval_us`。**seq と ts_ms は `trace()` が全行に付ける**） |
| `src-tauri/src/egui_shell/mod.rs:395-400` | main の `Moved` リスナー。**`position_results_below_main` を呼ぶだけで、main の repaint は要求しない** |
| `src-tauri/src/trace.rs:44` | `trace()`。`SNOTRA_TRACE` で stderr へ 1 行 JSON。`seq` はプロセス全体の単調カウンタ |
| `scripts/lib/SnotraSmoke.psm1` | 打鍵注入・窓待ち・フォアグラウンド化。`Send-SnotraKey` / `Send-SnotraKeyChord` は export 済み |
| `docs/adr/ADR-main-window-clamp-on-pointer-release.md` | 却下 1〜5。却下 1 が `WM_MOVING` フック＝tao の wndproc サブクラス化 |
| `docs/adr/ADR-no-test-only-injection-in-product-code.md` | 計測のための注入点を製品コードへ足さない |
| `SPEC.md:456-487` | §8.2。478 行目が今回変える記述 |

### main の位置を書く経路は 2 つだけである

`git grep -n "set_position\|SetWindowPos" -- 'src-tauri/**/*.rs'` の全 19 件を振り分けた結果:

- **main の位置を書く**: `window_coordinator.rs:262`（show 経路）と `:834`（クランプ）の **2 つだけ**
- results の位置: `results_window.rs:282-283`（`ResultsWindow::set_position`）と `window_coordinator.rs:971`
- Z オーダーのみ: `results_window.rs:187`（`SWP_NOMOVE | SWP_NOSIZE`）
- 残りはすべてコメント・doc 内の言及

main の**サイズ**を書く経路は `view.rs:1256`（毎フレームの動的高さ）と show 経路の 2 つ。

## 再利用できる既存パターン

- **`egui_frame` trace が既に在る**（`view.rs:1320`）。フレームの発火時刻・間隔はコードを 1 行も足さずに取れる。Q3 の「フレームが疎」側の計器は**新設不要**である
- **`trace()` の `seq` が全行に単調な全順序を与える**ので、フレームと位置の書き手を 1 本の系列に並べられる（`trace.rs` の `//!` が「main.rs と commands の行が 1 つの単調 seq で交錯する」と明記）
- **`WorkArea::clamp` はユニットテスト 7 件が固定する純粋核**。判定の差し替えでこの算術に触る必要はない
- **`ADR-egui-trace-hatch-empty-only` の前例**——計器を既存の `[trace]` 行へ乗せ、新しい env も新しい行も足さない（`heap_trace.rs` の `//!` が同じ方針）
- **`SNOTRA_EGUI_WAKE_TRACE` が既に在る**（`snotra-egui-runtime/src/runtime.rs:281-286`）。`RedrawRequested` を受けるたびに `window_id` / `runtime_id` を出す——**wake 源の実機確認に製品コードは 1 行も要らない**（U2）
- **`SNOTRA_CONFIG_DIR` の使い捨てプロファイル**（`docs/build-commands.md`「別プロファイルで起動するための env ハッチ」）
- **クランプ rustdoc が再測手順を逐語で持つ**: release ビルド + `SNOTRA_CONFIG_DIR` + DPI awareness を先に確立 + 対照（`any_down()` の分岐を短絡させた 1 行パッチ）+ 測り終えたら戻して release を再ビルド

## 技術的制約（一次資料で確認したもの）

### C1. `GetGUIThreadInfo` は追加依存なしで呼べる — **確認済み**

`windows` crate 0.62.2 の `Win32_UI_WindowsAndMessaging` feature（`src-tauri/Cargo.toml:38` で既に有効）に:

- `pub unsafe fn GetGUIThreadInfo(idthread: u32, pgui: *mut GUITHREADINFO) -> Result<()>`（`WindowsAndMessaging/mod.rs:871`）
- `pub const GUI_INMOVESIZE: GUITHREADINFO_FLAGS = GUITHREADINFO_FLAGS(2u32)`（同 `:3746`）
- `GUITHREADINFO` は `hwndMoveSize` を持つ（同 `:3696-3705`）

**`hwndMoveSize` は `GUI_INMOVESIZE` より強い判別子である**——フラグは「このスレッドの何かが move/size 中」しか言わないが、`hwndMoveSize` は**どの窓か**を名指す。main の HWND と突き合わせれば、settings 窓など他窓のループでクランプが黙る誤りを構造的に避けられる。

### C2. tao 0.35.3 は内部フラグを持ち、公開していない — **確認済み**

- `WindowFlags::MARKER_IN_SIZE_MOVE = 1 << 18`（`window_state.rs:109`）
- `WM_ENTERSIZEMOVE` で insert / `WM_EXITSIZEMOVE` で remove（`event_loop.rs:1028-1043`）
- 読むのは `event_loop.rs:1994` の DPI 変更処理のみ。**公開 API に出ていない**
- tao は `GetGUIThreadInfo` を使っていない（crate ソース全体への grep が 0 件）

### C3. `WM_EXITSIZEMOVE` の合成 `WM_LBUTTONUP` は**ドラッグ経路にしか出ない** — **確認済み**

`event_loop.rs:1036-1043` は `if state.dragging` の内側でのみ `PostMessageW(WM_LBUTTONUP)` する。`dragging` はドラッグ開始経路が立てるフラグであり、**キーボード移動では立たない**。ADR 却下 5 が「押下フラグの固着を受容できる」根拠にしたこの経路は、キーボード移動には無関係である。

### C4. runtime はイベント駆動で、モーダルループ中のフレームは保証されていない

`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件（#532 SU5）」——「通常フレームは勝手に回らない」。`mod.rs:390-392` のコメントも「ネイティブ移動ループ中は egui フレームが回る保証が無い」と明記し、だから `Moved` リスナーで results を**直接**追従させている。**そのリスナーは main の repaint を要求しない。**

**にもかかわらず #1173 はループ中にクランプが発火したことを実測している**（落ち着く値が作業領域の下端ちょうど）。ゆえに何かがフレームを起こしている。**その候補は U2 で名指しできた**（tao の `WM_PAINT` → `RedrawRequested` 直結）**が、実機での発火は未計測である。** これが Q3 と受け入れ条件の両方に効く。

### C5. status / toast 行の高さは `bar_height` と同値である

`layout.rs:56` が `toast_height: bar_height`、`view.rs:1201-1205` が `has_status.then_some(metrics.toast_height)`。導出は `Metrics::from_config`（`layout.rs:49-52`）の `bar_height = font_size + bar_padding`。

**入力値は 2 通りあり、どちらで測ったかが一次資料に無い**（敵対的調査の所見 3 で発見・こちらで再確認）:

| 経路 | `font_size` | `bar_height`（論理） | 125% で物理 |
|---|---|---|---|
| 既定（使い捨てプロファイル） | 15（`config.rs:397`） | 43.0 | 約 54 px |
| この開発機の生きた `config.toml` | 13（`%APPDATA%/Snotra/config.toml:28`） | 41.0 | 約 51 px |

`bar_padding` は両者とも既定 28（`config.rs:408`）。**27〜28 px はどちらの半分でもあり、行 1 本ぶんではない。ゆえに「行の出没では説明できない」は両方の入力で成立する。** ただし #1173 / PR #1193 の再測手順は「使い捨てプロファイル（`SNOTRA_CONFIG_DIR`）」としか書かず、**seed した `config.toml` の中身を記録していない**——これは引用元の記録漏れであり、今回の測定では `PERFORMANCE.md`「この文書へ記録するときの規約」に従って入力値まで書き残す。

### C6. 高さの判定に `GetWindowRect` を使ってはならない

`docs/build-commands.md:118`——不可視のリサイズ枠を含む。**位置の判定には使ってよい**（クランプが渡す物理 outer 座標系と同じ）。高さは `DwmGetWindowAttribute` の `EXTENDED_FRAME_BOUNDS`。

### C7. 単発観測で判定しない / 対照が要る

`docs/build-commands.md:120`——OS のモーダルループ中の値は同一手順・同一バイナリでも揺れる（top が 956 と 1050 の間で揺れた実測）。**実装の有無を切り替える対照実験だけが差を示す。**

### C8. 生ログはリポジトリの外へ置く

`AGENTS.md` の足場トリガー行——`[trace]` は利用者の実ファイルパスを逐語で載せる（#999）。このリポジトリは公開されており、`workspace/` へ置けば squash マージで main の履歴に残る。**コミットしてよいのは派生表だけ。**

## Q2 について：issue が挙げた疑うべき近傍の現況

issue は「`show_egui_main` の配置経路・hide 時の `read_placement_relative` と復元・`set_size` の 2 手・非クライアント分／DWM の影の勘定」を挙げている。現在のコードに当てると:

- **「`set_size` の 2 手」は既に退役している。** `BarRectPhys` の doc（`window_coordinator.rs:718-724`）が「かつては呼び出し側が `set_size(幅, バー高)` で窓を物理的に畳み、あちらが `outer_size()` で読み戻していた」と過去形で書き、`position_on_target_monitor` が矩形を引数で受け取る形へ #878 が反転させた。**この容疑者は現在のコードには存在しない**
- **`show_egui_main` の配置経路と `read_placement_relative` は、どちらも show / hide の契機がなければ走らない。** 測定手順（`Alt+Space` → `M` → 矢印 → `Enter`）には show も hide も含まれない
- **行の出没（C5）では 27〜28 px は出ない**
- 残る候補は **OS 側**（モーダルループ終了時に Windows 自身が位置を確定し直す）と、**測定の前提そのものの誤り**（下記 U1）

## 未解決の疑問

### U1（最優先）— 「窓高は不変の純平行移動」は測られていない

issue と rustdoc の表が記録しているのは **`bottom` だけ**である（rustdoc: 「窓の bottom（物理 px）」）。`top` も高さも系列に無い。**「純平行移動」は #1173 の測定から出た事実ではなく、前提として書かれている。** これが偽なら Q2 の探索範囲は「位置の書き手」ではなく「サイズの書き手」へ丸ごと移る（main のサイズを書くのは `view.rs:1256` と show 経路の 2 つ）。**新しい計装の 1 回目で `top` / `bottom` / `EXTENDED_FRAME_BOUNDS` を同時に刻めば、追加の測定なしで割れる。**

### U2 — モーダルループ中に何がフレームを起こしているか（**「未特定」から「候補が名指しできる・実機未計測」へ降格**）

敵対的調査の所見 2 が破った。機序は**こちらで一次証拠を取り直して裁定した**（採るのは所見であって添えられた説明ではない）:

- `tao-0.35.3/src/platform_impl/windows/event_loop.rs:1087-1100` — `WM_PAINT` を受けると `subclass_input.send_event(Event::RedrawRequested(...))` を**直接**撃つ。crate 独自のスケジューラを経由しない
- `snotra-egui-runtime/src/runtime.rs:277-289` — `Event::RedrawRequested` のハンドラは**発生源を問わず**窓を引き当てて描画へ進む

窓を動かせば `WM_PAINT` は来るので、これはループ中にフレームが回ることの十分な説明になる。**ただしコード上の存在証明であって、実機でこの経路が発火していることは未計測である。**

**さらに、この経路の計器が既に在る**（研究中に発見。所見 2 は名指ししていない）: `runtime.rs:281-286` の `SNOTRA_EGUI_WAKE_TRACE` ハッチが、`RedrawRequested` を受けるたびに `window_id` と `runtime_id` を stderr へ出す。**引き当て失敗で握りつぶされる経路も観測できるよう、引き当ての前に置かれている**（#697）。**wake 源の実機確認に製品コードは 1 行も要らない。**

副次仮説はこちらで棄却済み: 「`interval_us` が `None` になる条件がモーダルループかもしれない」は誤りで、`None` はプロセス起動後の最初のフレームにしか出ない（所見 7・`FrameTimer::begin`）。**`interval_us` の系列はループ中もそのまま読める。**

### U3（受け入れ条件に直結）— 判定を差し替えたあと、確定後のフレームは誰が起こすのか

受け入れ条件は「確定後の最初のフレームで 1 度だけ戻る」を要求する。移動中のフレームが移動そのものに誘発されているなら、**最後のフレームは `WM_EXITSIZEMOVE` より前に来うる**——そのときフラグは真のままなのでクランプは走らず、**確定後に窓が外に残ったまま、次の無関係な入力までフレームが来ない**。これは今のガード（`!any_down()`）には無い新しい失敗様式である。

対処の候補（どれも未検証）: (a) ループ中の各フレームで `ctx.request_repaint()` を撃ち、フラグが偽へ落ちた最初のフレームまで鎖を繋ぐ、(b) `Moved` リスナーから main の waker を叩く、(c) 何もしない（次の入力で戻る挙動を SPEC へ as-built として書く）。**(a) は `src-tauri/CLAUDE.md`「armed 期限」の「時間経過で解消する不成立にだけ再要求してよい」に照らして正当化が要る**——モーダルループの終了は時計と無関係な条件であり、素直に読めば「変えた側が wake する責務を負う」側である。ただしその「変えた側」は OS であり、こちらから触れない。

### U4 — `GetGUIThreadInfo` の実呼び出しは未検証

C1 は API の**存在**だけを確認した。次は未測定である: 呼び出しコスト（毎フレーム 1 回・可視中のみ）／`idthread` に `GetCurrentThreadId()` を渡してよいか（クランプはイベントループスレッドで走るが、モーダルループも同じスレッドである、という前提そのものが未確認）／`hwndMoveSize` が main の HWND と一致するか／**フラグが両向きに立つか**（`/symmetric-check` が要求する）。

### U5 — 多モニターでの封鎖は依然として演繹である

`SPEC.md:478` は「見込まれる」と書いており、実測ではない。**人間裁定（2026-08-27）により、単一モニターの代替判別子で進める**——移動中に `bottom` が作業領域の下端（1020）で止まるか、カーソル限界（1133〜1134）まで出られるか。多モニターの封鎖は演繹のまま残し、SPEC の当該記述も「見込まれる」の強さに保つ。

### U6 — 計装を恒久 trace にするか足場にするか

`ADR-no-test-only-injection-in-product-code` は「計測のための注入点を製品コードへ足さない」。一方 `egui_frame` は恒久の製品 trace として在る。クランプの `set_position` 発火を刻む行が前者（足場）か後者（`egui_frame` と同類の恒久 trace）かを計画で決める。**足場なら `AGENTS.md` の足場トリガー行が適用され、撤去条件をその成果物自身の doc へ書く義務が生じる**（撤去条件を「この issue が閉じたら」にすると、閉じるのが撤去 PR のとき自己参照して発火しない）。

### U7 — マウスドラッグ側の退行検知

受け入れ条件が「マウスドラッグの挙動が退行していないことを対照つきで実測している」を要求する。ドラッグの注入は `SnotraSmoke.psm1` に**無い**（`docs/build-commands.md:116` が「マウスの `SetCursorPos` + mouse_event 系はいまも無い」と明記）。ADR 却下 5 の再測手順は「治具を書いて `SetCursorPos` + `mouse_event` でドラッグを注入」と書くが、それはモジュール外＝画面ロック検出の外である。**手作業で測るか、モジュールへ足すか、別の判別子を立てるかを計画で決める。**

## 敵対的調査（3b）の採否

出力: `workspace/adversarial-1194.txt`（250 行）。母集団は research.md の全主張。走査は `.claude/worktrees/` 除外。

### 採った所見

| 所見 | 採否 | 理由 |
|---|---|---|
| 所見 2 — U2 の「未特定」は破れる（tao の `WM_PAINT` → `RedrawRequested` 直結） | **採用** | 機序を一次証拠で裁定し直した（`event_loop.rs:1087-1100` / `runtime.rs:277-289`）。U2 を「候補が名指しできる・実機未計測」へ降格。**副産物として `SNOTRA_EGUI_WAKE_TRACE` の存在に到達**——計装の設計がこれで変わる |
| 所見 3 — C5 の前提値が実 config と食い違う | **採用**（結論は不変・前提を訂正） | 実 `config.toml` の `font_size = 13`（既定は 15）。両方の入力で「27〜28 px は行 1 本ではない」は成立するが、**どちらで測ったかが一次資料に無い**という記録漏れが本体 |
| 所見 7 — 「`interval_us` が `None` になるのはループ中かも」の棄却 | **採用** | 依頼文自身の仮説の反証。`None` は起動後の最初のフレームだけ |

### 採らなかった／自分で閉じた項目

- **所見 5・6 の残余「`commands/window.rs` 全文と `platform/wndproc.rs` は未読了」** — こちらで閉じた。`git grep` を `src-tauri/src/commands/` と `src-tauri/src/platform/` へ限って `SendMessage|PostMessage|SetWindowPos|MoveWindow|SetWindowPlacement|set_position|set_outer_position` を走らせた結果、main の位置を書く経路は 0 件（`tray.rs` の `PostMessageW` はトレイと `WM_NULL` の wake、`commands/window.rs:155` はコメント）。**「main の位置を書く経路は 2 つだけ」は維持する**
- **所見 9 の残余「`docs/superpowers/specs/` を走査していない」** — こちらで閉じた。`クランプ|clamp_main` を含むのは 7 枚だが、**すべて 2026-07-22〜2026-08-04 の凍結文書であり、キーボード移動の挙動が判明した #1173（2026-08-26）より前**である。写しは構造的に存在しえない。加えて `docs/superpowers/` は #589 で非規範化されている
- **所見 4 の「#1173 がどちらの機体で測ったかは証明できない」** — 所見として受け取るが、**今回の対処は「過去を特定する」ではなく「今回の測定で機体と入力値を記録する」**である（`two-dev-machines-unlabeled-in-perf-doc`）。過去の表を現在値の基準にしない

## 前サイクルからの持ち越し（`RETROSPECTIVE.md` #999）

- **自分が今書いた表を、直後に母集団として扱わない**——この research.md の表も派生コピーである
- **訂正差分が、訂正した当の型を再生する**——検査の単位を決めた瞬間、その外側が検査されなくなる
- **「母集団はこれで導く」と書いた grep を、代表入力で実行して測る**（rustfmt の折り返しで 0 件を返した前例）
- **生ログはリポジトリ外**（C8）
