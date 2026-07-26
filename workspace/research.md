# research — 束B: results 窓のフレーム間プロトコル（#710 + #699 + #675）

調査日: 2026-07-26 / ブランチ: `fix/results-window-frame-protocol` / HEAD: `f277831`

対象 issue:

| # | 表題 | ラベル |
|---|---|---|
| **#710** | results 窓の操作中フレームレート調査（ホバー/スクロールで 300fps 級・`wake_results` の level-triggered） | なし・**優先度低** |
| **#699** | `ResultsShared.clicked` が世代を運ばず、消費フレームで結果が入れ替わると別の行を起動しうる | なし |
| **#675** | 結果窓が作業領域の下端をはみ出しうる | `size:S` / `type:fix` |

3 件とも `main` ↔ `results` の 2 窓間プロトコルに属する。#710 と #699 は**同じ設計判断（フレーム間で何を運び、いつ起こすか）の下**にあり、#675 は `drive_results_window` の同じ関数内を触る。

---

## 最重要の発見 — `wake_results` の無条件呼び出しは**冗長ではない**

#710 は「気になっている点」として次を挙げる。

> `view.rs:850` の `wake_results` は `drive_results_window` 末尾で**無条件**に呼ばれる（level-triggered）。main が 1 フレーム描くたびに results も 1 フレーム描くため、main 側のフレーム増は results へ等倍で伝播する。edge-triggered 版は同ファイル 1720 行に既に存在する

**「edge-triggered 版が既にあるのだから無条件の方を消せばよい」という読みは成立しない。** 実測:

`wake_results` の呼び出し点は **2 箇所だけ**（`grep -rn wake_results src-tauri/src` で全件）:

| `file:line` | 条件 | 何を伝えるか |
|---|---|---|
| `view.rs:1759` | **snapshot に差分があるとき**（`RowsSnapshot::matches` が false） | 行・選択・世代・settled の変化 |
| `view.rs:850` | **無条件**（`drive_results_window` 末尾） | それ以外のすべて |

そして results 窓側が自分で repaint を要求するのは**アイコン worker の 2 箇所のみ**（`results_view.rs:200`・`:431`）。

さらに `register_config_wake_listeners`（`mod.rs`）は `CONFIG_APPLIED` / `INDEXING_STARTED` / `INDEXING_COMPLETE` の 3 イベントで **`wake_main` しか呼ばない**。

**したがって、config hot-reload（`font_family` / `font_size` / 色 / `row_height`）が results 窓へ届く経路は `view.rs:850` の無条件 wake だけである。** `ResultsView` は `applied_font_family` を自前で持ち（`results_view.rs` の宣言に「ctx が窓ごとに独立なため複製必須」と明記）、毎フレームの live-read で config を拾う設計だが、**そのフレームを起こしているのがこの 1 行**である。

これは `AGENTS.md`「条件別チェック（トリガー → 参照先）」の

> 重複した読み・冗長に見える状態を束ねる/消す → 各箇所について「**後で**読まれる/立つことに依存していないか」を 1 行ずつ書き出してから着手（#671 PR A′ / #673 PR B）

が名指しする局面そのものである。**#710 の修正が「無条件 wake の削除」になるなら、config hot-reload の代替経路を同じ変更で用意しなければならない**（例: `register_config_wake_listeners` に `wake_results` を足す）。

---

## #710 — 操作中フレームレート

### 受け入れ条件（issue 本文より）

1. 同一条件（同じクエリ・同じホバー操作・release）で A/B を取り、操作中の fps が #628 前後で変わったかを判定する
2. 300 fps 級の区間があるなら、「アニメーションの収束までの正常な連続描画」か「収束しないループ」かを `since_prev_ms` の時間構造で切り分ける
3. **直すべき欠陥があった場合のみ修正する**（無ければ「正常」として記録して閉じる）

issue 自身が「**この issue はまず『同一条件で A/B を取る』ことから始める**」と述べ、既存の数字については「**修正前後の数字は比較できない**——別セッション・別操作」と明記している。

### 観測手段

- `SNOTRA_EGUI_REPAINT_TRACE`（既存）。egui の `Context::repaint_causes()` が `file:line` を返すため原因が名乗る
- #628 の教訓（`docs/development-principles.md`「デバッグ・バグ修正」）が 3 点効く:
  - **原因の同定は cause 名だけでは足りず、時間構造（バーストの長さ・間隔の分布）が要る**
  - **人手の観測窓には、観測条件を変える確認作業を混ぜない**（前回まさにホバーを混ぜて測り直しになった）
  - **プロトコル非依存の判別指標を用意する**（総フレーム数・平均 fps はセッション長に汚染される。局所構造は汚染された run からでも結論を出せる）
- `Tee-Object` はパイプが閉じるまで書き出さないため、**プロセスを終了させるまでログは 0 バイトに見える**

### 疑わしい増幅段の候補（issue が挙げたもの + 本調査）

