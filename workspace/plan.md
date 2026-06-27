# plan.md — issue #395 instant Program 種別に exe ファイルピッカー

## 方針（symmetric-check で確定）

**Option A: `ExePickerState` 構造体を流用 + poll/spawn は opener と同型に inline。opener.rs は非改変。**

symmetric-check で crate 全体に3つのピッカーが存在することが判明:
- `opener.rs` `ExePickerState`（`pick_file` + filter）
- `index.rs` `PickerState`（**独自定義** / `pick_folder` / filter なし）
- `backup.rs` inline（別状態モデル / toml）

`index.rs` は `ExePickerState` と構造同一なのに**統合せず inline 定義**している。すなわち作者の規約は
「ピッカーは各タブに inline、抽象化しない」。当初検討した「spawn/poll をメソッド抽出」案は opener/instant だけ
メソッド化し index を inline のまま残す**新たな非対称**を生むため**棄却**。

採用案: instant も opener と同型に poll/spawn を inline する。ただし構造体は新規定義せず
`ExePickerState` を import 流用する（issue の「ExePickerState 流用」要求に直接合致。instant の用途=exe
ファイルピッカーは opener と同一のため、index のように別構造体を作る必要がない）。

利点:
- **規約準拠**: 3ピッカーすべて inline で対称
- **KISS / YAGNI**: 作者が避けた抽象レイヤを新設しない
- **低リスク**: opener.rs を一切触らない（動作中コードの改変回避）。blast radius = instant.rs + SPEC のみ
- **DRY 整合**: inline 重複は既存（index↔opener が既に別 inline コピー）の確立パターンと一致。新規逸脱ではない

## 変更ファイル一覧

### 1. `snotra-settings/src/tabs/instant.rs`（参照ボタン追加 — 唯一のコード変更）

- import 追加:
  - `use std::sync::Arc;`（inline spawn で `Arc::clone` を使う）
  - `use crate::tabs::opener::ExePickerState;`（構造体流用）
- `InstantTabState` に `exe_picker: ExePickerState` フィールド追加（`ExePickerState` は `Default` 導出済み →
  `#[derive(Default)]` 維持可）
- `ui()` 冒頭（`tab_scroll_area` の前）に poll を inline（opener.rs:105-113 と同型）:
  ```
  if state.exe_picker.active
      && let Ok(mut guard) = state.exe_picker.result.try_lock()
      && let Some(result) = guard.take()
  {
      state.exe_picker.active = false;
      if let Some(path) = result {
          state.modal.edit_exe = path.display().to_string();
      }
  }
  ```
- `show_modal()` の `EditKind::Program` ブランチ（281-299）で exe フィールドを `ui.horizontal` に包み、
  `text_edit_singleline(&mut state.modal.edit_exe)` の右に参照ボタン（opener.rs:302-322 と同型、
  **フィルタのみ `&["exe"]`**）:
  ```
  ui.horizontal(|ui| {
      ui.text_edit_singleline(&mut state.modal.edit_exe);
      if ui.add_enabled(!state.exe_picker.active, egui::Button::new(tr.btn_browse())).clicked() {
          state.exe_picker.active = true;
          let result = Arc::clone(&state.exe_picker.result);
          let repaint_ctx = ctx.clone();
          let dialog_title = tr.dialog_select_exe().to_string();
          let filter_label = tr.filter_executables().to_string();
          std::thread::spawn(move || {
              let path = rfd::FileDialog::new()
                  .set_title(&dialog_title)
                  .add_filter(&filter_label, &["exe"])  // ← 防御: exe 限定
                  .pick_file();
              *result.lock().unwrap() = Some(path);
              repaint_ctx.request_repaint();
          });
      }
  });
  ```
  （`edit_args` ラベル以降は現状維持）

> **opener.rs / index.rs / backup.rs は変更しない**。index↔opener のピッカー統合は #395 スコープ外
> （folder vs file の一般化が必要）。将来の DRY 改善候補だが本 issue では扱わない。

