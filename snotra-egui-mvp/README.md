# Snotra egui MVP

Issue #532 の採用判断用に、Tauri のネイティブ `Window` へ egui を描画する独立バイナリです。製品版 `snotra.exe` の既定起動経路・設定・配布物は変更しません。

```powershell
cargo run -p snotra-egui-mvp
```

eframe/glowの構成要素別プローブは別binで実行します。採用判断用の数値は`--release`で取得してください。

```powershell
cargo run --release -p snotra-egui-mvp --bin snotra-egui-glow-mvp -- --font yugothic --font-storage static --scenario keep-window
```

主な比較オプションは`--host tauri|standalone`、`--hardware preferred|off`、`--font none|yugothic|msgothic|meiryo`、`--font-storage owned|static`、`--scenario keep-window|drop-font|recreate|deferred-viewport`です。`hardware=off`と`deferred-viewport`は成立しなかった案を再現するnegative probeであり、製品構成候補ではありません。

WGLライフサイクルの詳細プローブは、既定で同じHWND／Surface／Contextを保持し、非表示時だけ1×1へ縮小する候補構成を3サイクル計測します。

```powershell
cargo run --release -p snotra-egui-mvp --bin snotra-egui-glow-lifecycle-mvp
```

park-surface 統合スパイクは、Tauri managed host（updater／global shortcut）と park-surface レンダラーを同一プロセスで動かします。既定は対話モード（Alt+Q で表示切替、Esc で非表示、ウィンドウを閉じると終了）です。

```powershell
cargo run --release -p snotra-egui-mvp --bin snotra-egui-park-host-mvp
```

softbuffer 最小スパイクは、WebView2 も GPU ランタイムも持たない CPU ラスタ + GDI 転送構成の床（private bytes・コールドスタート・warm・raster 時間）を計測します。

```powershell
cargo run --release -p snotra-egui-mvp --bin snotra-egui-soft-mvp -- --cycles 3
```

softbuffer 統合スパイクは、Tauri managed host（updater／global shortcut Alt+Q）と softbuffer レンダラーを同一プロセスで動かします。既定は対話モード（Alt+Q 表示切替・Esc 非表示）で、`--cycles N` で自動反復計測になります。

```powershell
cargo run --release -p snotra-egui-mvp --bin snotra-egui-soft-host-mvp
```

自動反復計測は `--cycles N` で行います。`--focus off` は unpark 時の SetForegroundWindow／WM_NULL／Alt 解除注入を止めます（無人実行向け。Alt+Q 経路の focus 計測は対話モードで行います）。

```powershell
cargo run --release -p snotra-egui-mvp --bin snotra-egui-park-host-mvp -- --cycles 3000 --visible-wait-ms 40 --hidden-wait-ms 40 --focus off
```

主な切り分けオプションは`--context reuse|shared-child|park-window|park-surface`、`--egui-state recreate|reuse|skip`、`--frame full|paint-no-swap|run-only`、`--gpu-drain none|finish`、`--vsync wait|off`です。`park-surface`以外のcontext modeと、`skip`／`paint-no-swap`／`run-only`はボトルネックを再現するnegative probeです。

## 検証できる範囲

- WebView2 を生成しないネイティブウィンドウ
- 10,000件の固定データを使う実`snotra-core::Engine`検索、矢印キー選択、Enter決定
- マウス、ホイール、クリップボード、通常文字、IME preedit／確定文字の入力変換
- リサイズ、DPI 変更、即時／遅延 repaint
- 現行版と同じAlt解放待ちを含む`Alt+Q`表示／非表示
- IME composition／候補位置、Surface／Device障害注入、hide/show反復
- Tauri Updaterの`full`／`check_only`／`disabled`と署名検証付きダウンロード
- eframe/glowのprivate bytes、working set、handle、GDI／USER object、HWND数、初回／warmフレーム時間
- Yu Gothicのowned／static保持、Tauri／standalone、hide／Window破棄の同条件比較
- 同一WGL Contextの複数HWND present、shared Context、Surface再生成、1×1縮小の反復stress test

## MVP に含めない範囲

- 実ファイルシステムのインデックス、実アプリ起動、履歴の永続化
- フォルダー展開、ツール選択、インスタントコマンド等の製品状態機械
- Updaterのインストール実行と終了前保存
- 署名鍵を使う配布artifact生成、旧版からの実更新
- 製品版 `src-tauri` / SolidJS UI の置換

このMVPは採用判断用であり、製品移行では Issue #532 の後続フェーズとして上記を個別に実装・検証します。
