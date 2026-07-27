# MECE レビュー（責務分割そのもの） — #749 段 1: WindowCoordinator

対象: `workspace/plan.md`（#749）。レンズ: **分割後の責務が相互排他かつ網羅か**。
判定はすべて `src-tauri/` のコードを読んで自分で行った（引用はすべて実ファイルから）。

## 結論（先に）

**実装をブロックする指摘は無い。** 挙動不変は保たれ、指摘のどれも動作を変えない。
壊れるのは**恒久的な責務記述の正確さ**である——新設する `window_coordinator.rs` の `//!`、
plan.md:202 の全称文、`src-tauri/CLAUDE.md` の該当行。`//!` は責務の正本（#562）であり、
`governance:check` は文言内容を検査しない。**偽の責務宣言はこの PR で恒久化され、次の実装者を誤らせる。**

要修正 4 / 網羅の破れ 4 / グレーゾーン 5 / 成立確認 6。

### 構造的な主張（以下すべての親）

**分割の線が 5 つの異なる原理で引かれており、2 つが衝突したときどちらが勝つかが書かれていない。**

| # | 原理 | 計画での適用例 |
|---|---|---|
| P1 | 責務の種類（撃つ / 判定する / 所有する） | coordinator ⇄ `layout.rs` ⇄ `results_window.rs` の 3 分割 |
| P2 | 唯一の消費者がどこか | 「view.rs での利用点は drive の 1 か所だけ」→ `max_results` を移す（plan.md:183） |
| P3 | 実行スレッドの同一性 | 「`show_egui_main` はホットキー listener＝ Win32 メッセージループスレッドから走り」→ `reset_size_guard` は view に残す（plan.md:89） |
| P4 | 段 3（#666）の前提を動かさない | managed state の構成・main の毎フレーム `set_size`（plan.md:28-29） |
| P5 | フレーム内の読み点 | `result_count`（消費後）/ `plain_hidden`（消費前）だけ引数（plan.md:202） |

原理が 5 つあること自体は悪ではない（現実の制約が 5 種類あるため）。問題は **(a) どれが優先か
が書かれておらず、実際に P1 と P2 が衝突している箇所が処理されていないこと**（→ E1）、
**(b) P5 の適用範囲が誤って全称化されていること**（→ E3）である。

---

## 排他性の破れ（同じ種類が 2 か所・要修正）

### E1. `read_metrics` と `max_results` — 同じ形の 2 件が逆の扱いを受け、判別規則が無い

**事実（grep 実測・全件）:**

```
src-tauri/src/egui_shell/mod.rs:326:pub(crate) fn read_metrics(app: &tauri::AppHandle) -> layout::Metrics {
src-tauri/src/egui_shell/mod.rs:392:        let bar_h = read_metrics(app).bar_height;      ← 唯一の呼び出し点
src-tauri/src/egui_shell/view.rs:818:                max_results: self.max_results(),        ← 唯一の呼び出し点
```

- `read_metrics` の呼び出し点は `mod.rs:392`（`show_egui_main` の中）**1 か所だけ**。`show_egui_main` は Phase 3 で coordinator へ移る
- `max_results` の呼び出し点は `view.rs:818`（`drive_results_window` の中）**1 か所だけ**。`drive_results_window` は Phase 4 で coordinator へ移る

**両者は「config を読む小さなヘルパーで、唯一の消費者が coordinator へ移る」という同一の形である。**
計画は `max_results` を移し（plan.md:183「view.rs での利用点は drive の 1 か所だけであり、残すと
view-local ヘルパーと引数経路が二重になる」）、`read_metrics` は移さない（`research.md` §2 の
「しない」表に**理由欄が空のまま**載っている）。判別規則がどこにも書かれていない。

計画は移設後の参照を `show_egui_main` → `super::read_metrics`（plan.md:147）と書いており、
**この関数が coordinator 専用ヘルパーであることを自ら認めた形になっている。**

