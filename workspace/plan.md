# plan — 束B 残り: #699（clicked の世代）+ #675（結果窓の下端クランプ）

前提は `workspace/research.md`、#710 の実測は `workspace/measurement.md`。

**#710 は「正常」として閉じた**（2026-07-26・[issuecomment-5082153261](https://github.com/finelagusaz/Snotra/issues/710#issuecomment-5082153261)）。level-triggered `wake_results` の実額は 1 コアの 2.7% で、かつその 1 行が config hot-reload の唯一経路であるため触らない。見つかった別の欠陥（移動中 448fps / 84.7%）は **#737** へ分離済み。

→ **本計画は `wake_results` に手を触れない。** #675 は同じ関数（`drive_results_window`）を触るが、wake の条件は変えない。

---

## 全体の不変条件

| # | 不変条件 | 検知手段 |
|---|---|---|
| I1 | **`wake_results`（`view.rs:850`）の呼び出し条件を変えない** | `git diff` で当該行が無変更であること。変えると config hot-reload が results へ届かなくなる（#710 実測） |
| I2 | **`results_window_height` の「0 件は 0.0（呼び出し側が hide する契約）」を壊さない** | `layout.rs` の既存テスト 4 件 + 新規テスト（クランプしても 0 は 0） |
| I3 | **クリックの消費側が世代照合を迂回できない** | 型で担保する（`ResultsShared` に世代を要求するメソッドを置き、`clicked` フィールドを直接 take させない） |
| I4 | **論理 px と物理 px を混ぜない** | `set_size` は論理・`set_position` と `WorkArea` は物理（実装で確認済み）。変換点を 1 箇所に閉じ、単体テストは論理 px だけで書く |
| I5 | 挙動変更を伴うため **SPEC を同期する** | `SPEC.md` §4.5 の高さ算出式（#675 で条件が付く） |

---

## Phase 0 — #699 の**成立条件**: 世代の所有を `SearchState` へ移す

> **この Phase は当初の計画に無かった。** `/plan-review`「Step 2b」の独立再導出が「世代を同梱しても穴は塞がらない」と指摘し、**主エージェントが自分で裏を取って確認した**（下記「セルフレビュー」）。これが無ければ**効かない修正を出荷していた**。

### 0-1. 何が壊れているか（実測）

`snapshot_generation` を進めるのは **`view.rs:389` / `:535` / `:904` の 3 箇所だけ**。一方 `SearchState` が `self.results` を差し替える箇所は 5 つあり、**3 経路が bump されない**:

| `SearchState` の差し替え点 | 呼び出し元 | bump |
|---|---|---|
| `search_state.rs:173`（`set_results`） | view.rs:387 / 532 / 910-985 | ✅（呼び出し側が bump） |
| **`search_state.rs:288`（`enter_tool`）** | **view.rs:652** | **❌** |
| **`search_state.rs:311`（`on_escape` / tool 復帰）** | **view.rs:1293** | **❌** |
| **`search_state.rs:318`（`on_escape` / folder 復帰）** | **view.rs:1293** | **❌** |
| **`search_state.rs:332`（`reset`）** | **view.rs:455 / :1147** | **❌** |

`search_state.rs` に `generation` フィールドは**存在しない**（grep で確認・`:801` のヒットは index 世代のテスト名で無関係）。

**順序が決定的である**: `on_escape` は **`view.rs:1293`**、snapshot publish は **`:1751`**、`clicked` の take は **`:1763`**——**同一フレームで、消費より前**。

> results がツール行 3 件を描いて index 2 をクリック → 同フレームで main が Escape を処理して plain 結果へ復帰 → **世代は同じまま** → 世代ガードを素通り → 復帰後の行が 3 件以上なら**別の行が起動する**。

issue #699 が列挙した総入れ替え経路（folder drain / index 世代検知 / 起動 drain）は**すべて `run_search_with` を通る＝bump される側**だった。**issue の著者も当初の計画も、同じ盲点を継承していた。**

### 0-2. 設計: 世代を `SearchState` が所有する

