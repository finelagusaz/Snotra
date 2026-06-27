# research.md — issue #395 instant Program 種別に exe ファイルピッカー

## issue の要約

#394 でインスタントコマンドに exec（exe+args）種別を追加したが、exe フィールドは**手入力テキスト欄のみ**。
`opener.rs` の `ExePickerState`（rfd 非同期ファイルピッカー）を instant の Program 種別フィールド
（`instant.rs`）へ流用し、フィルタを **`["exe"]` 限定**で追加する。

価値:
1. UX 向上（参照ダイアログで exe を選べる）
2. 防御層: UI 経路で `.bat`/`.cmd`/`.lnk` を選ばせない（`Command::new` はこれらを正しく起動できない／
   `.bat`/`.cmd` は cmd.exe 経由で `{query}` がメタ文字に晒される。手入力は残存リスクのため UI で上積み）

優先度: 中（手入力で機能は動作する。UX + 防御の上積み）

## 関連コード

| ファイル | 関与 |
|---|---|
| `snotra-settings/src/tabs/opener.rs` | `ExePickerState` 定義（`pub`, `Default`, `Clone`）、ピッカー spawn/poll の既存実装（流用元） |
| `snotra-settings/src/tabs/instant.rs` | 変更対象。Program 種別 exe フィールドへ参照ボタン追加 |
| `snotra-settings/src/tabs/mod.rs` | `pub mod opener;` `pub mod instant;`（可視性 OK） |
| `snotra-settings/src/i18n.rs` | `btn_browse()`/`dialog_select_exe()`/`filter_executables()`（流用可。新規不要） |
| `SPEC.md` §19.8 | exe フィールドを「手入力の単一行テキストフィールド」と記述 → 挙動変更で同期必要 |
| `SPEC.md` §18.6 | opener 側の同等記述「exe パス入力にファイルブラウズダイアログ」（記述様式の参照先） |

## 既存パターン（流用元の構造）

### `ExePickerState`（opener.rs:10-15）
```rust
#[derive(Clone, Default)]
pub struct ExePickerState {
    pub result: Arc<Mutex<Option<Option<PathBuf>>>>, // None=実行中, Some(None)=キャンセル, Some(Some(p))=選択
    pub active: bool,                                 // true の間ボタン無効化
}
```

### poll（opener.rs:104-113, `ui()` 冒頭）
毎フレーム `try_lock()` → `take()` → `active=false` → 選択時のみ `edit_tool_exe` に反映。

### spawn（opener.rs:304-321, `show_modal()` 内、参照ボタン clicked 時）
`active=true` → `Arc::clone(result)` + `ctx.clone()` → `std::thread::spawn` で
`rfd::FileDialog::new().set_title(..).add_filter(label, &["exe","bat","cmd"]).pick_file()` →
`*result.lock() = Some(path)` → `repaint_ctx.request_repaint()`。

### instant.rs 側の現状
- `InstantTabState { modal }`（`#[derive(Default)]`）
- `ui(ui, ctx, config, state, tr)` — `ctx` を既に受け取る（poll 設置可）
- `show_modal(ctx, config, state, tr)` — `ctx` を既に受け取る（spawn 設置可）
- Program 種別フィールド（instant.rs:281-299）: `edit_exe` の `text_edit_singleline`、`edit_args`、プレビュー

## 技術的制約

- **rfd はブロッキング API** → 必ずスレッド spawn + `Arc<Mutex<Option<Option<PathBuf>>>>` + `request_repaint()`。
  UI スレッド直呼びはフリーズ（snotra-settings/CLAUDE.md「非同期ファイルピッカーパターン」）。
- **`active=false` の書き忘れ = ボタン永久無効化バグ**（CLAUDE.md 明記）。キャンセル・成功の両パスでリセット必須。
- **egui ユニットテスト不可**（モック困難）。検証はビルド + clippy + 視覚スモーク。
- **instant exec は `.exe` 限定でない（重要・当初前提の是正）**: `launch_exec_core`（`launch.rs:394`）は
  `Command::new(expand_env(exe))` で拡張子検証なし。受理範囲＝`.exe` / 拡張子なし（CreateProcessW が `.exe` 補完）
  / `.com` 等 PE / `cmd.exe`（SPEC §19.2:711-715 が例示）/ `.bat`・`.cmd`（Rust std が cmd.exe 経由起動）。
  `.lnk` のみ非対応。opener も同一機構で picker フィルタは `["exe","bat","cmd"]`。
  → フィルタ範囲はユーザー確認で **`["exe"]`（既定誘導）+ `["*"]`（全ファイル）** に確定（plan.md 参照）。
- **rfd 0.17.2 の複数フィルタ**（`dialog_ffi.rs:175-207`、実ソース確認）: `add_filter` を複数呼ぶとドロップダウン生成、
  **最初のフィルタが既定**（`set_default_extension`）、`["*"]` は spec `*.*`（全ファイル）。`extensions` は `&[impl ToString]`。

## 未解決の疑問

なし。要求・流用元・差分（フィルタ）すべて確定。
