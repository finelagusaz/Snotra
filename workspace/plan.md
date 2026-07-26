# plan — #749 段1: WindowCoordinator

前提: `workspace/research.md`。ブランチ `chore/window-coordinator`（基点 `a98312c`）。
**挙動は変えない**（issue 明記）。差分の大半は移設であり、意味を持つ判断は下の 2 点だけである:

1. `drive_results_window` のデルタガード 2 フィールドの行き先（→ `ResultsWindow` へ）
2. デルタガードの判定式を純粋核へ出すこと（→ 本 PR で唯一得られる自動カバレッジ）

## 変更ファイル一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| `src-tauri/src/egui_shell/window_coordinator.rs` | **新規** | 窓の可視性・位置・サイズ・wake の driver 群（mod.rs から 9 関数 + main.rs から 1 関数 + view.rs から 2 関数） |
| `src-tauri/src/egui_shell/mod.rs` | Modify | 上記 9 関数を削除し `mod window_coordinator;` + `pub(crate) use` の re-export を足す。**`//!` から移した責務を落とす** |
| `src-tauri/src/main.rs` | Modify | `position_on_target_monitor`（150-193）を移設。未使用になる `use` はコンパイラが検出する |
| `src-tauri/src/egui_shell/view.rs` | Modify | `drive_results_window` / `max_results` / results 用デルタガード 2 フィールドを撤去、呼び出しを 1 か所へ。**main 窓ガード（1830）の手書き式を `size_delta_exceeds` へ**（`/dry-check`） |
| `src-tauri/src/egui_shell/results_window.rs` | Modify | デルタガードを内包（`last_size: Mutex<(f64, f64)>`・`set_size` を `layout::size_delta_exceeds` で自己ガード化・`reset_size_guard`） |
| `src-tauri/src/egui_shell/layout.rs` | Modify | 純粋述語 `size_delta_exceeds` を追加（+ 境界テスト）。doc 内の `mod.rs::` 名指し 2 か所（102 / 118）を訂正 |
| `src-tauri/src/egui_shell/visual.rs` | Modify | doc コメント内の `mod.rs::position_results_below_main` を新モジュール名へ |
| `src-tauri/CLAUDE.md` | Modify | 「モジュール構成」に新規ファイル行を追加し、**`view.rs` の責務散文から「results 窓 driver」を落とす**・`mod.rs` の散文を更新 |
| `docs/architecture.md` | Modify | 83 行の駆動主体、172 行のシーケンス図の宛先 |

**変更しないと決めたもの**（根拠を伴う）:

- `SPEC.md` — §8.5 は「`main` の毎フレーム更新（`drive_results_window`）が駆動する」と書く。関数名を変えず、main の `update()` から呼ぶことも変えないため**記述は真のまま**。挙動変更が無いので同期対象にならない
- `commands/window.rs` / `results_view.rs` — re-export により参照パス `crate::egui_shell::<名前>` が不変ゆえ**差分ゼロ**
- `view.rs` の `//!`（1-8 行）— results 窓 driver を名指ししておらず（「動的高さ」までしか書いていない）、移設後も真のまま
- managed state の構成（`app.manage` の順序・`EguiShellState` / `ResultsWindow` の載せ方）— 段 3（#666）の前提を動かさないため
- **main 窓の毎フレーム `set_size`（`view.rs:1824-1837`）を Coordinator へ引き取らない** — ADR-0007 却下 1 の第 3 理由（main の高さは `show_egui_main` の `bar_height` collapse と `main_window_height` の**意図的な 2 導出**）を段 1 が巻き戻すことになる。main の幅・高さの適用は view に残す
- `docs/superpowers/` 配下の plan / spec — 歴史記録（`docs/superpowers/README.md`・`governance:check` 対象外）

## 実装順序

### Phase 1 — 純粋述語の追加（`layout.rs`）

デルタガードの判定式を Win32 依存から切り離す。`ResultsWindow` は `tauri::Window` を持つためユニットテストできない（`.claude/rules/src-tauri.md`）——判定式だけを純粋核に置けば、`0.5` の許容境界にテストを置ける。

