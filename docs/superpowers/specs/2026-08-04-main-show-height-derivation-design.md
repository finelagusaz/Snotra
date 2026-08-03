# show 経路が「これから描く高さ」を導出する設計（#755 / #801）

対象 issue: #755（hide 時に status/toast が出ていると再表示後に窓が伸びず案内が切り取られる）・#801（updater トースト表示中の show で高さが縮んでから伸びる）。親は #878（窓の幾何が OS のウィンドウオブジェクトを経由して部品間を渡っている）。

前提となる調査と実測は #878 のコメントと両 issue 本文にある。要点だけ再掲する——**両症状は 1 回の show では排他であり、同じ数行から出ている**。2026-08-04 に HEAD `7562cef` の release ビルドで実測した（`SNOTRA_EGUI_FAKE_UPDATE=1`・toast のみ）: show #1 = 論理 86px（伸びる）/ show #2・#3 = 論理 43px（固着）。hidden 中に幅だけを変えると show #3 が 86px へ戻り、**抑制器がデルタガードであることと、固着中も toast がモデル上は存在していることが直接示された**。

## 1. この spec が答えている問い

show 経路は窓を畳んでから位置を決める。畳む先が「バー高」固定である一方、最初のフレームが描くのは「バー高 + status? + toast?」である。**この 2 つが食い違うことが両 issue の共通の源**である。

問いは「食い違いをどう無くすか」であり、**memo（`SearchWindowView` のサイズデルタガード）をどう直すか**ではない。memo は #801 を偶然抑えている抑制器であって、原因ではない。

## 2. 却下した案と、その根拠（否定の知識）

### 2.1 reset-on-show で main のサイズガードだけを初期値へ戻す（#755 本文の修正案）

**実測により、#801 を普遍化することが分かっている。** ガードを戻すことは「次フレームで補正を必ず撃たせる」ことであり、いま #801 を抑えているのはまさにその補正が握り潰されていることである。#755 は直るが、status / toast が出ている全ての show で 43px からの伸びが出る。

`bar_height` を memo へ代入する案（#755 本文のもう一方）も同じ理由で却下する。ガードが開く点は変わらない。

### 2.2 memo を触らず、show の畳む先だけを実高にする

食い違いが無くなるので両 issue は消え、memo が stale でも撃たれる値が同値になって無害化される（#877 が幅で採った形の一般化）。**しかし失敗が沈黙する。** 導出が将来ずれた場合（reset-on-show が `launching` / `notice` を消さなくなる等）、memo と一致した瞬間に補正が握り潰され、#755 がそのまま再発する。**沈黙する失敗を選ばない。**

### 2.3 継ぎ目 2（位置計算が OS からサイズを読み戻す）まで踏み込む

`position_on_target_monitor` がサイズを引数で受ければ、show が窓を物理的に畳む必要そのものが消える。ただし同じ読み戻しは `position_results_below_main` にもあり、そちらは**毎フレーム + `Moved` リスナー**の 2 経路から呼ばれてフレームに閉じない。#738 / #760 の射程であり、本 spec では触らない。

### 2.4 show 側の導出をインラインで書き、一致条件をコメントで記録する

変更面積は最小になるが、導出の写しが 2 つになる。**前提（reset-on-show が何を消すか）がコメントにしか無い形は、前提が変わったときに grep で見つからない。**

### 2.5 `MainHeightInputs` のような入力構造体を切る

最も強い形だが、面積が大きく `ADR-results-presentation-two-stage` の却下 1（`main_size` を導出に入れない）の境界に近づく。**述語 1 本の共有で同じ保証が得られる**ため採らない。

### 2.6 高さの断言を既定プロファイル（toast なし）の smoke へ足す

**捕まえるはずのバグを 1 件も捕まえない。** toast も status も無い状態では main は常にバー高であり、そこは一度も壊れていない。検出器は「toast あり」かつ「2 回目の show」でなければ両 issue の分岐を踏まない。

## 3. 決定

### 決定 1: `status_row_present` を `notify.rs` へ置き、show と毎フレームの両方がそれを呼ぶ

