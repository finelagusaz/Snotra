# 実装計画: #878 継ぎ目 2 を塞ぎ、継ぎ目 4 の決着を記録する

調査の正本は `workspace/research.md`。基点 `main` = `6638f7f9`、ブランチ `chore/878-window-geometry-seams`。

## 目的

#878 に残っていた 2 つの役目を両方片付ける。

1. **継ぎ目 2（唯一の違反 R1）を塞ぐ**——`position_on_target_monitor` が寸法を引数で受け取り、
   「値を渡すためだけの `set_size`」を撤去する
2. **継ぎ目 4 の決着と、検出器の扱いを記録する**——issue 本文と 2026-08-04 コメントが今も
   「継ぎ目 4 は手つかず」と読める状態を訂正する

**あるべき姿（裁定則・`research.md` §3.1）**:

> 窓の矩形から読んでよいのは、コードが持っていない量だけである——ユーザーが動かした位置、
> 非クライアント差分、scale。コード自身が直前に書いた content 寸法を、渡す手段が無いという
> 理由で読み戻してはならない。渡す手段が無いなら、渡す手段を作る。
>
> 例外: 書き手のフレーム文脈が読み手に届かない経路（`Moved` リスナー）が呼び出し元に
> 含まれるとき、読み戻しは正当である。

## 受け入れ条件

1. `show_egui_main` の `set_size` が **1 回**になる（現在 2 回）。撤去するのは 1 手目
   （`window_coordinator.rs:338`）である
2. `position_on_target_monitor` は `main.outer_size()` を読まない。バー矩形の物理サイズを
   引数で受け取る
3. **窓の位置は変わらない**——同一 config・同一モニターで、変更前後の `egui_show:done` 時点の
   main の X/Y が一致する（実測で確かめる）
4. show の位置決めが退行したとき、**in-process の信号が発火する**。故障注入で実測する
5. smoke がその信号の**不在**を断言する（`Test-SnotraNoHeightMismatch` と同型）
6. `layout::bar_rect_height_phys` の doc・`src-tauri/CLAUDE.md`「モジュール構成」・
   `position_results_below_main` の doc が、変更後の構造と一致する
7. #878 へ、継ぎ目 4 の決着・裁定則・R4 の却下理由・射程外 1 件をコメントで残す

## 変更ファイル一覧と対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/layout.rs` | `logical_to_phys`（**新規**・純粋核） | 論理→物理の丸め規則を 1 か所へ。`bar_rect_height_phys` がこれへ委譲 |
| 同上 | `bar_rect_height_phys` | 委譲へ変更。doc から「show 経路は `outer_size()` を読み戻すため OS から供給される」を落とす |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `FrameGeom` / `read_frame_geom`（**新規**） | 窓の frame 幾何（`outer_size` / `inner_size` の差 = 非クライアント、`scale`、`outer`）を**1 回だけ**読む唯一の点 |
| 同上 | `BarRectPhys`（**新規**） | バー矩形の物理サイズ。構築点は `derive_bar_rect_phys` のみ |
| 同上 | `derive_bar_rect_phys`（**新規**） | show が「これから当てるバー矩形」を OS へ書かずに導く |
| 同上 | `position_on_target_monitor` | シグネチャに `BarRectPhys` を追加。`outer_size()` の読みを削除 |
| 同上 | `read_bar_anchor` | `read_frame_geom` から合成する形へ（非クライアント合成の写しを作らない） |
| 同上 | `show_egui_main` | 1 手目の `set_size` を撤去。`derive_bar_rect_phys` → `position_on_target_monitor` → `set_size(実高)` の順 |
| 同上 | `clamp_main_into_work_area` | 戻り値を `bool`（実際に動かしたか）へ。`#[cfg(not(windows))]` 版も同じ型 |
| 同上 | `position_results_below_main` | doc に「共有 atomic 案を却下した理由」を追記（コードは変えない） |
| `src-tauri/src/egui_shell/view.rs` | `update` 内のクランプ呼び出し（:1279-1281） | `was_reset_frame && 動いた` で `egui_main:position_clamped_after_show` を trace |
| `scripts/smoke-egui.ps1` | 新規判定関数 + シナリオ 2 の断言 | 可視区間に当該 trace が現れないことを断言 |
| `src-tauri/CLAUDE.md` | 「モジュール構成」`window_coordinator.rs` の項 | show が寸法を OS へ書いて読み戻す構造が消えたことを反映 |
| `docs/architecture.md` | :82 末尾 | 「show 時に bar_height（既定 43px）へリセットする」が**偽になる**——物理的に畳む瞬間が消えるため |
| `docs/adr/ADR-show-path-derives-bar-rect.md`（**新規**） | — | `ADR-show-path-derives-drawn-height` **却下 2 の反転**を記録する（下記） |

