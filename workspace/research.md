# 調査: #878 継ぎ目 2 と 4 は今も残っているか / あるべき姿は何か

対象 issue: #878「検討: 窓の幾何が OS のウィンドウオブジェクトを経由して部品間を渡っている」
基点: `main` = `6638f7f9`（2026-08-24）。以下の行番号・引用はすべてこの版で実測した。

## 0. issue の要約と、この調査が答える問い

#878 は症状ではなく**形**の報告である。「窓の幾何（サイズ・位置）が、コードではなく OS の
ウィンドウオブジェクトを経由して部品間を渡っている」という構造を 4 つの継ぎ目に分解し、
#755 / #801 / #738 / #760 をその帰結として集計した。症状 4 件はすべて決着済みである
（`gh issue view` 実測: #755 CLOSED / #801 CLOSED / #908 CLOSED）。

issue 本文が残る役目として名指すのは 2 つ:

1. 読み戻し 3 箇所のうち 2 箇所が現存する（＝継ぎ目 2 が手つかず）
2. 「検出器を持たないことをどう扱うか」が未決

そして 2026-08-04 のコメントが「残るのは継ぎ目 2 と 4 である」と結んでいる。

**ユーザーの問い**: 継ぎ目 2 と 4 は今も残っているのか。残っているなら**あるべき姿**は何か。

## 1. 継ぎ目 4 — **解消済み**（2026-08-04 コメントの「手つかず」は陳腐化している）

継ぎ目 4 の定義は「『作業領域の中にいる』不変条件を show が立て、フレームループが壊す」で、
根拠として issue 本文は「`view.rs` に `set_position` は 1 つも無い・grep 実測」を挙げていた。

**この根拠は失効している。**

| 観測 | 実測 |
|---|---|
| `view.rs:1280` | `clamp_main_into_work_area(&app, metrics.bar_height)` を呼ぶ。条件は `!ui.input(\|i\| i.pointer.any_down())`——**ポインタ非押下のフレームすべて**であり、ドラッグ解放の 1 回だけではない（level-triggered） |
| `window_coordinator.rs:670-691` | その実体。`read_bar_anchor` で材料を 1 回読み、`WorkArea::clamp` で算術し、**位置が変わるときだけ** `set_position` |
| `window_coordinator.rs:687` | `view.rs` から到達する `set_position`。すなわち「フレームループは位置を触らない」は現在偽である |

さらに**不変条件そのものが再交渉されている**。

- 旧: 窓**全体**が作業領域内 → 新: **バー矩形**が作業領域内（材料はバー高であって実高ではない・`layout::bar_rect_height_phys` の doc が正本）
- `SPEC.md:213`（§4.7）「バーの位置はユーザーが決める。行の出没では動かさない」——人間裁定
- `SPEC.md` §8.2「表示中の作業領域への復帰（#738）」が、復帰する条件・**復帰しない 3 条件**（フレームが動かない間の作業領域変化／バーが作業領域より幅広い場合／取得失敗）まで明文化している
- results のはみ出しは #917 で床ごと撤去され、§4.5 が受容と明記（#760 は症状消滅ではなく人間裁定）

新しい不変条件のもとで「書き手 × 破壊者」を数え上げると、**破壊者が残っていない**。

| 幾何を動かしうる経路 | バー矩形を作業領域外へ出しうるか |
|---|---|
| `view.rs:1256` の成長 `set_size` | 出さない。`set_size` は左上を動かさず、バー矩形は窓の上端側にあるため |
| ネイティブドラッグ | 出す。→ 非押下フレームのクランプが戻す（`SPEC.md` §8.2 が「離したら戻る」と宣言） |
| `show_egui_main` の配置 | 出さない（`position_on_target_monitor` が `WorkArea::clamp`） |
| 幅設定の変更 | 出しうる。→ 同一フレームのクランプが戻す（§8.2 に明記） |
| 作業領域**側**が動く（タスクバー移動・解像度/DPI 変更・モニター取り外し） | 可視中は次フレームのクランプが戻す（クランプは毎フレーム作業領域を読み直す）。hidden 中は次の show が戻す。**§8.2 が「フレームが動かない間の変化」として受容を明記** |