想定される反論を先に潰す: `mod.rs:344` の「**`read_metrics` は残す**（統合しない）——`show_egui_main` が
show 経路で高さだけを要り、色 parse を払わせないため」は、**`read_visual` と統合しないこと**を述べた
文であって、どのモジュールに置くかを述べた文ではない。据え置きの根拠にはならない。

対比として **`read_visual` を mod.rs に残すのは正しい**（消費者が `view.rs:1158` と
`results_view.rs:467` の 2 つあり、どちらも coordinator ではない）。**つまり P2 は使える規則であり、
使われていないのが `read_metrics` の 1 件だけである**——だからこそ判別規則の不在が露見する。

**対処（どちらでもよい。決めて書くことが対処である）:** ①`read_metrics` も移す、
または ②「config 読みのヘルパーは、消費者が coordinator だけでも `mod.rs` に残す（coordinator は
config 読みの所有点ではない）」と 1 行書き、`max_results` の移設をその例外として理由付ける。

### E2. サイズのデルタガードが 2 つの層に分かれ、既存の散文が偽になる

Phase 2 で results のガード状態は**リソース型**（`ResultsWindow.last_size`）へ入り、
main のガード状態は**driver**（`SearchWindowView.last_set_height` / `last_set_width`・`view.rs:261,304`）に
残る。plan.md:55 は式だけ共有し状態は分けると明記しており、判断としては通る（→ G1 で warrant を補う）。
**排他性の破れはその帰結として散文が壊れる点にある。**

`mod.rs:571-573`（`position_results_below_main` の doc・**Phase 3 で coordinator へそのまま移る**）。
なお「この文は `set_position` についての記述にすぎない」という読みは通らない——括弧内は
`set_position は同値でも安価` と `ガードは update 側の責務` の**2 節**で、後者は**ガードがどの層に
住むかについての一般則**として書かれており、かつ**その規則が書かれているのはここだけ**である:

```rust
/// デルタガードは持たない(set_position は同値でも安価・
/// ガードは update 側の責務)。
```

Phase 2 の後、「**ガードは update 側の責務**」は偽になる——`set_size` のガードは `ResultsWindow` の
内側へ移り、update 側には無い。plan.md:119 は「**「本文は一字も変えない」の例外はこの 4 か所と、
下の `//!` だけである**」と全称で宣言しているため、この文はそのまま新ファイルへ運ばれ、
**新設モジュールが自分の中で矛盾した所有権記述を持つ**ことになる。

`デルタガード` の全出現を grep して確認した（`src-tauri/` / `SPEC.md` / `docs/`・`docs/superpowers` 除く・8 件）。
移設で偽になるのは上記 1 件で、`view.rs:837`（`set_position` にガードを置かない理由）と
`view.rs:786,849,856,1188` は移設後も真、`view.rs:285,288` は削除対象。**クラスではなく 1 サイトである。**

**対処:** 例外を 5 か所にし、`mod.rs:572` の「ガードは update 側の責務」を
「`set_size` は自己ガードする（`ResultsWindow`）が、`set_position` は同値でも安価ゆえ持たない」へ直す。

### E3. plan.md:202 の全称文が偽 — 「読み点の制約」は 2 つではなく 4 つある

```
**読み点の制約があるのは `result_count`（消費後）と `plain_hidden`（消費前）の 2 つだけ**で、
引数に残すのはその 2 つ + 幾何 2 つ（`width` / `row_height`）とする
```

**「幾何 2 つ」は誤りである。両方とも config 由来の読みであり、両方とも読み点に制約がある。**

1. `row_height` — `metrics` は `view.rs:1158` の `read_visual` が返す `VisualSnapshot` の一部である
   （`view.rs:1165: let metrics = &visual.metrics;`）。`src-tauri/CLAUDE.md`:
   「**テーマ色・font・行高の読みは 1 フレーム 1 回（#673 spec 決定 4）** …**同じ値を後段で
   config から読み直さない**——間に `config_watcher` の適用が挟まると、同じフレームの中で新旧が混ざる
   （新 `font_size` を旧行高で描く等）」。**これは読み点の制約そのものである。**
