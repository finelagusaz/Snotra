# plan — #749 段1: WindowCoordinator

前提: `workspace/research.md`。ブランチ `chore/window-coordinator`（基点 `a98312c`）。
**挙動は変えない**（issue 明記）。差分の大半は移設であり、意味を持つ判断は次の 3 点だけである:

1. `drive_results_window` のデルタガード 2 フィールドの行き先（→ `ResultsWindow`）
2. その判定式を純粋核へ出すこと（本 PR で唯一得られる自動カバレッジ・呼び出し点は 2 つ）
3. どのヘルパーを一緒に運ぶかの規則（下記「分割の規則」）

## 分割の規則（衝突したときにどれが勝つか）

線が場当たりに引かれると、後から「これはどちらの責務か」に答えられなくなる。**規則は 1 本にし、例外を作らない**:

> **R: 移設する関数が、その中でしか使わないヘルパーは一緒に運ぶ。複数のモジュールから消費されるものは残す。**

適用結果（全 5 件・例外なし）:

| ヘルパー | 消費者 | 判定 |
|---|---|---|
| `read_metrics` | `show_egui_main` のみ（`mod.rs:392`・grep 実測） | **運ぶ** |
| `max_results` | `drive_results_window` のみ（`view.rs:818`・grep 実測） | **運ぶ** |
| `read_visual` | `view.rs` と `results_view.rs`（coordinator の外に 2 つ） | 残す |
| `window_width` | view の main 幅適用 + drive への引数（view 内に 2 用途） | 残す |
| `position_on_target_monitor` | `show_egui_main` のみ（`mod.rs:396`・grep 実測） | **運ぶ**（`main.rs` から） |

**listener 登録（`register_*` の 4 関数）はすべて `mod.rs` に残す。** これも例外を作らないための規則である——`register_hide_listener` だけを「hide の合流点だから」と動かすと、`show_egui_main` と `wake_main` を呼ぶ `register_initial_hotkey_failure_listener` が残る理由を説明できなくなる。setup 配線の一覧性は `main.rs` の 1 画面に残す設計（spec 決定 8）と揃える。

### 集約しきれないもの（**全称表現を使わないための明記**）

**z-order は 1 か所に集まらない。** main の最前面切り替えは `commands/window.rs:94,140` が `set_always_on_top` を直接叩き、results は `ResultsWindow::set_topmost` が持つ（tao の差分適用が results を消すため層が違う・#646 PR2）。呼び出し元はどちらも**設定サイドカー監視のポーリングスレッド**であり、coordinator を通らない。**段 1 で z-order を移さない**——依存の向きが増えるうえ、issue が段 1 を「ほぼ移設」と規定しているためである。ゆえに新モジュールの `//!` に z-order を書かない（`AGENTS.md`「全称表現は前提条件とセットで書く。書けないなら書かない」）。

同様に、**main 窓のサイズ適用は view に残る**（`view.rs` の毎フレーム `set_size`）。ADR-0007 却下 1 の第 3 理由——main の高さは `show_egui_main` の `bar_height` collapse と `main_window_height` の**意図的な 2 導出**であり、その材料（`has_status` / `has_toast`）は描画パスの副産物である——を段 1 が巻き戻さないため。

## 変更ファイル一覧（9 件）

| ファイル | 種別 | 内容 |
|---|---|---|
| `src-tauri/src/egui_shell/window_coordinator.rs` | **新規** | 移設先。`mod.rs` から 8・`main.rs` から 1・`view.rs` から 2 の**計 11 関数**（内訳は Phase 3 / 4） |
| `src-tauri/src/egui_shell/mod.rs` | Modify | 上記 8 関数を削除し `mod` 宣言 + re-export を足す。`//!` を更新。自己言及コメント 6 か所を訂正 |
| `src-tauri/src/main.rs` | Modify | `position_on_target_monitor` を削除（移設） |
| `src-tauri/src/egui_shell/view.rs` | Modify | `drive_results_window` / `max_results` / results 用ガード 2 フィールドを撤去。main 窓ガードの手書き式を `size_delta_exceeds` へ。`reset_size_guard` 呼び出しへ置換 |
| `src-tauri/src/egui_shell/results_window.rs` | Modify | `last_size: Mutex<(f64, f64)>` を内包。`set_size` を自己ガード化。`reset_size_guard` を追加。`//!` を更新 |
| `src-tauri/src/egui_shell/layout.rs` | Modify | 純粋述語 `size_delta_exceeds` + 境界テスト 2 本。doc 内の `mod.rs::` 名指し 2 か所を訂正 |
| `src-tauri/src/egui_shell/visual.rs` | Modify | doc 内の `mod.rs::position_results_below_main` を訂正（1 か所） |
| `src-tauri/CLAUDE.md` | Modify | 「モジュール構成」の `egui_shell/` **箇条書き 1 行**の中の 5 か所（判定表は Phase 5） |
| `docs/architecture.md` | Modify | 82 行（size writer の所在）/ 172 行（シーケンス図） |