**判定: 継ぎ目 4 は閉じている。** 残余は 3 件あるが、いずれも `SPEC.md` §8.2 と
`ADR-main-window-clamp-on-pointer-release` が**宣言済みの死角**である（`any_down()` の固着も
同 ADR が受容残余として記録）。#908（マルチモニター未検証 2 項目）は CLOSED。

→ **本調査の成果物には「継ぎ目 4 は解消済み」の記録が要る**（issue 本文と 2026-08-04 コメントが
今も「手つかず」と読める状態にあり、次の読者を誤らせる）。

## 2. 継ぎ目 2 — **現存する。ただし形が変わり、修正が当時より安くなっている**

### 2.1 OS 幾何の読み戻し全 7 箇所（`src-tauri/` + `snotra-egui-runtime/` 全数）

```
grep -rn "outer_size()\|inner_size()\|outer_position()\|scale_factor()" src-tauri/src/ snotra-egui-runtime/src/
```

| # | 場所 | 読む量 | 読む理由 |
|---|---|---|---|
| R1 | `window_coordinator.rs:238` `position_on_target_monitor` | `outer_size()`（幅・高さ） | クランプ／センタリングの材料 |
| R2 | `window_coordinator.rs:562` `read_placement_relative`（**非** Windows 分岐） | `outer_position()` | 保存位置 |
| R3 | `window_coordinator.rs:621-624` `read_bar_anchor` | `outer_position()` / `outer_size()` / `inner_size()` / `scale_factor()` | クランプと hide 保存の共通材料 |
| R4 | `window_coordinator.rs:720-722` `position_results_below_main` | `outer_position()` / `outer_size()` / `scale_factor()` | results の追従先 |
| R5 | `results_window.rs:247` | `scale_factor()` | 論理→物理 |
| R6 | `snotra-egui-runtime/src/runtime.rs:382` | `scale_factor()` | 描画 |
| R7 | `snotra-egui-runtime/src/runtime.rs:432-433` | `inner_size()` / `scale_factor()` | 描画 |

main のサイズ／位置の**書き手**（全数）:

| 場所 | 何を書くか |
|---|---|
| `window_coordinator.rs:338` | `set_size(width, bar_height)`（**1 手目**） |
| `window_coordinator.rs:343` | `set_size(width, height)`（実高・**2 手目**） |
| `window_coordinator.rs:257` | `set_position`（show の配置） |
| `window_coordinator.rs:687` | `set_position`（可視中のクランプ） |
| `view.rs:1256` | `set_size(width, height)`（毎フレーム・デルタガード内） |

（2026-08-03 コメントの全数調査は #904 前の版のもので、行番号も件数も陳腐化している。上表が
`6638f7f9` での再実測である。）

### 2.2 R1 は「OS を引数渡しの経路として使う」純粋形になった

`show_egui_main` は #904 で 2 手に分かれた（`window_coordinator.rs:337-344`）。

```rust
// 1 手目: バー高だけで position のクランプ材料を確定する。
let _ = window.set_size(tauri::LogicalSize::new(width, m.bar_height));
position_on_target_monitor(app, &window);
// 2 手目: 実高へ書き直す
let _ = window.set_size(tauri::LogicalSize::new(width, height));
```

**1 手目の `set_size` は、位置決めへ寸法を伝える以外の目的を持たない。** 呼び出し点のコメントが
そう明言しており（「1 手目: バー高だけで position のクランプ材料を確定する」）、
`position_on_target_monitor` の呼び出し元は `show_egui_main` ただ 1 つである（doc に明記・grep 一致）。

すなわち R1 は、issue 本文が名指した形——**「値を渡す手段が OS の窓しか無いことの帰結」**——が
**純化して残っている**。#904 は collapse 先の値を変えただけで、この経路には触れていない。

**当時より安い理由**: #801 の時点では collapse は「高さを伝える」ことと「畳んで見せる」ことが
混ざっていたが、いまは 1 手目を消しても**視覚的な影響がゼロ**である。

