# 実装計画: Issue #265 — 自動アップデート機能（Windows）

## 変更ファイル一覧

| # | ファイル | 変更内容 |
|---|---------|---------|
| 1 | `snotra-core/src/config.rs` | `GeneralConfig` に `auto_update_enabled: bool` 追加（default: true） |
| 2 | `src-tauri/Cargo.toml` | `tauri-plugin-updater = "2"` 追加 |
| 3 | `src-tauri/tauri.conf.json` | `bundle.targets = ["nsis"]` + `plugins.updater` セクション追加 |
| 4 | `src-tauri/capabilities/main.json` | `"updater:default"` 権限追加 |
| 5 | `src-tauri/src/main.rs` | updater プラグイン登録 + startup 更新チェック spawn |
| 6 | `src-tauri/src/commands/updater.rs` | `install_update` コマンド（新規） |
| 7 | `src-tauri/src/commands/mod.rs` | `updater` モジュール追加・再エクスポート |
| 8 | `ui/src/lib/i18n.ts` | `update.*` TranslationKey 追加 |
| 9 | `ui/src/lib/invoke.ts` | `installUpdate()` IPC ラッパー追加 |
| 10 | `ui/src/lib/types.ts` | 必要に応じて型追加 |
| 11 | `ui/src/components/UpdateToast.tsx` | トーストコンポーネント（新規） |
| 12 | `ui/src/MainApp.tsx` | `update-available` listen + トースト表示シグナル + 高さ計算更新 |
| 13 | `snotra-settings/src/i18n.rs` | `cb_auto_update()` メソッド追加 |
| 14 | `snotra-settings/src/tabs/general.rs` | 自動更新チェックボックス追加 |
| 15 | `.github/workflows/release.yml` | NSIS ビルド + 署名 + latest.json 生成・アップロード |
| 16 | `SPEC.md` | 自動更新仕様セクション追加 |
| 17 | `src-tauri/CLAUDE.md` | `commands/updater.rs` をモジュール構成に追記 |

合計: 17 ファイル（新規 2 件を含む）

## 実装順序

### フェーズ 1: Config 層（土台）

**1a. `snotra-core/src/config.rs`**

`GeneralConfig` に追加:
```rust
fn default_auto_update_enabled() -> bool {
    true
}

// GeneralConfig 構造体
#[serde(default = "default_auto_update_enabled")]
pub auto_update_enabled: bool,
```

`Default::default()` の明示的初期化にも `auto_update_enabled: true` を追加。

不変条件:
- `serde(default)` により既存の `config.toml` に項目がなくても `true` でデシリアライズされる
- `Config::default()` でも `true` になる

### フェーズ 2: Tauri バックエンド

**2a. `src-tauri/Cargo.toml`**
```toml
tauri-plugin-updater = "2"
```

**2b. `src-tauri/tauri.conf.json`**

`bundle` セクションに `targets` を追加（NSIS ビルドを有効化）:
```json
"bundle": {
  "targets": ["nsis"],
  ...
}
```

`plugins` セクションを新規追加:
```json
"plugins": {
  "updater": {
    "pubkey": "PLACEHOLDER_REPLACE_WITH_ACTUAL_PUBKEY",
    "endpoints": [
      "https://github.com/finelagusaz/Snotra/releases/latest/download/latest.json"
    ],
    "windows": {
      "installMode": "passive"
    }
  }
}
```

> **注意**: `pubkey` は実際のキーペア生成後に差し替える必要がある。
> 生成: `npx tauri signer generate -w tauri-update.key`
> 出力された公開鍵を `pubkey` に設定し、秘密鍵を GitHub Secrets の `TAURI_SIGNING_PRIVATE_KEY` に追加。

**2c. `src-tauri/capabilities/main.json`**
```json
"permissions": [
  ...既存権限...,
  "updater:default"
]
```

**2d. `src-tauri/src/commands/updater.rs`（新規）**
```rust
use tauri::{AppHandle, command};
use tauri_plugin_updater::UpdaterExt;

#[command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    if let Some(update) = update {
        update
            .download_and_install(|_chunk, _total| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        app.restart();
    }
    Ok(())
}
```

**2e. `src-tauri/src/commands/mod.rs`**
```rust
pub mod updater;
pub use updater::*;
```

**2f. `src-tauri/src/main.rs`**

プラグイン登録:
```rust
.plugin(tauri_plugin_updater::init())
```

