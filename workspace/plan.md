# Settings バグ修正 実装計画

作成日: 2026-03-11
対応ブランチ: `refactor/settings`
依拠: `workspace/research.md`（多角的検証済み版）

---

## フェーズ構成

| フェーズ | 対象 Bug | 難度 | 変更レイヤー |
|---------|----------|------|------------|
| Phase 1 | #2 #3 #4 #8 | LOW | `snotra-settings` のみ |
| Phase 2 | #7 | LOW | `src-tauri` のみ |
| Phase 3 | #10 | LOW | `snotra-settings` のみ |
| Phase 4 | #1 #9 | HIGH | `snotra-core` + `snotra-settings` + `src-tauri` |
| Phase 5 | #5 #6 | MEDIUM | `src-tauri` + `ui` |

**実装順序**: Phase 1 → 2 → 3 → 4 → 5

---

## Phase 1 — `reset_to_default` / `Discard` の副作用リセット漏れ（Bug #2 #3 #4 #8）

### 変更ファイル

- `snotra-settings/src/app.rs`

### 変更内容

#### Bug #2 #4 #8 — `reset_to_default()` の修正

```rust
// 現状
fn reset_to_default(&mut self) {
    self.draft = Config::default();
}

// 修正後
fn reset_to_default(&mut self) {
    self.draft = Config::default();
    self.tr = Tr(self.draft.general.language);          // #2: 言語同期
    self.hotkey_state = Default::default();             // #4: キャプチャ中断
    self.index_state = tabs::index::IndexTabState::default();   // #8: モーダルリセット
    self.instant_state = tabs::instant::InstantTabState::default(); // #8
    self.opener_state = tabs::opener::OpenerTabState::new();    // #8: new() を使う点に注意
}
```

`OpenerTabState::new()` を使う理由: `detect_opener_presets()` を実行するため `Default::default()` ではなく `new()` が必要。「初期設定に戻す」はユーザーの明示操作なので重い初期化コストも許容される。

#### Bug #3 — `Discard` ボタンの修正

```rust
// 現状（app.rs L370-372 付近）
if ui.add_enabled(self.has_changes(), egui::Button::new(self.tr.btn_discard())).clicked() {
    self.draft = self.saved.clone();
}

// 修正後
if ui.add_enabled(self.has_changes(), egui::Button::new(self.tr.btn_discard())).clicked() {
    self.draft = self.saved.clone();
    self.tr = Tr(self.draft.general.language);  // 追加
}
```

Discard でモーダルやホットキー状態をリセットしない理由: Discard は `draft` を保存済みに戻すだけで、UI 中間状態（モーダル・キャプチャ）は次フレームの再レンダリングで自然に整合する。`tr` だけはラベル表示に即影響するため必須。

### 受け入れ条件

1. 言語を En に変更（未保存）→「初期設定に戻す」→ UI ラベルが OS デフォルト言語（例: Ja）に即時切り替わる
2. ホットキーキャプチャ中（「キーを押してください...」表示中）に「初期設定に戻す」→ キャプチャが終了する
3. Index タブでスキャンパス追加モーダルが開いた状態で「初期設定に戻す」→ モーダルが閉じる
4. 言語を Ja に変更（未保存）→「破棄」→ UI ラベルが保存済み言語（例: En）に即時切り替わる

### 検証コマンド

```
cargo check -p snotra-settings
```

---

## Phase 2 — 監視スレッドの `.expect()` パニックリスク（Bug #7）

### 変更ファイル

- `src-tauri/src/commands/window.rs`

### 変更内容

```rust
// 現状（L98-100）
let proc_state = handle_for_monitor
    .try_state::<SettingsProcessState>()
    .expect("SettingsProcessState not managed");

// 修正後
let Some(proc_state) = handle_for_monitor.try_state::<SettingsProcessState>() else {
    eprintln!("[settings-monitor] SettingsProcessState not managed; exiting monitor thread");
    break;
};
```

スレッドがパニックするとプロセス全体がクラッシュする。`break` でループを抜けてスレッドを終了させる。実際には `SettingsProcessState` は `main.rs` で必ず `manage()` されるため本パスは実行されないが、防御的実装として必要。

### 受け入れ条件

1. 設定ウィンドウを開き、閉じる → 従来動作が維持される（モニタースレッドが正常に終了する）
2. `cargo check -p snotra` が通る