**変更しないもの**（根拠つき）:

- `SPEC.md` — §8.5 の「`main` の毎フレーム更新（`drive_results_window`）が駆動する」は、関数名も呼び出し元（main の `update()`）も変えないため**真のまま**。§8.6 の 4 連言は `layout::present_results` のまま無変更。§8.7 も挙動記述のみ
- `commands/window.rs` / `results_view.rs` — re-export により参照パスが不変ゆえ**差分ゼロ**
- `view.rs` の `//!`（1-8 行）— results 窓 driver を名指ししていない（実測）
- `main.rs` の `//!`（1-6 行）— 位置復元を名指ししていない（実測）
- managed state の構成（`app.manage` の順序・`EguiShellState` / `ResultsWindow` の載せ方）— 段 3（#666）の前提を動かさない
- `docs/superpowers/` 配下 — 歴史記録（`docs/superpowers/README.md`・`governance:check` 対象外）

## コミット粒度（Phase = コミットではない）

**Phase 1 単独・Phase 3 単独ではビルドが通らない。** `-D warnings` 下で未使用の新 API は `dead_code` で落ち、re-export は移設前のシンボルを指せないためである。次の 3 コミットに束ねる:

| コミット | 含む Phase | 通す検証 |
|---|---|---|
| **C1** | Phase 1 + Phase 2 | カテゴリ A（`cargo test -p snotra` = 176 passed） |
| **C2** | Phase 3 + Phase 4 | カテゴリ A + C |
| **C3** | Phase 5 | カテゴリ F |

## Phase 1 — 純粋述語の追加（`layout.rs`）

デルタガードの判定式を Win32 依存から切り離す。`ResultsWindow` は `tauri::Window` を持つためユニットテストできない（`.claude/rules/src-tauri.md`）——判定式だけを純粋核に置けば `0.5` の許容境界にテストを置ける。

```rust
/// 窓の再サイズが要るか（#749）。`set_size` は Win32 呼び出しゆえ、同値フレームで撃たない
/// ためのデルタガードである。**correctness のフラグではない**（results の可視性は
/// `ResultsWindow` の `visible` が持つ・#671 spec 決定 2 の意図的な分割）。
/// 許容 0.5 は論理 px の丸め差を吸収する値で、#646 PR2 からの実測値を保つ。
/// 引数は `(幅, 高さ)`（`set_size(width, height)` の引数順と揃える）。
pub fn size_delta_exceeds(prev: (f64, f64), next: (f64, f64)) -> bool {
    (next.0 - prev.0).abs() > 0.5 || (next.1 - prev.1).abs() > 0.5
}
```

**呼び出し点は 2 つあり、Phase 2 と同じコミット（C1）で両方移行する**:

1. `ResultsWindow::set_size`（results 窓・Phase 2）
2. **`view.rs` の main 窓デルタガード**（`/dry-check` で発見・現在は `(height - self.last_set_height).abs() > 0.5 || (width - self.last_set_width).abs() > 0.5` と手書き。`self.last_set_height` で grep して位置を特定する）

**main 側は「式だけ」を共有し、状態（`last_set_height` / `last_set_width`）は view に残す**（上記「集約しきれないもの」）。results 側だけがガード状態ごと `ResultsWindow` へ移るのは、**results の唯一の size writer が main であり、窓の所有型が既に存在するから**である（`ResultsWindow::set_size` の呼び出し元は `view.rs:861` の 1 か所・grep 実測）。

追加テスト（`layout.rs` の `mod tests`）:

- `size_delta_exceeds_only_past_half_pixel` — ちょうど 0.5 は false、0.51 は true、負方向の -0.6 も true
- `size_delta_exceeds_watches_both_axes` — 幅だけ変化 / 高さだけ変化のそれぞれで true（片軸落としの回帰検出）

## Phase 2 — デルタガードを `ResultsWindow` へ移す

`drive_results_window` が `&mut self` を要求する唯一の理由が `last_results_height` / `last_results_width` である。この 2 つを窓の所有型へ移せば、driver は `&AppHandle` だけで書ける。

`results_window.rs`:

```rust
/// 直近 `set_size` の (幅, 高さ)。**`visible` とは概念が別**——こちらは冗長な Win32 呼び出しを
/// 避ける性能上のガードで、correctness のフラグではない（#671 spec 決定 2 / #749）。
/// `Mutex` である理由は `visible` が `AtomicBool` である理由と同じ（managed state は
/// `Send + Sync` を要求する）。f64 の組ゆえ atomic では表せない。
last_size: Mutex<(f64, f64)>,
```

