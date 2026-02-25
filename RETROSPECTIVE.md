# Retrospective — フィードバック修正サイクル（4_feedback_plan）

対象フェーズ: 手動テスト (4_review.md) → 原因分析 → 計画 (4_feedback_plan.md) → 実装 → コードレビュー → 追加修正

---

## 1. 修正した内容

### Fix 1: ShellExecuteW の COM STA 初期化（`commands.rs`, `Cargo.toml`）

**症状**: ホットキー直後にフォルダ・画像ファイルをクリックしても起動しない。EXE は起動する。

**根本原因**: `ShellExecuteW` はフォルダ・画像などのシェル拡張に COM STA を要求するが、Tauri コマンドハンドラスレッドは COM 未初期化。EXE は `CreateProcess` 相当の経路をたどり COM 不要なため成功、フォルダ・画像は失敗という非対称が生じていた。

**修正**: `std::thread::spawn` で新規 OS スレッドを作り、`CoInitializeEx` → `ShellExecuteW` → `CoUninitialize` をその中で実行。`Win32_System_Com` feature を `Cargo.toml` に追加。

### Fix 2: エラーアイテムの視覚表示（`folder.rs`, `ResultRow.tsx`, `global.css`）

**症状**: `C:\System Volume Information\` を入力するとエラーアイテムが生成されるが、通常アイテムと見た目が同一でユーザーが区別できない。

**根本原因 A**: `folder.rs` が `name: "アクセスできません"` という UI 表示文字列を純ロジック層に持っていた（責務違反）。フロントエンドはこの `name` フィールドを一切表示しないため死んだコードになっていた。

**根本原因 B**: `ResultRow.tsx` が `isError` フラグを `classList` にも icon 分岐にも反映していなかった。

**修正**: `folder.rs:20` を `name: String::new()` に変更（ロジック層から UI 文字列を除去）。`ResultRow` に `error` クラスと ⚠️ アイコンを追加。`global.css` に `.result-row.error` スタイルを追加。

---

## 2. 今サイクルで生まれた新しいバグパターン

### パターン A: 「ロジック層に UI 文字列が紛れ込む」

`folder.rs` の `"アクセスできません"` は `snotra-core`（純ロジック lib crate）に埋め込まれた日本語 UI 文字列だった。フロントエンドで表示されないため機能的被害はなかったが、**意味（`is_error: true`）と表示文字列が同じ層に混在すること**は保守上の誤誘導になる。

発見の手順:
1. `ResultRow.tsx` に `isError` の視覚表示がない → Fix 2 を計画
2. `folder.rs` の `name: "アクセスできません"` に気づく
3. フロントエンドで `name` が一切参照されないことを grep で確認
4. 「純ロジック層に UI 文字列」という責務違反と判定

**教訓**: 新しい `SearchResult` を生成するコードを書くとき、`name` フィールドに表示文字列（特に日本語メッセージ）を入れていないか確認する。エラー状態の伝達は `is_error: true` フラグで十分。

### パターン B: 「計画書の更新漏れが内部矛盾を生む」

計画書を更新（`触る` に `folder.rs` を追加）した際、関連する `触らない` と `変更なし根拠（folder.rs）` のセクションを更新し忘れた。結果として:
- `触る`: `folder.rs` を変更する
- `触らない`: `folder.rs` は変更しない（旧記述のまま）

という矛盾が計画書内に発生した。

**教訓**: 計画書の「触る」セクションを更新したとき、「触らない」「変更なし根拠」セクションに同じファイルへの言及がないか必ず確認して整合させる。

---

## 3. コードレビューが発見した問題

コードレビュー（実装完了後）が3件を指摘し、全て対処した。

| 重要度 | 問題 | 対処 |
|--------|------|------|
| Medium | `commands.rs` のコメントが `S_FALSE` ケースを言及していない → 将来の保守者が `is_ok()` を `== S_OK` に変えると CoUninitialize が漏れる | コメントを3ケース（S_OK・S_FALSE・RPC_E_CHANGED_MODE）に分けて正確に記述 |
| Low | `selected + error` 同時付与時の CSS 挙動が未明示 | CSS に「selected の background が残るのは意図的設計」コメントを追加 |
| Low | `list_folder_nonexistent_dir_returns_empty` テストが `name: ""` を検証していない | `assert_eq!(results[0].name, "")` を追加 |

**学び**: COM の HRESULT 戻り値のコメントは「通常はこうなる」と「例外はこうなる」の両方を書く。`is_ok()` が `S_FALSE` を含むことは暗黙の知識であり、コードを読む人が全員知っているとは限らない。

---

## 4. 責務分担の確認フロー（今サイクルで確立）

フロントエンド表示バグを修正するとき、以下の手順で責務分担を確認する:

```
1. フロントエンドで「表示されるべきだが表示されていない」情報を特定する
2. その情報のソース（バックエンドのフィールド・型）を確認する
3. grep でそのフィールドがフロントエンドで参照されているか確認する
4. バックエンドのフィールドに「表示文字列（日本語メッセージ等）」が入っていないか確認する
   → 入っていれば責務違反: snotra-core に UI 文字列を持たない原則に反する
   → 修正: バックエンドは is_error: true のような意味フラグのみ持ち、表示はフロントが決める
```

---

## 5. 未解決の残存リスク（前サイクルから継続）

| 問題 | 場所 | 判断 |
|------|------|------|
| App.tsx の listen unlisten 未登録 | App.tsx 複数箇所 | アンマウントなし設計・HMR のみ影響・保留 |
| IME タイミング競合 | platform.rs | HWND を直接渡すことで緩和済み・理論上残存 |
| WM_CONTEXTMENU と WM_RBUTTONUP の二重メニュー | platform.rs | 一部環境での二重表示リスク・未対応 |
| requestId の二重管理 | App.tsx / search.ts | 現状整合・設計上の注意点として記録 |
| commands.rs:124 の `"hotkey_registration_failed"` | commands.rs | ユーザーに英語コードが表示される責務違反・今サイクル対象外 |