```rust
/// results 窓の再サイズが要るか（#749）。`set_size` は Win32 呼び出しゆえ、同値フレームで
/// 撃たないためのデルタガードである。**correctness のフラグではない**（可視性は
/// `ResultsWindow` の `visible` が持つ・#671 spec 決定 2 の意図的な分割）。
/// 許容 0.5 は論理 px の丸め差を吸収する値で、#646 PR2 からの実測値を保つ。
pub fn size_delta_exceeds(prev: (f64, f64), next: (f64, f64)) -> bool {
    (next.0 - prev.0).abs() > 0.5 || (next.1 - prev.1).abs() > 0.5
}
```

**引数はタプル `(幅, 高さ)` の順で固定する**（`set_size(width, height)` の引数順と揃える。逆順でも結果は同じだが、実装者の裁量に委ねると呼び出し側と読み合わせができない）。

**呼び出し点は 2 つあり、どちらも同じコミットで移行する**（`-D warnings` 下で未使用の新 API は `dead_code` で落ち、比較式を手書きで残すと導出が複数になる・`AGENTS.md`「条件別チェック」の「関数・型を新規定義」行）:

1. Phase 2 の `ResultsWindow::set_size`（results 窓）
2. **`view.rs:1830` の main 窓デルタガード**（`/dry-check` で発見）。式も閾値も同一の手書き重複である: `(height - self.last_set_height).abs() > 0.5 || (width - self.last_set_width).abs() > 0.5`

**main 側は「式だけ」を共有し、状態（`last_set_height` / `last_set_width`）は view に残す。** main の高さは `show_egui_main` の `bar_height` collapse と `main_window_height` の意図的な 2 導出であり（ADR-0007 却下 1 の第 3 理由）、そのガード状態を窓の所有型へ移すのは段 1 の範囲外である。results 側だけがガード状態ごと `ResultsWindow` へ移るのは、**results の唯一の size writer が main であり、窓の所有型が既に存在するから**である（`ResultsWindow::set_size` の呼び出し元は `view.rs:861` の 1 か所だけ・grep 実測）。

追加テスト（`layout.rs` の `mod tests`）:

- `size_delta_exceeds_only_past_half_pixel` — ちょうど 0.5 は false、0.51 は true、負方向の -0.6 も true
- `size_delta_exceeds_watches_both_axes` — 幅だけ変化 / 高さだけ変化のそれぞれで true（片軸落としの回帰検出）

### Phase 2 — デルタガードを `ResultsWindow` へ移す

`drive_results_window` が `&mut self` を要求する唯一の理由が `last_results_height` / `last_results_width` である。この 2 つを窓の所有型へ移せば、driver は `&AppHandle` だけで書ける。

`results_window.rs`:

```rust
/// 直近 `set_size` の (幅, 高さ)。**`visible` とは概念が別**——こちらは冗長な Win32 呼び出しを
/// 避ける性能上のガードで、correctness のフラグではない（#671 spec 決定 2 / #749）。
/// `Mutex` である理由は `visible` が `AtomicBool` である理由と同じ（managed state は
/// `Send + Sync` を要求する）。f64 の組ゆえ atomic では表せない。
last_size: Mutex<(f64, f64)>,
```