setup クロージャ末尾に更新チェック spawn（`auto_update_enabled` を読んでから起動）:
```rust
let auto_update_enabled = {
    let state = app.state::<AppState>();
    let engine = state.engine.lock().unwrap();
    engine.config().general.auto_update_enabled
};
if auto_update_enabled {
    let handle = app.handle().clone();
    tokio::spawn(async move {
        use tauri_plugin_updater::UpdaterExt;
        let updater = match handle.updater() {
            Ok(u) => u,
            Err(e) => { eprintln!("[updater] init failed: {e}"); return; }
        };
        match updater.check().await {
            Ok(Some(update)) => {
                handle.emit("update-available", update.version.to_string()).ok();
            }
            Ok(None) => {}
            Err(e) => eprintln!("[updater] check failed: {e}"),
        }
    });
}
```

`invoke_handler` に `install_update` を追加:
```rust
tauri::generate_handler![
    ...,
    commands::install_update,
]
```

### フェーズ 3: フロントエンド

**3a. `ui/src/lib/i18n.ts`**

`TranslationKey` に追加:
```typescript
| "update.available"    // "v{version} が利用可能です"
| "update.install_now" // "今すぐ更新"
| "update.later"       // "後で"
| "update.installing"  // "インストール中..."
```

翻訳辞書に追加（JA/EN 両方）。

**3b. `ui/src/lib/invoke.ts`**
```typescript
export async function installUpdate(): Promise<void> {
  return invoke("install_update");
}
```

**3c. `ui/src/components/UpdateToast.tsx`（新規）**

コンポーネント仕様:
- Props: `{ version: string; installing: boolean; onInstall: () => void; onDismiss: () => void }`
- 高さ: 32px 固定（MainApp.tsx の高さ計算と一致させる）
- レイアウト: 左側にバージョンラベル、右側に「今すぐ更新」「後で」ボタン
- `installing` が true のとき: ボタンを無効化してローディング表示
- テーマ: 既存 CSS 変数（`--bg-primary`, `--text-primary` 等）を使用

**3d. `ui/src/MainApp.tsx`**

シグナル追加:
```typescript
const UPDATE_TOAST_HEIGHT = 32;
const [updateVersion, setUpdateVersion] = createSignal<string | null>(null);
const [updaterInstalling, setUpdaterInstalling] = createSignal(false);
```

`onMount` 内に listen 追加:
```typescript
const unlistenUpdate = await listen<string>("update-available", (event) => {
  setUpdateVersion(event.payload);
});
unlistenFns.push(unlistenUpdate);
```

インストールハンドラ:
```typescript
const handleUpdateInstall = async () => {
  setUpdaterInstalling(true);
  try {
    await api.installUpdate();
    // アプリ再起動するためここには到達しない
  } catch (e) {
    console.error("Update install failed:", e);
    setUpdaterInstalling(false);
  }
};
```

ウィンドウ高さの `createEffect` 更新（既存 1 箇所のみ変更）:
```typescript
createEffect(() => {
  const show = shouldShowResults();
  const width = cachedWidth();
  const toast = updateVersion() !== null ? UPDATE_TOAST_HEIGHT : 0;
  const height =
    (show ? SEARCH_BAR_HEIGHT + maxResults() * RESULT_ROW_HEIGHT + RESULTS_PADDING : SEARCH_BAR_HEIGHT)
    + toast;
  void win.setSize(new LogicalSize(width, height));
});
```

JSX にトーストを追加（`SearchWindow` の下、`ResultsSection` の上）:
```tsx
<Show when={updateVersion() !== null}>
  <UpdateToast
    version={updateVersion()!}
    installing={updaterInstalling()}
    onInstall={handleUpdateInstall}
    onDismiss={() => setUpdateVersion(null)}
  />
</Show>
```

### フェーズ 4: 設定 UI

**4a. `snotra-settings/src/i18n.rs`**
```rust
pub fn cb_auto_update(&self) -> &'static str {
    match self.0 {
        Language::Ja => "新バージョンを自動で確認・更新する",
        Language::En => "Automatically check for and apply updates",
    }
}
```

**4b. `snotra-settings/src/tabs/general.rs`**

`// -- Behavior --` セクションの既存 checkbox 群の末尾に追加:
```rust
ui.checkbox(
    &mut config.general.auto_update_enabled,
    tr.cb_auto_update(),
);
```

### フェーズ 5: CI パイプライン

**5a. `.github/workflows/release.yml`**

