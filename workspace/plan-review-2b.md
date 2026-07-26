# plan-review-2b（#699 / #675 の独立再導出）

> **受け渡しの注記（主エージェントによる追記）**: このファイルは独立導出エージェント本人が書けなかった。
> `/plan-review`「Step 2b — 独立導出 + 差分（常に実施・盲点クラスの漏れ検出）」が指定する `Plan` タイプは
> **Write / Edit ツールを持たない**（agent type の定義が `Edit, Write` を除外している）。
> エージェントは `SendMessage` で本文を返し、主エージェントがそのまま保存した。
> **#733 で明文化した「成果物は `workspace/plan-review-2b.md` へ書かせる」は `Plan` タイプでは実行不能である**——
> 追随の修正が要る（下記「主エージェントによる検証」の末尾）。

---

## 0. 結論サマリ

- **#699 の対応案 1（`clicked` に世代を同梱）は、そのままでは成立しない。** `snapshot_generation` を進めずに行集合を総入れ替えする経路が **3 本**ある（`SearchState::reset` / `enter_tool` / `on_escape`）。特に `on_escape` は**同一フレームで消費より前**に走る（view.rs:1293 → publish view.rs:1750 → take view.rs:1763）ため、世代が一致したまま別の行集合に対して古い index が適用される——issue が塞ごうとした穴がそのまま残る。#699 は「clicked に世代を積む」+「世代 bump 漏れ 3 本を塞ぐ」の 2 点セットでなければ効かない。
- **#675 の対応案 1 は成立する。** ただし (a) 物理／論理の換算が要る（`set_size` は `LogicalSize`、work area と main 位置は物理 px）、(b) 換算に使う scale factor は **results 窓自身のもの**（tao 0.35.3 の `set_inner_size` が `self.scale_factor()` を使う）、(c) クランプ結果が 0 になると `results_should_show` の「高さ 0 = hide」契約に化けるため 1 行分の床が要る。issue が「`monitor.rs` の作業領域取得が既にある」と書いた前提は現存する（`window_monitor_work_area` は `save_placement_relative` が実使用中）。
- `wake_results`（view.rs:850）には触らない（#710 実測の結論どおり）。私の #675 の編集はその数行上（view.rs:837-844）に落ちる。

---

## 1. 変更が必要なファイル・行（全列挙）

### A 群: #699 本体（clicked に世代を積む）

**A-1 `src-tauri/src/egui_shell/results_view.rs:71-72`** — `Mutex<Option<usize>>` → `Mutex<Option<(u64, usize)>>`（`(描画時の generation, 行 index)`）。doc comment に「**index は世代とセットでしか意味を持たない**」と「`snapshot` は世代を運び `clicked` は運ばない非対称は見落としであり、本変更で解消した」を書く（issue「備考」が要求している結論の明文化）。

**A-2 `src-tauri/src/egui_shell/results_view.rs:487-489`** — `Some((snapshot.generation, i))`。`snapshot` は同 update() 冒頭で clone 済みなので、**実際に描いた行集合の世代**をそのまま添えられる。

**A-3 `src-tauri/src/egui_shell/view.rs:1763-1766`** — 消費側で世代照合。

- **`take()` は不一致でも行う**（＝破棄）。残すと次フレーム以降に同じ stale クリックが再評価される。
- **消費位置（publish の後）を動かさない**という順序不変条件を doc comment にする。理由は「guard が比較する世代は、そのフレームで行を差し替えうる全ハンドラ（escape=view.rs:1293 / index 世代検知=1184 / folder drain / launch 完了）の**後**の値でなければ、issue の窓を塞げないから」。
- stale 破棄時の trace（`egui_results:click_stale`）を出す。`scripts/smoke-egui.ps1` は `egui_results:show` / `:hide` しか見ないので影響なし。

**A-4 `src-tauri/src/egui_shell/layout.rs`** — 判定を純粋核へ出す（`click_is_current(stamped, current) -> bool`）。**推奨であって必須ではない**（inline の `==` でも動作は同じ）。

### B 群: #699 の**成立条件**（世代 bump 漏れ 3 本）— ここが本 issue の核心