- `set_size(&self, width, height)` を**自己ガード化**する。判定は **`layout::size_delta_exceeds((prev_w, prev_h), (width, height))` を呼ぶ**（Phase 1・比較式を手書きしない）。`show()` / `hide()` が「遷移したときだけ raw 操作を撃つ」idiom と同型に揃える。**これは `set_size` の契約変更である**（常に撃つ → 変化時だけ撃つ）が、呼び出し元は `drive_results_window` の 1 か所だけであり、そこには今このガードが手書きされているため**外から見た挙動は同一**である
- **戻り値は返さない（`()`）。** `show()` / `hide()` が `bool` を返すのは呼び出し側が trace を 1 回だけ出すためであり、`set_size` に対応する trace は無い。使われない `bool` を返すと「遷移を見て何かする」経路があるかのように読める
- **lock は Win32 呼び出しの前に手放す**（`/race-check` B1）。判定と memo 更新を guard 内で済ませ、**`drop(guard)` してから** `self.window.set_size(...)` を呼ぶ。`std::sync::Mutex` は**再入不可**であり、tao の `set_inner_size` は `set_window_flags` → `apply_diff` を経て窓プロシージャに至りうる——guard を握ったまま Win32 を呼ぶ形は、将来この経路が再入したときにデッドロックする。**手放したことによる TOCTOU は生じない**——書き手は下の単一スレッド性で閉じている
- **`last_size` の書き手はイベントループスレッドの 2 経路だけである**（`drive_results_window` と `reset_size_guard`）。`Mutex` にするのは managed state の `Send + Sync` 要件のためであって、競合する書き手が居るからではない。`commands/window.rs` のポーリングスレッドが触るのは `set_topmost` だけで `last_size` を読まない（実測）。**この単一スレッド性が崩れる変更を入れるなら、上の「lock を手放す」判断を再検討すること**
- `reset_size_guard(&self)` を足す。`last_size` を `(0.0, 0.0)` へ戻す（現行 view.rs:1194-1195 と同値）
- 初期値は `(0.0, 0.0)`（現行 `SearchWindowView::new` と同値）

`view.rs`:

- フィールド 2 つ（287 / 291）とその初期化（317-318）を削除
- `drive_results_window` 内の手書きガード（858-864）を `results.set_size(width, applied_height);` の 1 行へ
- reset-on-show ブロック（1194-1195）を `results.reset_size_guard()` へ置換（`try_state::<ResultsWindow>()` 越し）

**reset の置き場は view の reset-on-show ブロックに残す（`show_egui_main` へは移さない）。** `show_egui_main` はホットキー listener＝ Win32 メッセージループスレッドから走り、egui のフレームとは別スレッドである。今日この reset はイベントループスレッド上で、同一フレームの `drive` より前に起きる。`show_egui_main` へ移すと reset がフレーム進行中に割り込みうる——`Mutex` ゆえデータ競合は無く最悪でも「余分な `set_size` が 1 回」で済むが、**スレッド同一性という現行の前提を変えることになり「移設・意味変化ほぼゼロ」を外れる**。段 3 で view を割るときに再考する。

### Phase 3 — `window_coordinator.rs` を新設し mod.rs から移す

新規ファイルの `//!`（責務の正本・#562）:

```
//! 窓の可視性・位置・サイズ・z-order・wake を駆動する 1 つの責務（#749 段 1）。
//! 「撃つ主体」を集めた場所であって、「撃ってよいか」の判定は持たない——可視性の
//! 述語は `layout::present_results`（純粋核・#752）、raw 操作の所有点は
//! `results_window::ResultsWindow`（#671 PR A′）である。
//! **wake は primitive として公開する**（#711）——「いつ起こすか」を本モジュールが
//! 決めた瞬間に「armed 期限は保持者が毎フレーム再要求する」契約が壊れる。
```

移設する 9 関数（`research.md` §2 の表・**本文は変えない**。例外は下の「自己言及コメント」だけ）:

`show_egui_main` / `hide_egui_main` / `save_placement_relative` / `register_hide_listener` / `wake_main` / `wake_results` / `position_results_below_main` / `results_available_height` / **`position_on_target_monitor`**（`main.rs:150-193`）

**`position_on_target_monitor` を含める理由**: 保存側 `save_placement_relative` が移るのに復元側だけ `main.rs` に残ると、位置の save / restore が 2 モジュールへ割れる（`/symmetric-check` の対象）。呼び出し元は `show_egui_main`（`mod.rs:396`）の 1 か所だけで（grep 実測）、それも同時に移る。`#[cfg(windows)]` のみで非 Windows の双子は無く、呼び出し側も `#[cfg(windows)]` ブロック内にある——**この非対称を移設で崩さない**。`main.rs` 側で未使用になる `use`（`monitor` / `window_data`）はコンパイラが検出する。

**`#[cfg(not(windows))]` の双子 arm を落とさない（検出器が無い）。** CI の rust-check は Windows のみで走るため、非 Windows の arm を移設で落としても**誰も一度もコンパイルせず永久に気づかない**。移設対象に含まれるのは 2 か所である（実測）:

| 位置 | 対象 |
|---|---|
| `mod.rs:521` | `save_placement_relative` 内の `#[cfg(not(windows))]` ブロック |
| `mod.rs:621` | `results_available_height` の非 Windows 実装（`None` を返す） |

Phase 3 の完了条件に**件数の照合**を入れる: `grep -c "cfg(not(windows))" src-tauri/src/egui_shell/*.rs src-tauri/src/main.rs` の合計が移設前後で不変であること（ベースラインは `egui_shell/mod.rs` 2 + `egui_shell/results_window.rs` 3 + `main.rs` 2 = 7）。

**自己言及になるコメントだけは直す。** `mod.rs` の 4 か所が「`view.rs` の `drive_results_window`」と名指ししているが、Phase 4 で当の関数も同じモジュールへ来るため、そのままでは誤記になる（457 / 481 / 489 / 494 行）。`view.rs` の → `window_coordinator` の（同一モジュール内）へ改める。**「本文は一字も変えない」の例外はこの 4 か所と、下の `//!` だけである**——ほかの本文を触ったら移設ではなくなる。

**`mod.rs` の `//!` を更新する。** 現在の「window 生成・show/hide・blur 自動非表示・位置永続」は Phase 3 の後に偽になる（`//!` はモジュール責務の正本・#562）。`governance:check` は `//!` の文言内容を検査しないため、この drift は機械検査に掛からない。

**移設時に「取り違えても全部通る」2 か所を目で照合する**（`/symmetric-check` 2c）。どちらも同型の値を対称な 2 対象へ配線しており、入れ替えても型・`cargo test`・`smoke:egui` がすべて通る:

| 箇所 | 取り違えの形 | 区別できる観測 |
|---|---|---|
| `wake_main` / `wake_results` | `EguiShellState` の `main_waker` と `results_waker` は**同じ `WindowWaker` 型**（`mod.rs:88,90`）。2 つの 15 行関数を並べて移すとき入れ替わりうる（#671 PR D が「newtype でも取り違えは compile を通る」と実測） | 目視 5（results が visual 変更で描き直される = `wake_results` が正しい）と、設定変更時に main が即座に描き直される（= `wake_main` が正しい） |
| `results_available_height` | 作業領域は **main の HWND** から引き、換算は **results の scale** を使う（`mod.rs:604-610` が明記）。移設中に「揃える」方向へ正規化すると混在 DPI で高さが狂う | 混在 DPI 環境が要り**常時の検出器は無い**。移設前後の diff を目で照合するのが唯一の防御 |

mod.rs 側:

```rust
mod window_coordinator;
// main.rs（hotkey / tray / setup）・view.rs（driver）・results_view.rs（クリック逆流）が
// 消費する。窓操作の実体は window_coordinator.rs へ移した（#749 段 1）。
pub(crate) use window_coordinator::{
    drive_results_window, hide_egui_main, position_results_below_main, register_hide_listener,
    results_available_height, save_placement_relative, show_egui_main, wake_main, wake_results,
    DriveResultsInputs,
};
```

`position_on_target_monitor` は**再エクスポートしない**——移設後の呼び出し元は同一モジュール内の `show_egui_main` だけになる（`main.rs` からは消える）。

移設に伴う参照の付け替え（コンパイラが検出する）:

- `show_egui_main` → `super::read_metrics` / `super::EguiShellState`
- `hide_egui_main` → `super::EguiShellState` / `super::ResultsWindow`
- `position_results_below_main` / `results_available_height` → `super::layout` / `super::ResultsWindow`
- `position_on_target_monitor` → `crate::monitor` / `crate::AppState` / `snotra_core::window_data`（`main.rs` の crate ルート private 関数だったため、`crate::position_on_target_monitor` 形の呼び出しは消える）
- doc コメント中の `mod.rs::position_results_below_main` / `mod.rs::results_available_height`（`layout.rs:102,118` / `visual.rs:5` の 3 件・grep 実測で全件）を `window_coordinator::` へ

