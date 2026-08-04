# research — #838 SPEC §6.3（フォルダ展開中の検索）の記述が実装とずれている 2 点

## issue の要約

`SPEC.md` §6.3 は 3 行しかなく、フォルダ展開中の絞り込みについて実装と食い違う点が 2 つある。どちらも **as-built の明文化であり、挙動は変えない**。

- **ずれ 1**: 列挙に失敗しているとき（§6.6）のエラー行は絞り込みの対象外だが、§6.3 は「文字入力時は現在フォルダ内で絞り込み」と無条件に書いている
- **ずれ 2**: 絞り込みの打鍵が選択を 1 行目へ戻すことが §6.3 に無い（#743 で §6.1 側だけが選択の記述を得た非対称）

受け入れ条件は 3 つ（§6.3 が実装と一致 / §6.1・§6.3・§6.6 で二重に書かない / 挙動を変えず純粋核テストで固定できる範囲は固定する）。

issue コメント（2026-08-04・`b28d2b9` 時点）が「ずれ 2 点はどちらも現存」と再検証済み。本調査でも同じ結論に達した（下記の一次証拠）。

## 一次証拠（今回自分で読んだ箇所）

### ずれ 1 — エラー行は filter 非適用、かつ打鍵では消えない

`src-tauri/src/egui_shell/launcher_controller.rs`

- `run_search_with` の `ViewKind::Folder` 分岐（`launcher_controller.rs:757-766`）
  - `folder_error` が `Some` → `self.state.set_results(err.clone())`（コメントに「列挙失敗行（filter 非適用）」）
  - `folder_cache` が `Some` → `ctx.filter_sorted(sorted, self.state.folder_filter())` を通してから `set_results`
  - どちらも未着（ロード中）→ **`set_results` を呼ばずに返る**（前フレームの行を保持）
- `folder_error = None` を撃つ経路は 6 か所で、**打鍵経路はそこに含まれない**
  - `consume_reset_pending`（show 時の reset・`:935`）
  - `FolderMsg::Loaded` 到着（`:1028`）
  - Escape の `RestoredSearch`（`:1062`）
  - `→` の folder 突入／深掘り（`:1162`）
  - `←` の folder 内上昇（`:1174`）
  - `←` の Results からの突入（`:1192`）
- 打鍵経路 `on_input_changed`（`:1203-1227`）は folder 中なら `set_folder_filter(buf)` → `run_search()` だけで `folder_error` に触れない

→ **「エラー行表示中は打鍵しても絞り込まれない」は打鍵の間ずっと持続する状態**であり、一瞬の窓ではない（issue コメントの記述どおり）。

**注意（文言を強く書きすぎない根拠）**: 打鍵のたびに `set_results(err.clone())` が走るため `rows_generation` は毎打鍵**進む**（`search_state.rs:196-200`）。「何も起きない」は状態レベルでは偽。ユーザーが観測できる粒度＝**候補行が変わらない**で書く（`rows_generation` は実装識別子ゆえ SPEC には入れない）。

### ずれ 2 — 絞り込みの打鍵は選択を 1 行目へ戻す

`src-tauri/src/egui_shell/search_state.rs:348-351`

```rust
pub fn set_folder_filter(&mut self, f: String) {
    self.folder_filter = f;
    self.selected = 0;
}
```

- 呼び出し点は `launcher_controller.rs:1226`（`on_input_changed` の folder 分岐）ただ 1 つ（grep 実測）
- `folder` が `Some` かどうかに関係なく `selected = 0` を撃つ（テスト設計上の罠 → plan の「テスト方針」）

### 既存 SPEC の周辺（二重記述の回避に効く）

- §6.1（`SPEC.md:235-239`）: 左右カーソルキーの選択の扱い 3 段 + 「フォルダ内の絞り込み文字列（§6.3）はクリアされる」
  - **到着時のクランプは §6.1:237 が既に所有している** → §6.3 で再説しない
- §6.3（`SPEC.md:247-251`）: 現行 3 行。`git blame` 上 2026-02 から未変更（issue コメント）
- §6.6（`SPEC.md:266-272`）: エラー行の Enter 無効・右/左/Escape は通常どおり有効
- §6.7（`SPEC.md:273-282`）: プレースホルダは絞り込みが空のときだけ表示 / 列挙失敗時も現在地を示し続ける
  - **打鍵でプレースホルダが消えることは §6.7:276 が所有している** → §6.3 で再説しない

## 再利用できる既存パターン

- **テスト**: `search_state.rs:785-868` の「階層移動と選択（#743・SPEC §6.1 の as-built）」ブロック。`res()` ヘルパ・`enter_folder` → `set_results` → `move_selection` → assert の型がそのまま使える
  - ブロック冒頭コメント（`:787-794`）が **射程の限界**（キー割り当てと driver 分岐は射程外）と **無効テストの罠**（`enter_folder` を忘れると何も検証せず緑）を明記しており、新規テストもこの流儀に合わせる
  - `:831-832`「**非ゼロから始める**——0 から始めると `enter_folder` の初期値と区別が付かず、何も実証しない」
- **SPEC の書き方**: §6.1 の `（as-built）` 注記、`§N.M` 形式の相互参照

## 技術的制約

- **ずれ 1 は純粋核テストで固定できない**。`folder_error` は `LauncherController` のフィールドで、`run_search_with` の呼び出しには `AppHandle` を持つコントローラの構築が要る（`snotra` は `[lib]` を持たないため統合テストからも触れない）。純粋関数へ抽出するのは挙動不変の範囲を越える拡張なので取らない
  - 腐り検知の錨は `launcher_controller.rs:760` の既存コメント「列挙失敗行（filter 非適用）」（SPEC の文言と同じ事実を指す）
- `cargo test -p snotra --lib` は常に失敗する（`[lib]` target 無し・`src-tauri/CLAUDE.md`）。`--lib` を付けない
- `SPEC.md` は PostToolUse hook の検査割り当てが無く、**沈黙は「何も走らなかった」**（ルート `CLAUDE.md`）→ `npm run governance:check` を手で回す
- `.claude/rules/spec.md` の「セクション番号整合」は箇条書きの追加のみで見出しを増減しないため非該当

## 未解決の疑問

- （裁定済み・plan の「明示した仮定」へ）ずれ 1 の事実を §6.3 と §6.6 のどちらに置くか。issue の提案が §6.3 を名指しし、AC 2 が二重記述を禁じるため **§6.3 のみ・§6.6 は無改変**とする。ただし §6.6 の「右/左/Escape は通常どおり有効」は閉じた列挙に読めるままで、issue 本文自身が「文字入力の扱いだけがどちらの節にも正確に書かれていない」と述べている → Step 5c で人間へ 1 問だけ確認する

## 観察（今回は触らない）

- **§4（検索システム）にも「打鍵は選択を 1 行目へ戻す」の記述が無い**。実装では通常検索でも `on_input_changed` が `reset_selection()` を撃つ（`launcher_controller.rs:1230`）。#838 の射程は §6.3 なので本 PR では触らない（広げると AC の範囲を越える）