2. `width` — `view.rs:1829: let width = self.window_width();` の 1 回の読みが、`view.rs:1834` の main の
   `set_size` と `view.rs:1838` の drive へ**同一値として**渡る。`window_width` の doc（`view.rs:771-777`）が
   その理由を明記する: 「**main（本 view）が両窓（main・results）の唯一の size writer に一意化されている**」
   「config_watcher（notify スレッド）の幅 set_size と 2 次元 read-modify-write で潰し合う race の片翼だった」。
   coordinator が独自に読むと、main の `set_size` を挟んで 2 回読むことになり、その間に
   `config_watcher` が適用すれば両窓の幅が 1 フレーム食い違う。

**具体的な危険:** 計画が明示した規則（「config の live-read であり読み点の制約が無い」→ 内側で読む・
plan.md:202）を実装者が忠実に適用すると、`width` と `row_height` も内側で読む方向へ倒れる。
`cargo test` では落ちない。plan.md:202 の「引数を増やすほど I1 を壊す誘惑が増える」は
**費用の話であって境界の原理ではない**ため、この誤りを止める力を持たない。

**対処:** 「制約は 2 つだけ」を撤回し、引数 4 つそれぞれに制約の種類を書く
（②③=フレーム内の読み順 / `row_height`=#673 決定 4 の 1 フレーム 1 読み / `width`=両窓同一値）。
`AGENTS.md`「全称表現は前提条件とセットで書く。**書けないなら書かない**」の直接の適用対象である。

### E4. listener 登録が 2 か所に分かれ、判別規則が無い

計画の分担は「`mod.rs` — 窓生成・共有状態・config 読み・**listener 登録**」だが、
`register_hide_listener` は coordinator へ移る（plan.md:106）。**同じ種類が 2 か所にある。**

最も自然な判別規則は「呼ぶ相手が coordinator の関数なら coordinator へ」だが、
**`register_initial_hotkey_failure_listener` で破れる**（`mod.rs:669-680`・据え置き）:

```rust
        show_egui_main(&handle, Instant::now());
        wake_main(&handle);
```

coordinator の driver を 2 つ呼んでいる。`register_config_wake_listeners`（`mod.rs:630-641`）も
`wake_main` を呼ぶ。したがって現行の分け方の実際の規則は「**その listener が coordinator の関数**
**だけ**を呼び、他に何もしないなら移す」になるが、これは書かれていない。

**対処:** `register_hide_listener` を移す理由を 1 行書く（例: 「main の hide の合流点は
`hide_egui_main` であり、その受け口を同じモジュールに置く。ほかの listener は payload 整形・
pending 格納など coordinator の外の仕事を持つため `mod.rs` に残る」）。
移設対象を変える必要は無い。

---

## 網羅の破れ（どこにも属さない・2 か所に跨る）

### N1. z-order — 分割後もどこにも集約されず、`//!` だけが集約を主張する【最重要】

issue が挙げる 5 責務（可視性・位置・サイズ・z-order・wake）のうち、**z-order だけは 1 行も coordinator へ入らない。**

| 対象 | 実体 | 現在地 | 段 1 後 |
|---|---|---|---|
| results | `ResultsWindow::set_topmost` | `results_window.rs:90` / `:111` | 変わらず（所有型） |
| main | `main.set_always_on_top(false/true)` | `commands/window.rs:94` / `:140` | **変わらず・直呼び・所有点は無い** |
| 初期値 | `.always_on_top(true)` × 2 | `mod.rs:243` / `:261` | 変わらず（`create`） |

呼ぶのは `launch_settings_process` 本体と、そこから spawn される監視スレッド（`commands/window.rs:104`）で、
**どちらも coordinator を通らない。**

それにもかかわらず Phase 3 の `//!` 草稿（plan.md:96）は次を宣言する:

```
//! 窓の可視性・位置・サイズ・z-order・wake を駆動する 1 つの責務（#749 段 1）。
```

**この文は書いた瞬間に偽である。**

さらに: `plan-review/independent-derivation.md:54` が既にこの点を指摘し、
「段 1 では `ResultsWindow::set_topmost` を通す現状を維持し、**「z-order の所有点は `ResultsWindow`」と
記述で閉じるのが最小**」と対処まで書いている。plan.md の「独立導出との差分」節（312-319 行）は
漏れ 3 件（`position_on_target_monitor` / `cfg(not(windows))` / smoke の `-ResultsQuery`）を取り込んだが、
**この 1 件だけが取り込まれず、言及もされていない。**

**対処（移設は推奨しない）:** `commands/window.rs` → `egui_shell` の依存方向が増えるうえ、
issue の規定は「ほぼ移設・意味変化ほぼゼロ」である。`//!` を実態に合わせる。→ 末尾の「`//!` 案」。

### N2. サイズが 2 か所に跨る（results = coordinator / main = view）

`view.rs:1824-1837` の main の毎フレーム `set_size` は view に残る（plan.md:29 で明示）。
判断自体は妥当（→ G1）だが、`//!` の「**サイズ**を駆動する 1 つの責務」は同じく偽になる。

### N3. 位置が 3 ファイルに跨る

| 実体 | 位置 | 段 1 後 |
|---|---|---|
| `position_on_target_monitor` / `save_placement_relative` / `position_results_below_main` | `main.rs:150` / `mod.rs:505` / `:580` | coordinator |
| `frame.drag_window()`（`view.rs:1150`）— ユーザードラッグによる main の移動開始 | `view.rs` | **view に残る** |
| `Moved` リスナーの登録と本体（`mod.rs:283-290`・`position_results_below_main` を呼ぶ） | `mod.rs`（`create` 内） | **mod.rs に残る**（生成の一部） |

`drag_window` は「窓の位置を変える」経路そのものであり、その結果として `Moved` が発火して
results が追従する。**位置の入力側（drag）と反応側（Moved 登録）は coordinator の外にある。**
どちらも移すべきとは思わない（drag は入力処理、`Moved` 登録は窓生成と不可分）が、
「位置を 1 つの責務へ集めた」という主張はこの 2 件によっても偽になる。

**列挙は完全である。** `drag_window` は `tauri::Window` ではなく `RuntimeFrame` のメソッドゆえ、
窓操作 API の grep には掛からない別クラスである。`grep -n "frame\." view.rs results_view.rs` の
結果は `view.rs:1150` の **1 件のみ**で、同クラスの他の窓操作は無い。

### N4. Phase 5 の文書同期表は「他ファイルの変更で偽になる散文」を捕まえない

Phase 5 の表（plan.md:208-215）は**責務が変わる 6 ファイル**を単位にしており、
各ファイルの `//!` と `src-tauri/CLAUDE.md` の行を見る設計である。
E2 の `mod.rs:572` は「`results_window.rs` の変更によって偽になる `position_results_below_main` の doc」で、
**この表の網の目からこぼれる**（`mod.rs` は表に載っているが、見るのは `//!` と CLAUDE.md 行だけ）。

**対処:** Phase 5 に「所有権を述べた関数 doc」の照合を 1 行足す。今回は `デルタガード` の grep で
全数（8 件）を確認でき、対象は 1 件だけである（→ E2）。

---

## グレーゾーン（判断は妥当だが理由付けが弱い／明記が要る）

### G1. main の毎フレーム `set_size` を view に残す判断は正しいが、掲げた理由が的を外している

plan.md:29 / :55 が挙げる理由は ADR-0007「却下 1 の第 3 理由」（main の高さは `show_egui_main` の
`bar_height` collapse と `main_window_height` の**意図的な 2 導出**）である。しかし ADR-0007 のその理由は
**「純粋な導出（`layout.rs`）へ束ねるか」についての理由**であって、**driver をどこに置くか**の理由ではない。
main の `set_size` を coordinator へ移しても 2 導出は 2 導出のまま残る（`row_height` と同じく
高さを引数で渡すだけ）。**warrant が層をまたいで流用されている。**