- `set_size(&self, width, height)` を**自己ガード化**する。判定は **`layout::size_delta_exceeds` を呼ぶ**（比較式を手書きしない）。`show()` / `hide()` の「遷移したときだけ raw 操作を撃つ」idiom と同型
- **戻り値は返さない（`()`）。** `show()` / `hide()` が `bool` を返すのは呼び出し側が trace を 1 回だけ出すためで、`set_size` に対応する trace は無い
- **lock は Win32 呼び出しの前に手放す**（`/race-check` I12）。判定と memo 更新を guard 内で済ませ、**`drop(guard)` してから** `self.window.set_size(...)` を呼ぶ。`std::sync::Mutex` は再入不可であり、tao の `set_inner_size` は `set_window_flags` → `apply_diff` を経て窓プロシージャに至りうる
- `reset_size_guard(&self)` を足す。`last_size` を `(0.0, 0.0)` へ戻す（現行 `view.rs:1194-1195` と同値）。初期値も `(0.0, 0.0)`
- **`//!` を更新する**——現在の「生 Win32 の 3 点セットと**可視フラグ**を 1 つの型が同時に所有する」に、サイズ memo が加わる

`view.rs`:

- results 用ガードのフィールド 2 つとその初期化を削除
- `drive_results_window` 内の手書きガードを `results.set_size(width, applied_height);` の 1 行へ
- reset-on-show ブロックの 2 行を `results.reset_size_guard()` へ置換（`try_state::<ResultsWindow>()` 越し）
- main 窓ガードの手書き式を `layout::size_delta_exceeds` へ（Phase 1 の呼び出し点 2）

**`last_size` の書き手はイベントループスレッドの 2 経路だけである**（`drive_results_window` と `reset_size_guard`）。`Mutex` にするのは managed state の `Send + Sync` 要件のためであって、競合する書き手が居るからではない。`commands/window.rs` のポーリングスレッドが触るのは `set_topmost` だけで `last_size` を読まない（実測）。

**reset の置き場は view の reset-on-show ブロックに残す（`show_egui_main` へは移さない）。** `show_egui_main` はホットキー listener＝ Win32 メッセージループスレッドから走り、egui のフレームとは別スレッドである。今日この reset はイベントループスレッド上で、同一フレームの `drive` より前に起きる（I11）。`show_egui_main` へ移すと reset がフレーム進行中に割り込みうる——`Mutex` ゆえデータ競合は無く最悪でも「余分な `set_size` が 1 回」で済むが、**スレッド同一性という現行の前提を変えることになり「移設・意味変化ほぼゼロ」を外れる**。段 3 で view を割るときに再考する。

## Phase 3 — `window_coordinator.rs` を新設し 9 関数を移す

新規ファイルの `//!`（責務の正本・#562）:

```
//! 窓の可視性・位置・サイズ・wake を駆動する 1 つの責務（#749 段 1）。
//! 「撃つ主体」を集めた場所であって、「撃ってよいか」の判定は持たない——可視性の
//! 述語は `layout::present_results`（純粋核・#752）、results の raw 操作の所有点は
//! `results_window::ResultsWindow`（#671 PR A′）である。
//! **wake は primitive として公開する**（#711）——「いつ起こすか」を本モジュールが
//! 決めた瞬間に「armed 期限は保持者が毎フレーム再要求する」契約が壊れる。
//! **z-order は本モジュールに無い**——main は `commands/window.rs` が `set_always_on_top` を
//! 直接叩き、results は `ResultsWindow::set_topmost` が持つ（層が違うため・#646 PR2）。
//! 同じく **main 窓のサイズ適用は `view.rs` にある**（意図的な 2 導出・ADR-0007 却下 1）。
```

移設する 9 関数（本文は変えない。例外は下の「自己言及コメント」だけ）:

| 出所 | 関数 |
|---|---|
| `mod.rs`（8） | `show_egui_main` / `hide_egui_main` / `save_placement_relative` / `wake_main` / `wake_results` / `position_results_below_main` / `results_available_height`（`#[cfg(windows)]` と `#[cfg(not(windows))]` の 2 実装）/ `read_metrics` |
| `main.rs`（1） | `position_on_target_monitor` |

**`position_on_target_monitor` を含めるのは、issue の責務表を超える意図的なスコープ拡大である**（PR 本文にも書く）。issue の表はこれを挙げていない。それでも含めるのは、保存側 `save_placement_relative` が移るのに復元側だけ `main.rs` に残ると**「位置を 1 つの責務へ集めた」が偽になる**からである。`#[cfg(windows)]` のみで非 Windows の双子は無く、呼び出し側（`show_egui_main` 内）も `#[cfg(windows)]` ブロック内にある——**この非対称を移設で崩さない**。

**`main.rs` に「未使用になる `use`」は生じない**（実測）——`mod monitor;` はモジュール宣言であり、`monitor.rs` は `window_monitor_work_area` 経由で使われ続ける。`snotra_core::window_data` の `use` は関数本体内のローカル `use` で、関数ごと移る。**「コンパイラが検出する」ものは無いので探さないこと。**

mod.rs 側:

```rust
mod window_coordinator;
// main.rs（hotkey / tray / setup）・view.rs（driver）・results_view.rs（クリック逆流）が
// 消費する。窓操作の実体は window_coordinator.rs へ移した（#749 段 1）。
pub(crate) use window_coordinator::{
    DriveResultsInputs, drive_results_window, hide_egui_main, show_egui_main, wake_main,
    wake_results,
};
```