### 3. `SPEC.md` §19.8（as-built 同期）

- line 879 の「手入力の単一行テキストフィールド」を、参照ボタン併設に更新。
  手入力も従来通り可能であることを残す（ピッカーは加算的）。
- **記述粒度の決定**（plan-review 軽微懸念への対応）: opener §18.6 は「exe パス入力にファイルブラウズダイアログ」と
  極簡潔（フィルタ仕様なし）。instant の `["exe"]` 限定は opener（`.bat`/`.cmd` 許容）より**意図的に厳しく**、
  セキュリティ防御が理由。よって §19.8 には参照ボタン併設 + **`.exe` 限定フィルタ** + 防御理由を**一行**で添える。
  これは既存 §19.4（「env 変数へのインジェクション防止」を簡潔に明記）の様式と調和する。冗長な説明はしない。
  記述案: 「`exe` パス入力に `.exe` 限定のファイルブラウズダイアログ（参照ボタン）を併設。手入力も可。
  `.bat`/`.cmd` は `cmd.exe` 経由で `{query}` がメタ文字に晒されるため除外する。」

## 実装順序（依存関係）

1. instant.rs に import（`Arc` + `ExePickerState`）+ `InstantTabState` フィールド追加
2. instant.rs `ui()` 冒頭に poll を inline
3. instant.rs `show_modal()` Program ブランチに参照ボタン（inline spawn, filter `&["exe"]`）
4. SPEC.md §19.8 同期
5. ビルド + clippy（PostToolUse フックが自動実行）+ 視覚スモーク

## 不変条件

- **`active` の真偽ペア**: spawn が `true`、poll が（選択・キャンセル両方で）`false`。
  poll は `if let Some(path)` 分岐の**前**に `active=false` を置く（opener と同型）ため、キャンセル
  （`result==Some(None)`）でも確実にリセットされ、ボタン永久無効化を防ぐ。
  rfd スレッドが panic した場合 → `*result.lock()` に到達せず `result` は `None` のまま `active` は `true` のまま。
  ただし release は `panic=abort` のためスレッド panic はプロセス終了（wedge ではなく即終了）。
  通常の選択/キャンセルでは必ず `Some(_)` が書き込まれ poll が `active=false` に戻す。→ 回復経路あり。
- **`edit_exe` への反映タイミング**: poll は `ui()` 冒頭（モーダル描画前）。選択中はボタン無効化 + active=true で
  モーダルは開いたまま運用される。opener と同一構造のため挙動差なし（対称）。
- **opener / index / backup は非改変**: ピッカー関連の既存3実装は一切触らない。挙動不変。
- **borrow**: モーダル closure 内で `state.exe_picker`（spawn 時 `&mut`）と `state.modal.edit_exe`
  （`text_edit_singleline` で `&mut`）を**逐次**アクセス。同時借用ではないため成立（opener の既存パターンと同型）。
- **`#[derive(Default)]` 維持**: `ExePickerState` は `Default` 導出済み。`InstantTabState` の derive を壊さない。

## テスト方針

- **egui UI のためユニットテストは書かない**（snotra-settings/CLAUDE.md 方針）。ロジックは spawn/poll のみで
  純粋関数化できない（スレッド + rfd 副作用）ため、検証はビルド + clippy + 視覚スモーク。
- **検証コマンド**（docs/build-commands.md カテゴリ: snotra-settings = Rust UI crate）:
  - `cargo build -p snotra-settings`（import・field・borrow の整合確認）
  - `cargo clippy -p snotra-settings`（PostToolUse フック自動）
  - **視覚スモーク**: `cargo run -p snotra-settings` → Instant タブ → コマンド追加 → 種別 Program →
    参照ボタン押下 → ダイアログが `.exe` フィルタで開く → exe 選択で `edit_exe` に反映 → レイアウト崩れなし
    （horizontal レイアウトで text_edit + ボタンが収まるか確認）