**触らない**: `SPEC.md`（挙動不変。§8.2 の「クランプはバー高を対象」は変更後も同じ）／
`monitor.rs`（`WorkArea::clamp` の算術は再利用する・新しい算術を書かない）／`ResultsWindow`／
**既存の ADR**（`ADR-adr-frozen-history`: ADR は凍結された歴史。反転は新しい ADR に書く）。

### 消してはならないもの

- **`read_bar_anchor` そのもの**——`docs/comment-guidelines.md:29` が「構造が同一性を担保して
  いるので要石」の**模範例として名指している**。本計画は内部を `read_frame_geom` からの合成へ
  変えるだけで、「クランプと hide 保存が同じ 1 つの関数を通る」という当該記述は真のまま
- `PERFORMANCE.md:548`（`clamp_main_into_work_area` の実測行）と
  `scripts/manual-smoke.ps1:118,123`（`position_on_target_monitor` の移設確認）は、
  戻り値の型変更・引数追加では偽にならない（**grep で現物を確認済み**）

## 実装順序

### Phase 1 — R1 を塞ぐ（挙動不変）

- [ ] `layout::logical_to_phys(logical: f64, scale: MainScale) -> i32` を追加し、
      `bar_rect_height_phys` を委譲へ変える。ユニットテストで丸め規則を固定する
      （`f64::round` = 0 から遠ざかる丸め。`dpi 0.1.2` の `Pixel::from_f64` と同じ）
- [ ] `FrameGeom { outer: PhysicalSize<u32>, inset_w: i32, inset_h: i32, scale: MainScale }` と
      `read_frame_geom(window) -> Option<FrameGeom>` を追加する（`outer_size` / `inner_size` /
      `scale_factor` の読みは**ここ 1 回**。取得失敗は `None`）
- [ ] `read_bar_anchor` を `outer_position()` + `read_frame_geom` の合成へ書き替える
      （既存の `outer - inner` の直書きを消す。**Win32 の読みの回数は増やさない**）
- [ ] `derive_bar_rect_phys(window, width_logical, bar_height_logical) -> Option<BarRectPhys>` を
      追加する。`width = logical_to_phys(width) + inset_w` / `height = bar_rect_height_phys(bar_h) + inset_h`
- [ ] `position_on_target_monitor` に `bar: BarRectPhys` を足し、`main.outer_size()` の読みを削除する。
      `WorkArea::clamp` / `center` へ渡す `win_w` / `win_h` をその値にする
- [ ] `show_egui_main` から 1 手目の `set_size`（`:338`）を撤去し、`derive_bar_rect_phys` の
      結果を `position_on_target_monitor` へ渡す。**`derive_bar_rect_phys` が `None` のときは
      位置決めをしない**（現行の「取得失敗ならクランプしない側へ倒す」と同じ倒し方）
- [ ] `cargo build -p snotra-tauri` と `cargo test` が通る（カテゴリ A）

### Phase 2 — 退行を見る in-process 信号（セーフティネットの新設）