**re-export するのはモジュール外に消費者があるものだけ**である（`drive_results_window` / `DriveResultsInputs` は Phase 4 で足るので、**C2 として 1 コミットに束ねる**）。次の 5 つは再エクスポートしない——`save_placement_relative`（`hide_egui_main` のみ）・`results_available_height`（drive のみ）・`read_metrics`（`show_egui_main` のみ）・`max_results`（drive のみ）・`position_on_target_monitor`（`show_egui_main` のみ）。`mod.rs` 自身が使う `position_results_below_main`（`create` の `Moved` リスナー）は親モジュールから `window_coordinator::` で直に呼べるため re-export は要らない。

**`#[cfg(not(windows))]` の双子 arm を落とさない（検出器が無い）。** CI の rust-check は Windows のみで走るため、非 Windows の arm を落としても**誰も一度もコンパイルせず永久に気づかない**。移設対象は 2 か所（`save_placement_relative` 内のブロックと `results_available_height` の非 Windows 実装）。完了条件に**件数の照合**を入れる: `grep -c "cfg(not(windows))" src-tauri/src/egui_shell/*.rs src-tauri/src/main.rs`（**ファイルごとの件数が出るので手で合算する**）が移設前後で合計 7 のまま（ベースライン: `egui_shell/mod.rs` 2 + `egui_shell/results_window.rs` 3 + `main.rs` 2）。

**自己言及になるコメントだけは直す（6 か所）。** 移設後は「`view.rs` の drive」が同一モジュール内の関数を指すことになり誤記になる:

| 位置 | 現在の記述 |
|---|---|
| `mod.rs:28` | 「`view.rs`（drive）・`commands/window.rs`（topmost）が消費する」 |
| `mod.rs:457` | 「`view.rs` の `drive_results_window`」 |
| `mod.rs:481` | 「`view.rs` の `drive_results_window` は update **内**」 |
| `mod.rs:489` | 「ここと `view.rs` の `drive_results_window`」 |
| `mod.rs:494` | 「results 単独 hide（`view.rs` の drive）」 |
| `mod.rs:573` | 「ガードは update 側の責務」——Phase 2 でガードが `ResultsWindow` へ移るため偽になる |

**この 6 か所と `//!` 以外の本文は変えない**（触ったら移設ではなくなる）。なお `mod.rs:471` / `483` / `556` は `drive_results_window` に触れるがファイル名を伴わず、移設後も真である。

**移設時に「取り違えても全部通る」2 か所を目で照合する**（`/symmetric-check` 2c）:

| 箇所 | 取り違えの形 | 区別できる観測 |
|---|---|---|
| `wake_main` / `wake_results` | `EguiShellState` の `main_waker` と `results_waker` は**同じ `WindowWaker` 型**。2 つの短い関数を並べて移すとき入れ替わりうる（#671 PR D が「newtype でも取り違えは compile を通る」と実測） | 目視 5（results が visual 変更で描き直される）と、設定変更時に main が即座に描き直されること |
| `results_available_height` | 作業領域は **main の HWND** から引き、換算は **results の scale** を使う（doc が明記）。移設中に「揃える」方向へ正規化すると混在 DPI で高さが狂う | 混在 DPI 環境が要り**常時の検出器は無い**。移設前後の diff を目で照合するのが唯一の防御 |

移設に伴う参照の付け替え（コンパイラが検出する）:

- `show_egui_main` → `super::EguiShellState`、同モジュール内の `read_metrics` / `position_on_target_monitor`
- `hide_egui_main` → `super::EguiShellState` / `super::ResultsWindow`
- `position_results_below_main` / `results_available_height` → `super::layout` / `super::ResultsWindow`
- `position_on_target_monitor` → `crate::monitor` / `crate::AppState` / `snotra_core::window_data`
- doc コメント中の `mod.rs::position_results_below_main` / `mod.rs::results_available_height`（`layout.rs:102,118` と `visual.rs:5` の 3 件・grep 実測で全件）を `window_coordinator::` へ
- **`cargo doc --workspace --no-deps --document-private-items` を通す**——intra-doc link は CI で deny（`broken_intra_doc_links`）であり、モジュール移動で切れうる

## Phase 4 — `drive_results_window` と `max_results` を view.rs から移す

`window_coordinator.rs` へ自由関数として移す。**関数名を変えない**（`SPEC.md` / `docs/architecture.md` が名指しするため）。

```rust
/// `drive_results_window` の 1 フレーム分の入力（#749 段 1）。
///
/// **`result_count` の読み点は呼び出し側の責務である**（#752 F2 / ADR-0007）。同一フレーム内で
/// `plain_hidden` はクリック逆流の消費**前**、`result_count` は消費**後**に読む。
/// **この構造体を作る式を `plain_hidden` の算出の隣へ動かしてはならない**——行クリック起動
/// フレームで古い行が 1 フレーム描かれる。`cargo test` では落ちない種類の回帰である。
pub(crate) struct DriveResultsInputs {
    pub(crate) plain_hidden: bool,
    pub(crate) result_count: usize,
    pub(crate) width: f64,
    pub(crate) row_height: f64,
}

pub(crate) fn drive_results_window(app: &tauri::AppHandle, i: DriveResultsInputs) { ... }
```