### Phase 4 — `drive_results_window` を view.rs から移す

`window_coordinator.rs` へ自由関数として移す。**関数名を変えない**（`SPEC.md:430` / `docs/architecture.md` が名指しするため）。

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

本体は現行 788-876 をそのまま運ぶ。差し替えは次の 4 点のみ:

| 現行 | 移設後 |
|---|---|
| `self.state.results().len()` | `i.result_count`（読み点は呼び出し側へ） |
| `self.max_results()` | 同モジュールへ移した `max_results(app)`（**引数にしない**・下記） |
| `metrics.row_height` | `i.row_height` |
| 手書きデルタガード（858-864） | `results.set_size(i.width, applied_height);`（Phase 2） |

`max_results()`（view.rs:753-760）も本モジュールへ移す（`fn max_results(app: &tauri::AppHandle) -> u32`）。view.rs での利用点は drive の 1 か所だけであり（grep 済み）、残すと view-local ヘルパーと引数経路が二重になる。

view.rs の呼び出し点（1838）:

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

**`max_results` は引数にせず `drive_results_window` の内側で読む。** config の live-read であり読み点の制約が無いからである。**読み点の制約があるのは `result_count`（消費後）と `plain_hidden`（消費前）の 2 つだけ**で、引数に残すのはその 2 つ + 幾何 2 つ（`width` / `row_height`）とする——引数を増やすほど「呼び出し側で並べて書きたくなる」面積が広がり、I1 を壊す誘惑が増える。

### Phase 5 — 文書同期と検査

`src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` 段落を 3 か所直す。**責務散文の正本は各ファイルの `//!` だが（#562）、この段落は「ファイル名 + 一言の責務要約」を添える書式である**（既存行と段 2 の PR #756 の実差分で確認）——字面どおり「索引だから名前だけ」と読んで要約を省かない:

1. `window_coordinator.rs` の行を追加（一言要約 + `//!` が正本である旨は段落冒頭の既存規約が担う）
2. **`view.rs` の要約から「results 窓 driver」を落とす**（driver 本体が出ていくため偽になる）
3. `mod.rs` の要約から show/hide・位置永続を落とす（`//!` の更新と対）

- `docs/architecture.md:83`（駆動主体）/ `:172`（シーケンス図の宛先）を更新
- `npm run governance:check`（新規ファイル追加＝索引更新漏れが #629/#630 で 2 回再発。G1 がモジュール索引を検査するが、**母集団は追跡ファイルゆえ `git add` の後に実行する**）

## 不変条件

| # | 不変条件 | 壊れたときの症状 | 検知手段 |
|---|---|---|---|
| I1 | `result_count` は `take_clicked_for` より**後**に読む | 行クリック起動フレームで古い行が 1 フレーム描かれる | **自動検出器なし**（目視 3・受容残余） |
| I2 | `plain_hidden` は 1 フレーム 1 回だけ読み、クリック逆流の消費**前** | `indexing` の live-read がフレーム内で食い違い、snapshot と窓判定が矛盾 | コードレビュー（`plain_results_hidden` の呼び出しが 1 か所であること） |
| I3 | `hide_egui_main` は `main_visible=false` → `results.hide()` の順 | main だけ消えて results が最前面に残る | 目視 2 / `smoke:egui` は presence のみで**不十分**（#671 PR A′ 実績） |
| I4 | `show_egui_main` は `window.show()` → `main_visible=true` の順 | show 完了前のホットキートグルが hide する | 目視 1 |
| I5 | `drive_results_window` 末尾の `wake_results` は無条件（level-triggered） | visual だけの config 変更が results に反映されない | 目視 5 |
| I6 | 可視判定は `desired_height`（クランプ**前**）、`set_size` と デルタガードは `applied_height`（クランプ**後**） | 不可視フレームでも `SetWindowPos` を撃つ / 下端クランプが効かない | 目視 7 + `layout` の既存テスト |
| I7 | results の 3 操作（show / hide / topmost）は raw のまま、`set_size` / `set_position` は tao 経由のまま | `set_always_on_top` 系が results を消す / `hide()` が no-op になる | 目視 2・6 |
| I8 | wake は primitive（Coordinator が期限を決めない） | armed 期限の再要求が止まり、hide や再検索が次の入力まで宙吊り | コードレビュー（`request_repaint_after` が view にしか無いこと） |
| I9 | `#[cfg(not(windows))]` の双子 arm を移設で落とさない | 非 Windows ビルドが壊れる。**CI は Windows のみゆえ永久に気づかない** | Phase 3 の件数照合（移設前後で 7 件） |
| I10 | `position_on_target_monitor` は `#[cfg(windows)]` のみ・呼び出し側も `#[cfg(windows)]` ブロック内 | 非 Windows で未定義参照 | `cargo check`（Windows では検出できない・I9 と同じ残余） |
| I11 | `reset_size_guard` は**同一フレームの `drive_results_window` より前**に呼ぶ（reset-on-show ブロックは `update()` 冒頭側・drive は末尾） | 再 show 後の 1 フレーム目が旧 metrics のサイズのまま描かれる | 目視 1・8（`/race-check` 4d） |
| I12 | `set_size` は memo の lock を**手放してから** Win32 を呼ぶ | 将来 tao 側が再入したときにデッドロック（`std::sync::Mutex` は再入不可） | コードレビュー（`drop(guard)` の位置） |