現状 `snapshot_generation` を進めるのは **3 箇所だけ**（view.rs:389 / 535 / 904）。一方 `results` を差し替える経路は:

| 経路 | 実装 | 世代 bump |
|---|---|---|
| `run_search_with` | view.rs:904（先頭で +1）→ set_results 910/913/924/946/969/983/985 | ✅ |
| `start_launch` | view.rs:387 set_results + 389 bump | ✅ |
| `clear_search` | view.rs:532 set_results + 535 bump | ✅ |
| **`SearchState::reset()`** | search_state.rs:330-338（`self.results.clear()`）呼び出し元 view.rs:1145 付近 | ❌ |
| **`SearchState::enter_tool()`** | search_state.rs:269-290（`self.results = rows`）呼び出し元 view.rs:652 | ❌ |
| **`SearchState::on_escape()`** | search_state.rs:308-326（`self.results = t.restore_results` / `= f.restore_results`）呼び出し元 view.rs:1293 | ❌ |

- **`on_escape` が最も鋭い**: Escape 処理は view.rs:1293（publish=1750 / take=1763 より前）。results がツール行 3 件を描いて index 2 をクリック → 同フレームで main が Escape を処理して plain 結果へ復帰 → 世代は同じ → guard を通過 → `.get(2)` が None で助かる**か**、復帰結果が 3 件以上なら**別の行が起動する**。issue が言う「実害が薄い理由（reset 直後は空）」はこの経路を覆っていない。
- **`enter_tool` も同型**（ツール一覧へ総入れ替えなのに世代据え置き）。
- **これは #632 Fix 3（scroll gate）自体のバグでもある**: `RowsSnapshot.generation` が進まないので、Escape 復帰／ツール突入で行が総入れ替えされても results 側の scroll gate がリセットされない。

**推奨（案 A）: 世代を `SearchState` が所有する。**

- `search_state.rs` の `struct SearchState` に `rows_generation: u64` を追加、`new()` で 0 初期化
- `set_results` / `enter_tool` / `on_escape` の復帰 2 分岐 / `reset` で `self.rows_generation += 1`
- `pub fn rows_generation(&self) -> u64` を追加。doc に「**`results` を差し替える全メソッドがここを進める**」と、対象を「この型の中の `self.results` への代入」に限定して書く
- view.rs の手動 bump（279/318/389/535/904）を削除し、参照（1751/1755）を `self.state.rows_generation()` へ置換
- 効果: bump 漏れが構造的に起こらない。加えて view.rs:904 の**空撃ち**（`set_results` を呼ばないフレームでも +1）が消え、世代の意味が「行が差し替わった」と一致する

**代替（案 B）: view.rs 側の手動 bump のまま 3 箇所を足す** — 差分は小さいが、**bump 漏れを 3 回やった経路をもう一度手作業に委ねる**ため非推奨。

### C 群: #675（results 窓の高さを作業領域下端でクランプ）

**C-1 `layout.rs` に純粋核** — `clamp_results_height(desired, available: Option<f64>, row_height) -> f64`。`desired == 0.0` は素通し（`results_should_show` の「高さ 0 = hide」契約値ゆえクランプで 0 を作ってはならない）。`available` が 1 行未満でも **1 行 + padding 8 を床**にする。

**C-2 `mod.rs:568-585` `position_results_below_main` を `-> Option<i32>`** に変更（設定した results 上端の**物理** y を返す）。view.rs 側で再計算すると `outer_position` / `outer_size` / `window_gap` の 2 度読みになり、フレーム内 live-read の食い違いを作る。呼び出し元は mod.rs:283（戻り値無視）と view.rs:837（使用）。

**C-3 `results_window.rs` に `scale_factor()` アクセサ** — **`set_size` が渡す `LogicalSize` を tao が物理へ戻すときと同じ factor でなければならない**（tao 0.35.3 `platform_impl/windows/window.rs:273-276` の `set_inner_size` は **この窓の** `self.scale_factor()` で `to_physical` する）。main の scale を流用すると混在 DPI 環境で高さが食い違う。

