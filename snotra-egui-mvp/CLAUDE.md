# snotra-egui-mvp

Issue #532の採用判断に使う、WebViewなしの独立MVPバイナリ。

## モジュール構成

- `src/main.rs`: 10,000件の実`Engine`検索、Rust Updater確認、採用条件の実機プローブを持つegui画面
- `src/glow_main.rs`: eframe/glowのフォント保持、Window／GL contextライフサイクル、Tauri固定費を切り分けるrelease計測プローブ
- `src/glow_lifecycle_main.rs`: WGLのWindow／Surface／Contextを構成要素別に反復し、メモリ・再表示時間・handleリークを計測するreleaseプローブ
- `tauri.conf.json`: WebView WindowとfrontendDistを持たないMVP専用設定

## 不変条件

- 製品版`src-tauri`の既定起動経路・設定を変更しない
- `app.windows`は空とし、Windowは`tauri::Window::builder`だけで生成する
- Updater確認中にeguiイベントループをブロックしない
- 検証用履歴はプロセス固有の未使用一時パスから読み込むだけとし、永続データを書き換えない
- Alt+Q表示は製品版と同じくAlt解放待ち、フォーカス確認、残留Alt解除の順序を守る
- GPU障害注入、hide/show反復、日本語IME強制起動は明示的な`SNOTRA_EGUI_MVP_*`環境変数でのみ有効化する
- glowプローブの比較値は同一バイナリ・独立プロセス・release buildで取得し、内部エラーを非0終了コードへ伝播する
- Windowsフォントのstatic保持はプロセス寿命の`OnceLock`を使い、再表示ごとのメモリリークを作らない
- glow lifecycleの製品候補は単一HWND／Surface／Contextを再生成せず、非表示時に1×1へ縮小する。WGLのpresent先・Surface・shared Contextの反復生成はnegative probeとしてのみ使う
- WGL ContextをunbindしてSurfaceを扱うときは、Context → Surface/HDC → Window/HWNDの順でライフサイクルを管理し、消費型Context APIの失敗でHGLRCを失わない