| 候補 | 状態 |
|---|---|
| `wake_results` の無条件呼び出し（main のフレーム増が results へ等倍伝播） | **実在を確認**（`view.rs:850`）。ただし上記のとおり load-bearing |
| `position_results_below_main` が main のフレームごとに `SetWindowPos` を無ガードで撃つ | **実在を確認**（`view.rs:837` → `mod.rs`。#646 PR2 決定 10 で意図的に無ガード）。results 側の OS 由来 repaint（cause `-`）の出所候補 |
| `predicted_dt = 0`（#628）で sleep 明けフレームの `stable_dt` が 0 になる | **未検証**（issue も「未検証である」と明記） |
| `ScrollArea` のフローティングスクロールバーの hover フェード（`scroll_area.rs:1473` `animate_bool_responsive`） | egui 内蔵。#628 の再測でアイドル計測を汚染した実績あり |

---

## #699 — `clicked` が世代を運ばない

### 経路（実測で確認）

| 段 | `file:line` | コード |
|---|---|---|
| 積む | `results_view.rs:487-490` | `if let Some(i) = clicked { *shared.clicked.lock().unwrap() = Some(i); wake_main(&self.app_handle); }` |
| 型 | `results_view.rs:72` | `pub clicked: std::sync::Mutex<Option<usize>>` — **裸の行 index** |
| 消費 | `view.rs:1763-1766` | `let clicked = shared.clicked.lock().unwrap().take(); if let Some(i) = clicked { self.activate_or_execute(i, &ctx); }` |

消費は snapshot を publish した**直後**の同一ブロック内にある（`view.rs:1751-1766`）。

### 世代は既に存在する

`RowsSnapshot.generation`（`results_view.rs`）は「結果集合が総入れ替えされるたびに main が加算するカウンタ」として**既にある**。加算点は 3 箇所:

| `file:line` | 契機 |
|---|---|
| `view.rs:389` | 起動 spawn（`set_results(Vec::new())` と同時） |
| `view.rs:535` | reset（クエリ・結果クリア） |
| `view.rs:904` | `run_search_with` — **folder ナビ drain / index 世代検知 / 起動 drain のすべてがここへ合流する** |

issue が「総入れ替えを起こす」と列挙した 3 経路は、いずれも `run_search_with` を通る。**世代カウンタは既に全経路を覆っている**——欠けているのは `clicked` がそれを参照しないことだけである。

### 対応案 1 が既存構造に載る根拠

results 側は `update()` の中で `snapshot`（`generation` を含む）を手に持ったまま行を描いており、**クリックを積む時点で `snapshot.generation` をそのまま添えられる**。消費側は `self.snapshot_generation` と照合すればよい。

`clicked: Mutex<Option<(u64, usize)>>` への変更で、**フィールドを足したら `matches` の分解束縛が漏れを compile-fail にする**という `RowsSnapshot` の既存規律（`results_view.rs` の `matches` doc）と同型の守り方は使えない（`clicked` は `matches` を持たない）。ここは別途、消費側の照合を書く。

### 既存ガードが覆わない理由（issue の主張・コードで確認）

- `activate_or_execute` → `activate` の `.get(index)` は**境界チェックであって行の同一性チェックではない**
- `clicked` は **reset-on-show のクリア対象に入っていない**——`view.rs` の `reset_pending` 消費ブロックは view-local だけを一掃し、managed state の `ResultsShared` を触らない

**実害の有無は未確認**（issue も「観測ではなくコード読解に基づく報告である」と明記）。

---

## #675 — 結果窓が作業領域の下端をはみ出す

### 現状（実測）

- 高さ: `layout::results_window_height(count, max_results, row_height)` = `min(count, max_results) * row_height + 8.0`。**作業領域を参照しない純粋関数**（`layout.rs`。ユニットテスト 4 件あり）
- 位置: `mod.rs::position_results_below_main` が `main.outer_position().y + main.outer_size().height + gap*scale` へ**無条件**に置く
- main 側のクランプ: `position_on_target_monitor` は**メイン窓単体のサイズ**で作業領域に収める

### 使える既存 API

`src-tauri/src/monitor.rs`:

| 関数 | 用途 |
|---|---|
| `window_monitor_work_area(hwnd_raw) -> Option<WorkArea>` | 窓が載っているモニタの作業領域 |
| `WorkArea::height()` / `clamp(x, y, win_w, win_h)` / `center(...)` | 既存の算術 |

`cursor_monitor_work_area` / `primary_monitor_work_area` もある。

### 設計上の分岐（純粋核 / imperative shell）

issue の対応案 1 は「結果窓の高さを作業領域の下端でクランプする（あふれた分は既存の `ScrollArea` に委ねる）」。実装位置に 2 案ある。

- **(a) `results_window_height` に引数を足す**（利用可能高を渡す）: 純粋関数のままユニットテスト可能。`docs/development-principles.md`「構造的設計原則と強制の階梯」の 3（純ロジックと副作用の分離をフラクタルに適用）に沿う
- **(b) 呼び出し側（`drive_results_window`）でクランプする**: Win32 呼び出しの直近だが、**クランプ算術がテストの外へ出る**