**理由は「フレームが 1 枚も描かれないから」ではない**（3b で訂正・§8 の 採用 1）。
中間サイズの消費者は**実在する**——`mod.rs:377-383` が main の `Moved` を購読しており、
`position_on_target_monitor` の `set_position`（`:257`）がそれを起こしうる。購読先は
`position_results_below_main` で、そこは `main.outer_size()` を読む。**フレームの外で走る経路
である。**

正しい理由は **results がまだ hidden であること**である。`ResultsWindow::show` を呼ぶのは
`drive_results_window` ただ 1 つで（`:882`）、その**直前の `:870` が
`position_results_below_main` を呼ぶ**。すなわち results は「可視になる前に必ず再配置される」。
1 手目の有無で変わるのは、hidden な results が一時的に持つ y 座標だけである。

### 2.3 R4 は「継ぎ目の形に見えるが、塞ぐと継ぎ目 1 を再生産する」

`position_results_below_main` は `main.outer_position()`（ユーザーが動かした位置＝コードが
持っていない）と `main.outer_size().height`（コードが同フレームに書いた値）を同時に読む。
高さだけを見れば継ぎ目の形である。しかし:

- 呼び出し元は **2 つ**——`view.rs` の毎フレーム driver と、**`Moved` リスナー**（`mod.rs:375` 近傍で登録）。
  後者はネイティブ移動ループ中に走り、**egui フレームの文脈を持たない**（doc に明記）
- ゆえに「書き手が高さを渡す」形にするには、フレームを跨ぐ記憶が要る。それは
  **継ぎ目 1（記憶の持ち主が 1 人・書き手が 2 人）そのもの**である
- doc（`:701-704`）が既に「Win32 を読んでそれを適用する場所はここだけ」と 1 点化を宣言しており、
  これは**意図的な設計**である（ルート `CLAUDE.md`「意図的なリファクタリングの結果を元に戻さない」の射程）

### 2.4 R3 は OS しか知らない量を読んでいる

`read_bar_anchor` の `outer.height - inner.height` は**非クライアント領域**（`decorations: false` でも
DWM の影が乗り、DPI 125% で 10 物理 px 実測・#738 のカテゴリ D）。コードは持っていない。
`scale_factor()` も同様。`outer_position()` はユーザー入力の取り込み。

## 3. あるべき姿

### 3.1 裁定則（1 文）

> **窓の矩形から読んでよいのは、コードが持っていない量だけである**——ユーザーが動かした位置、
> 非クライアント差分、scale。**コード自身が直前に書いた content 寸法を、渡す手段が無いという
> 理由で読み戻してはならない。渡す手段が無いなら、渡す手段を作る。**

例外は 1 つだけ許す:

> **書き手のフレーム文脈が読み手に届かない経路（`Moved` リスナー）が呼び出し元に含まれるとき、
> 読み戻しは正当である**——渡す形にすると、フレームを跨ぐ記憶（継ぎ目 1）を再生産するため。

### 3.2 全 7 箇所をこの則で分類した結果

| # | 判定 | 理由 |
|---|---|---|
| R1 | **違反**（唯一） | 1 手目の `set_size` が「値を渡すため」だけに存在する |
| R2 | 適合 | ユーザー入力の取り込み（非 Windows 分岐） |
| R3 | 適合 | 非クライアント差分・scale は OS のみが知る。位置はユーザー入力 |
| R4 | 適合（例外条項） | 呼び出し元に `Moved` リスナーを含む |
| R5・R6・R7 | 適合 | scale / 描画サイズは OS のみが知る |

**差分は R1 の 1 件である。** これが「あるべき姿」と現状の距離のすべてであり、issue が
「窓の矩形が事実上の共有可変グローバル」と呼んだ構造のうち、残っているのはここだけである。

### 3.3 R1 を塞ぐと何が良くなるか（副次効果 3 件）