手動 bump を 3 箇所足す案（代替 B）は採らない——**同じ手作業で 3 回漏らした経路を、もう一度手に委ねることになる**。`docs/development-principles.md`「構造的設計原則と強制の階梯」の 2（不変条件は単一の通過点に閉じ込める）を適用する。

- `search_state.rs` の `SearchState` に `rows_generation: u64` を追加（`new()` で 0）
- **`self.results` を差し替える 5 箇所すべて**で `self.rows_generation += 1`
- `pub fn rows_generation(&self) -> u64` を公開
- doc に「**この型の中で `self.results` へ代入・`clear` するメソッドは、必ずここを進める**」と書く（全称の射程を型の内側に限定する——`AGENTS.md`「検証の作法」の全称表現の規律）
- `view.rs` の手動 bump（`:279` フィールド・`:318` 初期化・`:389` / `:535` / `:904`）を**削除**し、参照（`:1751` / `:1755`）を `self.state.rows_generation()` へ置換

**副次的な改善**: `view.rs:904` は `run_search_with` の先頭で無条件に bump しており、**folder cache 未着で `set_results` を呼ばずに返るフレームでも世代が進んでいた**（空撃ち）。所有を移すと世代の意味が「行が差し替わった」と一致する。

### 0-3. 偽陰性のリスクを測った（自分で）

`set_results` が bump するなら「毎フレーム `set_results` が呼ばれる経路」があると世代が毎フレーム進み、**すべてのクリックが破棄される**。実測で確認した:

- 製品コードの `set_results` 呼び出しは **view.rs の 9 箇所のみ**（他は `search_state.rs` のテスト内）
- その 9 箇所は `start_launch` / `clear_search` / `run_search_with` の中にあり、
- **`run_search*` の呼び出し 12 箇所はすべて事象駆動**（打鍵 `response.changed()` / launch drain / folder drain 到着 / `needs_index_refresh` / `search_debounce.poll` の発火）で、**毎フレーム走るものは無い**

→ **偽陰性は「描画と消費の間に行が実際に変わった」場合に限られる**。それは破棄が**正しい**ケースである。

**受容する残余**: trailing 検索の発火とクリックが同一フレームに当たると、そのクリックは落ちる（再クリックで復帰する）。**誤った行を起動するより落とすほうが安全**という非対称に基づく判断であり、`docs/adr/0006-plan-ownership-boundary.md` が「誤りの代償の非対称で倒す向きを決める」と述べたのと同型。

### 0-4. テスト（`search_state.rs` の `mod tests`）

| テスト名 | 固定する不変条件 |
|---|---|
| `rows_generation_advances_on_set_results` | `set_results` で +1 |
| `rows_generation_advances_on_enter_tool` | `enter_tool` で +1（**当初計画が落としていた経路**） |
| `rows_generation_advances_on_escape_both_branches` | `on_escape` の tool 復帰・folder 復帰の**両方**で +1 |
| `rows_generation_advances_on_reset` | `reset` で +1 |
| `rows_generation_is_stable_on_selection_change` | `move_selection` / `reset_selection` では**進まない**（選択は行の差し替えではない） |

最後の 1 件が重要である——**進めすぎも欠陥**（全クリックが落ちる）なので、両方向を固定する。

---

## Phase 1 — #699: `clicked` に世代を同梱する

### 1-1. 型（`results_view.rs`）

```rust
pub(crate) struct ResultsShared {
    pub snapshot: std::sync::Mutex<RowsSnapshot>,
    /// クリックされた行（last-wins）。**世代を同梱する**（#699）——裸の index は、
    /// 積んだフレームと消費するフレームの間に結果集合が総入れ替えされると別の行を指す。
    /// 取り出しは `take_clicked_for` だけを使う（フィールドを直接読まない）。
    clicked: std::sync::Mutex<Option<(u64, usize)>>,
}
```

**`clicked` を `pub` から private へ落とす**（同一 crate 内 `pub(crate)` 構造体のフィールドなので `mod` 外から触れなくなる）。