本体は現行の `SearchWindowView::drive_results_window` をそのまま運ぶ（**行番号ではなくシンボルで探す**——Phase 2 でガードが 7 行から 1 行に縮むため行範囲はずれる）。差し替えは次の 4 点のみ:

| 現行 | 移設後 |
|---|---|
| `self.state.results().len()` | `i.result_count`（読み点は呼び出し側へ） |
| `self.max_results()` | 同モジュールへ移した `max_results(app)` |
| `metrics.row_height` | `i.row_height` |
| 手書きデルタガード | `results.set_size(i.width, applied_height);`（Phase 2 で自己ガード化済み） |

**引数の制約は 2 種類あり、混同しない**:

- **クリック逆流の消費との前後関係**を持つのは `result_count`（消費後）と `plain_hidden`（消費前）の 2 つだけである。`max_results` はこの制約を持たないため引数にせず `drive_results_window` の内側で読む——引数を増やすほど「呼び出し側で並べて書きたくなる」面積が広がり、I1 を壊す誘惑が増える
- **`width` と `row_height` は別種の制約を持つ**。`row_height` はフレーム冒頭の `VisualSnapshot` 由来でなければならず（#673 決定 4: テーマ値は 1 フレーム 1 回）、`width` は view が main へ適用するのと**同一フレームの同一値**でなければならない（両窓の唯一の size writer が main である前提・`view.rs` の `window_width` doc）。ゆえにこの 2 つは**引数のまま**にし、coordinator の内側で読み直さない

view.rs の呼び出し点（`update()` 末尾・`self.drive_results_window(...)` で grep）:

```rust
// **`result_count` はここで読む**——`take_clicked_for`（クリック逆流の消費・上のブロック）
// より後でなければならない（#752 F2 / ADR-0007）。この式を `plain_hidden` の算出
// （`show_results` の直前）へ動かすと、行クリック起動フレームで古い行が 1 フレーム描かれる。
crate::egui_shell::drive_results_window(
    &self.app_handle,
    crate::egui_shell::DriveResultsInputs {
        plain_hidden,
        result_count: self.state.results().len(),
        width,
        row_height: metrics.row_height,
    },
);
```

## Phase 5 — 文書同期

**責務記述の drift はサイトではなくクラスとして潰す。** 責務が変わるファイルは 6 つ。**各ファイルについて `//!` と `src-tauri/CLAUDE.md` の該当記述の両方を読んで判定する**（`governance:check` は `//!` の文言も CLAUDE.md の散文の妥当性も検査しない）。実測済みの判定:

| ファイル | `//!` | `src-tauri/CLAUDE.md` の記述 |
|---|---|---|
| `mod.rs` | **要更新**（show/hide・位置永続が偽になる） | **要更新** |
| `view.rs` | 不要（results 窓 driver を名指ししていない・実測） | **要更新**（「results 窓 driver」を落とす） |
| `results_window.rs` | **要更新**（Phase 2 でサイズ memo が加わる） | **要更新**（同文言） |
| `layout.rs` | 要判断（純粋核の列挙に `size_delta_exceeds` を足すか） | **要更新**——**シンボルを明示列挙している**（`Metrics` / `results_window_height` / `present_results` / `results_top_y` / `available_below` / `Debouncer`）ため `size_delta_exceeds` が欠ける |
| `main.rs` | 不要（1-6 行に位置復元の記述なし・実測） | 不要（`//!` へ委譲する 1 行） |
| `window_coordinator.rs` | 新規（Phase 3 の草稿） | 新規の記述 |

真のまま据え置くもの（実測）: `src-tauri/CLAUDE.md`「show の操作順序制約」は `position_on_target_monitor` を名指しするが**所在ではなく順序**を述べており移設後も真。`SPEC.md:412-415`（`follow_cursor_monitor` / 中央フォールバック）は関数名を持たない。

**`src-tauri/CLAUDE.md` の編集単位は「行」ではない。** `egui_shell/` の項目は**単一の箇条書き行**の中に全サブモジュールの責務が読点区切りで同居している（実測）。その 1 行の中の 5 か所（新規 `window_coordinator.rs` / `mod.rs` / `view.rs` / `results_window.rs` / `layout.rs`）を直す。**書式は「ファイル名 + 一言の責務要約」**（既存記述と段 2 の PR #756 の実差分で確認）——「索引だから名前だけ」と読んで要約を省かない。

`docs/architecture.md`:

