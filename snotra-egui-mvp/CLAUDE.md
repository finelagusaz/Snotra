# snotra-egui-mvp

Issue #532の採用判断に使う、WebViewなしの独立MVPバイナリ。

## モジュール構成

- `src/main.rs`: softbuffer runtime 駆動 probe。10,000件の実`Engine`検索、Rust Updater確認、採用条件の実機プローブを持つegui画面
- `tauri.conf.json`: WebView WindowとfrontendDistを持たないMVP専用設定

## 不変条件

- 製品版`src-tauri`の既定起動経路・設定を変更しない
- `app.windows`は空とし、Windowは`tauri::Window::builder`だけで生成する
- Updater確認中にeguiイベントループをブロックしない
- 検証用履歴はプロセス固有の未使用一時パスから読み込むだけとし、永続データを書き換えない
- Alt+Q表示は製品版と同じくAlt解放待ち、フォーカス確認、残留Alt解除の順序を守る
- hide/show反復、日本語IME強制起動は明示的な`SNOTRA_EGUI_MVP_*`環境変数でのみ有効化する
- Windowsフォントのstatic保持はプロセス寿命の`OnceLock`を使い、再表示ごとのメモリリークを作らない
- warm frame の日次比較はしない——同一ホストでも日によってwarm frameが3倍変わることを実測済み（2026-07-17: standalone 26-30ms vs 7/14の8-10ms）。採用判断の比較は必ず同日・同条件で両構成を測る
- 混在スクリプト（Latin+CJK）を描く bin は `jp_font` を families の**先頭**に置く（`insert(0, …)`＝Yu Gothic が Latin も描く単一フォント）。`push`＝末尾 fallback にすると Latin=egui 既定フォント/CJK=Yu Gothic の 2 フォントに分かれ、vertical metrics 差でベースラインがずれる。**カバレッジ AA を持たない softbuffer ラスタライザ（`fill_mesh`）は分数差を整数 px に丸めて顕在化させる**（glow/wgpu は sub-pixel AA で吸収するため不可視）。これは `snotra-settings/CLAUDE.md` の #399（「混在は単一フォントで」）の再発で、**新規 bin ごとに `push` を再導入していた**（#579）。**型検査・clippy・単体テストを素通りし視覚スモークでのみ顕在化する**ため、フォント登録時は先頭配置を守る。フォント設定を製品（egui/runtime）へ移すときは「jp_font が先頭」を config テストで固定する（機構化）。**製品（`src-tauri/src/egui_shell/view.rs`）ではこの単一フォント不変条件が 2 枝へ進化した**: fontdb で config の `font_family` を解決できたとき（`Some`）は user_font を先頭・jp_font を fallback（index 1）に置き、WebView2 CSS スタックと parity を取る。解決に失敗したとき（`None`）のみ、ここに記した「jp_font 単一・先頭」を維持する（#532 SU4）。