### 1-2. 単一の通過点（I3）

```rust
impl ResultsShared {
    /// results 側が積んだクリックを、**世代が一致するときだけ**取り出す（#699）。
    /// 不一致なら破棄する——積んだ後に結果集合が総入れ替えされており、その index は
    /// 別の行を指すため。`.get(index)` の境界チェックは行の同一性を見ないので代わりにならない。
    ///
    /// **消費側がこの関数を通らずに index を得る経路は無い**（フィールドは private）。
    /// `docs/development-principles.md`「構造的設計原則と強制の階梯」の 2（不変条件は
    /// 単一の通過点に閉じ込める）。
    pub(crate) fn take_clicked_for(&self, generation: u64) -> Option<usize> {
        match self.clicked.lock().unwrap().take() {
            Some((g, i)) if g == generation => Some(i),
            _ => None,
        }
    }

    /// results 側からクリックを積む（世代は描画中の `RowsSnapshot.generation`）。
    pub(crate) fn push_clicked(&self, generation: u64, index: usize) {
        *self.clicked.lock().unwrap() = Some((generation, index));
    }
}
```

`take()` を使うので**不一致でもスロットは空く**（stale が残り続けない）。

### 1-3. 積む側（`results_view.rs:487-490`）

```rust
if let Some(i) = clicked {
    // 世代は**描画中の snapshot のもの**を添える（#699）。この行は snapshot.rows の i 番目
    // であり、main が総入れ替えしていれば消費側で破棄される。
    shared.push_clicked(snapshot.generation, i);
    crate::egui_shell::wake_main(&self.app_handle);
}
```

### 1-4. 消費側（`view.rs:1763-1766`）

```rust
// クリック逆流の消費(決定 5): 起動ロジックは main の一箇所に保つ。
// 世代照合は `take_clicked_for` の中（#699）——ここで書くと迂回できてしまう。
match shared.take_clicked_for(self.state.rows_generation()) {
    ClickTake::Current(i) => self.activate_or_execute(i, &ctx),
    // 破棄は目に見えない経路なので trace を出す（手で再現できないため）。
    ClickTake::Stale { stamped } => crate::trace_main(
        "egui_results:click_stale",
        serde_json::json!({ "stamped": stamped, "current": self.state.rows_generation() }),
    ),
    ClickTake::None => {}
}
```

**消費が snapshot publish の後にあるという既存の順序を変えない。** この順序が不変条件である——ガードが比較する世代は、**そのフレームで行を差し替えうる全ハンドラ**（Escape `:1293` / index 世代検知 `:1186` / folder drain `:1277` / launch 完了 `:477`）**より後**の値でなければ、#699 の窓を塞げない。doc comment に理由ごと書く。

`ClickTake` を導入するのは trace のためである（`Option<usize>` だと「無かった」と「捨てた」が区別できない）。**破棄は手で再現できない経路**なので、観測点を作っておく——`src-tauri/CLAUDE.md`「モジュール構成」が言う「trace の presence 検査は状態の検査ではない」に留意し、**これは診断用であって不変条件の担保には使わない**（担保はユニットテスト）。

### 1-5. テスト（`results_view.rs` の `mod tests`）

| テスト名 | 固定する不変条件 |
|---|---|
| `clicked_survives_matching_generation` | 世代一致なら index が取り出せる |
| `clicked_is_discarded_on_generation_mismatch` | 世代不一致なら `None`。**かつスロットが空く**（2 度目も `None`） |
| `clicked_is_last_wins` | 2 回積むと後勝ち（既存の last-wins 契約） |

---

## Phase 2 — #675: 結果窓の高さを作業領域の下端でクランプ

### 2-1. 純粋核（`layout.rs`）