### 新たに導入する状態の異常系

`ResultsWindow.last_size: Mutex<(f64, f64)>` が本 PR で増える唯一の状態である。

- **`reset_size_guard` が呼ばれなかったら**: 再 show 後、前回と同じ論理サイズなら `set_size` を撃たない。窓は前回のサイズを保っているため**実害は無い**（ガードは性能用であり correctness ではない）。ただし hide 中に font_size を変えた場合、再 show 後の 1 フレーム目で旧行高のサイズが残りうる → 目視 1 で確認する
- **他スレッドからの並行アクセス**: 現在の書き手はイベントループスレッドの `drive_results_window` と `reset_size_guard` の 2 経路のみで、どちらも同一スレッドである。`commands/window.rs` のポーリングスレッドが触るのは `set_topmost` だけで `last_size` を読まない（`/race-check` で確認する）
- **lock poisoning**: `lock().unwrap()` は `EguiShellState.pending_hotkey_failure` 等の既存様式に揃える。release は `panic="abort"` ゆえ到達しない
- **`ResultsWindow` が managed でない**（setup 完了前の理論経路）: `try_state` が `None` を返し `drive_results_window` は早期 return する（現行と同一）

## テスト方針

| カテゴリ | 内容 |
|---|---|
| A（必須） | `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`（doc コメントを多数動かすため必須） |
| C（必須） | `npm test` / `npm run smoke:startup` / **`pwsh -NoProfile -File scripts/smoke-egui.ps1 -ResultsQuery <開発機の索引に一致する 1 文字>`**（ウィンドウ表示順に触れるため） |
| D（**必須**） | `cargo run -p snotra` で下の 7 項目を目視。issue が「カテゴリ D の目視を必須とし、見るべき項目を PR 本文に列挙する」と要求している |
| F（必須） | `npm run governance:check`（新規ファイル追加） |

**素の `npm run smoke:egui` を使わない。** 引数なしの npm script は `-SeedConfig` も `-ResultsQuery` も渡さないため、**results 窓の検査が自動的に skip される**（`docs/build-commands.md`「スモーク運用メモ」・skip は黄色 NOTE で報告されるだけで exit 0）。本 PR の対象は当の results 窓であり、skip されたら**変更点が一切検証されない**。`-ResultsQuery` に開発機の索引に一致する 1 文字（A-Z 単字）を渡して `egui_results:show` / `egui_results:hide` の観測まで走らせる。

**追加テスト**: `layout::size_delta_exceeds` の 2 本（Phase 1）。これが本 PR で増える唯一の自動カバレッジである。既存の `cargo test -p snotra` は **174 passed / 0 failed / 2 ignored**（`a98312c` で実測）で、移設によって**移動するテストは 0 件**である（`view.rs` のテストは font 系のみ）——受け入れ条件は「件数が 174 + 2 になり全緑」。

