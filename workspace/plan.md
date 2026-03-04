# 実装計画: Issue #12 — 多言語対応

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `ui/src/lib/i18n.ts` | **新規作成**: TranslationKey 型 + JA_JP 辞書 + `t()` 関数 |
| `ui/src/components/SearchWindow.tsx` | プレースホルダー・オーバーレイ文字列を `t()` に置換 |
| `ui/src/components/SettingsWindow.tsx` | タブラベル・ボタン・ステータス文字列を `t()` に置換 |
| `ui/src/components/SettingsGeneral.tsx` | 全ラベル・グループタイトル・説明を `t()` に置換 |
| `ui/src/components/SettingsSearch.tsx` | 全ラベル・グループタイトル・オプション・説明を `t()` に置換 |
| `ui/src/components/SettingsIndex.tsx` | 全ラベル・ボタン・モーダルタイトル・ステータスを `t()` に置換 |
| `ui/src/components/SettingsOpener.tsx` | 全ラベル・ボタン・モーダルタイトルを `t()` に置換 |
| `ui/src/components/SettingsVisual.tsx` | グループタイトル・カラーフィールドラベル・ボタンを `t()` に置換 |
| `ui/src/components/SettingsEditableList.tsx` | `"追加"` デフォルトを `t()` に置換 |
| `ui/src/components/AboutWindow.tsx` | メール subject を `t()` に置換 |
| `ui/src/lib/commands.ts` | コマンド description を `t()` に置換 |
| `ui/src/lib/openerTarget.ts` | `"フォルダ"` ラベルを `t()` に置換 |
| `ui/src/stores/search.ts` | 起動通知プレフィックスを `t()` に置換 |
| `ui/src/stores/settings.ts` | ステータス文字列を `t()` に置換 |

計: 1新規 + 13修正 = 14ファイル

---

## 実装順序

### Phase 1: `ui/src/lib/i18n.ts` を作成する

全 TranslationKey 型と JA_JP 辞書を定義する。`t()` 関数を実装する。

**TranslationKey 一覧（全キー）:**

```
search.placeholder.default
search.placeholder.folder        ← テンプレート: {dir}
search.placeholder.tool_select
search.status.indexing
search.status.launching
search.status.no_results

settings.tab.general
settings.tab.search
settings.tab.index
settings.tab.visual
settings.tab.opener
settings.loading
settings.unsaved_changes
settings.save
settings.no_changes
settings.discard

settings.general.group.hotkey
settings.general.hotkey.label
settings.general.hotkey.none
settings.general.toggle.label
settings.general.toggle.description
settings.general.group.appearance
settings.general.max_results.label
settings.general.max_results.description
settings.general.window_width.label
settings.general.window_width.description
settings.general.show_icons.label
settings.general.show_icons.description
settings.general.group.behavior
settings.general.show_on_startup.label
settings.general.show_on_startup.description
settings.general.auto_hide.label
settings.general.auto_hide.description
settings.general.tray_icon.label
settings.general.tray_icon.description
settings.general.ime_off.label
settings.general.ime_off.description

settings.search.group.mode
settings.search.normal_mode.label
settings.search.normal_mode.description
settings.search.folder_mode.label
settings.search.folder_mode.description
settings.search.mode.prefix
settings.search.mode.substring
settings.search.mode.fuzzy
settings.search.group.visibility
settings.search.show_hidden.label
settings.search.show_hidden.description
settings.search.group.history
settings.search.history_size.label
settings.search.history_size.description
settings.search.group.history_score
settings.search.history_normalization.label
settings.search.history_normalization.description
settings.search.normalization.disabled
settings.search.normalization.fuzzy_relative_cap
settings.search.history_cap.label
settings.search.history_cap.description

settings.index.group.scan
settings.index.empty
settings.index.path.unset
settings.index.extensions.unset
settings.index.include_folders.badge
settings.index.edit
settings.index.modal.edit_title
settings.index.modal.add_title
settings.index.path.label
settings.index.path.hint
settings.index.extensions.label
settings.index.include_folders.label
settings.index.browse
settings.index.delete
settings.index.cancel
settings.index.save
settings.index.merged_duplicate
settings.index.merged_existing

settings.opener.group.rules
settings.opener.description
settings.opener.empty
settings.opener.edit
settings.opener.move_up
settings.opener.move_down
settings.opener.modal.edit_title
settings.opener.modal.add_title
settings.opener.target.label
settings.opener.target.folder
settings.opener.target.ext
settings.opener.tool_name.label
settings.opener.tool_name.unset
settings.opener.exe.label
settings.opener.browse
settings.opener.args.label
settings.opener.args.hint
settings.opener.delete
settings.opener.cancel
settings.opener.save

settings.visual.group.preview
settings.visual.group.theme
settings.visual.custom_theme.label
settings.visual.custom_theme.delete
settings.visual.custom_theme.save
settings.visual.group.colors
settings.visual.color.background
settings.visual.color.input_bg
settings.visual.color.text
settings.visual.color.selected_row
settings.visual.color.hint_text
settings.visual.group.font
settings.visual.font_family.label
settings.visual.font_loading
settings.visual.font_size.label

common.add

cmd.history.description
cmd.about.description
cmd.settings.description
cmd.rebuild_index.description
cmd.quit.description

opener.target.folder

notice.launch.timeout
notice.launch.failed

status.load_failed
status.saved
status.save_failed.hotkey
status.save_failed.prefix

about.email_subject
```