```rust
/// 結果窓の高さを作業領域の下端でクランプする（#675）。単位はすべて**論理 px**。
///
/// - `desired`: `results_window_height` の値
/// - `available`: 結果窓の上端から作業領域下端までの高さ
/// - `row_height`: 1 行の高さ
///
/// **0 件（`desired == 0.0`）は 0.0 のまま返す**——`results_should_show` が hide 側へ倒れる
/// 契約（`results_window_height` の doc）を壊さないため。
///
/// `available` が 1 行に満たなくても **1 行 + padding は返す**。潰すと窓が無意味になるので、
/// その場合のはみ出しは受容する（行はスクロールで到達できる）。main を画面下端ぎりぎりへ
/// 動かした場合の縮退であり、常用の経路ではない。
pub fn clamp_results_height(desired: f64, available: f64, row_height: f64) -> f64 {
    if desired <= 0.0 {
        return 0.0;
    }
    desired.min(available).max(row_height + 8.0)
}
```

テスト:

| ケース | 期待 |
|---|---|
| `desired == 0.0` | `0.0`（hide 契約・I2） |
| `desired < available` | `desired`（無変更＝既存挙動） |
| `desired > available` | `available` |
| `available < row_height + 8.0` | `row_height + 8.0`（下限） |
| `available` が負（main が作業領域外） | `row_height + 8.0` |

### 2-2. 結果窓の上端を単一点から得る（`mod.rs`）

`position_results_below_main` は `pos.y + size.height + (gap * scale).round()` を計算しているが**返さない**。同じ式を `drive_results_window` に書くと**写しが 2 つになる**（このサイクルで繰り返し見た形）。

```rust
/// results の上端 y（**物理 px**）と scale を返す。`position_results_below_main` と
/// `drive_results_window` の両方が要るため、式の正本をここに 1 つだけ置く（#675）。
pub(crate) fn results_top(app: &tauri::AppHandle) -> Option<(i32, f64)> { ... }
```

`position_results_below_main` はこれを呼ぶ形へ書き換える（**外部から見た挙動は不変**）。

### 2-2b. 換算に使う scale は **results 窓自身のもの**（独立導出の指摘）

`ResultsWindow` に `scale_factor()` を足す。理由: tao 0.35.3 の `set_inner_size` は **その窓の** `self.scale_factor()` で `LogicalSize` を物理へ戻す。main の scale を流用すると**混在 DPI 環境で高さが食い違う**。

**受容する残余（未測定）**: `set_position` 直後は tao 側の scale factor がまだ旧モニターのものである可能性がある（Windows は移動後に `WM_DPICHANGED` を送る）。実害はモニター跨ぎの 1 フレームに限られる見込み。**「main の scale で割らない理由」をコメントに残し、残余は受容する**。

### 2-3. 呼び出し側（`view.rs::drive_results_window`）

```rust
let desired = crate::egui_shell::layout::results_window_height(count, self.max_results(), metrics.row_height);
// 作業領域の下端でクランプする（#675）。**物理 → 論理の変換はここ 1 箇所**（I4）。
let res_h = match crate::egui_shell::results_top(&self.app_handle)
    .zip(crate::egui_shell::work_area_for_main(&self.app_handle))
{
    Some(((top, scale), area)) => crate::egui_shell::layout::clamp_results_height(
        desired,
        (area.bottom - top) as f64 / scale,
        metrics.row_height,
    ),
    // 作業領域が取れない環境（非 Windows・API 失敗）は従来どおりクランプしない
    None => desired,
};
```

`work_area_for_main` は `monitor::window_monitor_work_area(main.hwnd())` の薄いラッパー（`#[cfg(windows)]` の面倒をここで吸収し、非 Windows では `None`）。

**可視判定（`results_should_show`・`view.rs:823-824`）は素の `res_h` のままにする**（独立導出の指摘で当初案から変更）。理由は 2 つ:

1. **順序**: クランプには `position_results_below_main` が決めた上端が要るが、位置決めは可視判定の**後**にある（不可視なら早期 return する）。クランプ後の値で判定しようとすると**位置決めを判定より前へ動かす再構成が要る**——不可視フレームでも `SetWindowPos` を撃つことになり、#646 PR2 決定 10 の設計を変えてしまう
2. **契約**: 「0 件 ⇔ 高さ 0 ⇔ hide」を判定側で無傷に保てる。クランプは `set_size` に渡す値だけに効かせる

