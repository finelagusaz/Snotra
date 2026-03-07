# plan: issue #159 — ホットキー登録失敗時のユーザー通知

## 設計方針

- **新規イベント `hotkey-registration-failed`** を統一窓口とする:
  - 初回失敗: 既存 `platform-event: "initial-hotkey-failed"` を維持（ウィンドウ表示用）+ 新イベントを追加 emit
  - 設定変更失敗: `eprintln!` に加え新イベントを emit
- **ペイロード型**: `{ hotkey: String, is_initial: bool }` を `serde_json::json!()` で構成
- **フロントエンド通知**: `setHotkeyFailureNotice(msg)` を `search.ts` に追加 export し、`MainApp.tsx` で listen

## 変更ファイル一覧（6ファイル）

| ファイル | 変更内容 |
|---------|---------|
| `src-tauri/src/platform/mod.rs` | `RegisterInitialHotkey` 失敗時に `hotkey-registration-failed` を追加 emit |
| `src-tauri/src/config_watcher.rs` | `Ok(false)\|Err(_)` ブランチで `hotkey-registration-failed` を emit |
| `ui/src/lib/i18n.ts` | `notice.hotkey.initial_failed` / `notice.hotkey.change_failed` キーを追加 |
| `ui/src/stores/search.ts` | `setLaunchNoticeWithAutoClear` に timeout 引数追加。`setHotkeyFailureNotice` を追加 export |
| `ui/src/MainApp.tsx` | `hotkey-registration-failed` リスナーを追加 |
| `SPEC.md` | §9.2 line 393 を2行に分割・詳細化 |

## 実装順序

### Phase 1: Rust — イベント emit を追加

**`src-tauri/src/platform/mod.rs`** — `RegisterInitialHotkey` ブランチ:
```rust
PlatformCommand::RegisterInitialHotkey => {
    if !hotkey::register(current_hotkey) {
        let _ = app_handle.emit("platform-event", "initial-hotkey-failed");
        // 新規追加
        let hotkey_str = format!("{}+{}", current_hotkey.modifier, current_hotkey.key);
        let _ = app_handle.emit("hotkey-registration-failed", serde_json::json!({
            "hotkey": hotkey_str,
            "is_initial": true,
        }));
    }
}
```

**`src-tauri/src/config_watcher.rs`** — `Ok(false) | Err(_)` ブランチ:
```rust
Ok(false) | Err(_) => {
    eprintln!("[config-watcher] hotkey registration failed: {} + {}",
        new_config.hotkey.modifier, new_config.hotkey.key);
    // 新規追加
    let hotkey_str = format!("{}+{}", new_config.hotkey.modifier, new_config.hotkey.key);
    let _ = app.emit("hotkey-registration-failed", serde_json::json!({
        "hotkey": hotkey_str,
        "is_initial": false,
    }));
}
```

> `serde_json` は Tauri が推移的に依存しており追加不要。

### Phase 2: フロントエンド — i18n キー追加

**`ui/src/lib/i18n.ts`**:

```ts
// TranslationKey union に追加
| "notice.hotkey.initial_failed"
| "notice.hotkey.change_failed"

// JA_JP レコードに追加
"notice.hotkey.initial_failed": "ホットキー ({hotkey}) の登録に失敗しました。他のアプリが使用中の可能性があります",
"notice.hotkey.change_failed": "ホットキー ({hotkey}) の登録に失敗しました。元のホットキーを維持します",
```

### Phase 3: フロントエンド — search.ts に通知関数を追加

**`ui/src/stores/search.ts`**:
```ts
// timeout 引数を追加（デフォルト値で既存呼び出しは無変更）
function setLaunchNoticeWithAutoClear(message: string, delayMs = 2400) {
  clearLaunchNotice();
  setLaunchNotice(message);
  launchNoticeTimer = setTimeout(() => {
    launchNoticeTimer = undefined;
    setLaunchNotice(null);
  }, delayMs);
}

// 新規 export: ホットキー登録失敗通知（5秒表示）
export function setHotkeyFailureNotice(message: string) {
  setLaunchNoticeWithAutoClear(message, 5000);
}
```

### Phase 4: フロントエンド — MainApp.tsx にリスナー追加

