# 実装計画: Issue #107 — 設定画面に「デフォルトに戻す」機能を追加

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `src-tauri/src/commands/config.rs` | `get_default_config` コマンド追加 |
| `src-tauri/src/main.rs` | コマンド登録に `get_default_config` 追加 |
| `ui/src/lib/invoke.ts` | `getDefaultConfig()` ラッパー追加 |
| `ui/src/lib/i18n.ts` | 翻訳キー追加（ボタンラベル・確認メッセージ） |
| `ui/src/stores/settings.ts` | `resetToDefault()` 関数を追加・export |
| `ui/src/components/SettingsWindow.tsx` | フッターに「デフォルトに戻す」ボタン + 二段階確認 UI 追加 |
| `ui/src/styles/settings.css` | 新ボタンのスタイル追加 |

計: 7ファイル変更

---

## 実装順序

### Phase 1: Rust 側コマンド追加

**`src-tauri/src/commands/config.rs`** に追加:
```rust
#[tauri::command]
pub fn get_default_config() -> Config {
    Config::default()
}
```

**`src-tauri/src/main.rs`** のコマンド登録に `commands::get_default_config` を追加。

### Phase 2: フロントエンド IPC ラッパー

**`ui/src/lib/invoke.ts`** に追加:
```typescript
export async function getDefaultConfig(): Promise<Config> {
  return tracedInvoke<Config>("get_default_config");
}
```

### Phase 3: i18n キー追加

**`ui/src/lib/i18n.ts`** に以下のキーを追加:
- `"settings.reset_to_default"`: 「初期設定に戻す」
- `"settings.reset_to_default.confirm"`: 「本当に初期設定に戻しますか？」

### Phase 4: 設定ストアに resetToDefault 関数追加

**`ui/src/stores/settings.ts`** に追加:
```typescript
async function resetToDefault() {
  const defaultConfig = await api.getDefaultConfig();
  setDraft(defaultConfig);
}
```

### Phase 5: UI 実装（SettingsWindow.tsx + CSS）

**`ui/src/components/SettingsWindow.tsx`** のフッターに:
- 「デフォルトに戻す」ボタンを追加（左寄せ、保存ボタンとは反対側）
- 二段階押し方式: 初回クリックでボタンテキストが「本当にデフォルトに戻しますか？」に変わり、再度クリックで実行。一定時間（3秒）経過 or 他操作で元に戻る
- ボタンの状態管理には `createSignal<boolean>` を使用

**`ui/src/styles/settings.css`** に:
- `.btn-reset-default` スタイル（控えめな外観、テキストボタン or セカンダリボタン）
- `.btn-reset-default.confirming` スタイル（確認状態、警告色）

---

## 不変条件

1. 「デフォルトに戻す」はドラフトに反映するだけで、保存は既存の「保存」ボタンで行う
2. デフォルト反映後、`hasChanges()` が正しく `true` を返す（savedConfig はそのままなので自動的に成立）
3. 確認状態は時間経過またはタブ切り替え等で自動解除される
4. `Config::default()` の `paths.scan` はシステム環境変数から動的生成されるため、Rust 側で毎回生成する

---

## テスト方針

- `npm run build` でフロントエンドビルド確認
- `cargo check -p snotra-core -p snotra` で Rust 型チェック
- `npm test` で既存フロントエンドテストが壊れていないことを確認
- 手動テスト: 設定変更後に「デフォルトに戻す」→ ドラフトがデフォルト値になる、保存ボタンが活性化する

---

## SPEC.md 更新要否

設定画面に新しいボタンを追加するため、SPEC.md の設定画面セクションに「デフォルトに戻す」機能の記述を追加する。

---

## セルフレビュー

### 1. 対称コードパス

- `loadDraft()` と `resetToDefault()` が対称ペア。`loadDraft` は `savedConfig` も更新するが、`resetToDefault` は `draft` のみ更新する点が意図的な差異。✅

### 2. 影響範囲の網羅性

- `setDraft()` の呼び出し箇所: `loadDraft`、`updateDraft`、新規 `resetToDefault`。既存の `hasChanges` / `canSave` は `draft()` と `savedConfig()` の比較なので自動的に正しく動作する。✅
- フッターの既存ボタン（保存・破棄）の動作に影響なし。✅

### 3. 境界条件

- デフォルト設定が現在の保存済み設定と同一の場合: `hasChanges()` が `false` になり保存ボタンが無効化 → 正常動作。✅
- `getDefaultConfig` の IPC 失敗: `resetToDefault` で catch してエラー表示（or 無視）する。

### 4. リソース管理

- 確認状態のタイマー（3秒自動解除）: `setTimeout` を使用。再クリック時に `clearTimeout` で前回タイマーをクリアする。✅

### 5. 既存パターンとの整合

- `get_default_config` は `load_config` と同じパターン（state 不要、Config を返す）。既存パターン踏襲。✅
- 二段階押し方式は新規 UI パターンだが、既存のバナー方式より KISS（追加 DOM 要素が少ない）。✅

### 6. YAGNI 違反

- 個別フィールドのリセットは提供しない（issue の要件は「全設定値をデフォルトに」のみ）。✅
- リセット履歴やアンドゥ機能は提供しない。✅

### 修正した点

- 初期案で確認バナー方式を検討したが、二段階押し方式のほうがシンプル（DOM追加なし、状態管理が局所的）なので変更。
- `resetToDefault` で IPC エラー時の処理を計画に追加。
