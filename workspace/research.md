# WebView2 パフォーマンス最適化 — 調査結果

参考: [WebView2 アプリのパフォーマンスのベスト プラクティス](https://learn.microsoft.com/ja-jp/microsoft-edge/webview2/concepts/performance)

## 記事の主要テクニックと Snotra への適用可否

### 実装済み（変更不要）

| 記事の推奨事項                       | Snotra の現状                                                    |
|--------------------------------------|------------------------------------------------------------------|
| 冗長な WebView2 インスタンスを避ける | ウィンドウは1つ（main）、show/hide で制御。create/destroy しない |
| Web コンテンツの IPC 最小化          | アイコンはバッチ API + バイナリエンコード (`commands/icon.rs`)   |
| ハードウェアアクセラレーション有効   | デフォルトのまま（`disable-gpu` 未使用）                         |
| メモリリーク防止（Blob URL）         | LRU 200件 + `revokeObjectURL` (`lruIconCache.ts`)                |
| Web コンテンツ最適化                 | 検索デバウンス 150ms、2フェーズアイコンロード                    |
| 初期ペイロード軽量化                 | SolidJS（軽量）、`esnext` ターゲット                             |
| プロセス優先順位管理                 | Rust 側で rayon 並列化、UI スレッド非ブロック                    |
| WebView2 環境の共有                  | WebView2 インスタンスが1つなので自明に達成                       |
| 背景色同期（白フラッシュ防止）       | `config_watcher::sync_webview_background` で実装済み             |

### 適用可能な最適化候補

#### 1. TrySuspendAsync / Resume（優先度: 高）

**記事の推奨**: WebView2 インスタンスがしばらく使用されない場合、`TrySuspendAsync()` でレンダラープロセスを中断。`Resume()` で再開。

**Snotra への適合性**: ランチャーは表示時間が数秒、非表示時間が大半。非表示中にレンダラーを中断すれば CPU/メモリを大幅削減できる。

**実装方針**:
- hide 時: `with_webview()` 経由で WebView2 COM API `TrySuspend()` を呼ぶ
- show 時: `Resume()` で復帰
- `TrySuspend` はベストエフォート（失敗しても問題なし）
- WebView2 COM インターフェース: `ICoreWebView2_3::TrySuspendAsync`

**調査事項**:
- [ ] Tauri v2 の `with_webview()` で `ICoreWebView2_3` にアクセスできるか
- [ ] `webview2-com` クレートに `TrySuspend` / `Resume` のバインディングがあるか
- [ ] Suspend 中に IPC（Tauri の `emit` / `invoke`）が正常にキューイングされるか

**リスク**:
- Suspend → Resume の遷移に数十 ms のラグがある可能性。show 直後の検索応答性への影響を計測する必要あり
- Suspend 中に `emit("window-shown")` が届かない場合、フロントエンドの初期化シーケンスが崩れる

#### 2. MemoryUsageTargetLevel（優先度: 高）

**記事の推奨**: 非アクティブな WebView に `MemoryUsageTargetLevel = Low` を設定。キャッシュ削除やメモリスワップをブラウザーエンジンに要求。アクティブ時は `Normal` に復帰。

**Snotra への適合性**: TrySuspend と組み合わせて非表示中のメモリフットプリントを最小化。TrySuspend が使えない場合の代替手段としても有効。

**実装方針**:
- hide 時: `put_MemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW)`
- show 時: `put_MemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL)`
- WebView2 COM インターフェース: `ICoreWebView2_19::put_MemoryUsageTargetLevel`

**調査事項**:
- [ ] `ICoreWebView2_19` は現在の WebView2 ランタイムでサポートされているか（比較的新しい API）
- [ ] `webview2-com` クレートのバージョンが対応しているか

**リスク**:
- `Low` → `Normal` 復帰時のキャッシュ再構築コストが show 時のレイテンシに影響する可能性
- ランタイムバージョンが古い環境では `QueryInterface` が失敗する → フォールバック必要

#### 3. 初期 HTML の静的化（優先度: 中）

**記事の推奨**: 初期ペイロードは静的 HTML を先に表示し、JS フレームワークは後から。HTML の読み込み・解析・レンダリングは JS による UI 生成より高速。

**Snotra への適合性**: 現在 `main.html` → Vite バンドル → SolidJS マウントの流れ。検索バーのシェル（input 要素 + スタイル）をインライン HTML として直接記述すれば、SolidJS 初期化完了前に検索バーが表示される。

**実装方針**:
- `main.html` に検索バーの HTML 構造を直接記述（`<input>` + 最小限の CSS）
- SolidJS がマウントされたらハイドレーション or 置換
- JS ロード前でもユーザーに「準備完了」の印象を与えられる

**リスク**:
- SolidJS はハイドレーションを標準サポートしていない（SSR 向け機能はある）
- HTML 直書きと SolidJS コンポーネントの二重管理コスト
- Snotra は hotkey トグルで表示するため、コールドスタートは初回のみ → 効果は初回限定

#### 4. ユーザーデータフォルダ (UDF) の明示指定（優先度: 低）

**記事の推奨**: UDF をローカルの高速ドライブに配置。ネットワーク共有や低速ドライブを避ける。

**Snotra への適合性**: 現在は Tauri デフォルト（`%LOCALAPPDATA%` 配下）。大半の環境では問題ないが、企業環境で `%LOCALAPPDATA%` が OneDrive やネットワークドライブにリダイレクトされているケースで起動遅延が発生しうる。

**実装方針**:
- `tauri.conf.json` または Rust setup で UDF パスを明示指定
- Tauri v2 では `WebviewWindowBuilder` の設定で制御可能か要調査

**リスク**: ほぼなし。ただし効果が限定的。

#### 5. バンドルサイズの計測と分割（優先度: 低）

**記事の推奨**: 初期ペイロードを減らし、重いコンポーネントを遅延読み込み。

**実装方針**:
- まず `npm run build` 後のバンドルサイズを計測
- Vite の `rollup-plugin-visualizer` で内訳を可視化
- 必要に応じて `manualChunks` で分割

**前提**: SolidJS + 小規模 UI なのでバンドルは小さいはず。計測結果次第で判断。

## 適用しない項目

| 記事の推奨事項                  | 理由                                               |
|---------------------------------|----------------------------------------------------|
| Service Worker によるキャッシュ | ローカルファイルのみ使用。ネットワークリソースなし |
| 複数 WebView2 の環境共有        | WebView2 インスタンスが1つのみ                     |
| アプリレベルのプロセス共有      | 単一アプリ。他アプリとの共有不要                   |
| ホストオブジェクトの最適化      | `AddHostObjectToScript` 未使用（Tauri IPC を使用） |
| DevTools 無効化                 | リリースビルドではデフォルトで無効                 |

## 実装計画（優先順）

| # | 施策                     | 効果                       | 実装コスト | 依存関係                      |
|---|--------------------------|----------------------------|------------|-------------------------------|
| 1 | TrySuspend / Resume      | メモリ・CPU 大幅削減       | 中         | WebView2 COM API アクセス調査 |
| 2 | MemoryUsageTargetLevel   | メモリ削減                 | 中         | ICoreWebView2_19 対応確認     |
| 3 | 初期 HTML 静的化         | コールドスタート体感改善   | 小〜中     | SolidJS ハイドレーション調査  |
| 4 | UDF 明示指定             | 特殊環境での起動改善       | 小         | Tauri v2 API 調査             |
| 5 | バンドルサイズ計測・分割 | 初期ロード改善（効果未知） | 小         | 計測から                      |

## webview2-com API 調査結果

### バージョン

- `webview2-com`: **0.38.2**（`src-tauri/Cargo.toml` で `"0.38"` 指定）
- `webview2-com-sys`: **0.38.2**（transitive dependency）

### TrySuspend / Resume — ✅ 利用可能

`ICoreWebView2_3`（Edge 86+ 必須）に定義。`webview2-com-sys` にバインディングあり。

```
インターフェース継承: ICoreWebView2_3 : ICoreWebView2_2 : ICoreWebView2 : IUnknown
```

```rust
// TrySuspend: 非同期（コールバック付き）
pub unsafe fn TrySuspend<P0>(&self, handler: P0) -> windows_core::Result<()>
where P0: windows_core::Param<ICoreWebView2TrySuspendCompletedHandler>;

// Resume: 同期
pub unsafe fn Resume(&self) -> windows_core::Result<()>;

// 中断状態の確認
pub unsafe fn IsSuspended(&self, issuspended: *mut BOOL) -> windows_core::Result<()>;
```

高レベルラッパー: `webview2_com::TrySuspendCompletedHandler::create(Box<dyn FnOnce(Result<()>, bool)>)`

### MemoryUsageTargetLevel — ✅ 利用可能

`ICoreWebView2_19`（Edge 114+ 必須）に定義。sys バインディングのみ（高レベルラッパーなし、不要）。

```
インターフェース継承: ICoreWebView2_19 : ... : ICoreWebView2_3 : ICoreWebView2 : IUnknown
```

```rust
pub unsafe fn SetMemoryUsageTargetLevel(
    &self, value: COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL,
) -> windows_core::Result<()>;

pub unsafe fn MemoryUsageTargetLevel(
    &self, value: *mut COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL,
) -> windows_core::Result<()>;
```

定数:
- `COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL` = 0
- `COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW` = 1

### COM アクセスパターン（既存コードから）

`main.rs:427-458` の AcceleratorKeyPressed ハンドラが参考になる:

```rust
// setup フェーズで with_webview() → controller() を取得
main.with_webview(move |platform_webview| {
    let controller = platform_webview.controller();
    // controller から ICoreWebView2 を取得し、cast で上位インターフェースへ
});
```

**ICoreWebView2_3 への到達**:
```rust
use windows::core::Interface;
let webview: ICoreWebView2_3 = unsafe { controller.CoreWebView2()?.cast()? };
```

**ICoreWebView2_19 への到達**:
```rust
let webview: ICoreWebView2_19 = unsafe { controller.CoreWebView2()?.cast()? };
```

### 重要な制約

1. **`with_webview()` は setup フェーズでのみ安全**。イベントループやIPCハンドラからはデッドロック
2. **ランタイム hide/show で呼ぶには**: setup 時に COM インターフェースをキャプチャし、`Arc` / マネージドステートで保持する必要がある
3. **`cast()` は失敗しうる**: WebView2 ランタイムが古い場合 `QueryInterface` が `E_NOINTERFACE` を返す → graceful fallback 必須
4. **追加の Cargo feature flag は不要**: `webview2-com = "0.38"` で全インターフェースにアクセス可能

### 調査事項の更新

- [x] `with_webview()` で `ICoreWebView2_3` にアクセスできるか → **可能**（`controller.CoreWebView2()?.cast()?`）
- [x] `webview2-com` に `TrySuspend` / `Resume` のバインディングがあるか → **あり**（+ 高レベルコールバックラッパーも提供）
- [x] `ICoreWebView2_19` が `webview2-com` でサポートされているか → **あり**（0.38.2 で対応済み）
- [ ] Suspend 中に IPC（Tauri の `emit` / `invoke`）が正常にキューイングされるか → **要実機検証**

## 実装方針（確定）

### アーキテクチャ

setup フェーズで COM インターフェースをキャプチャし、マネージドステートとして保持:

```rust
// 新しいマネージドステート
pub struct WebView2Perf {
    /// ICoreWebView2_3 (TrySuspend/Resume) — Edge 86+, ほぼ確実に利用可能
    webview3: Option<ICoreWebView2_3>,
    /// ICoreWebView2_19 (MemoryUsageTargetLevel) — Edge 114+, 古い環境では None
    webview19: Option<ICoreWebView2_19>,
}
```

### hide 時の処理順序

```
1. MemoryUsageTargetLevel → Low  (webview19 があれば)
2. TrySuspend()                  (webview3 があれば)
```

### show 時の処理順序

```
1. Resume()                      (webview3 があれば)
2. MemoryUsageTargetLevel → Normal (webview19 があれば)
3. (既存の show_main_and_emit 処理)
```

### フォールバック戦略

| 条件 | 動作 |
|---|---|
| Edge 114+ | TrySuspend + MemoryUsageTargetLevel 両方有効 |
| Edge 86〜113 | TrySuspend のみ有効 |
| Edge < 86 | 最適化なし（従来通り動作） |

## 次のステップ

1. ~~`webview2-com` クレートの API サーフェスを調査~~ → 完了
2. hide/show フックに suspend/resume ロジックを組み込むプロトタイプを作成
3. タスクマネージャーで非表示時のメモリ使用量を before/after 比較