### 検証コマンド

```
cargo check -p snotra
```

---

## Phase 3 — 最小化状態で閉じると古い位置が保存される（Bug #10）

### 変更ファイル

- `snotra-settings/src/app.rs`

### 変更内容

```rust
// 現状（L403-408 付近）
if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
    let pos = rect.left_top();
    self.last_position = Some(WindowPlacement {
        x: pos.x as i32,
        y: pos.y as i32,
    });
}

// 修正後
let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
if !minimized {
    if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
        let pos = rect.left_top();
        self.last_position = Some(WindowPlacement {
            x: pos.x as i32,
            y: pos.y as i32,
        });
    }
}
```

`viewport().minimized` は `Option<bool>` を返す可能性があるため `unwrap_or(false)` で安全に処理する。

### 受け入れ条件

1. 設定ウィンドウを最小化 → × ボタンで閉じる → 次回起動時にウィンドウが最小化前の位置に表示される
2. 最小化なしで閉じる → 次回起動時に最後の表示位置に表示される（従来動作を維持）

### 検証コマンド

```
cargo check -p snotra-settings
```

---

## Phase 4 — `top_n_history` / `max_history_display` を `SearchConfig` へ移動（Bug #1 #9）

### 方針

`snotra-core` の `Config` 構造体を変更するため、TOML の互換性維持（マイグレーション追加）と `engine.rs` / `main.rs` への参照変更が必要。「`top_n_history` が `AppearanceConfig` にある」という設計上の誤りを根本から解消する。`max_history_display` の設定 UI 追加もこのフェーズで実施する。

### 変更ファイル

- `snotra-core/src/config.rs`
- `snotra-core/src/engine.rs`
- `src-tauri/src/main.rs`
- `snotra-settings/src/tabs/search.rs`
- `snotra-settings/src/i18n.rs`
- `snotra-settings/src/app.rs`

### 変更内容

#### 1. `snotra-core/src/config.rs` — 構造体のフィールド移動 + マイグレーション

**ステップ 1: `SearchConfig` にフィールド追加**:
```rust
#[serde(default = "default_top_n_history")]
pub top_n_history: usize,
#[serde(default = "default_max_history_display")]
pub max_history_display: usize,
```

`default_top_n_history()` と `default_max_history_display()` 関数は既存のものをそのまま使う。

**ステップ 2: `Config::default()` の更新**:
- `appearance` ブロックから `top_n_history` / `max_history_display` を削除
- `search` ブロックに `top_n_history: 200, max_history_display: 8` を追加

**ステップ 3: `apply_migrations()` にマイグレーション追加**:

既存の `migrate_additional_to_scan()` パターン（L712-741）に倣い、`apply_migrations()` 内に直接記述する。`AppearanceConfig` にはまだ `top_n_history` / `max_history_display` が存在するため、旧 TOML は自然にデシリアライズできる。その値を `search` へ移し、次回保存時に `[appearance]` キーは自動消去される（`skip_serializing` 不要）。

```rust
// apply_migrations() 内に追加
// 旧 [appearance].top_n_history / max_history_display → [search] へ移行
if self.appearance.top_n_history != default_top_n_history() {
    self.search.top_n_history = self.appearance.top_n_history;
    changed = true;
}
if self.appearance.max_history_display != default_max_history_display() {
    self.search.max_history_display = self.appearance.max_history_display;
    changed = true;
}
```

**注意**: デフォルト値のまま変更していない場合もマイグレーション後の初期値になるため、`!=` 判定で変更があった場合のみ移行する。

**ステップ 4: `AppearanceConfig` からフィールド削除**（最後のステップ）:

`engine.rs` / `main.rs` / `snotra-settings` の全参照を変更済みであることを `cargo check` で確認してから削除する。

**既存テストの更新**:
- `config.appearance.top_n_history` / `config.appearance.max_history_display` を参照しているテストアサーションを `config.search.*` に変更
- マイグレーションテストを追加: 旧形式 TOML（`[appearance]` セクションに `top_n_history = 300`）をパースした結果が正しく `search.top_n_history` に入ることを検証

#### 2. `snotra-core/src/engine.rs` — 参照変更（3箇所）

```rust
// L78: 変更
let fetch_limit = self.config.search.top_n_history;   // appearance → search

// L84: 変更
let max = self.config.search.max_history_display;     // appearance → search

// L92: 変更
max_results: self.config.search.top_n_history,        // appearance → search
```