- [ ] `clamp_main_into_work_area` の戻り値を `bool`（`set_position` を撃ったか）にする。
      `#[must_use]` を付ける（#957 の揃え方に倣う）
- [ ] `view.rs` のクランプ呼び出しで、`was_reset_frame` かつ戻り値が `true` のとき
      `egui_main:position_clamped_after_show` を trace する。payload は
      `{"from_x","from_y","to_x","to_y"}`
- [ ] `scripts/smoke-egui.ps1` に不在断言を足す（`Test-SnotraNoHeightMismatch` と同型・
      `seq` で可視区間を切る）
- [ ] **故障注入で発火を実測する**——`derive_bar_rect_phys` の `height` を実高へ差し替えた
      複製ビルドで、作業領域の下端付近に置いた main が trace を出すことを確認する。
      **稼働中のガードは弱めない**（`.claude/rules/safety-nets.md`）
- [ ] 故障注入を巻き戻し、**通常ビルドで trace が 0 件**であることを確認する
- [ ] **検出器の死角を、発火条件の隣（`view.rs` の呼び出し点コメント）へ宣言する**——下記

### Phase 3 — 記録を構造へ一致させる

- [ ] `layout::bar_rect_height_phys` の doc を書き替える（show 経路の読み戻しが消えたこと・
      丸め規則への依存が**この doc 1 か所へ集約された**こと）
- [ ] `position_results_below_main` の doc に、R4 を読み戻しのまま残す理由と、共有 atomic 案を
      却下した理由（`research.md` §8 却下 1 の 3 点）を書く
- [ ] `src-tauri/CLAUDE.md`「モジュール構成」の `window_coordinator.rs` の項を更新する
- [ ] `docs/architecture.md:82` 末尾の「show 時に bar_height（`font_size + bar_padding`・既定 43px）へ
      リセットする」を、変更後の事実へ直す（畳む瞬間が無くなるため。**この 1 文だけを直す**）
- [ ] `docs/adr/ADR-show-path-derives-bar-rect.md` を新規作成する（内容は上記 5 点。
      **既存 ADR は編集しない**）
- [ ] `npm run governance:check` が通る（カテゴリ F）
- [ ] 実装差分を確定させる（`git diff` で変更ファイルが上表と一致することを確認する）

### Phase 4 — issue への記録

- [ ] #878 へコメントする: (a) 継ぎ目 4 は #909 + #917 + `SPEC.md` §8.2/§4.7 で閉じており、
      本文と 2026-08-04 コメントの「手つかず」は失効している。(b) 裁定則と 7 箇所の分類。
      (c) R4 を読み戻しのまま残す裁定と却下理由。(d) 検出器の扱い（#904 の形の 2 例目）。
      (e) 射程外として、キーボード移動（Alt+Space → M）とクランプの相互作用が未検証であること

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| show が置いた位置は、直後のフレームのクランプが動かす必要のない位置である（**死角つき**——下記） | **Phase 2 の trace**（新設）。故障注入で発火を実測する |
| 変更前後で main の X/Y が変わらない | 未確定 3 の実測（下） |
| バー矩形が作業領域内へ戻る（可視中・非押下フレーム） | 既存（`SPEC.md` §8.2・変更しない） |
| 「Win32 の読みはここ 1 回」（`read_bar_anchor` の doc の宣言） | `read_frame_geom` が唯一の読み点になることで構造的に保つ |

### 検出器の死角（宣言して止める・縛りを広げない）

plan-review A-1 の指摘を採用する。**ただし到達条件は自分で導出した**——指摘は「越境しうる」と
だけ述べていたが、実際に必要な条件は次のとおり狭い。