1. **`set_size` の書き手が 3 → 2 に減る**（`window_coordinator.rs:338` が消える）
2. **`layout.rs:307-315` の doc が名指す「2 経路の偶然の一致」が、1 経路になる。**
   現在 show 経路の物理高は「tao/dpi が `LogicalSize` を変換した結果を `outer_size()` で読む」もので、
   フレーム経路の `bar_rect_height_phys`（`.round()`）と**別経路で偶然一致**している。doc は
   「一致を固定するテストは書けない・上流が銀行家丸めへ変われば 1px 食い違う」と明記している。
   show 側も `bar_rect_height_phys` を通せば導出は 1 つになる。
   **上流の丸め規則への依存そのものは消えない**——窓の実矩形を決めるのは最後まで tao だからである。
   消えるのは「2 つの導出が一致し続けなければならない」という条件のほうで、依存は
   `layout::bar_rect_height_phys` の doc 1 か所に集約される（doc はそのように書き替える）
3. **#877 の治療法（2 つの書き手を同じ SSOT から導かせる）が位置についても成立する**

### 3.4 検出器をどう扱うか（issue の未決事項 2）

**この修正の退行は外から見えない。** 2026-08-04 コメントが観測した「安全機構が外部の検出器を
無力化する」形がそのまま当てはまる——show の位置決めが退行してバー矩形が作業領域外へ出ても、
**可視になった次のフレームで `clamp_main_into_work_area` が戻す**（`view.rs:1280`・毎フレーム）。
外から DWM で測る限り、正しい位置しか観測できない。

ゆえに **#904 と同型の in-process 信号が要る**: reset-on-show の消費フレームで、
`clamp_main_into_work_area` が**実際に位置を動かしたか**を trace する
（`egui_main:height_mismatch` の位置版）。smoke はその**不在**を断言する
（`scripts/smoke-egui.ps1:548-670` の `Test-SnotraNoHeightMismatch` が雛形）。

**この 1 件をもって issue の未決事項 2 に答える**: 検出器は「継ぎ目を塞ぐ変更ごとに、
その退行が外から見えるかを問い、見えないなら in-process 信号を同じ差分で置く」。
一般の規範として新設するのではなく、**#904 が既に踏んだ形の 2 例目**として記録する。

## 4. 関連ファイル・シンボル（実在を grep で確認済み）

| パス | シンボル / 行 |
|---|---|
| `src-tauri/src/egui_shell/window_coordinator.rs` | `position_on_target_monitor`（:214-260）/ `show_egui_main`（:271-）/ `read_bar_anchor`（:619-644）/ `clamp_main_into_work_area`（:670-694）/ `position_results_below_main`（:709-732）/ `read_placement_relative`（:546-565） |
| `src-tauri/src/egui_shell/layout.rs` | `bar_rect_height_phys`（:316）/ `bar_rect_center`（:330）/ `main_window_height` / `size_delta_exceeds` / `results_top_y` / `MainScale` |
| `src-tauri/src/egui_shell/view.rs` | :1256（成長 `set_size`）/ :1280（クランプ呼び出し）/ :1230-1241（`height_mismatch` 検出器） |
| `src-tauri/src/monitor.rs` | `WorkArea::clamp` / `WorkArea::center` / `point_monitor_work_area` / `cursor_monitor_work_area` / `primary_monitor_work_area` |
| `src-tauri/src/egui_shell/mod.rs` | `EguiShellState`（`show_read_indexing` / `show_read_toast` / `show_applied_height_bits`） |
| `scripts/smoke-egui.ps1` | `Test-SnotraNoHeightMismatch`（:556-567）/ 断言ブロック（:660-670） |
| `SPEC.md` | §4.7（:213）/ §8.2（:456-483） |
| `docs/adr/` | `ADR-main-window-clamp-on-pointer-release.md` / `ADR-show-path-derives-drawn-height.md` / `ADR-results-presentation-two-stage.md` |
| `src-tauri/CLAUDE.md` | 「モジュール構成」の `window_coordinator.rs` の項（:51） |

## 5. 再利用できる既存パターン

- **#877 / #904 の治療法**: ガードを直すのではなく、2 つの書き手を同じ導出へ通す
- **`read_bar_anchor`**: 「Win32 の読みはここ 1 回」＋「クランプと hide 保存が同じ関数を通ることが
  一致の担保」という形。R1 の材料もここへ寄せられる可能性がある