- **82 行**——「`results` の高さは…`main` が算出し `set_size` する（**view 単独 size writer**）」。算出は view、適用は coordinator + `ResultsWindow` になるため**偽になる**
- **83 行**——「`results` の位置・可視性は `main` の毎フレーム更新（`drive_results_window`）が駆動する」は**真のまま**（駆動主体も関数名も変わらない）。触らない
- **172 行**——シーケンス図の `View->>View: … （drive_results_window）` は自己呼び出しの矢印。`participant` 一覧（155-160 行）に coordinator が無いため、**participant を足さずラベルだけを実態に合わせる**（段 3 で view を割るときに図の構造を見直す方が安い）

`npm run governance:check` を実行する（新規ファイル追加＝索引更新漏れが #629/#630 で 2 回再発）。**母集団は `fs.readdirSync` による作業ツリー走査であり追跡状態を見ない**（`scripts/governance-check.mjs:33-37` の実装コメントが「列挙は fs 自身に問う」と明言）——ゆえに `git add` の前後どちらでも新規ファイルは検出される。

## 不変条件

| # | 不変条件 | 壊れたときの症状 | 検知手段 |
|---|---|---|---|
| I1 | `result_count` は `take_clicked_for` より**後**に読む | 行クリック起動フレームで古い行が 1 フレーム描かれる | 目視 3（**自動検出器なし**） |
| I2 | `plain_hidden` は 1 フレーム 1 回だけ読み、クリック逆流の消費**前** | `indexing` の live-read がフレーム内で食い違い、snapshot と窓判定が矛盾 | コードレビュー（`plain_results_hidden` の呼び出しが 1 か所） |
| I3 | `hide_egui_main` は `main_visible=false` → `results.hide()` の順 | main だけ消えて results が最前面に残る | 目視 2（**自動検出器なし**——`smoke:egui` は presence のみ・#671 PR A′） |
| I4 | `show_egui_main` は `window.show()` → `main_visible=true` の順 | show 完了前のホットキートグルが hide する | **自動検出器なし・目視も困難**（競合窓が狭い）。移設で行の並びを変えないことをコードレビューで担保 |
| I5 | `drive_results_window` 末尾の `wake_results` は無条件（level-triggered） | visual だけの config 変更が results に反映されない | 目視 5 |
| I6 | 可視判定は `desired_height`（クランプ**前**）、`set_size` とガードは `applied_height`（クランプ**後**） | 不可視フレームでも `SetWindowPos` を撃つ / 下端クランプが効かない | 目視 7 + `layout` の既存テスト |
| I7 | results の 3 操作（show / hide / topmost）は raw、`set_size` / `set_position` は tao 経由 | `set_always_on_top` 系が results を消す / `hide()` が no-op になる | 目視 2・6 |
| I8 | wake は primitive（Coordinator が期限を決めない） | armed 期限の再要求が止まり、hide や再検索が次の入力まで宙吊り | コードレビュー（`request_repaint_after` が view にしか無いこと） |
| I9 | `#[cfg(not(windows))]` の双子 arm を落とさない | 非 Windows ビルドが壊れる。**CI は Windows のみゆえ永久に気づかない** | Phase 3 の件数照合（合計 7・下の「Phase 完了条件」） |
| I10 | `position_on_target_monitor` は `#[cfg(windows)]` のみ・呼び出し側も同ブロック内 | 非 Windows で未定義参照 | I9 と同じ残余（Windows では検出できない） |
| I11 | `reset_size_guard` は**同一フレームの `drive_results_window` より前**（reset-on-show は `update()` 冒頭側・drive は末尾） | 再 show 後の 1 フレーム目が旧 metrics のサイズで描かれる | 目視 10 |
| I12 | `set_size` は memo の lock を**手放してから** Win32 を呼ぶ | 将来 tao 側が再入したときデッドロック（`std::sync::Mutex` は再入不可） | コードレビュー（`drop(guard)` の位置） |
| I13 | `position_results_below_main` の**第 2 の消費者**（`create` の `Moved` リスナー）が生きている | ネイティブ移動ループ中に results が追従しない | 目視 4 |

### 新たに導入する状態の異常系

`ResultsWindow.last_size: Mutex<(f64, f64)>` が本 PR で増える唯一の状態である（他は既存 view-local フィールドの移設であり純増ではない）。

- **`reset_size_guard` が呼ばれなかったら**: 再 show 後、前回と同じ論理サイズなら `set_size` を撃たない。窓は前回のサイズを保っているため**実害は無い**（ガードは性能用）。ただし hide 中に `font_size` を変えた場合、再 show 後の 1 フレーム目で旧行高のサイズが残りうる → 目視 10
- **並行アクセス**: 書き手はイベントループスレッドの 2 経路のみ（上記 Phase 2）。`commands/window.rs` のポーリングスレッドは `set_topmost` だけを触る
- **lock poisoning**: `lock().unwrap()` は `EguiShellState.pending_hotkey_failure` 等の既存様式に揃える。release は `panic="abort"` ゆえ到達しない
- **`ResultsWindow` が managed でない**（setup 完了前の理論経路）: `try_state` が `None` を返し `drive_results_window` は早期 return（現行と同一）
- **新モジュールそのものの異常系は無い**（状態を持たない自由関数の集合であり、初期化も破棄も無い）