show は `target_wa.clamp(...)` で置くので、**バー矩形が作業領域に収まる限り、その中心も収まる**
——ゆえに 1 フレーム目の `point_monitor_work_area(中心)` は同じモニターを返し、クランプは
no-op になる。破れるのは `WorkArea::clamp` が「左上へ寄せるだけ」へ倒れるとき、すなわち
**バー矩形の幅が作業領域の幅を超えるとき**である（`monitor.rs:36-42`）。そのとき中心は
`left + win_w/2`（`layout::bar_rect_center`）なので、**中心が作業領域を出るには
`win_w > 2 × 作業領域幅` が要る**。この状態は `SPEC.md:483` が既に受容済みの残余であり、
`appearance.window_width` に上限が無いため到達可能ではある。

**対処は「発火条件を絞ること」ではなく「死角として宣言すること」**である
（検知器は必要な分だけ縛る——広く縛ると正当な変更まで赤くする）。宣言先は `view.rs` の
発火点コメントと新規 ADR。**smoke は単一モニター・`window_width=600` でこの経路へ到達しない**
ため、CI の断言は成立する。

**もう 1 つの死角（plan-review 軽微 1）**: 発火は `!any_down()` の内側にあるため、show 直後の
1 フレーム目でポインタが押されていれば**その回の検証機会が黙って落ちる**。偽陽性にも
偽陰性にもならないが、`egui_main:height_mismatch` には無い性質なので同じ場所へ書く。

**異常系**: `read_frame_geom` が `None`（`outer_size` / `inner_size` / `scale_factor` のいずれかが
失敗）のとき、show は**位置決めをしない**。現行の `position_on_target_monitor` が
`outer_size()` 失敗時に `return` するのと同じ倒し方であり、挙動は変わらない。
`clamp_main_into_work_area` の `None` 側も現行どおり「クランプしない」。

### 新規 ADR が要る理由 — これは既存 ADR の**却下の反転**である

`docs/adr/ADR-show-path-derives-drawn-height.md` の **却下 2** は、逐語で本計画そのものである。

> ### 2. 継ぎ目 2（`position_on_target_monitor` の OS 読み戻し）まで踏み込む
>
> サイズを引数で渡す形にできれば、show が窓を物理的に畳む必要そのものが消える。ただし同じ
> 読み戻しは `position_results_below_main` にもあり、そちらは毎フレーム + `Moved` リスナーの
> 2 経路から呼ばれてフレームに閉じない。フレームに閉じない消費者を巻き込む（#738 / #760 の
> 射程であり、ここでは触らない）。

**却下理由は「R1 を動かすと R4 を巻き込む」であり、本調査はそれが成り立たないことを示した**
——R4 は裁定則の例外条項で正当と判定され、**触らないまま R1 だけを動かせる**
（`research.md` §3.2・§8 却下 1）。同 ADR 自身が「**旧 ADR は編集しない。反転はここに記録する**」
という前例を持つ（#904 が `ADR-results-presentation-two-stage` 却下 1 を反転したときの形）。

新 ADR `ADR-show-path-derives-bar-rect.md` に書くこと:

- 採用: show がバー矩形の物理サイズを**導出して引数で渡す**（`ADR-show-path-derives-drawn-height`
  却下 2 の反転。**巻き込みが起きない**ことが反転の根拠）
- 却下: R4 を共有 atomic で塞ぐ案（`research.md` §8 却下 1 の 3 点）
- 却下: `read_bar_anchor` を show からも呼ぶ案（あちらは `outer_position()` を読み、show は
  これから位置を**書く**側なので、要らない読みが 1 つ増える）
- 帰結: `ADR-main-window-clamp-on-pointer-release`「残っている代価」の申し送り
  （「#760 に着手するときは、継ぎ目 2 を先に動かすほうが安いかを必ず量ること」）に答えたこと。
  同 ADR が記録した「同じ物理バー高が 2 通りに導出されている」代価が解消されること