#### 3. `src-tauri/src/main.rs` — `HistoryStore::load` 引数変更

```rust
// L332-334: 変更
let history = HistoryStore::load(
    config.search.top_n_history,       // appearance → search
    config.search.max_history_display, // appearance → search
);
```

#### 4. `snotra-settings/src/tabs/search.rs` — フィールド参照変更 + `max_history_display` UI 追加

```rust
// L45: フィールドパス変更
egui::DragValue::new(&mut config.search.top_n_history).range(10..=1000)
//                              ↑ appearance → search

// L45 の直後に max_history_display の行を追加
ui.end_row();
ui.label(tr.label_max_history_display());
ui.add_sized(
    [60.0, ui.spacing().interact_size.y],
    egui::DragValue::new(&mut config.search.max_history_display).range(1..=50),
);
ui.end_row();
```

#### 5. `snotra-settings/src/i18n.rs` — 翻訳キー追加

```rust
pub fn label_max_history_display(&self) -> &'static str {
    match self.0 {
        Language::Ja => "最大履歴表示件数:",
        Language::En => "Max history display:",
    }
}
```

#### 6. `snotra-settings/src/app.rs` — `has_changes()` の Visual タブ判定確認

`top_n_history` / `max_history_display` が `SearchConfig` に移動したため、`TabId::Visual` の `draft.appearance != saved.appearance` は `max_results` / `window_width` / `show_icons` のみを比較するようになり、自動的に正しくなる。**明示的な変更不要**。

`TabId::Search` の `draft.search != saved.search` は `SearchConfig` 全体の比較であり、移動後の `top_n_history` / `max_history_display` を自動的に含む。**明示的な変更不要**。

### 受け入れ条件

1. 旧形式 `config.toml`（`[appearance]` に `top_n_history = 300`）でアプリ起動 → Search タブの「最大列挙数」に 300 が表示される
2. `top_n_history` を Search タブで変更 → Search タブにダーティ `•` が表示される。Visual タブには `•` が表示されない
3. `max_history_display` の DragValue が Search タブに表示され、変更・保存が機能する
4. `cargo check -p snotra-core -p snotra -p snotra-settings` が通る
5. `cargo test -p snotra-core` が通る（マイグレーションテスト含む）

### 検証コマンド

```
cargo check -p snotra-core -p snotra -p snotra-settings
cargo test -p snotra-core
```

---

## Phase 5 — インデックス中の `open_settings` 空振りと `/o` エラーハンドリング（Bug #5 #6）

### 前提判断

Bug #5 を修正すると `open_settings` の戻り型の意味が変わり（`Ok(())` = 成功のみ、`Err` = インデックス中か起動失敗）、呼び出し元の `/o` コマンドがエラーを受け取るようになる。Bug #6 の修正（try-catch）はこれとセットで必要。

エラー時はステータスバー（または検索結果欄の下部）に一時メッセージを表示する（**案 B**）。

### 変更ファイル

- `src-tauri/src/commands/window.rs`
- `ui/src/lib/commands.ts`
- `ui/src/lib/i18n.ts`
- `ui/src/stores/search.ts`（`setLaunchNoticeWithAutoClear` の export 追加のみ）

### 変更内容

#### 1. `src-tauri/src/commands/window.rs` — `Err` を返すように変更

```rust
// 現状（L145-148）
if state.indexing.load(Ordering::SeqCst) {
    trace_command("cmd:open_settings:noop_indexing", json!({}));
    return Ok(());
}

// 修正後
if state.indexing.load(Ordering::SeqCst) {
    trace_command("cmd:open_settings:noop_indexing", json!({}));
    return Err("indexing_in_progress".to_string());
}
```

#### 2. `ui/src/lib/i18n.ts` — 翻訳キー追加

`TranslationKey` ユニオン型に新キーを追加し、`JA_JP` と `EN_US` の両テーブルに値を追加する。

```typescript
// TranslationKey に追加
| "notice.settings.unavailable_while_indexing"

// JA_JP テーブルに追加
"notice.settings.unavailable_while_indexing": "インデックス構築中のため、設定を開けません",

// EN_US テーブルに追加
"notice.settings.unavailable_while_indexing": "Cannot open settings while indexing.",
```

#### 3. `ui/src/stores/search.ts` — `setLaunchNoticeWithAutoClear` を export