- **後方互換**: config フォーマット変更なし（UI 入力経路の追加のみ）。既存 settings.toml への影響なし。

## SPEC.md 更新要否

**要**。§19.8 line 879 の挙動記述を変更（手入力のみ → 参照ボタン併設）。AGENTS.md Step 0「文書化された挙動を
変えたら仕様変更」に該当。§18.6 の opener 記述様式に合わせる。セクション番号の増減はなし（記述追記のみ）。

## セルフレビュー

### 5a. check スキル結果

- **/plan-review**（Explore 2体並列）: 「要対処」なし。Rust 層=全観点問題なし。SPEC 層=軽微懸念1件（§19.8 の
  記述粒度）→ §19.8 に防御理由を一行添える方針で解決済み（§3 に記録）。Step 2b（独立導出）は局所変更のため省略。
- **/symmetric-check**: crate に3ピッカー（opener/index/backup）が存在し、index は構造同一でも inline 独自定義。
  → 「メソッド抽出」案が新たな非対称を生むと判明し、**Option A（instant も inline・opener.rs 非改変）に方針転換**。
  candidate 判定: opener↔instant=[適用]、index `PickerState`=[不要]（folder, スコープ外）、backup=[不要]（別モデル）、
  spawn↔poll=[適用]、active true↔false=[適用]（選択・キャンセル両パスでリセット）。
- **/cache-check, /state-check**: 非該当（キャッシュ・新規 UI モード/ガードなし。`active` は既存パターンの流用）。
- **/race-check**: async/await 関数なし（thread::spawn + Arc<Mutex>）。既存 opener と同一の実証済みパターンで
  追加の競合リスクなし。poll の `try_lock`（非ブロッキング）も既存同型。

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: 5a /symmetric-check で検証済み。3ピッカー inline で対称。opener↔instant 適用。
2. **影響範囲の網羅性**: `rfd::|FileDialog|pick_file|PickerState|ExePickerState|.active|request_repaint|add_filter`
   を grep し全ピッカー（opener/index/backup）を列挙。instant 以外は非改変と確定。
3. **境界条件**: キャンセル（`Some(None)`）でも `active=false`。複数フレーム連続 poll 安全（active=false 時は try_lock せず）。
4. **リソース管理**: spawn（active=true / thread）↔ poll（active=false / take）のペアを inline で両立。
   thread は detach だが rfd 完了で必ず result 書込 → poll が回収。panic 時は panic=abort で即終了（wedge なし）。
5. **既存パターンとの整合**: 新規パターン導入なし。opener の inline poll/spawn を同型流用。`ExePickerState` 構造体を再利用。
6. **YAGNI 違反**: なし。issue 要求（ExePickerState 流用 + `["exe"]` フィルタ）のみ。index 統合・新抽象は持ち込まない。
7. **シンプル化の挑戦**: メソッド抽出案（新抽象）を棄却し inline（既存規約）を採用。`AtomicBool`/`Mutex`/子プロセス等の
   新状態は増えない（`ExePickerState` の既存 `Arc<Mutex>` を流用）。「参照ボタン押下で spawn 失敗時」は active=true の
   まま残るが、release=abort のため即プロセス終了で wedge にならない（5a /race-check 参照）。
8. **破壊不変条件の明示**: 「壊れたら即アウト」級なし（UI 入力経路の加算的追加。Win32 フック/ホットキー/IPC に非接触）。
   検知手段=ビルド + clippy + 視覚スモーク（ダイアログが exe フィルタで開く・選択で反映・レイアウト崩れなし）。

### 修正した点（plan-review/symmetric-check 起点）

- 当初の「spawn/poll メソッド抽出（opener.rs 改変）」案を symmetric-check で棄却 → **Option A（inline・opener.rs 非改変）**へ。
- SPEC §19.8 の記述粒度を plan-review 指摘で確定（防御理由を一行、§19.4 様式に調和）。