issue 本文は (b) 寄りの書き方（「`results_window_height` の**呼び出し側**に効かせる」）だが、**Win32 から作業領域を取るのは shell、`min` を取るのは純粋核**という分け方なら (a) でも矛盾しない。計画で決める。

### 注意（`layout.rs` の契約）

`results_window_height` は「**0 件は 0.0（呼び出し側が hide する契約）**」を持つ。クランプで 0 になると `results_should_show` が hide 側へ倒れるため、**下限のガードが要る**（例: 1 行分は必ず残す）。

---

## 関連コード（実在確認済み）

| パス | 役割 | 触る見込み |
|---|---|---|
| `src-tauri/src/egui_shell/view.rs:800-851` | `drive_results_window`（#675 の高さ・#710 の無条件 wake） | **高** |
| `src-tauri/src/egui_shell/view.rs:1751-1766` | snapshot publish + `clicked` 消費（#699） | **高** |
| `src-tauri/src/egui_shell/results_view.rs:66-73` | `ResultsShared`（#699 の型） | **高** |
| `src-tauri/src/egui_shell/results_view.rs:487-490` | `clicked` を積む（#699） | **高** |
| `src-tauri/src/egui_shell/layout.rs:71-79` | `results_window_height`（#675） | **高** |
| `src-tauri/src/egui_shell/mod.rs:557-586` | `wake_results` / `position_results_below_main` | 中 |
| `src-tauri/src/monitor.rs:77` | `window_monitor_work_area` | 中（#675） |
| `src-tauri/src/egui_shell/mod.rs` の `register_config_wake_listeners` | #710 で無条件 wake を消すなら**代替経路の置き場** | 条件付き |
| `src-tauri/src/egui_shell/results_window.rs` | raw Win32 の show/hide/topmost/set_size/set_position | 低（読むだけ） |

---

## 既存パターン

- **世代で stale を弾く**: `RowsSnapshot.generation` ↔ `ResultsView` の scroll gate（#632 Important 3）。#699 はこれを `clicked` へ広げるだけで、**新しい概念を導入しない**
- **純粋核へ算術を寄せる**: `layout.rs` は `Metrics` / `results_window_height` / `main_window_height` / `results_should_show` / `Debouncer` を持つテスト可能な層。#675 の `min` はここに載る形
- **無ガードの単一点**: `position_results_below_main` は「デルタガードを持たない（`set_position` は同値でも安価・ガードは update 側の責務）」と明記された設計（#646 PR2 決定 10）。#710 でここを触るなら、**この決定を覆す判断**になる

---

## 技術的制約

- **Win32 依存**: `monitor.rs` は `MonitorFromWindow` / `GetMonitorInfoW`。物理座標ベース（`position_results_below_main` も物理座標で `gap * scale` 換算している）
- **2 窓の層の混在禁止**（`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」）: results は show/hide/topmost の 3 操作すべて raw。`set_size` / `set_position` は `apply_diff` に入るが MAXIMIZED 差分が空ゆえ冒頭 return で助かるため tao 経由のまま——**差分を生む操作を足すと results 窓が消える**
- **hidden 中は `update()` が走らない**（SU5 要石）。時限処理は可視中しか効かない
- **`.await` は updater 経路のみ**。本束は同期コード
- **検証**: `.rs` を触るので PostToolUse hook が clippy + テストを自動実行（沈黙 = 合格）。`docs/build-commands.md` カテゴリ A に加え、**ウィンドウ表示順・フレーム挙動を触るためカテゴリ C（`smoke:egui`）と D（目視）も該当**（`.claude/rules/src-tauri.md`「トリガー → 検査」）

---

## 未解決の疑問（ユーザー確認事項）

**#710 の測定をいつ・どう取るか。**

issue の受け入れ条件 1 は「同一条件（同じクエリ・同じホバー操作・release）で A/B を取る」で、**人間の実操作が要る**（`GetAsyncKeyState` 依存の自動化不能な系ではないが、ホバー・スクロールの操作量を揃える必要がある）。

さらに順序の問題がある。**#710 の結論が「無条件 wake を edge-triggered へ」なら、それは `drive_results_window` の同じ関数を #675 も触る**。測定を後回しにすると、測る対象が自分の変更で動いてしまう。

したがって:

- **測定を先に取る**なら、`main`（= `f277831`）で release ビルドして 1 セッション実施 → その結果で #710 の要否を判定 → 3 件の設計を確定
- **#675 / #699 を先に実装する**なら、測定基線が「束B の変更後」になり、#628 前後との比較という受け入れ条件 1 から離れる

**私の推奨は「測定を先に取る」。** ただし #710 は issue 自身が「**優先度は低い**」「ユーザーからの体感の訴えも出ていない」と述べており、**#710 を今サイクルから外して #699 + #675 だけを実装する**判断もありうる（その場合 #710 は単独で残す）。