- **in-process 不変条件検出器**（#904）: show が読んだ生の入力を `EguiShellState` の atomic に残し、
  reset-on-show の消費フレームが突き合わせ、**食い違ったときだけ** trace。smoke は不在を断言
- **`WorkArea::clamp` / `center`**: 純粋核・ユニットテスト 7 件が算術を固定済み。**新しい算術を書かない**

## 6. 技術的制約

- **hidden 中は `update()` が走らない**（`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）。
  show 経路が第 2 の実装であることの根本原因であり、これは消せない
- **`position_on_target_monitor` は `#[cfg(windows)]`**。非 Windows ビルドではサイズ／位置を一切
  設定しない（`applied_height` が常に `None`）。変更は cfg 境界を跨ぐ
- **`WorkArea::clamp` は物理 outer サイズを要求する**（`win_h` の doc）。論理値を渡す形にするなら、
  非クライアント差分と scale の合成が呼び出し側に要る——`read_bar_anchor` が既にやっている合成
- **show の順序制約は不変**（「高さ決定 → 位置 → show」）。旧 WebView2 経路から引き継いだもので、
  #904 のコメントが「修正後もこの順序は不変」と明記
- **`_el: &EventLoopProof`** はイベントループスレッドの証人。シグネチャから外してはならない

## 7. 未解決の疑問 — 3 件は一次資料で決着した

### Q1（決着）hidden な窓に対する `outer_size()` / `inner_size()` / `scale_factor()`

**有効である。** 二重の証拠がある。

1. **現行コードが既にそれをしている。** `position_on_target_monitor` は show の途中——**窓が
   hidden のまま**——で `outer_size()` を読み、その値で位置を決めている（`:238`）。これが
   正しい位置を出していること自体が、hidden 窓での読みが成立する実証である
2. **tao 0.35.3 の実装**（`platform_impl/windows/window.rs:255-270` + `util.rs:468-492`）:
   `inner_size()` = `GetClientRect`、`outer_size()` = `GetWindowRect`。どちらも可視性に依存しない

新規に増える読みは `inner_size()` の 1 つだけであり、同じ窓・同じ状態・同じ API 族である。

### Q2（決着）1 手目を消して位置が 1px でも変わるか → **変わらない**

tao 0.35.3 の `set_inner_size`（`window.rs:272-313`）を読んだ。undecorated + shadow の窓では

```rust
desired = size.to_physical::<i32>(scale)            // dpi::Pixel::from_f64 = f64::round
        + (window_rect - client_rect)               // = outer - inner
```

を `set_inner_size_physical` へ渡し、`adjust_window_rect` は装飾なしのとき境界スタイルを剥がして
恒等になる。すなわち

> **`set_size(logical)` の後の `outer_size()` = `round(logical × scale)` + (`outer` − `inner`)**

これは `layout::bar_rect_height_phys(logical, scale)`（`.round()`）＋ `read_bar_anchor` が既に
行っている非クライアント合成と**同一の式**である。`dpi 0.1.2` の `round()` は `f64::round`
（`lib.rs:128-135`）で、`bar_rect_height_phys` の `.round()` と同じ規則。

**副産物**: `set_inner_size_physical` は `SWP_NOMOVE` を立てている（`util.rs:114`）。
**成長 `set_size` が左上を動かさない**という §1 の破壊者表の前提が、これで一次資料に接地した。

### Q3（決着）2 手のあいだの中間サイズを観測する消費者は居るか → **居る。だが無害**

`on_window_event` の登録は `mod.rs:378` の 1 つだけ（`Moved` → `position_results_below_main`）。
`Resized` は `snotra-egui-runtime/src/input.rs:213` が握り潰し、`runtime.rs:255-266` の
`Moved` / `ScaleFactorChanged` はリフレッシュレート再取得のみで幾何を読まない。

唯一の消費者 `position_results_below_main` が読む値は、1 手目の有無で変わる（バー高 → hide 時の
残存高）。しかし **results は hidden であり、可視化の直前（`:870`）に必ず再配置される**ため、
どちらの値でも画面に出ない。

### Q4（計画で決める）クランプ発火 trace の射程