- **前提が失効する既存 ADR を名指しする**（`ADR-show-path-derives-drawn-height:11` が採った形に倣う。
  **旧 ADR は編集しない**）:
  - `ADR-results-presentation-two-stage` **却下 6**（「`show_egui_main` の `bar_height` collapse を
    `main_window_height` へ統合する」）と `:27`——却下理由は「位置クランプが展開時の高さで効くのを
    防ぐために**意図的に**畳んでいる」だったが、**寸法を引数で渡せば畳む必要そのものが無い**。
    #904 が導出式の共有だけを反転したのに対し、本決定は collapse という**手段**を撤去する
  - `ADR-window-coordinator-split-rule` **決定 4** の括弧書き（「show 経路の bar_height collapse は
    coordinator」）——決定の本体（サイズ適用が 2 か所に分かれたままであること）は**真のまま**で、
    失効するのは片方を「collapse」と呼ぶ描写だけである

## テスト方針と検証コマンド

`docs/build-commands.md` が SSOT。該当カテゴリは **A（Rust）・C（表示順・trace イベント名）・
D（UI レイアウト）・F（ガバナンス文書）**。

- **ユニット**: `logical_to_phys` の丸め規則（0.5 境界・負値なし・scale 1.0 / 1.25 / 1.5）
- **既存の純粋核テスト**: `WorkArea::clamp` の 7 件は変更しない（算術を再利用するため）
- **カテゴリ C**: `scripts/smoke-egui.ps1`（trace イベント名を 1 つ増やすため前提に触れる）
- **カテゴリ D**: 目視——show 直後に位置が飛ばないこと・toast ありの show で高さスナップが
  出ないこと（既存の #801 断言が守る領域を壊していないこと）
- **故障注入**: Phase 2 の 2 項目（発火の実測と、巻き戻し後の 0 件）
- **CI の実測は PR 本文のチェックリストへ送る**（`.claude/rules/safety-nets.md`——`ci.yml` は
  `pull_request` でのみ起動するため、計画に置くと循環する）
- **`/race-check` は実装差分に対して走らせる**——スキル本文が「計画段階では起動しない」と
  明示している（#784）。Phase 2 完了後、コミット前に実行する（`Moved` リスナーが読む値と
  `was_reset_frame` 連言が母集団）

## `SPEC.md`・関連文書の更新要否

| 文書 | 要否 | 判定理由 |
|---|---|---|
| `SPEC.md` | **不要** | 挙動不変。§8.2「クランプはバー高を対象に行う」は変更後も真であり、§4.7 の「バーの位置は行の出没で動かさない」も変わらない |
| `src-tauri/CLAUDE.md` | **要** | 「show は寸法を OS へ書いてから読み戻す」構造が消えるため |
| `docs/architecture.md` | **要（1 文）** | :82 末尾が偽になる。**当初「不要」と判定したのは誤りで、plan-review が訂正した**（下の未確定 6） |
| `docs/adr/` | **要（新規 1 枚）** | 上記「新規 ADR が要る理由」。既存 ADR は編集しない |
| `docs/build-commands.md` | 不要 | trace イベント名は smoke script 側に閉じる（→未確定 5 で確認する） |

## 未確定（実装前に潰す）

- [x] **hidden な窓で `outer_size()` / `inner_size()` / `scale_factor()` が有効か** —
      **有効**。(1) 現行 `position_on_target_monitor` が hidden のまま `outer_size()` を読んで
      正しい位置を出している、(2) tao 0.35.3 は `GetWindowRect` / `GetClientRect` を呼ぶだけで
      可視性に依存しない（`platform_impl/windows/window.rs:255-270`・`util.rs:468-492`）。
      新規に増えるのは `inner_size()` 1 つのみ
- [x] **`set_size(logical)` 後の `outer_size()` を、コード側で導出できるか** —
      **できる**。tao の `set_inner_size`（`window.rs:272-313`）は
      `to_physical::<i32>(scale)`（= `f64::round`）に `(outer − inner)` を足して
      `set_inner_size_physical` へ渡し、装飾なしでは `adjust_window_rect` が恒等になる。
      ゆえに `outer_size_after = round(logical × scale) + (outer − inner)` であり、
      `bar_rect_height_phys` + 非クライアント合成と**同一式**