**実際に効いている理由はもっと強い**——main の高さの入力が**そのフレームの描画パスの副産物**だからである:

```
view.rs:1609:        let has_status = overlay_text.is_some();
view.rs:1634:        let has_toast = toast_row.is_some();
view.rs:1826:            has_status.then_some(metrics.toast_height),
```

`overlay_text` は `launching` / `notice` / `indexing` から、`toast_row` は `UpdaterUiState` の
`toast()` から作られ、どちらも描画の途中（1590-1640 行）で確定する。**フレームの外から与えられる
入力ではないため、coordinator の引数にできない。** これを書けば、段 3 で view を割るときにも効く。

### G2. `width` を引数にする判断は正しいが、「幾何」という分類が誤り

→ E3 に統合。判断（引数のまま）は維持でよい。理由を「両窓が同一フレームで同一値を使うため」
（`view.rs:771-777` の `window_width` doc が正本）へ差し替える。

### G3. `reset_size_guard` を view に残す判断（P3・スレッド同一性）だけが別原理

plan.md:89 の理由（`show_egui_main` は Win32 メッセージループスレッドから走り、reset がフレーム
進行中に割り込みうる）は**正しく、一次資料に接地している**（`src-tauri/CLAUDE.md`「`app.listen` の
コールバックは emit した呼び出し元スレッド上で同期実行される」）。

指摘は 1 点だけ: **この 1 件だけが「実行スレッド」という別の軸で線を引いている。**
ほかの線は「責務の種類」「消費者」「読み点」で引かれている。
`//!` か `reset_size_guard` の doc に「**この関数の呼び出し点はイベントループスレッドに限る**
（`show_egui_main` から呼ばない理由）」と書かないと、段 3 で view を割る人が
「reset は show の一部だから coordinator へ」と自然に動かす。I11 は「drive より前」しか述べておらず、
**スレッドの制約は不変条件表に無い。**

### G4. `//!` 草稿の「撃ってよいかの判定は持たない」は全称としては成り立たない

plan.md:98-99:

```
//! 「撃つ主体」を集めた場所であって、「撃ってよいか」の判定は持たない——可視性の
//! 述語は `layout::present_results`（純粋核・#752）
```

Phase 4 の後、coordinator は (a) `main_visible` をどの時点で読むか、(b) `max_results` をどこで読むか、
(c) `clamp_results_height` をいつ適用するか（`view.rs:851-855` の現行ブロック）を持つ。
**判定式は持たないが、判定の入力をいつ読むかという方針は持つ。** ADR-0007 が
「クランプは導出の外（driver が行う）」と決めた以上これは正しい設計だが、`//!` の文はそれを隠す。
「**判定式**は持たない（正本は `layout::present_results`）。**読み点とクランプの適用**は本モジュールが持つ」
と 2 文に割るのが実態に合う。

### G5. 段 3（#666）との整合 — 段 1 が「ガードは窓の所有型が持つ」先例を作るが、main には所有型が無い

段 3 は `view.rs` を LauncherController / MainView へ割る（#666 のコメント）。
段 1 が終わると main のサイズガード（`last_set_height` / `last_set_width`）は view に残り、
段 3 でどちらへ行くか決める必要が出る。段 1 の先例（「ガードは窓の所有型へ」）は
**main に所有型が無いため適用できない**——`ResultsWindow` に相当する `MainWindow` は存在しない。

段 3 を難しくするほどではない（`has_status` / `has_toast` 依存ゆえ MainView 側へ行くのが自然で、
G1 の正しい warrant を書いておけば自明になる）。ただし **G1 の warrant を書き損ねると、
段 3 の実装者は段 1 の先例に引かれて `MainWindow` 型の新設へ向かう**——これは段 1 が
「managed state の構成を変えない」（plan.md:28）で避けた当のものである。