status 行の有無を返す述語を、`overlay_kind` の隣（`notify.rs`）に置く。3 源の優先順は `overlay_kind` が正本のままで、この述語は「行が 1 本出るか」だけを返す。

```rust
pub fn status_row_present(indexing: bool, results_view: bool, launching: bool, notice: bool) -> bool
```

**`layout.rs` へは置かない。** 同ファイルは `use std::time::Duration` 以外の依存を 1 つも持たない自己完結した純粋核であり、そこへ述語を置くと `overlay_kind` を呼ぶためにこのファイル初のモジュール間依存が生じる。述語の中身は「status 行が出るか」という overlay の意味論であって高さの算術ではない。

`main_window_height`（`layout.rs`）は変更しない（既存の純粋核テストに手を入れない）。共有するのはこの述語である。

### 決定 2: show 経路は畳む先を `main_window_height` の値にする

`show_egui_main` の `#[cfg(windows)]` ブロックで、バー高固定をやめて実高を導出する。**reset-on-show への依存は 3 つのリテラル引数として呼び出し点に現れる**:

```rust
let status = layout::status_row_present(
    read_indexing(app),
    /* results_view */ true,   // reset-on-show が空クエリ・tool/folder なしへ戻す
    /* launching    */ false,  // reset-on-show が消す
    /* notice       */ false,  // reset-on-show が消す
);
```

**コメントではなく引数の形で置くこと**が要点である。前提が変わったとき、`status_row_present` の呼び出し点を grep すれば必ずここに来る。

読み口は `read_metrics` と同型の薄いヘルパー 2 本（`AppState.indexing` と updater toast の有無）を `window_coordinator.rs` に足す。

**順序制約は不変である**——高さを決める → 位置を決める → show。位置クランプがサイズを OS から読み戻す以上、サイズは位置より前に確定していなければならない。変わるのは畳む先の値だけである。

### 決定 3: reset-on-show で main のサイズ memo も初期値へ戻す（fail-safe）

results 側と対称にする。決定 2 の導出が正しい限り、1 フレーム目は同値の `set_size` を 1 回余分に撃つだけで見た目は変わらない。導出がずれた場合は**その 1 フレームで実際に描く高さへ直る**——固着せず、スナップとして現れる。

**この決定は単独では #801 を悪化させる**（2.1）。決定 2 と束ねてはじめて無害になる。**2 つを別々のコミットに割ってはならない。**

### 決定 4: 検出器は「toast あり・2 回目の show」で置く

`scripts/smoke-egui.ps1` に 2 つ目のシナリオを足す。`SNOTRA_EGUI_FAKE_UPDATE` は起動時に読まれるため既存シナリオに相乗りできず、**起動が 1 回増える**（`smoke:egui` は必須検査なので実行時間が伸びる）。配管（プロファイル seed・VK 導出・trace 待ち）は共有する。

```
SNOTRA_EGUI_FAKE_UPDATE=1 で起動
  → show #1 → 高さ == バー高 + toast 高 を断言
  → Escape で hide
  → show #2 → 高さ == バー高 + toast 高 を断言      ← #755 を捕まえる
  → 一定時間サンプリングして min == max を断言       ← #801 を捕まえる
```

**高さは DWM の実表示矩形（`DWMWA_EXTENDED_FRAME_BOUNDS`）で読む。** `GetWindowRect` は不可視のリサイズ枠を含み、2 行の高さが 1 行の 2 倍にならない（実測: 118 / 64 に対し DWM は 110 / 56）。論理 px への換算は **config が幅を固定していることを較正点にする**（実測幅 / `window_width`）——DPI API を別に読まない。

## 4. 触らないもの

- 継ぎ目 2（`position_on_target_monitor` / `position_results_below_main` の OS 読み戻し）— #738 / #760
- `main_window_height` 自体と results 窓まわりの導出（`ADR-results-presentation-two-stage` 却下 1 の境界）
- 幅の経路（#877 で解決済み。show と毎フレームが同じ読み口を共有しており、本 spec が高さについて作る形と既に同型）
- `max_results = 0`（同 ADR 却下 7）