## Phase 完了条件

| Phase | 完了条件 |
|---|---|
| 1 | `cargo test -p snotra` に 2 本増えて全緑（C1 として Phase 2 と同時に測る） |
| 2 | カテゴリ A が全緑。`view.rs` に `last_results_` で始まる識別子が 0 件（grep） |
| 3 | `cargo check --workspace` が通る（C2 として Phase 4 と同時）。`cfg(not(windows))` の合計が 7（手で合算）。`cargo doc` が通る |
| 4 | カテゴリ A + C が全緑。`view.rs` に `drive_results_window` の**定義**が 0 件（呼び出しは 1 件） |
| 5 | `npm run governance:check` が緑。上の判定表 6 行すべてに ✓ が付く |

## テスト方針

| カテゴリ | 内容 |
|---|---|
| A（必須） | `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`（doc コメントを多数動かすため必須） |
| C（必須） | `npm test` / `npm run smoke:startup` / **`pwsh -NoProfile -File scripts/smoke-egui.ps1 -ResultsQuery <開発機の索引に一致する 1 文字>`** |
| D（**必須**） | `cargo run -p snotra` で下の 10 項目を目視。issue が「カテゴリ D の目視を必須とし、見るべき項目を PR 本文に列挙する」と要求している |
| F（必須） | `npm run governance:check`（新規ファイル追加） |

**素の `npm run smoke:egui` を使わない。** 引数なしの npm script は `-SeedConfig` も `-ResultsQuery` も渡さないため、**results 窓の検査が自動的に skip される**（`docs/build-commands.md`「スモーク運用メモ」・skip は黄色 NOTE で報告されるだけで exit 0）。本 PR の対象は当の results 窓であり、skip されたら**変更点が一切検証されない**。

**追加テスト**: `layout::size_delta_exceeds` の 2 本（Phase 1）。これが本 PR で増える唯一の自動カバレッジである。ベースラインは `cargo test -p snotra` = **174 passed / 0 failed / 2 ignored**（`ed6d68a` で自分で実測・`finished in 1.55s`）。移設によって**移動するテストは 0 件**（`view.rs` のテストは font 系のみ）——受け入れ条件は **176 passed / 0 failed / 2 ignored**。

**段 3（#666）とのマージ順**: 段 1 が先である。#666 は同じ `view.rs` の分割で、段 1 は同ファイルから `drive_results_window` と 2 フィールドを抜く。段 3 は未着手のため衝突は生じない。

### カテゴリ D 目視項目（PR 本文へそのまま転記する）

各項目に、それが守る不変条件を併記する（**どの不変条件も守らない目視項目を置かない**）。

| # | 手順 | 守る不変条件 |
|---|---|---|
| 1 | ホットキーで show → 1 文字打鍵 → results が main の直下に出て、**2 文字目が打てる**（フォーカスを奪わない） | I7 |
| 2 | results 可視中にホットキーで hide → **両窓が同時に消える**（results だけ残らない） | I3・I7 |
| 3 | 行をクリックして起動 → 起動フレームで**古い行がちらつかない** | I1 |
| 4 | main をドラッグ移動 → results が追従する | I13 |
| 5 | results 可視中に設定で色 / `font_size` を変更 → results が**新しい見た目で再描画される** | I5 |
| 6 | 設定画面を開く → 両窓の最前面が解除され、閉じると復帰する | I7 |
| 7 | 画面下端近くで多件数を出す → results が作業領域の下端でクランプされ、はみ出しは ScrollArea で辿れる | I6 |
| 8 | main を動かして hide → 再 show で**同じ位置に戻る** | 保存 / 復元の両方が移設される確認 |
| 9 | 別モニターへカーソルを置いて hotkey → そのモニターに出る | `position_on_target_monitor` の移設確認 |
| 10 | **hide 中に `font_size` を変更 → 再 show** で results が新しい行高で出る（1 フレーム目から） | I11 |

**受容残余**: I1・I3・I4 には自動検出器が無い。trace の presence を見るスモークは同クラスの回帰を緑のまま通した実例があり（#671 PR A′）、`smoke:egui` の orphan 検査（#690）も順序入れ替えクラスには非決定的にしか鳴らない。I4 は目視でも競合窓が狭く再現困難であり、**移設で行の並びを変えないことをコードレビューで担保する**のが唯一の防御である。I9・I10（非 Windows arm）は CI が Windows のみゆえ**恒久的に未検証**である。**これらを PR 本文に明記する。**

## SPEC.md 更新要否

**不要。** 挙動変更が無く、`SPEC.md:430` が名指しする `drive_results_window` は名前も呼び出し元も変わらない。§8.6 の 4 連言は `layout::present_results` のまま無変更。§8.7 のライフサイクル記述も挙動のみ。

`docs/architecture.md` は**実装事実の記述**（size writer の所在・シーケンス図）を含むため更新する。

## セルフレビュー