現在は内部関数。`commands.ts` から呼べるよう `export` を追加する。

```typescript
// 変更前
function setLaunchNoticeWithAutoClear(message: string, delayMs = 2400) {

// 変更後
export function setLaunchNoticeWithAutoClear(message: string, delayMs = 2400) {
```

#### 4. `ui/src/lib/commands.ts` — try-catch + フィードバック表示

`action` の型は `() => void | Promise<void>`。エラーは Tauri IPC で文字列として伝達される。

```typescript
// 修正後
action: async () => {
  try {
    await api.openSettings();
  } catch (e) {
    const msg = String(e);
    if (msg.includes("indexing_in_progress")) {
      setLaunchNoticeWithAutoClear(
        t("notice.settings.unavailable_while_indexing"),
        3000,
      );
    }
    // 起動失敗等は既存の notice.launch.failed で対応済みのため何もしない
  }
},
```

`SearchWindow.tsx` は変更不要。`launchNotice` シグナルは既存の `<Show when={launchNotice()}>` で表示される。

### 受け入れ条件

1. インデックス構築中に `/o` Enter → 既存の `launchNotice` 表示領域に「インデックス構築中のため、設定を開けません」が 3 秒表示される
2. インデックス完了後に `/o` Enter → 設定ウィンドウが開く（従来動作）
3. `npm run typecheck` が通る
4. `npm run build` が通る

### 検証コマンド

```
cargo check -p snotra
npm run typecheck
npm run build
```

---

## 影響範囲サマリー

| ファイル | Phase 1 | Phase 2 | Phase 3 | Phase 4 | Phase 5 |
|---------|:-------:|:-------:|:-------:|:-------:|:-------:|
| `snotra-settings/src/app.rs` | ✅ | | ✅ | ✅ | |
| `snotra-settings/src/tabs/search.rs` | | | | ✅ | |
| `snotra-settings/src/i18n.rs` | | | | ✅ | |
| `snotra-core/src/config.rs` | | | | ✅ | |
| `snotra-core/src/engine.rs` | | | | ✅ | |
| `src-tauri/src/main.rs` | | | | ✅ | |
| `src-tauri/src/commands/window.rs` | | ✅ | | | ✅ |
| `ui/src/lib/commands.ts` | | | | | ✅ |
| `ui/src/lib/i18n.ts`（翻訳キー追加） | | | | | ✅ |
| `ui/src/stores/search.ts`（export 追加） | | | | | ✅ |

---

## 各フェーズの独立性

- **Phase 1, 2, 3** は互いに完全独立。並列開発可能
- **Phase 4** は Phase 1〜3 と独立だが、`config.rs` 変更が大きいため単独コミット推奨
- **Phase 5** は Phase 4 と独立。ただし Bug #5/#6 はセットで修正する

---

## 実装チェックリスト

### Phase 1 — `reset_to_default` / `Discard` 副作用リセット漏れ（Bug #2 #3 #4 #8）

**変更: `snotra-settings/src/app.rs`**
- [x] `reset_to_default()` に `self.tr = Tr(self.draft.general.language);` を追加
- [x] `reset_to_default()` に `self.hotkey_state = Default::default();` を追加
- [x] `reset_to_default()` に `self.index_state = tabs::index::IndexTabState::default();` を追加
- [x] `reset_to_default()` に `self.instant_state = tabs::instant::InstantTabState::default();` を追加
- [x] `reset_to_default()` に `self.opener_state = tabs::opener::OpenerTabState::new();` を追加
- [x] `Discard` ボタンのクリック処理に `self.tr = Tr(self.draft.general.language);` を追加

**検証**
- [x] `cargo check -p snotra-settings` が通る

---

### Phase 2 — 監視スレッドの `.expect()` パニックリスク（Bug #7）

**変更: `src-tauri/src/commands/window.rs`**
- [x] L98 の `.expect("SettingsProcessState not managed")` を `let Some(...) else { break; }` に変更

**検証**
- [x] `cargo check -p snotra` が通る

---

### Phase 3 — 最小化状態で閉じると古い位置が保存される（Bug #10）

**変更: `snotra-settings/src/app.rs`**
- [x] `outer_rect` の位置保存ブロック（L403-409）を `minimized` ガードで囲む

**検証**
- [x] `cargo check -p snotra-settings` が通る

