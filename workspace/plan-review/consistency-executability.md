# 実行可能性レビュー — plan.md（#749 段1 WindowCoordinator）

レンズ: 上から順に実行したとき指示が一意に定まるか。設計の当否は扱わない。

検証方法: `plan.md` が参照する全ファイル（`src-tauri/src/egui_shell/{mod,view,layout,visual,results_window}.rs`・`main.rs`・`SPEC.md`・`docs/architecture.md`・`src-tauri/CLAUDE.md`・`docs/build-commands.md`・`package.json`・`scripts/{governance-check,smoke-egui}.*`）を実読し、plan.md が挙げる行番号・関数名・grep 件数・テスト件数を現物と突き合わせた。`cargo test -p snotra` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo doc --workspace --no-deps --document-private-items` / `npm run governance:check` を実行してベースラインを実測した。ブランチは `chore/window-coordinator`（現在 `ed6d68a`、`a98312c` 基点から plan.md 更新のみでソース差分ゼロを確認済み）。dead_code の挙動は本リポジトリ外の使い捨て scratch crate で実測した。

---

## 手が止まる（要修正）

### 1. Phase 3 の mod.rs 再エクスポートが Phase 4 の成果に依存している（ビルドが壊れる）

Phase 3 は次のコードを提示する（plan.md 130-141行）:

```rust
mod window_coordinator;
pub(crate) use window_coordinator::{
    drive_results_window, hide_egui_main, position_results_below_main, register_hide_listener,
    results_available_height, save_placement_relative, show_egui_main, wake_main, wake_results,
    DriveResultsInputs,
};
```

しかし Phase 3 が移設すると明言する関数は 9 個（plan.md 106行）:

> `show_egui_main` / `hide_egui_main` / `save_placement_relative` / `register_hide_listener` / `wake_main` / `wake_results` / `position_results_below_main` / `results_available_height` / **`position_on_target_monitor`**（`main.rs:150-193`）

この列挙に `drive_results_window` と `DriveResultsInputs` は含まれない——両者は Phase 4 の見出し「`drive_results_window` を view.rs から移す」で初めて `window_coordinator.rs` に追加される（plan.md 153-183行）。

つまり Phase 3 を字面通り単独で実行すると、`window_coordinator.rs` にまだ存在しない `drive_results_window` / `DriveResultsInputs` を `pub(crate) use` する行がそのまま `unresolved import` でビルドを壊す。Phase 1→Phase 2 の間には（分かりにくい位置ながら）「呼び出し点は2つあり、どちらも同じコミットで移行する」という明示の警告があるが（55行）、Phase 3→Phase 4 の間には同種の警告が一切無い。

`AGENTS.md` の「各 Phase の検証 green 後にコミット」という長時間委譲の作法（ルート `CLAUDE.md`「サブエージェント委譲と worktree」）を字面通り適用すると、Phase 3 の完了時点で `cargo check` が失敗し、実装者はここで手が止まる。Phase 3 と Phase 4 を 1 コミットへ統合すべきだと判断するための根拠が本文に無い。

### 2. Phase 1 の完成に Phase 2 の成果（の一部）が要ることが、Phase 見出しの粒度と噛み合っていない

Phase 1（層 = layout.rs）は次を追加する:

```rust
pub fn size_delta_exceeds(prev: (f64, f64), next: (f64, f64)) -> bool { ... }
```

実測: この関数を追加しても本番コードから 1 度も呼ばなければ、`cargo clippy --all-targets -- -D warnings` は non-test ビルドで `dead_code`（`-D dead-code` implied by `-D warnings`）で失敗する（`--all-targets` は test cfg 有無の両方をビルドするため、`#[cfg(test)] mod tests` 内の呼び出しだけでは non-test ビルドを救えない。使い捨て crate で実測・下記コマンド）:

```
$ cargo clippy --all-targets -- -D warnings
error: function `size_delta_exceeds` is never used
```

