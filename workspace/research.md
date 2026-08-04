# 調査 — #738 メインウィンドウが作業領域の外へ出る

## issue の要約

`main` 窓が作業領域の外へ出る経路が **2 本**ある。性質が違い、扱いも分かれる。

| 経路 | 外へ出るもの | 現状 |
|---|---|---|
| (a) ユーザーのドラッグ | **バー本体ごと** | 無防備。`Moved` リスナーは results の追従だけで main をクランプしない |
| (b) status 行・toast 行による伸長 | **伸びた分だけ**（左上は不動） | 無防備。ただし #904 で**受容残余として SPEC に明記済み** |

実機で観測された #738・#760 の再現（issue コメント 3・2026-07-27）は、どちらも **(a)** を通っている。

## 前提が変わっている（issue 本文より新しい事実）

issue 本文が「同時に直すべき文書の欠陥」として挙げた `SPEC.md` の全称主張

> ターゲットモニターの作業領域にクランプし、**ウィンドウが画面外に出ないことを保証する**

は **既に存在しない**。#904（`0083620`・2026-08-04）が `SPEC.md:440` を次へ書き換えている。

> ターゲットモニターの作業領域にクランプする。クランプはバー高を対象に行うため、status 行・toast 行が出ている間はその分だけウィンドウが作業領域の外へ出うる——バーの位置は行の出没で動かさないため、伸びた高さに合わせた再クランプはしない（§4.7 参照・#738 の対象）

ゆえに **issue の受け入れ条件 1（「または受容残余として明記」の側）と 2 は充足済み**である。本タスクが扱うのは、issue 本文が明記していない **(a) ドラッグ経路**（本文の「関連（同時に直すべき文書の欠陥）」節が現象として記述だけしている部分）。

これは「fix」の名を持つが **仕様変更である**（`AGENTS.md` ワークフロー 1: 文書化された挙動を変えるなら SPEC 同期）。現行 SPEC はドラッグでの持ち出しについて何も書いておらず、今回「可視中はバー矩形を作業領域内に保つ」という新しい保証を足す。

## 人間裁定による制約（動かせない前提）

`window_coordinator.rs:265` と `SPEC.md:189`（#904・2026-08-04）:

> **バーの位置はユーザーが決め、行の出没では動かさない**

ゆえに issue 本文の案 1（伸長時に窓を上へずらす）は**採れない**。(b) は受容残余のまま。

## 関連ファイル・シンボル（grep 実在確認済み）

| パス | シンボル | 役割 |
|---|---|---|
| `src-tauri/src/monitor.rs:36` | `WorkArea::clamp` | 位置クランプの算術。**ユニットテスト 7 件が既にある**（境界安定性・負原点モニター・窓が作業領域より大きい場合を含む） |
| `src-tauri/src/monitor.rs:77` | `window_monitor_work_area(hwnd_raw)` | 窓が乗っているモニターの作業領域。`MONITOR_DEFAULTTONEAREST` ゆえ**完全に外へ出た窓でも最寄りを返す** |
| `src-tauri/src/egui_shell/window_coordinator.rs:180` | `position_on_target_monitor` | show 時の位置決め。`WorkArea::clamp` の唯一の呼び出し元。呼び出し元は `show_egui_main` だけ |
| `src-tauri/src/egui_shell/window_coordinator.rs:570` | `position_results_below_main` | results 追従。main の `outer_position` / `outer_size` / `scale_factor` を読む既存パターン |
| `src-tauri/src/egui_shell/window_coordinator.rs:604` | `results_available_height` | `window_monitor_work_area` を main の HWND から引く既存パターン |
| `src-tauri/src/egui_shell/layout.rs` | `results_top_y` / `available_below` | 「Win32 の読みは driver、算術は純粋核」の分担例。スカラーを受け `WorkArea` 型は受けない |
| `src-tauri/src/egui_shell/view.rs:300-307` | `drag_resp` / `frame.drag_window()` | ドラッグ移動の起点。`drag_started_by` で OS の move loop へ委ねる |
| `src-tauri/src/egui_shell/view.rs:971` | `drive_results_window` 呼び出し | main の `set_size` ブロック（947-966）の直後。クランプはこの**間**に入る |
| `src-tauri/src/egui_shell/mod.rs:345` | `Moved` リスナー | results 追従のみ。main のクランプはしない |