**C-4 `mod.rs` に `results_available_height(app, top_y_physical) -> Option<f64>`** — 作業領域は **main の HWND** で決める（既に誤配置された results から引くと別モニターを掴みうる）。`#[cfg(windows)]` / `#[cfg(not(windows))]` の本体分割は `save_placement_relative`（mod.rs:501-522）に倣う——`monitor.rs` の関数群は**全て `#[cfg(windows)]` で非 Windows 版が無い**ため、cfg を跨いだ直呼びはコンパイルを壊す。

**C-5 `view.rs:837-844`** — 位置 → クランプ → サイズの順。**可視判定（`results_should_show`）は素の `res_h` のまま**にし、クランプは `set_size` に渡す値だけに効かせる。**`self.last_results_height` にはクランプ後の値を入れる**（素の値だとデルタガードの照合対象がずれる）。

**C-6 テスト（layout.rs）** — 取得不能 / 余裕あり / 下端で切る / 床 / 0 件は 0 の 5 ケース。

### D 群: 文書同期

**D-1 `SPEC.md:172`（results 高さの SSOT）** に「作業領域の下端でクランプし、あふれた行はスクロールで到達する。作業領域が 1 行に満たない場合でも 1 行分は表示する」を追記。`docs/superpowers/specs/2026-07-24-646-two-window-ui-design.md` は派生コピーなので SSOT を直したうえで参照を保つ。

**D-2 `SPEC.md:413`** の全称主張の限定（下記 3-③）。

**D-3** `src-tauri/CLAUDE.md` のモジュール構成節は新規ファイルが無いので変更不要。

---

## 2. 変更不要と判断した候補（と理由）

1. **`wake_results`（view.rs:850）** — 触らない。#710 実測で正常、かつ config hot-reload の唯一経路。C-5 の編集はこの直前に落ちるので、**削らないこと**を PR 本文に明記する
2. **`clicked` の reset-on-show クリア（issue が「クリア対象に入っていない」と指摘）** — **追加不要**。B 群で `reset()` が世代を進めれば hide→show を跨いだ stale クリックは自動的に破棄される。専用 clear を足すと「クリアする経路」と「世代で弾く経路」の 2 本立てになり、どちらが正本か読めなくなる
3. **issue の案 2（path 等の行同一性を運ぶ）** — 採らない。(a) 同一 path が複数行に出る経路があり view.rs:667-670 が「パス文字列照合は禁止」と明記、(b) `activate_or_execute` は index を取る 3 分岐で、path→index の逆引きは分岐ごとに規則が要る
4. **issue の案 3（doc comment で不変条件化し実害無しと判定）** — 採らない。B 群で実経路（`on_escape`）を特定したので「確率が低い」の前提が崩れている
5. **issue #675 の案 2（main のクランプに結果窓の想定高を含める）** — 採らない（issue の判断に同意）。`main_window_height` は status/toast で伸縮するため、結果高まで足すと入力中の縦揺れが 2 要因になる
6. **`set_size` を `PhysicalSize` 化** — 採らない。幅が config の**論理** px なので物理化すると幅の換算が新たに要り、単位の混在点が減らない。なお同関数 doc の「tao 経由のままにする」根拠（フラグ差分が空）は `PhysicalSize` でも同じで、**単位の話ではない**——採否の理由を取り違えないこと
7. **`monitor.rs`** — 変更不要。`clamp()` は「窓全体を作業領域に収める」x/y 補正で、今回要るのは「上端固定で高さを削る」なので流用しない
8. **`results_window_height`** — 変更不要。0 件 → 0.0 の契約を維持し、クランプは別関数で外掛け（既存テストが無傷 = その不変条件を孤立させない）
9. **`RowsSnapshot` / `matches`** — 変更不要。`clicked` の型変更は等値判定に触れない
10. **`scripts/smoke-egui.ps1`** — 変更不要

---

## 3. issue に無いが同クラスの発見事項

**① `enter_tool` / `on_escape` / `reset` の世代据え置きは #632 Fix 3（scroll gate）のバグでもある**（B 群参照）。#699 の修正で副次的に直る。