### `/plan-review`（Step 5a）

台帳 4 エントリ全件が実在し実質を伴っていた（**独立レビュー不成立なし**）。要対処 5 件はすべて根拠を自分で開いて再照合し、5 件とも成立（降格 0 件）。

| エントリ | 主な指摘 |
|---|---|
| L1 coordinator 移設 | `DriveResultsInputs` の自己矛盾 / `mod.rs` の自己言及コメント |
| L2 窓の状態と純粋核 | Phase 1 の述語が Phase 2 と接続されていない |
| L3 文書同期 | `//!` と `src-tauri/CLAUDE.md` の責務散文の drift |
| Step 2b 独立導出 | `position_on_target_monitor` の非対称 / `cfg(not(windows))` の双子 arm / 素の `smoke:egui` が results 検査を skip |

### 条件別チェック表からの追加検査

- **`/race-check`** — 境界 3 件（新設は `last_size` の 1 件のみ。spawn / send / drain / await の追加なし）。要修正 2 件を反映（I11・I12）
- **`/symmetric-check`** — 候補 5 件。save / restore ペアの片側欠落を解消。同型ペアの取り違え 2 件を Phase 3 の照合項目へ
- **`/dry-check`** — `size_delta_exceeds` と同一の式が main 窓側にも手書きされていた。呼び出し点 2 つを同じコミットで移行

### 一貫性・MECE のマルチパースペクティブレビュー（4 レンズ）

`plan-review/` に 4 本（内部矛盾 / 節間の覆い / 責務分割 / 実行可能性）。**Lens D は API エラーで落ちたが成果物は完走していた**（ファイル実在・103 行）。反映した主な指摘:

| 指摘 | 反映 |
|---|---|
| 「mod.rs から 9 関数」が main.rs 由来を二重計上（実際は 8 + 1 + 2 = 11） | 内訳表に置き換え |
| 目視「7 項目」と実列挙 9 項目の不一致 | **10 項目**に確定し、各項目へ不変条件を併記 |
| `src-tauri/CLAUDE.md` は「5 行」でなく**単一の箇条書き行**の中の 5 か所 | 訂正 |
| 自己言及コメントは 4 か所でなく **6 か所**（`mod.rs:28` と `:573` が漏れ・`:494` は略記のため関数名 grep で届かない） | 表で全件列挙 |
| **Phase 1・Phase 3 は単独でビルドが通らない**（`dead_code` / 未解決 import） | 「コミット粒度」節を新設（C1/C2/C3） |
| `read_metrics` と `max_results` に判別規則が無い（どちらも唯一の消費者が移設対象の中） | **規則 R** を明文化し `read_metrics` も移設対象へ |
| `register_hide_listener` だけ移すと `register_initial_hotkey_failure_listener` が残る理由を説明できない | listener 登録は**全 4 関数とも mod.rs に残す**へ変更 |
| **z-order はどこにも集約されないのに `//!` が宣言していた** | `//!` から外し、「集約しきれないもの」節で残余として明記 |
| 「読み点の制約は 2 つだけ」が偽（`row_height` は #673 決定 4、`width` は同一フレーム同一値の制約下） | 制約を 2 種類に分けて記述 |
| `governance:check` の母集団は `fs.readdirSync`（追跡状態を見ない） | 「`git add` の後に」の理由を訂正 |
| `main.rs` に「未使用になる `use`」は生じない | 「探さないこと」と明記 |
| Phase 4 の行番号は Phase 2 の後にずれる | シンボルで指す形へ |
| `save_placement_relative` 等 5 つは re-export 不要 | re-export を 6 シンボルへ絞る |
| `architecture.md` は 83 行より **82 行**が偽になる。172 行は participant が無い | 3 行それぞれの扱いを明記 |
| I4・I11 の検知手段が症状と対応していない / 目視 4・9 がどの不変条件にも紐づかない | I11・I13 を追加し、目視表に不変条件列を追加。I4 は「検出器なし」と正直に記載 |

### 5b の 3 観点（plan-review が扱わないもの）

1. **境界条件**: `size_delta_exceeds` の 0.5 ちょうど（false）/ 0.51（true）/ 負方向（true）/ 片軸のみ（true）をテストで固定。`max_results = 0`（連言④を②から独立に false にする唯一の入力）は `layout.rs` の既存テストが覆う。作業領域の下端クランプは目視 7
2. **シンプル化の挑戦**: 新規に増える状態は `last_size` の 1 つだけで、既存 view-local フィールド 2 つの移設ゆえ純増ではない。managed struct `WindowCoordinator` を新設する案は、`main.rs` の manage 構成を組み替えて段 3 の前提を動かすため採らない。`set_size` の戻り値を `bool` にしない判断も同じ向き
3. **破壊不変条件 + 検知手段**: I1〜I13 に検知手段を併記済み。**I1・I3・I4 と非 Windows arm には自動検出器が無い**——カテゴリ D の目視 10 項目とコードレビューが唯一の検出器であり、受容残余として PR 本文に明記する