**`ui/src/MainApp.tsx`**:
```ts
// import に追加
import { setHotkeyFailureNotice } from "./stores/search";

// listen ブロック内に追加
listen<{ hotkey: string; is_initial: boolean }>("hotkey-registration-failed", (event) => {
  const { hotkey, is_initial } = event.payload;
  const msg = is_initial
    ? t("notice.hotkey.initial_failed", { hotkey })
    : t("notice.hotkey.change_failed", { hotkey });
  setHotkeyFailureNotice(msg);
}),
```

`unlisten` 配列（`unlistenFns`）にも必ず追加する。

### Phase 5: SPEC.md 更新

line 393 を分割:
```
- 初回ホットキー登録失敗時は操作不能回避のため検索UIを表示し、ウィンドウ内にエラー通知を表示する
- 設定変更によるホットキー登録失敗時は旧ホットキーに復帰し、ウィンドウ内に一時エラー通知を表示する
```

## 不変条件

1. `platform-event: "initial-hotkey-failed"` の既存ペイロード型（`string`）・ハンドラを変更しない
2. `hotkey-registration-failed` リスナーは `unlistenFns` に push して `onCleanup` で unlisten する
3. `setHotkeyFailureNotice` は `setLaunchNoticeWithAutoClear` 内の `clearLaunchNotice()` 先頭呼び出しを通じて、タイマーを単一管理する（競合防止）
4. `format!("{}+{}", modifier, key)` は UI 表示文字列ではなくホットキー識別子。snotra-core の「UI 文字列を持たない」原則には抵触しない（これは src-tauri 内のコード）

## テスト方針

- `cargo check -p snotra-core -p snotra -p snotra-settings`: Rust 型チェック
- `npm run typecheck` + `npm run build`: i18n キー・型変更の検証
- 手動確認: 別アプリで同じホットキーを専有した状態で Snotra を起動 → エラー通知が表示されることを確認

## SPEC.md 更新要否

あり（Phase 5 で対処）。

---

## セルフレビュー

### 1. 対称コードパス確認

- `hotkey-pressed` / `hotkey-registration-failed`: 対称ペアに相当するが、`hotkey-pressed` は既存で変更しない → 影響なし
- `platform-event` の既存ハンドラ（window show ロジック）はそのまま維持 → ウィンドウ表示 + 通知の両立

### 2. 影響範囲の網羅性

- `platform-event` ペイロード型は `string` のまま変更しない → 既存リスナーへの破壊的変更なし
- `hotkey-registration-failed` は新規イベント → 既存コードへの影響ゼロ
- `setLaunchNoticeWithAutoClear` に引数追加（デフォルト値あり）→ 既存の2呼び出し箇所（line ~430, ~532）は変更不要

### 3. 境界条件

- `hotkey-registration-failed` が `main` ウィンドウ不可視時に届いた場合: `launchNotice` は設定されるが表示されない。`resetForShow()` 時に `clearLaunchNotice()` で消えるため残留しない。初回失敗時はウィンドウ表示と同時なので問題なし。設定変更失敗時はウィンドウが可視（ユーザーが設定を変更したばかり）→ 問題なし。
- `format!("{}+{}", modifier, key)` で生成する識別子: "Alt+Q" のような短い文字列。i18n テンプレートの `{hotkey}` に埋め込まれる。

### 4. リソース管理

- `listen()` の戻り値（unlisten 関数）を `unlistenFns` 配列に追加する → `onCleanup` で自動破棄

### 5. 既存パターンとの整合

- イベントリスナー + i18n + `setLaunchNoticeWithAutoClear` の組み合わせは `notice.launch.*` で確立済みパターン。新パターン不要。

### 6. YAGNI 違反

- 2種類のメッセージを1イベント + `is_initial` フラグで賄う。イベントを2種類に分けない → シンプル。
- `setHotkeyFailureNotice` のタイムアウト 5000ms は固定値。設定化不要。

### 7. シンプル化の挑戦

`serde_json::json!()` マクロを使う。匿名 struct + `#[derive(Serialize)]` より軽量で、局所的な1回きりの用途に適する。新型を追加しない選択は YAGNI に沿っている。

### 8. 破壊不変条件の明示

- `platform-event: "initial-hotkey-failed"` のウィンドウ表示ロジックを維持 → 「ホットキー登録失敗時でも UI は必ず操作可能」の不変条件を維持。
- `hotkey-registration-failed` リスナーの unlisten 漏れがあると、ウィンドウ再マウント時にダブルリスナーになる。`unlistenFns` への追加で防止。