### Phase 2: コンポーネント・ストアを修正する

各ファイルで `import { t } from "../lib/i18n"` (or 相対パス) を追加し、ハードコード文字列を `t("key")` に置換する。

テンプレート文字列の例:
```typescript
// Before:
return `${fs.currentDir} 内を検索...`;
// After:
return t("search.placeholder.folder", { dir: fs.currentDir });
```

プレフィックス連結の例:
```typescript
// Before:
setLaunchNoticeWithAutoClear(`起動に時間がかかっています${detail}`);
// After:
setLaunchNoticeWithAutoClear(t("notice.launch.timeout") + detail);
```

---

## 不変条件

1. `t("opener.target.folder")` は常に `"フォルダ"` を返す（`openerTarget.test.ts` がこれに依存）
2. `t()` は同期関数（コンポーネントレンダリングで `await` 不要）
3. `TranslationKey` 型によりコンパイル時にキー不存在を検出できる
4. 既存の UI 表示内容は変わらない（リファクタリングのみ）
5. `SettingsVisual.tsx` の `COLOR_FIELDS` 配列は `t()` を呼び出せるように module 内で定義する（関数スコープへの移動 or トップレベルで `t()` を呼ぶ）

---

## テスト方針

- 新規ユニットテストは追加しない（`t()` 関数は薄いラッパーで自明）
- 既存テストが壊れていないことを `npm test` で確認
- TypeScript 型チェックで TranslationKey の網羅性を確認: `npm run typecheck`
- フロントエンドビルドで全ファイルの整合性を確認: `npm run build`

---

## SPEC.md 更新要否

なし。外部から観察できる挙動変化なし（リファクタリングのみ）。

---

## セルフレビュー

### 1. 対称コードパス

`stores/search.ts` で起動通知が2箇所ある（`launchWithToolSelection` と `launchItem`）。
両方同じ `t("notice.launch.timeout")` / `t("notice.launch.failed")` を使う。✅

`settings.ts` の保存失敗パスも2ケース（hotkey失敗 vs. その他）で別々のキーを定義済み。✅

### 2. 影響範囲の網羅性

全 TSX/TS ファイルの日本語文字列を grep で網羅的に確認済み（research.md 参照）。
`"Snotraについて"` (AboutWindow) も含めた。✅

### 3. 境界条件

- キー不存在時: `JA_JP[key] ?? key` でキー文字列自体をフォールバックとする（デバッグ時に視認可能）
- テンプレート変数未提供時: `{dir}` がそのまま残る（エラーは出ない）
- `SettingsVisual.tsx` の `COLOR_FIELDS` がモジュールスコープで `t()` を呼ぶ: 問題なし（`t()` は pure function で副作用なし）✅

### 4. リソース管理

新たなリソース生成なし。✅

### 5. 既存パターンとの整合

`lib/` に新しいユーティリティモジュールを追加するパターンは既存（`theme.ts`, `invoke.ts` など）。✅

### 6. YAGNI 違反

外部 `locales/` ファイルのロード機能は今回対象外。
現要件（日本語固定）に必要な最小実装のみ。✅

### 修正した点

レビューで `SettingsVisual.tsx` の `COLOR_FIELDS` 配列がモジュールスコープで定義されており、
`label` フィールドに `t()` を使う場合に問題がないか確認。
`t()` は副作用のない純粋関数のため、モジュール評価時の呼び出しも安全。✅