**評価: 段 1 の線が段 3 を難しくしてはいない。** ただし G1・G3 の明記がその前提である。
なお `//!` を段 3 でどう割り直すかは、#666 の設計書が扱う範囲として据え置いてよい。

---

## ADR-0007 との整合（レビュー観点 5 への明示の判定）

**判定: 段 1 の線は ADR-0007 の線と矛盾していない。ただし段 1 が引用した warrant は層をまたいでおり、
そこだけが破れである（= G1）。**

両者は**別の境界**を引いている。ADR-0007 の線は **導出 / driver** の境界（何を純粋な導出の中へ
入れるか）、段 1 の線は **driver / driver** の境界（適用する側をどのモジュールが持つか）である。
「同じ原理か」を直接問えない関係にあり、**層が本当に別であることは `results_origin` が実証する**:

- ADR-0007 は `results_origin` を**導出から外した**（却下 1 の第 1 理由: 第 2 の消費者が `Moved`
  リスナーでフレームに閉じない）
- 段 1 は `position_results_below_main` を**coordinator へ入れる**（plan.md:106）。
  `Moved` リスナー（`mod.rs:287`）は自由関数として同じ実体へ到達し続けるため、**矛盾しない**

`main_size` も同型である——導出から外した理由（第 3 理由・2 導出）と、driver を view に残す理由
（描画パスの副産物）は**別の理由**であり、結論が一致しているだけである。plan.md:29 / :55 は
前者を後者の根拠として引いており、**これが唯一の破れ**（→ G1 で正しい warrant を提示した）。

「クランプは導出の外」との整合も成立している（→ C2・G4）: クランプは `layout.rs` に純粋関数として
在りつつ**適用は driver**という ADR の形が、段 1 でもそのまま保たれる。`size_delta_exceeds` を
`layout.rs` へ置くのは同じ形の反復であって、新しい原理ではない。

---

## 確認して MECE が成立していたもの

- **C1. `read_visual` を `mod.rs` に残す** — 消費者が `view.rs:1158` と `results_view.rs:467` の 2 つで、
  どちらも coordinator ではない。E1 の判別規則（P2）と整合する側の実例
- **C2. `size_delta_exceeds` を `layout.rs` へ置く** — `layout.rs` の `//!` は「純粋レイアウト/タイミング
  ヘルパー」であり、既に `Debouncer`（タイミング）と `clamp_results_height`（ADR-0007 が「導出の外」と
  決めた関数）が同居している。**`layout.rs` は「導出」ではなく「純粋関数」の置き場**であり、
  性能ガードの述語を入れても ADR-0007 の線と矛盾しない。テスト可能性の対価も実在する
  （`.claude/rules/src-tauri.md`「Win32 依存モジュールはユニットテスト前提にしない」の裏返し）
- **C3. `ResultsWindow` を coordinator へ吸収しない** — `results_window.rs` の `//!` が
  「`Deref` を実装しない」「raw 3 点セット」の根拠を持っており、吸収すると散る。
  「所有点」と「駆動」は別の責務として立っている
- **C4. `position_on_target_monitor` を含める** — `save_placement_relative`（保存）が移るのに
  `position_on_target_monitor`（復元）が `main.rs` に残ると対称が壊れる。呼び出し元は
  `mod.rs:396`（`show_egui_main`）1 か所で、それも同時に移る。**排他性・網羅ともこの取り込みで改善する**
- **C5. wake を primitive のまま出す（#711）** — `wake_main` / `wake_results` は移設のみで、
  `request_repaint_after` 系は view に残る。`view.rs:1801`（snapshot 差分の edge wake）を
  coordinator へ移さない判断も正しい（publish 経路に属する）
- **C6. `create()` を `mod.rs` に残す** — 窓の生成（`.visible(false)` / `.inner_size` /
  `.always_on_top(true)` / `Moved` リスナー登録）は「初期値の宣言」であって「駆動」ではない。
  `src-tauri/CLAUDE.md`「ウィンドウ生成の制約」（生成は setup 限定）とも整合する