## 5. 検証

- **純粋核テスト**: `status_row_present` の真理値表。特に `indexing && !results_view → false`（tool / folder 段では indexing 案内が出ない）
- **カテゴリ D**: 決定 4 の断言と同じ手順を人間の目でも 1 度通す。**fix 前の実測値が取ってあるので before/after を直接比較できる**——show #2 が 43px から 86px へ変わり、どの show でもサンプリング区間で高さが動かないこと
- **カテゴリ A / C**: `docs/build-commands.md` のとおり。`show_egui_main` に触るのでカテゴリ C（`smoke:startup` / `smoke:egui`）が要る
- **`npm run governance:check`**: ADR と規範文書を触る

## 6. 同期する文書

挙動が変わるので `SPEC.md` の同期は**仕様変更として**行う（`AGENTS.md`「開発ワークフロー」1）。

| 文書 | 何が偽になるか |
|---|---|
| `SPEC.md` §20.3（トースト UI） | 「show 時はバー高への collapse 後に toast 分へ拡張する（1 フレームの高さスナップを受容）」——受容していた挙動そのものが無くなる。#877 が足した幅側の記述（§4.7 結果表示制御）と**同じ 1 つの規則**へ揃える。**#902 の降格後の文体に従い、実装シンボル名を書かない観測文で書く** |
| `src-tauri/CLAUDE.md`「モジュール構成」 | 「main のサイズは 2 か所に分かれる——show 経路のバー高 collapse はここ」。2 か所である事実は残るが、**同じ純粋核を共有する 2 つの呼び出し点**へ性格が変わる |
| `src-tauri/CLAUDE.md`「実装パターン」 | 「show の操作順序制約」。順序そのものは不変で、畳む先だけが変わる |
| `view.rs` のデルタガード近傍のコメント | 「意図的な 2 導出」の説明 |

**`ADR-results-presentation-two-stage` は編集しない**（`ADR-adr-frozen-history`: ADR は凍結された歴史）。却下 6 の反転は新しい ADR（`docs/adr/ADR-show-path-derives-drawn-height.md`）に記録する。そこに書く否定の知識は本 spec の §2 と、**却下 6 が依存していた前提**である——旧 ADR は「毎フレームの導出をそのまま使えば結果件数で伸びた高さでクランプする」と読んで統合を禁じたが、実際に共有するのは `main_window_height`（バー + status + toast のみで結果件数に伸びない）であり、**実表示の高さでクランプするほうが正しい**。同じ ADR の却下 3 が残した教訓（禁止を恒久文書へ書く前に、その禁止が依存している前提を明示できるか確かめよ）の適用例である。

## 7. 副次的に閉じるもの

- #878 の**継ぎ目 3**（「これから描く高さ」は show 前に導出できるのにしていない）が閉じる
- #878 の**継ぎ目 1**（記憶の持ち主が 1 人・書き手が 2 人）は、決定 3 で results 側と対称になる。ただし**構造として閉じるのではなく、失敗が固着しない形へ変わる**だけである
- 位置クランプが実表示の高さで効くようになるため、**#738 の前提が改善する**（解決はしない——伸びた後の再クランプが無いことは変わらない）

## 8. 残余

- **show と 1 フレーム目のあいだで導出の入力が変わりうる。** index 構築の完了・updater toast の到着がその窓に入ると高さが食い違う。fail-safe ゆえ固着はせず、1 フレームのスナップとして現れる。**この窓は塞がない**——塞ぐには show とフレームを同一の時刻へ閉じる必要があり、それは継ぎ目 2 の解体を要する
- **show 側の 3 リテラルが依存する前提（reset-on-show が何を消すか）はユニットテストで固定できない。** `consume_reset_pending` は `app_handle` を握る driver 側にある。前提が壊れた場合の症状はスナップの再来であり、気づき方は決定 4 の検出器とカテゴリ D である
- **決定 4 の検出器は「toast あり」の 1 形だけを見る。** status 行（indexing 案内）側の同型は見ない——`AppState.indexing` を smoke から制御する手段が無いためである