**既存テストの役割**: `layout.rs` の `present_results` テスト群（真理値表・legacy 等価グリッド・main hidden）は無変更で通ることが移設の回帰検出器になる。**改名・転用はしない**（`AGENTS.md`「既存テストを改名・転用するとき」）。

### カテゴリ D 目視項目（PR 本文へそのまま転記する）

1. ホットキーで show → 1 文字打鍵 → results が main の直下に出て、**2 文字目が打てる**（フォーカスを奪わない）
2. results 可視中にホットキーで hide → **両窓が同時に消える**（results だけ残らない）
3. 行をクリックして起動 → 起動フレームで**古い行がちらつかない**（I1 の目視版）
4. main をドラッグ移動 → results が追従する（`Moved` リスナー経路）
5. results 可視中に設定で色 / font_size を変更 → results が**新しい見た目で再描画される**（I5・無条件 wake）
6. 設定画面を開く → 両窓の最前面が解除され、設定を閉じると復帰する（`set_topmost` の対称）
7. 画面下端近くまで main を下げて多件数を出す → results が作業領域の下端でクランプされ、はみ出した行は ScrollArea で辿れる
8. main を動かして hide → 再 show で**同じ位置に戻る**（`save_placement_relative` の保存と `position_on_target_monitor` の復元。**両方が本 PR で移る**）
9. 別モニターへカーソルを置いて hotkey → そのモニターに出る（`follow_cursor_monitor` = true の経路。復元側の移設で壊れていないこと）

**受容残余**: I1・I3 には自動検出器が無い。trace の presence を見るスモークは同クラスの回帰を緑のまま通した実例がある（#671 PR A′）。PostToolUse hook（clippy + crate テスト）の沈黙もここのカバレッジにならない。**この事実を PR 本文に明記する。**

## SPEC.md 更新要否

**不要。** 挙動変更が無く、`SPEC.md:430` が名指しする `drive_results_window` は名前も呼び出し元（main の毎フレーム更新）も変わらない。§8.6「検索結果ウィンドウの可視性（従属軸）」の 4 連言は `layout::present_results` のまま無変更。

`docs/architecture.md` は**実装事実の記述**（駆動主体・シーケンス図）を含むため更新する。

## セルフレビュー

### `/plan-review`（Step 5a）

台帳 4 エントリ全件が実在し、いずれも実質を伴っていた（**独立レビュー不成立は無し**）。

| エントリ | 成果物 | 結果 |
|---|---|---|
| L1 coordinator 移設 | `plan-review/rust-coordinator-move.md`（70 行） | 要対処 2 / 軽微 1 |
| L2 窓の状態と純粋核 | `plan-review/rust-guard-and-layout.md`（92 行） | 要対処 1 / 軽微 2 |
| L3 文書同期 | `plan-review/docs-sync.md`（64 行） | 要対処 2 / 軽微 1 |
| Step 2b 独立導出 | `plan-review/independent-derivation.md`（389 行） | 漏れ 3 / スコープ確認 1 |

**要対処 5 件はすべて根拠を自分で開いて再照合し、5 件とも成立**（降格 0 件）。反映内容:

1. `DriveResultsInputs` のコード例と直後の決定文が `max_results` を巡って矛盾していた → フィールドを落とし、内側で読むことを 1 か所に確定
2. `mod.rs` の 4 コメント（457 / 481 / 489 / 494）が「`view.rs` の `drive_results_window`」と名指し → 移設後は自己言及の誤記になるため、例外として直すことを明記
3. Phase 1 の `size_delta_exceeds` が Phase 2 の `set_size` と接続されていなかった → 呼び出し点を明記し、同一コミットに束ねることを追加（`dead_code` と導出 2 か所化の回避）
4. `src-tauri/CLAUDE.md` の `view.rs` 責務散文「results 窓 driver」が偽になる → Phase 5 に訂正を追加
5. `mod.rs` の `//!`（責務の正本・#562）が古くなる。`governance:check` は `//!` の文言を検査しない → Phase 3 に更新を追加

### 独立導出との差分（Step 2b）