「reset-on-show の消費フレームに限る」を採る。可視中ずっと出すとドラッグ解放のたびに出る
（正常動作）ので、smoke の**不在**断言が成立しない。`egui_main:height_mismatch` の
`was_reset_frame` 連言（`view.rs:1216`）と同じ形。

### Q5（人間裁定）#878 をこの作業で閉じるか

→ `plan.md`「人間レビュー」の争点として載せる。

## 8. 敵対的調査（3b）の反映

出力: `workspace/adversarial-878.txt`（sonnet 1 体・読み取りのみ）。

### 採用 1 — 「1 手目撤去が無害な理由」が誤っていた（**壊せた項目**）

指摘: 「2 手の間にフレームが 1 枚も描かれないから安全」は、`Moved` リスナー経由の
`position_results_below_main` という**フレーム外の消費者**を見落としている。

**採用。§2.2 を書き換えた。** ただし**機序は自分で裁定した**（`CLAUDE.md`「採るのは所見であって
機序ではない」）——レビュアは「results が hidden だから安全」で締めたが、それだけでは
「hidden のうちに可視化されたら？」が残る。実際の担保は**順序**である:
`drive_results_window` は `position_results_below_main`（`:870`）を `results.show()`（`:882`）の
**前**に呼ぶ。この順序が担保であり、結論は生き残る。

### 却下 1 — ⚠️3「R4 は共有 atomic で塞げるのでは」

指摘: `show_applied_height_bits` と同じ形で「main の直近の書き込み高さ（物理 outer）」を
atomic に持てば、`position_results_below_main` は読み戻しを止められるのでは。

**却下する。理由は 3 つある。**

1. **OS の読みは消えない。** 同関数は `main.outer_position()`（ユーザーが動かした位置）を
   どのみち読む。消えるのは 1 タプルのうち 1 要素だけで、Win32 呼び出しの回数は変わらない
2. **物理 outer 高を atomic へ入れるには、書き込み点で非クライアント差分を読む必要がある**
   ——読み戻しが `position_results_below_main` から `set_size` の隣へ**移動するだけ**である
3. **書き手が 2 人（show と毎フレーム）の memo をフレーム跨ぎで作る形は、継ぎ目 1 そのもの**
   である。#904 はそれを fail-safe 化して収めたが、results 側には補正フレームの相当物が無い

→ この却下理由は `position_results_below_main` の doc へ書く（否定の知識の置き場。ADR は
立てない——読者接点が当該関数の doc しか無い局所的な決定であるため）。

### 射程外 1 — ⚠️4「キーボード移動（Alt+Space → M）とクランプの相互作用」

指摘は妥当だが**本作業の射程外**である。本作業は show の位置**導出**を変えるだけで、
クランプの**発火条件**（`!any_down()`）には触れない。ゆえにこの論点は #909 以降ずっと在り、
本作業で新たに生じも悪化もしない。→ #878 へのコメントで観測として残す。

### 未採用（確認のみ）— ⚠️1「`Moved` の配送タイミングが同期か非同期か」

**どちらでも結論が変わらない**（採用 1 の順序担保がタイミングに依存しないため）ので、
実測を要求しない。`SWP_ASYNCWINDOWPOS` は呼び出しスレッドと窓所有スレッドが異なるときだけ
非同期化する指定であり、`show_egui_main` は `EventLoopProof` により所有スレッド上でしか
呼べない。

### 壊せなかった項目（レビュア宣言・こちらで再確認済み）

継ぎ目 4 の閉包（`view.rs` の `update()` に早期 `return` 0 件を grep 実測）／#908 の
クローズ理由（`COMPLETED`・実機確認）／R1 唯一性と 7 箇所の列挙（同一 grep + `GetWindowRect` /
`DwmGetWindowAttribute` / `inner_position` 等の拡張パターンで 0 件）／R2・R5〜R7 の適合判定／
**既存 smoke に main の X/Y を断言する箇所が 1 つも無いこと**（§3.4 の前提）／`config.toml`
既定値との整合（`follow_cursor_monitor=true` / `window_width=600` / `window_gap=4` /
`visible_rows=8`）／非 Windows 分岐でのスコープ外性。