**`self.last_results_height` にはクランプ後の値を入れる**——素の値を入れるとデルタガードの照合対象が `set_size` の実引数とずれ、毎フレーム撃つか必要な再サイズを撃たなくなる。

---

## 実装順序

Phase 1 → Phase 2。依存は無いが、**Phase 1 のほうが小さく、テストで閉じる**ので先に緑にする。

---

## テスト方針

| 追加/更新 | 場所 | 不変条件 |
|---|---|---|
| `clicked_survives_matching_generation` | `results_view.rs` | 世代一致で取り出せる |
| `clicked_is_discarded_on_generation_mismatch` | 同上 | 不一致で破棄・スロットが空く |
| `clicked_is_last_wins` | 同上 | 既存の last-wins 契約（回帰） |
| `clamp_results_height_*`（5 ケース） | `layout.rs` | 上表のとおり |
| 既存 `results_window_height` テスト 4 件 | `layout.rs` | **無変更で通ること**（I2 の回帰） |

検証（`docs/build-commands.md`）:

- **カテゴリ A**: clippy + `cargo test`（PostToolUse hook が `.rs` 編集で自動実行・沈黙 = 合格）
- **カテゴリ C**: `npm run smoke:egui`（ウィンドウ表示順に触れるため。`.claude/rules/src-tauri.md`「トリガー → 検査」）
- **カテゴリ D（目視）**: **#675 は目視でしか確かめられない**——main を画面下端付近へドラッグしてから検索し、結果窓がタスクバーの下へ潜らないこと。#699 は総入れ替えとクリックの競合窓が狭く、**手で再現する手段が無い**（受容する残余。ユニットテストで純粋核を固定する）

---

## SPEC.md 更新要否 — **更新する**

`SPEC.md:172`（§4.5）:

> 検索結果ウィンドウ（`results`）の高さは実件数にフィットする（`min(表示件数, 最大表示件数) × 行高 + 8px`）。ヒット数が最大表示件数未満なら高さも小さくなり、超過時はスクロールバーを表示する（#646 PR2 決定7）

→ **作業領域の下端でクランプする条件を追記する**（#675）。式が無条件でなくなるため、書かないと嘘になる（`AGENTS.md`「開発ワークフロー」1 の「fix でも文書化された挙動を変えたら仕様変更」）。

§4.8 のクリック記述は**変更しない**——「シングルクリック: アイテムを起動する」は真のままで、世代不一致の破棄は**誤った行を起動しないための内部保護**であり、正常系の観測可能な挙動を変えない。

---

## セルフレビュー

### Step 2b（独立再導出）— 委譲

**`name:` を渡さず**（#731 の機序）、成果物は **`workspace/plan-review-2b.md`** へ書かせる（#733 で明文化した手順の初回適用）。全文は同ファイル。

**配送は成功した**——ただし**ファイルへは書けなかった**。詳細は下記「機構の欠陥」。

#### 導出 ∖ plan（＝当初計画の漏れ）— **1 件が致命的**

| # | 指摘 | 主エージェントによる検証 | 反映 |
|---|---|---|---|
| **1** | **世代 bump 漏れ 3 本**（`enter_tool` / `on_escape` ×2 / `reset`）。`on_escape` は消費より前の同一フレームゆえ、世代を同梱しても穴が塞がらない | **裏取り済み**: `search_state.rs:288/311/318/332` が `self.results` を差し替え、`generation` フィールドは不在。呼び出しは `view.rs:652/1293/455/1147`、bump は `:389/535/904` のみ。順序は `on_escape:1293` < `publish:1751` < `take:1763` | **Phase 0 を新設**（世代の所有を `SearchState` へ） |
| 2 | 換算の scale は **results 窓自身**のもの（tao が `self.scale_factor()` で戻す） | 一次ソース読解を採用（未実測） | 2-2b に追加 + 受容残余を明記 |
| 3 | 可視判定は**素の `res_h`** のままにすべき | **自分で検証**: クランプは位置決め後にしか計算できず、位置決めは可視判定の後——当初案は再構成を強いる | 2-3 を変更 |
| 4 | `last_results_height` にはクランプ後の値を入れる | デルタガードの照合対象の整合として妥当 | 2-3 に明記 |
| 5 | 作業領域は **main の HWND** から引く（誤配置済み results から引くと別モニターを掴む） | 妥当 | 2-3 に反映 |
| 6 | `monitor.rs` は全関数 `#[cfg(windows)]`。cfg 分割は `save_placement_relative` に倣う | **裏取り済み**（`monitor.rs` 冒頭） | 2-3 に反映 |
| 7 | 破棄経路の trace（手で再現できないため） | 妥当。ただし **trace は診断用で不変条件の担保ではない** と限定 | 1-4 に追加 |