plan.md 自身もこれを認識しており（50行）:

> **呼び出し点は 2 つあり、どちらも同じコミットで移行する**（`-D warnings` 下で未使用の新 API は `dead_code` で落ち、比較式を手書きで残すと導出が複数になる）:
> 1. Phase 2 の `ResultsWindow::set_size`（results 窓）
> 2. **`view.rs:1830` の main 窓デルタガード**（`/dry-check` で発見）

これは実質「Phase 1 の函数追加は、Phase 2（ResultsWindow の自己ガード化）と view.rs:1830 の書き換え（どちらの Phase 見出しにも明記されない孤立指示）を含めて初めて 1 コミットとして成立する」という意味だが、Phase 1 の見出しは「純粋述語の追加（`layout.rs`）」としか書かれておらず、実装者が Phase 1 の完了条件を「layout.rs にテスト付き関数を足すこと」と字面通り読むと dead_code で止まる。view.rs:1830 の書き換えは変更ファイル一覧表（16行）と `/dry-check` セクション（325行）にしか現れず、どの Phase の作業として実施するかが本文中に明記されていない。

---

## つまずきうる（要確認）

- **関数の移設先内訳が自己矛盾している**: 冒頭の変更ファイル一覧（13行）は「mod.rs から 9 関数 + main.rs から 1 関数 + view.rs から 2 関数」（合計 12）と書くが、Phase 3 の実列挙（106行）は mod.rs 由来 8 個 + main.rs 由来 1 個 = 9 個であり、Phase 4 の 2 個（`drive_results_window` / `max_results`）を足すと実際の総数は 11 である。「9」という数字が「mod.rs だけの内訳」と「Phase 3 全体の内訳（re-export 個数と偶然一致）」で混同されている。実装対象の関数リスト自体は 106行に明示されており実装は迷わないが、件数の見出しを信じて「mod.rs から 9 個探す」と数えると 1 個足りず戸惑う。

- **「未使用になる use はコンパイラが検出する」の対象が実在しない**: 変更ファイル一覧の main.rs 行（15行）と Phase 3 の付け替え節（150-151行付近、"main.rs 側で未使用になる use（monitor / window_data）はコンパイラが検出する"）は、main.rs にトップレベルの `use monitor;` / `use snotra_core::window_data;` があるかのように書くが、実測では:
  - `monitor` は `mod monitor;`（宣言。main.rs:17）であり `use` ではない。かつ `crate::monitor::{cursor,primary}_monitor_work_area` は `position_on_target_monitor` 専用ではあるが、モジュール自体（`monitor.rs`）は `egui_shell/mod.rs` からも `crate::monitor::window_monitor_work_area` で使われ続けるため `mod monitor;` 自体は消えない
  - `snotra_core::window_data` の `use` は `position_on_target_monitor` 関数本体内のローカル `use`（main.rs:156）で、関数ごと移設されるため main.rs にも window_coordinator.rs にも「取り残されて unused になる use」は生じない
  - main.rs トップレベルの `use tauri::{AppHandle, ...}` は `AppHandle` が他の複数関数（`setup_platform_thread` 等）で使われ続けるため無関係
  実害は無い（何も削除する必要が無いだけ）が、実装者が「clippy が何か指摘するはず」と探して見つからず時間を使う可能性がある。

- **governance:check の前提「母集団は追跡ファイル」が実装と矛盾する**: plan.md 226行「`npm run governance:check`（新規ファイル追加＝索引更新漏れが #629/#630 で 2 回再発。G1 がモジュール索引を検査するが、**母集団は追跡ファイルゆえ `git add` の後に実行する**）」を確認するため `scripts/governance-check.mjs` の `makeSnapshot()` を読んだところ、`git ls-files` 等は使わず `fs.readdirSync` で作業ツリーを直接歩く実装だった（スクリプト冒頭のコメント自体が「列挙は fs 自身に問う（`git ls-files` の pathspec `**` 意味論の罠を避ける）」と明言している）。つまり **untracked な新規ファイルも `git add` 前から検出される**——「追跡ファイルが母集団」という前提は誤りである。`git add` の後に実行しても副作用は無い（誤りが手順を壊すことはない）ため実行そのものは安全だが、書かれている理由は事実と異なる。