**② main 窓自身が作業領域の下端をはみ出しうる**（#675 と同クラス・issue 未記載）。`show_egui_main` は **bar 高へ畳んでから** `position_on_target_monitor` でクランプする（mod.rs:378-392）が、その後 view が status 行 + toast 行を積んで伸ばす（view.rs:1775-1785）際に**再クランプが無い**。既定 `bar_height=43` に対し status + toast で最大 +86 論理 px。下端付近で「indexing 中に updater トーストが出る」と main の下部が作業領域外へ出る。**確信度**: コードで裏取り済み・実機未観測。

**③ `SPEC.md:413` の全称主張が as-built と食い違う。**
> ターゲットモニターの作業領域にクランプし、**ウィンドウが画面外に出ないことを保証する**

#646 PR2 決定 10 のドラッグ移動（view.rs:1117-1126 `frame.drag_window()`）は OS の move loop に委ね、`Moved` リスナーも results の追従だけで main のクランプをしない。主張は「**ホットキー表示時の配置について**保証する」に限定すべき。**確信度**: 高。

**④ 並行境界: `clicked` の消費と `snapshot` の publish は別ロック**で、間に results 窓のフレームが挟まりうる。世代を積む修正はこのケースでも安全側。**新規のロック順序を作らない**こと（snapshot → clicked の順は現状維持）。**確信度**: 中（イベントループ単一スレッド性は未検証）。

**⑤ ドラッグ中は結果窓の高さが再クランプされない（#675 の受容残余）。** `Moved` リスナーは位置だけを追従させ、C-2 の戻り値を無視する。リスナー側で高さを直すと「main が唯一の size writer」の不変条件を壊す。→ **受容残余として明記する**（是正はしない）。

**⑥ 混在 DPI の残余。** `set_position` 直後の tao 側 scale factor がまだ旧モニターのものである可能性がある。実害は「モニター跨ぎの瞬間の 1 フレーム」に限られる見込み。**確信度**: 低（未測定）。

**⑦ 影の有無（単位の落とし穴の確認）。** `WindowBuilderWrapper::new()` は `shadow()` を呼ばないため tao の `MARKER_UNDECORATED_SHADOW` は立たず、`set_inner_size` の影オフセット加算に入らない = **`outer_size` と `inner_size` が一致する**。C 群の計算で client/window 矩形の差を考慮しなくてよい根拠。**確信度**: 高（一次ソース読解・実測なし）。

---

## 4. 確信度

| 項目 | 確信度 |
|---|---|
| `clicked` が裸の index | **コードで確認** |
| 世代 bump が 3 箇所だけ／`reset`・`enter_tool`・`on_escape` に無い | **コードで確認** |
| `on_escape` が消費より前のフレーム内に走る | **コードで確認** |
| `results_should_show` の「高さ 0 = hide」契約 | **コードで確認** |
| `set_size` が論理 px・位置が物理 px | **コードで確認** |
| 換算に使うのは results 窓の scale factor | 一次ソース読解（未実測） |
| 影オフセットが乗らない | 一次ソース読解（未実測） |
| main 自身の下端はみ出し（②） | コードで確認・実機未観測 |
| SPEC:413 の全称主張の破れ（③） | 高 |
| クリック取りこぼし（false negative）の頻度 | **読解のみ・未測定**。受容として明記すること |
| 混在 DPI 残余（⑥） | 未検証 |

---

## 5. 実装順序と検証（導出側の提案）

1. **B 群（世代の所有権移動）を単独で入れる**。`rows_generation` の前進テスト（`set_results` / `enter_tool` / `on_escape` 両分岐 / `reset` で +1、`move_selection` では不変）を追加。**ここが緑にならない限り A 群は意味を持たない**
2. **A 群（clicked の世代同梱）**。型変更で両端が compile-fail する（移行漏れ検出器）
3. **C 群（#675）**。純粋核テストを先に赤で書く
4. **D 群（SPEC 同期）**
5. 検証: カテゴリ A + **カテゴリ C**（`smoke:startup` / `smoke:egui`）+ **カテゴリ D**（main を画面下端へドラッグ → 8 件以上ヒット → 下端がタスクバー上端で止まる／スクロールで最終行に到達）+ `governance:check`
6. 該当スキル: `/race-check`・`/state-check`・`/symmetric-check`