- [x] **2 手のあいだの中間サイズを観測する消費者は居るか** — **居る**（`mod.rs:378` の
      `Moved` リスナー → `position_results_below_main`）。**だが無害**——results は hidden で、
      可視化の直前（`window_coordinator.rs:870`）に必ず再配置される（`:882` の `show` より前）
- [x] **成長 `set_size` が左上を動かさないか** — **動かさない**。
      `util::set_inner_size_physical` は `SWP_NOMOVE` を立てる（`util.rs:114`）
- [x] **クランプ発火 trace の射程** — `was_reset_frame` との連言に限る。可視中ずっと出すと
      ドラッグ解放のたびに出て（正常動作）、不在断言が成立しない
- [x] **`docs/architecture.md` / `docs/build-commands.md` の更新要否** — **当初の判定は誤りだった。**
      `grep -c "outer_size\|position_on_target_monitor\|bar_rect"` は両ファイル 0 件だが、
      **パターンが識別子に寄りすぎていた**——`docs/architecture.md:82` は同じ事実を
      「show 時に bar_height（`font_size + bar_padding`・既定 43px）へリセットする」という
      **概念ラベル**で書いており、この 1 文は変更後に偽になる（plan-review A-2 が指摘・再照合済み）。
      **概念ラベル（`bar_height` / 「バー高」）で引き直した結果**、`docs/build-commands.md` は
      引き続き 0 件、`SPEC.md`（:208 / :213 / :470 / :476）は**すべて真のまま**、
      失効するのは `docs/architecture.md:82` と既存 ADR 2 本の描写のみと確定した
- [x] **R4 の共有 atomic 案（3b ⚠️3）の採否** — 却下。理由 3 点は `research.md` §8 却下 1

## 人間レビュー

- [x] 承認済み — 2026-08-24 / 問い: "1. **Phase 1（R1 を塞ぐこと）に着手してよいか** — 挙動不変の構造変更 / 2. **Phase 2 は `scripts/**` に触れるためセーフティネットの変更です**（`CLAUDE.md` 最重要ルール 2「合意してから」）/ 3. **Phase 4 は GitHub への外向き・不可逆な操作**（#878 へのコメント投稿）。投稿の可否と、**#878 をこの PR で閉じるか / 開けておくか**" / 回答: "承認する。Phase 4 のコメント投稿も可、#878 は閉じよう。"

### 裁定の結果

1. **Phase 1 に着手してよい** — R1 を塞ぐ（`ADR-show-path-derives-drawn-height` 却下 2 の反転）
2. **Phase 2（セーフティネットの変更）に合意あり** — trace 検出器の新設と
   `scripts/smoke-egui.ps1` への不在断言。故障注入での実測は計画どおり行う
3. **Phase 4 のコメント投稿は可。#878 は閉じる**
   - **閉じる手段は PR 本文の closing keyword である**（`gh issue close` を直接打たない）
     ——マージで閉じる issue を決めるのはPR 本文であり、機構の理由は
     `docs/adr/ADR-squash-merge-issue-autoclose.md`、手順は `/merge-pr` が正本
   - ゆえに**「#878 を閉じる」は作業項目に置かない**（コミット以降の手順であり、
     `/merge-pr` のマージ後 3 点検証がその実体である）。PR 本文へ `Closes #878` を書く

## セルフレビュー

- リスク: **高**
  - セーフティネットの新設（trace 検出器 + `scripts/smoke-egui.ps1`）
  - `AGENTS.md` 条件別チェックの該当: 「関数・型を新規定義／導入」（`/dry-check`）・
    「Tauri listener / フレーム内 live-read」（`/race-check`）・「導出の入力を変更」・
    「trace イベント名の変更」（smoke 前提）・「ガバナンス文書を変更」（`governance:check`）