- **`src-tauri/CLAUDE.md`「モジュール構成」の編集単位が「5 行」ではない**: 変更ファイル一覧（20行）は「「モジュール構成」の 5 行（新規 `window_coordinator.rs` / `mod.rs` / `view.rs` / `results_window.rs` / `layout.rs`）」と書くが、実物の `egui_shell/` 項目は 1 個の長い箇条書き項目内に全サブモジュールの責務がカンマ区切りで埋め込まれた**単一の段落**であり、5 本の独立した行ではない。Phase 5 本文（219行）は正しく「段落」と呼んでおり自己修正できているが、冒頭サマリ表の「5 行」だけを読んで実装に入ると、5 本の行を探して見つからず戸惑う。

- **`docs/architecture.md:83` / `:172` の具体的な書き換え内容が指定されていない**: Phase 5 は「`docs/architecture.md:83`（駆動主体）/ `:172`（シーケンス図の宛先）を更新」（225行）としか書かない。実物を確認したところ:
  - 83行「`results` の位置・可視性は `main` の毎フレーム更新（`drive_results_window`）が駆動する」——関数名も呼び出し元（main の update()）も変わらないため、**そもそも文言変更が要るのか自体が自明でない**（他の Phase 5 記述は「偽になる」という具体的な破れを示すが、ここは何が偽になるのか本文に無い）
  - 172行のシーケンス図は `View->>View: results_window_height 算出 → ... （drive_results_window）` という**自己呼び出し**の矢印であり、関数が別モジュール（`window_coordinator.rs`）へ移ることで実体は `View->>Coordinator` 相当になり得るが、mermaid の `participant` 一覧（155-159行）に `Coordinator` に相当する participant は無い。新規 participant を追加するのか、矢印はそのままでラベルだけ触るのかの判断材料が無い

- **Phase 4 の行番号参照が Phase 4 実行時点で既にずれている**: 「本体は現行 788-876 をそのまま運ぶ」（174行）は Phase 1 実行前の行番号であり、Phase 2 でデルタガード（858-864 の 7 行）が `results.set_size(width, applied_height);` の 1 行に置き換わるため、Phase 4 着手時点では関数末尾の行番号が約 6 行分ずれている。直後の差分表（176-182行）が実質的な変更点を明示しているため実害は小さいが、「788-876」という具体的な行範囲を Phase 4 の時点でそのまま探すと一致しない。

- **`grep -c` の合計は自動で出ない**: Phase 3 完了条件（117行）「`grep -c "cfg(not(windows))" src-tauri/src/egui_shell/*.rs src-tauri/src/main.rs` の合計が...7」は正しい値だが（後述の「確認して実行可能」参照）、`grep -c` は複数ファイル指定時に**ファイルごとの件数**を出すのみで合計を計算しない。実装者が手で合算する前提が明示されていない（合算自体は自明だが、「合計」という語から `grep -c` が総数を返すと誤解する余地がある）。

- **`save_placement_relative` の再エクスポートが実利用箇所を持たない**: grep実測では `save_placement_relative` の呼び出し元は `hide_egui_main`（同一モジュール内）のみで、モジュール外からの呼び出しは無い。それでも Phase 3 のコード例は `pub(crate) use window_coordinator::{ ..., save_placement_relative, ... }` として再エクスポートしている。`pub(crate) use` が `unused_imports` の対象になるかは未検証（下記）。

---

## 確認して実行可能だったもの（Phase 1〜5）