**同クラスの発見（本 PR のスコープ外）**: main 窓自身も status/toast で伸びた後に再クランプが無く下端をはみ出しうる（`mod.rs:378-392` → `view.rs:1775-1785`）／`SPEC.md:413`「ウィンドウが画面外に出ないことを保証する」がドラッグ移動経路で偽。→ **受け皿を名指しして issue 化する**（束A の教訓）。

#### plan ∖ 導出（スコープ過剰候補）

- **`take_clicked_for` による通過点化**: 導出側は `layout.rs` に純粋述語 `click_is_current` を置く案（消費側が呼び忘れられる）。**当方の案を維持する**——フィールドを private にすれば**照合を迂回する経路が型から消える**（階梯 1 段上）。導出側も「推奨であって必須ではない」と留保しており矛盾しない

#### 一致（盲点が無いことの能動的証拠）

`wake_results` に触らない判断 / `results_window_height` を変えず外掛けする判断 / issue 案 2（path で同一性）を採らない判断 / issue #675 案 2 を採らない判断 / 0 件 = hide 契約の床 / 実装順序（世代 → clicked → クランプ → SPEC）。**独立に再一致した。**

#### 機構の欠陥（#733 の追随修正が要る）

**`/plan-review`「Step 2b」が指定する `Plan` タイプは Write / Edit ツールを持たない**（agent type の定義が両者を除外）。#733 で明文化した「成果物は `workspace/plan-review-2b.md` へ書かせる」は**そのままでは実行不能**である。今回はエージェントが `SendMessage` で全文を返し、主エージェントが保存した。

→ **受け皿を用意する**（本 PR には含めない）。選択肢: (a) Step 2b の agent type を書ける型へ変える、(b) 手順を「`SendMessage` で返させ、主エージェントが指定パスへ保存する」に直す。**(b) は「返り値に依存させない」という #731 の規範と衝突する**ので、(a) が筋に見える。

### 5b の 3 観点

1. **境界条件**: 0 件（hide 契約）/ available が 1 行未満・負 / 世代が一致・不一致 / `on_escape` の 2 分岐——いずれもテストで 1 件ずつ用意した。**進めすぎ**（全クリック落下）の側も `rows_generation_is_stable_on_selection_change` で固定する
2. **シンプル化の挑戦**: 新しい状態は `rows_generation: u64` **1 つだけ**で、しかも既存の `snapshot_generation` を**移設**するので純増はゼロ。`ClickTake` は enum 1 つだが、`Option` では「無かった」と「捨てた」を区別できず trace が書けない。**手動 bump 3 箇所追加（代替 B）のほうが差分は小さいが、漏らした経路を手に戻すので採らない**
3. **破壊不変条件 + 検知手段**: 「誤った行を起動しない」が壊れたら即アウト。検知は (a) `search_state.rs` の世代テスト 5 件、(b) `results_view.rs` のクリック 3 件、(c) `layout.rs` のクランプ 5 件、(d) `egui_results:click_stale` trace（診断のみ）。**#699 の競合窓は手で再現できない**——ユニットテストで純粋核を固定するのが唯一の担保であり、これは受容する残余ではなく**設計上の到達点**である