---

### Phase 4 — `top_n_history` / `max_history_display` を `SearchConfig` へ移動（Bug #1 #9）

**変更: `snotra-core/src/config.rs`**
- [x] `SearchConfig` に `top_n_history: usize` フィールドを追加（`#[serde(default = "default_top_n_history")]`）
- [x] `SearchConfig` に `max_history_display: usize` フィールドを追加（`#[serde(default = "default_max_history_display")]`）
- [x] `Config::default()` の `search` ブロックに `top_n_history: 200, max_history_display: 8` を追加
- [x] `Config::default()` の `appearance` ブロックから `top_n_history`, `max_history_display` を削除
- [x] `apply_migrations()` に `appearance.top_n_history → search.top_n_history` の移行ロジックを追加
- [x] `apply_migrations()` に `appearance.max_history_display → search.max_history_display` の移行ロジックを追加
- [x] `AppearanceConfig` から `top_n_history` フィールドを削除（最後）
- [x] `AppearanceConfig` から `max_history_display` フィールドを削除（最後）

**変更: `snotra-core/src/engine.rs`**
- [x] L78 `self.config.appearance.top_n_history` → `self.config.search.top_n_history`
- [x] L84 `self.config.appearance.max_history_display` → `self.config.search.max_history_display`
- [x] L92 `self.config.appearance.top_n_history` → `self.config.search.top_n_history`

**変更: `src-tauri/src/main.rs`**
- [x] L333 `config.appearance.top_n_history` → `config.search.top_n_history`
- [x] L334 `config.appearance.max_history_display` → `config.search.max_history_display`

**変更: `snotra-settings/src/tabs/search.rs`**
- [x] L45 `config.appearance.top_n_history` → `config.search.top_n_history`
- [x] `max_history_display` の DragValue 行を `top_n_history` 行の直後に追加（`ui.label(tr.label_max_history_display())` + `DragValue::new(&mut config.search.max_history_display).range(1..=50)`）

**変更: `snotra-settings/src/i18n.rs`**
- [x] `label_max_history_display()` メソッドを追加（Ja: `"最大履歴表示件数:"`, En: `"Max history display:"`）

**テスト: `snotra-core`**
- [x] `config.appearance.top_n_history` / `config.appearance.max_history_display` を参照している既存テストを `config.search.*` に変更
- [x] マイグレーションテストを追加: 旧形式 TOML（`[appearance]` に `top_n_history = 300`）をパースした結果が `search.top_n_history == 300` になることを検証

**検証**
- [x] `cargo check -p snotra-core -p snotra -p snotra-settings` が通る
- [x] `cargo test -p snotra-core` が通る

---

### Phase 5 — インデックス中の `open_settings` 空振りと `/o` エラーハンドリング（Bug #5 #6）

**変更: `src-tauri/src/commands/window.rs`**
- [x] インデックス中の早期リターンを `return Ok(())` から `return Err("indexing_in_progress".to_string())` に変更

**変更: `ui/src/lib/i18n.ts`**
- [x] `TranslationKey` ユニオン型に `"notice.settings.unavailable_while_indexing"` を追加
- [x] `JA_JP` テーブルに `"notice.settings.unavailable_while_indexing": "インデックス構築中のため、設定を開けません"` を追加
- [x] `EN_US` テーブルに `"notice.settings.unavailable_while_indexing": "Cannot open settings while indexing."` を追加

**変更: `ui/src/stores/search.ts`**
- [x] `setLaunchNoticeWithAutoClear` 関数に `export` を追加

**変更: `ui/src/lib/commands.ts`**
- [x] `openSettings` の `action` を try-catch で囲む
- [x] catch 内で `String(e).includes("indexing_in_progress")` を判定し、`setLaunchNoticeWithAutoClear(t("notice.settings.unavailable_while_indexing"), 3000)` を呼ぶ

**検証**
- [x] `cargo check -p snotra` が通る
- [x] `npm run typecheck` が通る
- [x] `npm run build` が通る

---

## 決定済み事項

| 項目 | 決定内容 |
|------|--------|
| Phase 4: A案 vs B案 | **A案**（`SearchConfig` への構造移動）を採用 |
| Phase 5: フィードバック方針 | **案 B**（ステータスバーへの一時メッセージ表示）を採用 |
| `max_history_display` の UI 追加 | **Phase 4 とセットで実施** |