---

## 提案（3 つの指摘が 1 つの修正に落ちる）

N1・N2・N3 と G4 は**同じ 1 か所の欠陥**である。`//!` を実態へ絞れば全部閉じる:

```rust
//! **results 窓**の可視性・位置・サイズと、**main 窓**の show / hide / 位置復元・保存、
//! および両窓の wake を駆動する（#749 段 1）。「撃つ主体」を集めた場所である。
//!
//! **判定式は持たない**——可視性の述語は `layout::present_results`（純粋核・#752）、
//! raw 操作の所有点は `results_window::ResultsWindow`（#671 PR A′）。
//! ただし**読み点の順序とクランプの適用**は本モジュールが持つ（ADR-0007「クランプは導出の外」）。
//!
//! **本モジュールに**入っていない**もの**（意図的・段 1 の範囲外）:
//! - **z-order**: results は `ResultsWindow::set_topmost` が所有点、main は
//!   `commands/window.rs` が `set_always_on_top` を直呼びする（設定サイドカーのライフサイクルに従属）
//! - **main 窓の毎フレームのサイズ**: 高さの入力（status / toast 行の有無）が描画パスの
//!   副産物であり、フレームの外から与えられない（`view.rs`）
//! - **ユーザードラッグによる main の移動**（`view.rs` の `drag_window`）と、それに応答する
//!   `Moved` リスナーの登録（`mod.rs` の `create`）
//!
//! **wake は primitive として公開する**（#711）——「いつ起こすか」を本モジュールが
//! 決めた瞬間に「armed 期限は保持者が毎フレーム再要求する」契約が壊れる。
```

同じ絞り込みを `src-tauri/CLAUDE.md` の新規行にも反映する（Phase 5-1）。

---

## 未検証（理由）

- **`view.rs` の全 1948 行は読んでいない。** 読んだのは 1-120 / 260-380 / 730-890 / 1150-1270 /
  1590-1650 / 1770-1900 と、`row_height` / `metrics.` / 窓操作 API の grep。
  中盤（400-730 / 890-1150 / 1270-1590）に窓操作が残っていないことは grep
  （`set_always_on_top|set_topmost|drag_window|set_background_color|\.show\(\)|\.hide\(\)|set_focus`）で
  確認したが、**「窓に触る」の別名（`frame.` 経由の runtime API 等）までは網羅していない**
- **`#[cfg(not(windows))]` の arm は実際にコンパイルしていない**（I9 / I10 と同じ残余）。
  件数照合の妥当性は plan の主張を読んだだけである
- **先行レビュー 4 本のうち全文を読んだのは無い。** `independent-derivation.md` は 18-87 行のみ、
  他 3 本は grep（`topmost|z-order|drag_window|read_metrics|window_width`）のみ。
  ただし N1 の「z-order を指摘したのは `independent-derivation.md:54` だけ」は grep で substantiate
  できている——`rust-coordinator-move.md:12` は「移設対象の 8 関数とは無関係」、
  `rust-guard-and-layout.md:8,12,27` は `set_topmost` を**呼び出し元スレッドの列挙**として挙げるのみで、
  いずれも `//!` の z-order 主張には触れていない。**全文を読んでいないことによる残余は、
  この 3 本が z-order 以外の観点で同じ結論に達している可能性である**
- **`snotra-egui-runtime` 側（`WindowWaker` / `EguiView` の契約）は見ていない。** wake の
  primitive 性（C5）は `#711` の記述と `mod.rs` の doc を根拠にしており、実装は追っていない
- **段 3 の設計書は存在しない**（#666 は 1 行本文 + 位置づけコメントのみ）。G5 は
  「たたき台の箱割り」からの推測であり、実在の設計との突き合わせではない
- **`SPEC.md` は §8.5 / §8.6 の該当行を plan.md 経由で参照しただけ**で、全文は読んでいない