- plan-review: 計画準拠レビュー 1 体（観点 A: 検出器の妥当性 / 観点 B: 偽になる散文の網羅）
- エージェント数: 2（3b の敵対枠 1 + plan-review 1）
- 要対処: **計 4 件**——3b から 1 件（`research.md` §8 採用 1）、plan-review から 2 件（A-1・A-2）、
  主エージェントの自己照合から 1 件（新規 ADR の要否。当初「不要」としたのを覆した）。
  **すべて計画へ反映済み**
- 未検証: 受け入れ条件 3（位置が変わらないこと）と Phase 2 の故障注入は**実装時の実測**である。
  計画段階では tao / dpi のソース読解までで接地しており、`plan.md` 上でこれ以上潰せない

## plan-review 結果

- リスク: **高**
- レビュー方式: 計画準拠レビュー 1 体（成果物 `workspace/plan-review-878-seam2.md`）
- エージェント数: 1

### 要対処（再照合済み）

- **A-1 検出器が偽陽性を出しうる残余** — 死角として宣言する（発火条件は絞らない）。
  **再照合の結果、指摘は成立するが到達条件はより狭い**——`win_w > 2 × 作業領域幅`。
  導出は「不変条件と異常系」節に記載。指摘は採り、機序は自分で導いた
- **A-2 `docs/architecture.md:82` が偽になる** — 変更ファイル一覧・Phase 3・更新要否表・
  未確定 6 へ反映。**再照合で成立を確認**（`sed -n '78,86p'` で現物を読んだ）。さらに概念ラベルで
  引き直したところ、`ADR-window-coordinator-split-rule` 決定 4 と
  `ADR-results-presentation-two-stage` 却下 6 の**描写も失効する**と判明し、新規 ADR の記載事項へ追加

### 軽微

- **`!any_down()` により show 直後の検証機会が落ちる回がある** — 死角として同じ場所へ宣言（反映済み）
- **config 変更が show と 1 フレーム目の間に挟まる残余** — `egui_main:height_mismatch` に既に
  存在する同種の残余であり、本計画が新規に持ち込むものではない。追加対処なし

### 未検証

- 故障注入の強さが「守りたい退行と同じか」は実装時にしか測れない（Phase 2 の作業項目）
- 非クライアント差分が show 直前と 1 フレーム目で同一であること。**tao 自身が
  `set_inner_size` でこの不変性に依存している**（読み取った差分を足して使う・`window.rs:285-297`）
  ため、破れれば tao 経由の全経路が同時に破れる。本計画固有のリスクではない

### 判断

- 実装着手: **人間の裁定待ち**（下の「人間レビュー」3 争点）

### 主エージェントの自己照合（Step 5a の 5 点）

1. **issue の全要件に作業項目が対応するか** — #878 に残る役目は 2 つ（読み戻しの現存・
   検出器の未決）。前者は Phase 1、後者は Phase 2 + Phase 4 (d) が受ける
2. **境界条件と検証** — `read_frame_geom` の `None`（異常系の表）・作業領域より幅広いバー矩形
   （`SPEC.md` §8.2 の宣言済み残余・変更しない）・scale 1.0/1.25/1.5（ユニットテスト）
3. **新しい状態・リソースの正常/失敗/破棄経路** — 新規の状態は持たない（`EguiShellState` に
   フィールドを足さない設計にした。検出器は「クランプが動いたか」だけを見る）
4. **より単純な既存パターンで置き換えられないか** — `read_bar_anchor`（「Win32 の読みはここ 1 回」）
   と `WorkArea::clamp`（純粋核・テスト 7 件）を再利用し、**新しい算術を書かない**
5. **壊してはならない不変条件に検知手段があるか** — 上の「不変条件と異常系」表のとおり。
   **今まで検知手段が無かった「show の位置決めの正しさ」に、Phase 2 が初めて手段を与える**
