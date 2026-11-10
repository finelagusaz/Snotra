# 台帳: src-tauri/src/egui_shell/launcher_controller/activation.rs（基準 0265463c・候補 22 行・17 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! 起動の入口と、そこから走る dispatch・in-flight 回収・slash の即実行（#631 / #666 段 3）。 | | | | |
| B2 | 5-6 | //! どの子モジュールに在っても検査は生き続ける**（移設して測った——移すだけなら 3 本とも緑、 ⏎ //! 移した先でゲートを落とすと赤。同じ移設を変更前の版へ当てると赤だった）。**どの入口が対象かの | | | | |
| B3 | 41-41 | /// worker 側で行う（spec 決定 5: WebView2 が backend 側で UI 可視性と無関係に記録する parity。 | | | | |
| B4 | 124-125 | /// クリアする（withLaunchLifecycle の await 前 clearResults parity・spec 決定 7）—— ⏎ /// launching 中は results ウィンドウが hide される（結果 0 件→snapshot.show=false・#646 PR2 決定 6）・ | | | | |
| B5 | 185-185 | // 履歴記録は worker 側（spec 決定 5）。timeout で drain が破棄済みでも記録は行われる | | | | |
| B6 | 196-200 | // 状態を変えたら起こす（#648 A）。旧実装は「results クリア → 高さ collapse → ⏎ // set_size → Resized → repaint」の暗黙連鎖で初回 overlay と timeout 予約を成立させて ⏎ // いたが、#646 PR2 決定 6 で main 高さが bar(+toast)固定になり results クリアで main は ⏎ // 伸縮しなくなった（暗黙連鎖は消滅・results 側の repaint は snapshot 差分 wake が明示的に ⏎ // 担う）。ここは toast dismiss の同型バグ（SU5・e746826）と同じ規範で、この行単体を | | | | |
| B7 | 205-205 | /// drain が回収した結果の UI 後処理（成功列は M1/M3 同期版と同じ末尾へ合流）。 | | | | |
| B8 | 228-228 | // 旧 Timeout ステータスは IPC の run_launch_blocking 専用で #532 SU7 PR3 で消滅 | | | | |
| B9 | 260-260 | // 4 秒経過＝「結果不明」（spec 決定 8）。rx ごと破棄→遅着は自然消滅。 | | | | |
| B10 | 291-291 | /// slash コマンドを実行する（§15.3 即実行・#532 SU3 M3）。SolidJS handleCommandQueryInput と | | | | |
| B11 | 343-344 | /// **抽出が engine lock を経ないのは #1076 の移行による**（それ以前はここで `engine.lock()` ⏎ /// を取っていた）——ここは egui フレームの中であり、検索 worker が `engine.search` で | | | | |
| B12 | 390-390 | /// Enter/クリックの単一 dispatch（§19.6/§4.8・#532 SU3 M3）。判定は live config の prefix | | | | |
| B13 | 441-441 | // #1072 が塞いだのは同じ族の unsettled 側の切片だけだった。 | | | | |
| B14 | 567-567 | /// [`crate::egui_shell::read_config`] へ移した）。`find_matching_tools` は錠も I/O も取らない | | | | |
| B15 | 581-581 | /// フレームの IME 確定・paste が旧 state で起動されるのを防ぐ（不変条件 3）。 | | | | |
| B16 | 597-597 | // **何をもって「未反映」とするかは `SearchState::is_unsettled` の doc が正本である**（#1038。`armed` だけを渡していた頃に開いていた隙もそこが記す。#1039 で `search_dispatch.rs` の自由関数から移設）。 | | | | |
| B17 | 617-617 | // **どちらの分岐でも同期で行を差し替える**——空クエリ・indexing 中にクリアを落とすと、古い行が残ったまま直後の activate_or_execute がそれを起動する（`run_search_with` の Plain 早期 return が旧実装で担っていた処置である）。 | | | | |