| Phase | 判定 | 根拠 |
|---|---|---|
| Phase 1（純粋述語の追加） | **単独では不可**（上記「手が止まる」#2）。Phase 2 の `ResultsWindow::set_size` 呼び出しと view.rs:1830 の書き換えを同一コミットに含めれば可 | `size_delta_exceeds` の追加のみでは `-D warnings` 下で `dead_code`（scratch crate で実測・上記コード） |
| Phase 2（デルタガードを ResultsWindow へ） | 実行可能 | `results_window.rs` の既存 `AtomicBool` フィールド・`lock().unwrap()` 様式（`EguiShellState.pending_hotkey_failure`）と整合。`view.rs:287/291`（フィールド）・`317-318`（初期化）・`858-864`（ガード本体）・`861`（`set_size` 唯一の呼び出し元・grep実測で確認）・`1194-1195`（reset-on-show）全て記載どおりの内容と行番号を実ファイルで確認 |
| Phase 3（window_coordinator.rs 新設） | **単独では不可**（上記「手が止まる」#1）。Phase 4 と合わせて 1 コミットにすれば可 | 移設対象 9 関数の呼び出し元・`#[cfg(not(windows))]` 双子（`mod.rs:521,621`）・自己言及コメント 4 箇所（`mod.rs:457,481,489,494`——実ファイルで文言まで一致確認）は全て実測どおり。完了条件の grep 件数（mod.rs 2 + results_window.rs 3 + main.rs 2 = 7）も実測一致 |
| Phase 4（drive_results_window の移設） | 実行可能 | `view.rs:788-876`（関数本体）・`753-760`（`max_results`、view.rs内の唯一の利用点が818行であることも実測一致）・`1824-1837`/`1830`/`1838`（main 窓ガードと呼び出し点）全て記載どおり。`take_clicked_for`（1809行）が `result_count` の読み点（818行相当）より前にあることも実コードで確認——読み点の非対称に関する警告は正確 |
| Phase 5（文書同期） | 実行可能（ただし docs/architecture.md の具体的な書き換え内容は実装者の解釈に委ねられる・上記参照） | `layout.rs:102,118` と `visual.rs:5` の `mod.rs::` 名指しは grep で全件（3件）確認。SPEC.md:412-415 が関数名を持たないことも確認 |

**テスト方針の baseline 実測**: `cargo test -p snotra` = `174 passed; 0 failed; 2 ignored; finished in 1.55s` — plan.md 265行の記載と完全一致（実測日: 現ブランチ `ed6d68a`、ソースは `a98312c` と同一）。`cargo clippy --workspace --all-targets -- -D warnings` および `cargo doc --workspace --no-deps --document-private-items` も現状クリーン（0 warnings）。`npm run governance:check` も現状 green。`npm test` / `smoke:startup` / `smoke:egui -ResultsQuery` / `governance:check` は `package.json` に実在するスクリプトと一致し、`docs/build-commands.md` のカテゴリ A/C/D/F の定義とも一致する。

---

## 未検証（理由）

- **`pub(crate) use` 経由の再エクスポートが `unused_imports` を発火させるか**（`save_placement_relative` の件）: ソースコードを書き換えずに実測する手段が無い（本タスクは plan.md 以外のファイル編集が禁止されている）。リポジトリ外の scratch crate で検証するには private module から `pub(crate) use` する構造を再現する必要があり、今回の時間内では実施しなかった。実害があっても対処は「再エクスポート一覧から1シンボルを削る」程度で軽微
- **`-ResultsQuery <開発機の索引に一致する1文字>` に実際どの文字を渡すべきか**: 環境（開発機のファイルインデックス内容）に依存するため、この場では検証不能。plan.md も意図的に具体値を保留しており、それ自体は妥当な設計
- **カテゴリ D の目視 9 項目の実施可否**: コード変更が未着手のため目視確認自体が実施不可（レビュー対象外）