- **漏れ（導出 ∖ plan）**:
  - `position_on_target_monitor`（`main.rs:150-193`）— 位置の save が移って restore が残る非対称。**取り込む**ことにした（Phase 3）
  - `#[cfg(not(windows))]` の双子 arm — CI が Windows のみゆえ落としても永久に気づかない。I9 として不変条件化し、件数照合を Phase 3 の完了条件に追加
  - 素の `npm run smoke:egui` が results 検査を skip する — カテゴリ C のコマンドを `-ResultsQuery` 付きへ差し替え
- **スコープ過剰（plan ∖ 導出）**: なし。逆に導出は「新規テストを足せる箱は無い」としたが、`layout.rs` の既存 `mod tests` が箱であり、判定式を純粋核へ出せばテストできる——**この 1 点は計画側を採る**（`.claude/rules/src-tauri.md`「Win32 依存モジュールはユニットテスト前提にしない」の裏返しとして正当）
- **一致（完全性の証拠）**: 移設対象の 8 関数と `drive_results_window`、re-export で呼び出し元差分ゼロになること、`mod.rs::` 名指し 3 件、`drive_results_window` の md 参照 3 件、読み点の非対称が最大のリスクであること、main 窓の毎フレーム `set_size` を含めてはならないこと（ADR-0007 却下 1）——いずれも独立に再一致した

### 条件別チェック表からの追加検査（Step 5a）

- **`/race-check`** — 境界 3 件（新設は `ResultsWindow.last_size` の 1 件のみ。①spawn ②send ③drain ⑦await の追加は無し）。要修正 2 件を反映: **memo の lock を Win32 呼び出しの前に手放す**（`std::sync::Mutex` は再入不可・I12）、**`reset_size_guard` は同一フレームの drive より前**（I11）。「results の唯一の size writer は main」は grep で実測（`ResultsWindow::set_size` の呼び出し元は `view.rs:861` のみ・`config_watcher` は窓を一切触らない）
- **`/symmetric-check`** — 候補 5 件。save / restore ペアの片側欠落は独立導出の指摘で解決済み。**同型ペアの取り違え 2 件**（`wake_main`/`wake_results` は同じ `WindowWaker` 型・`results_available_height` の「main の HWND / results の scale」）を Phase 3 の照合項目へ追加。`set_size`（ガード有）と `set_position`（ガード無）の非対称は #646 PR2 決定 10 として意図的
- **`/dry-check`** — `size_delta_exceeds` と**同一の式が main 窓側にも手書き**されていた（`view.rs:1830`）。呼び出し点を 2 つとも同じコミットで移行する。状態（`last_set_*`）は view に残し、式だけを共有する

### 5b の 3 観点（plan-review が扱わないもの）

1. **境界条件**: `size_delta_exceeds` の 0.5 ちょうど（false）/ 0.51（true）/ 負方向（true）/ 片軸のみ変化（true）をテストで固定。`max_results = 0`（連言④を②から独立に false にする唯一の入力）は `layout.rs` の既存テストが覆う。作業領域の下端クランプは目視 7
2. **シンプル化の挑戦**: 新規に増える状態は `ResultsWindow.last_size` の 1 つだけで、これは既存 view-local フィールド 2 つの**移設**であり純増ではない。managed struct `WindowCoordinator` を新設する案（`main.rs` の manage 構成の組み替え）は、段 3（#666）の前提を動かすうえ issue が段 1 を「ほぼ移設・意味変化ほぼゼロ」と規定しているため**採らない**。`set_size` の戻り値を `bool` にしない判断も同じ向き（使わない情報を型に載せない）
3. **破壊不変条件 + 検知手段**: I1〜I10 の表に検知手段を併記済み。**I1（読み点の非対称）と I3（hide の順序）には自動検出器が無い**——trace の presence を見るスモークは同クラスの回帰を緑のまま通した実例があり（#671 PR A′）、`smoke:egui` の orphan 検査（#690）も順序入れ替えクラスには非決定的にしか鳴らない。カテゴリ D の目視 9 項目が唯一の検出器であり、**受容残余として PR 本文に明記する**