## 再利用できる既存パターン

1. **`window_monitor_work_area(main.hwnd())`** — `results_available_height:607` と `read_placement_relative:515` の 2 箇所が既に採用。「窓が乗っているモニター」を基準にする形は新規発明ではない
2. **「Win32 の読みは 1 回・算術は `layout.rs` の純粋核」** — `position_results_below_main:587` が `layout::results_top_y` を呼ぶ形
3. **`WorkArea::clamp`** — show 経路が使うのと**同じ導出**を可視中の経路にも通す。#877 が幅について採った治療法（二人の書き手を同じ SSOT から導かせる）と同型で、#878 が求めている形

## 技術的制約

- **`view.rs` に `set_position` は 1 つも無い**（#878 が grep 実測として記録）。この性質は保つ——実体は `window_coordinator` に置き、`view.rs` はそれを呼ぶだけにする（位置は `window_coordinator` の責務・`src-tauri/CLAUDE.md`「モジュール構成」）
- **証人型（`EventLoopProof`）は不要**。可視性を変える操作ではない。`position_results_below_main` も取っていない
- **hidden 中は `update()` が走らない**（`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）。ドラッグは可視中にしか起こらないので問題にならない
- **`Moved` リスナーの中でクランプしてはならない** — `set_position` が `Moved` を再発火する / OS の move loop と競合する / ユーザーが issue コメント 1 で懸念した「ディスプレイ間移動で邪魔になる」がまさにこれ
- **物理座標で扱う**。`bar_height` は論理 px なので `scale_factor()` で換算が要る（`results_top_y` / `available_below` と同じ）

## 未解決の疑問 → すべて決着（codex の敵対的レビュー + 人間裁定・2026-08-04）

初版の疑問 3 点は、**そのうち 2 点が「疑問」ではなく実際の欠陥だった**。codex（非対話 CLI・read-only）へ反証を求めたところ、設計を変える指摘が 5 件返った。

1. **ネイティブ move loop 中に egui フレームが回るか** → **問い自体が的外れだった**。回っても回らなくても、**毎フレームのクランプは横並びモニター間の移動を封鎖する**（幅 600px の窓は左端 1320〜1620 の区間で毎回 1320 へ引き戻され、隣モニターが優勢になる位置へ到達できない）。「ゴムバンドでも受け入れ条件を満たす」という判定は誤り。→ **ポインタ非押下のフレームでのみクランプする**設計へ変更（plan.md 要石 2）
2. **モニターをまたぐドラッグで引っかからないか** → 引っかかる（同上）。加えて `MonitorFromWindow` は**ウィンドウ全体の矩形**でモニターを選ぶため、上下モニター構成で status/toast が伸びると基準が下側へ切り替わり、**行の出没だけでバーが飛ぶ**（#904 の裁定を修正自身が破る）。→ **バー矩形の中心から `MonitorFromPoint` で決める**設計へ変更（plan.md 要石 1）
3. **窓が 2 モニターにまたがる配置を禁止することになる** → **人間裁定「ディスプレイまたぎで表示されるユースケースは想定しなくてOK」**（2026-08-04）

追加で判明した事実:

- **`set_position` は `Moved` リスナーをその場で同期実行しない** — `RedrawRequested` ハンドラが tao の runner を借用中のため `EventLoopRunner::send_event` がバッファし、`update()` 終了後に配送する（`tao-0.35.3` の `event_loop/runner.rs`）。`emit_filter` が同期なのは tao の `WindowEvent` 配送**後**の段である
- **`Config::validate` に `window_width` の上限は無い** — 低解像度 × 高 DPI では窓幅が作業領域幅を超えうる。`WorkArea::clamp` は左上寄せするだけなので右端は外に残る（plan.md 前提条件 3）

## #760 との境界

本修正で **#760 の主要経路（main がドラッグで外→results が丸ごと外）は閉じる**。ただし完全には閉じない——バー矩形が作業領域内でも、status/toast で伸びた main の下端は外へ出うる（(b) の受容残余）ので、その直下に置かれる results は依然として作業領域外に置かれうる。**#760 は独立に残る**。