変更点:
1. `npx tauri build --no-bundle` → `npx tauri build --bundles nsis`
2. 環境変数追加: `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（GitHub Secrets から）
3. NSIS インストーラと `.sig` ファイルをリリースにアップロード
4. `latest.json` を PowerShell で生成してアップロード

`latest.json` 生成（PowerShell）:
```powershell
$ver = "${{ env.TAG_NAME }}".TrimStart('v')
$installerPath = Get-ChildItem "target/release/bundle/nsis" -Filter "*_x64-setup.exe" | Select-Object -First 1
$sigPath = "$($installerPath.FullName).sig"
$sig = Get-Content $sigPath -Raw
$installerName = $installerPath.Name

$json = @{
  version  = $ver
  notes    = ""
  pub_date = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
  platforms = @{
    "windows-x86_64" = @{
      signature = $sig.Trim()
      url       = "https://github.com/finelagusaz/Snotra/releases/download/${{ env.TAG_NAME }}/$installerName"
    }
  }
} | ConvertTo-Json -Depth 5

$json | Set-Content "latest.json" -Encoding utf8
```

リリースアップロードの `files` に追加:
```yaml
files: |
  Snotra-${{ env.TAG_NAME }}.zip
  target/release/bundle/nsis/*_x64-setup.exe
  target/release/bundle/nsis/*_x64-setup.exe.sig
  latest.json
```

> **ユーザーに必要な手動作業（CI 設定前に実施）:**
> 1. `npx tauri signer generate -w tauri-update.key` でキーペア生成
> 2. 出力された公開鍵を `tauri.conf.json` の `plugins.updater.pubkey` に設定
> 3. 秘密鍵を GitHub Secrets `TAURI_SIGNING_PRIVATE_KEY` に登録
> 4. パスフレーズを GitHub Secrets `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` に登録

### フェーズ 6: ドキュメント

**6a. `SPEC.md`** — 自動更新セクション追加

**6b. `src-tauri/CLAUDE.md`** — `commands/updater.rs` をモジュール構成に追記

## 不変条件

| 条件 | 検証方法 |
|------|---------|
| `auto_update_enabled` の serde default が `true` | 既存 config.toml に項目がない状態でデシリアライズしても `true` |
| `set_size()` の呼び出しは createEffect 1箇所のみ | コードレビュー |
| 更新チェック失敗時はサイレント（eprintln のみ） | エラーをユーザーに表示しない（起動を妨げない） |
| トースト表示中も検索は正常動作 | 手動確認 |
| 更新通知はセッションに 1 回のみ | startup spawn が 1 回だけ emit する |

## テスト方針

- Rust: `cargo check -p snotra-core -p snotra -p snotra-settings`（フェーズ 1, 2 後）
- TS: `npm run typecheck` + `npm run build`（フェーズ 3, 4 後）
- 手動確認:
  - 設定 UI に「自動更新」チェックボックスが表示される
  - チェックをオフにして保存後、再起動してもチェックが外れたままである
  - （updater フル動作確認は署名済みリリース後に実施）

## セルフレビュー

### 対称コードパス
- トースト表示（`setUpdateVersion`）に対し、dismissal（`null` セット）が両パスで機能する: ✅
- `update-available` listen に対し `unlistenFns.push()` でクリーンアップ: ✅

### 影響範囲の網羅性
- `GeneralConfig` 新フィールドを `Default::default()` にも反映: ✅ フェーズ 1 で明示
- `install_update` を `invoke_handler` に追加: ✅ フェーズ 2f で明示
- ウィンドウ高さ計算: ✅ `toast` 変数を既存 createEffect に組み込む

### 境界条件
- 更新チェック失敗（ネットワークなし）: eprintln のみ、ユーザー操作に影響なし ✅
- `install_update` 時に既に最新版だった場合: `Ok(None)` → コマンドは `Ok(())` を返す。トーストは `installing` 状態で止まるが再起動しない（発生頻度が極めて低いため許容）
- プレリリース版: `releases/latest` は非プレリリースのみ解決するため問題なし ✅

### リソース管理
- `unlistenUpdate` は `onCleanup` 経由で解放: ✅ `unlistenFns.push()` パターン
- `tokio::spawn` タスクは spawn-and-forget で自動回収: ✅

### シンプル化の挑戦
- `Update<R>` オブジェクトを Managed State に保存しない（型パラメータ複雑）→ `install_update` で再チェック: ✅ 採用
- トーストシグナルは `MainApp.tsx` に置く（不必要な store 作成を避ける）: ✅

### 破壊不変条件
- `set_size()` を createEffect 以外から呼ばない: コードレビューで確認
- 秘密鍵をリポジトリにコミットしない: ✅ GitHub Secrets のみに保管
