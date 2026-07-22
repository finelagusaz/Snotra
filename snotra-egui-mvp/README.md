# Snotra egui MVP

Issue #532 の採用判断用に、Tauri のネイティブ `Window` へ egui を softbuffer（CPU ラスタ）で描画する独立バイナリです。製品版 `snotra.exe` の既定起動経路・設定・配布物は変更しません。

```powershell
cargo run -p snotra-egui-mvp
```

環境変数で検証項目を切り替えます（既定値はいずれも無効・未設定）。

| 環境変数 | 効果 |
|---|---|
| `SNOTRA_EGUI_MVP_DISABLE_UPDATER` | Updater確認を無効化する |
| `SNOTRA_EGUI_MVP_UPDATE_MODE` | `full`（既定）／`check_only`／`disabled` |
| `SNOTRA_EGUI_MVP_UPDATER_DOWNLOAD` | Updater確認で新版があれば実際にダウンロードして検証する |
| `SNOTRA_EGUI_MVP_FORCE_JAPANESE_IME` | 起動時にIMEを日本語配列へ強制切り替える（検証プロセスのUIスレッドのみ、終了時に破棄） |
| `SNOTRA_EGUI_MVP_HIDE_SHOW_CYCLES` | 指定回数だけhide/show反復を計測してから終了する |

## 検証できる範囲

- WebView2 を生成しないネイティブウィンドウ
- 10,000件の固定データを使う実`snotra-core::Engine`検索、矢印キー選択、Enter決定
- マウス、ホイール、クリップボード、通常文字、IME preedit／確定文字の入力変換
- リサイズ、DPI 変更、即時／遅延 repaint
- 現行版と同じAlt解放待ちを含む`Alt+Q`表示／非表示
- IME composition／候補位置、hide/show反復（`SNOTRA_EGUI_MVP_HIDE_SHOW_CYCLES`）
- Tauri Updaterの`full`／`check_only`／`disabled`と署名検証付きダウンロード

## MVP に含めない範囲

- 実ファイルシステムのインデックス、実アプリ起動、履歴の永続化
- フォルダー展開、ツール選択、インスタントコマンド等の製品状態機械
- Updaterのインストール実行と終了前保存
- 署名鍵を使う配布artifact生成、旧版からの実更新
- 製品版 `src-tauri` / SolidJS UI の置換

このMVPは採用判断用であり、製品移行では Issue #532 の後続フェーズとして上記を個別に実装・検証します。
