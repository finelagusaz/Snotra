# 台帳: src-tauri/src/egui_shell/search_state.rs（基準 0265463c・候補 43 行・36 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! egui 検索ウィンドウの純粋状態核（#532 SU3）。query/選択/結果と 2 軸モード導出・遷移を | | | | |
| B2 | 13-13 | /// 到達可能化・#532 SU3 M2、Tool は SU3.5 で到達可能化・#532 SU3.5）。 | | | | |
| B3 | 78-78 | /// フォルダ展開モードの退避/復元単位（`Option<FolderFrame>`・#532 SU3 M2）。深掘りは push でなく | | | | |
| B4 | 86-86 | /// 突入した時点で [`SearchState::is_unsettled`] が真だったか（#1079）。**復帰させる行が | | | | |
| B5 | 106-106 | /// ツール選択モードの退避/復元単位（#532 SU3.5・SolidJS ToolSelectionFrame parity）。 | | | | |
| B6 | 112-112 | /// tool_frame() 越しに読む（`shift_activate` / `execute_tool_selected`・#532 SU3.5 Task 3）。 | | | | |
| B7 | 144-144 | /// driver（launcher_controller）が消費する（#532 SU3 M3 Task 2）。 | | | | |
| B8 | 155-155 | /// driver（launcher_controller）が消費する（#532 SU3 M3 Task 2）。 | | | | |
| B9 | 167-167 | /// M2 で追加・#532 SU3 M2）と tool 軸（tool・SU3.5 で追加・#532 SU3.5）を持つ。 | | | | |
| B10 | 178-179 | /// `results` が総入れ替えされるたびに進む世代（#699。旧 `view.rs` の ⏎ /// `snapshot_generation` をここへ移した）。 | | | | |
| B11 | 269-269 | /// 書く規約だった頃は 11 箇所の手書きと 3 箇所の漏れがあった。 | | | | |
| B12 | 303-303 | /// setSelected(0) parity・#532 SU3 M3）。 | | | | |
| B13 | 393-393 | /// 遷移先を返し続ける。**これは意図した挙動である**——「どこへ移ったか」を行の入れ替わりより | | | | |
| B14 | 412-412 | /// getter としては未消費（#532 SU3 M2 Task 3）。 | | | | |
| B15 | 430-430 | /// `shift_activate` が呼ぶ（#532 SU3.5 Task 3）。 | | | | |
| B16 | 463-463 | /// driver（`shift_activate` / `execute_tool_selected`）が読む（#532 SU3.5 Task 3）。 | | | | |
| B17 | 490-492 | /// #1039 当時のこの doc は [`SearchState::accept_worker_rows`] の view ガードが捨てる段を並びへ ⏎ /// 含めていたが、`invalidate` は届いたかを見ずに pending を落とすので**遅着の有無によらず成立する** ⏎ /// （2026-08-14 に検知器で実測）。当時「稀」を受容の根拠の一つに置けたのは、この過剰仕様のためである。 | | | | |
| B18 | 503-503 | /// 乗せる」は現在のコードに当たらない**——`run_search_with` の Plain 分岐は #1004 の worker 化以降 | | | | |
| B19 | 508-508 | /// 費用に数えていたが、#1076 でその読みが `read_config` へ移り消えた。** | | | | |
| B20 | 547-548 | // hide を跨いだ in-flight もここで失効する（#1039。以前は呼び出し側の ⏎ // `consume_reset_pending` が別に撃っていた）。 | | | | |
| B21 | 603-606 | /// 自由関数からここへ移設）。[`should_flush_on_enter`] の第 3 引数がこれである。 ⏎ /// ⏎ /// **`armed` だけでは足りない。** 同期実装の頃は `armed == false` が「反映済み」を含意して ⏎ /// いたが、worker 化（#1004）で trailing 発火の直後は必ず「`armed == false` かつ in-flight | | | | |
| B22 | 625-625 | /// である。#1039 が規約から機構へ移したのも in-flight の失効に限る。 | | | | |
| B23 | 632-632 | /// いま取り残しが起きないのは `Debouncer` ごと捨てているからであり、フィールドだけを移した瞬間に | | | | |
| B24 | 636-636 | /// フレームで `poll` が「隠れていた時間」を経過として読み、その場で trailing を撃つ。**同型の | | | | |
| B25 | 673-673 | /// **`unsettled` は「最終クエリの結果がまだ行へ反映されていないか」である**（#1038）。その中身と経緯は [`SearchState::is_unsettled`] の doc が正本である（#1039 で `search_dispatch.rs` の自由関数から移設）。 | | | | |
| B26 | 729-729 | /// §4.7 表示ゲート（#633・SU6 spec 決定 3）: 再インデックス中は plain 結果のみ隠す。 | | | | |
| B27 | 744-747 | /// #633 世代トリガ（SU6 spec 決定 3）: index build 完了で bump される世代が last-seen と ⏎ /// 異なれば再検索。bool エッジ検出と違い、started/complete の repaint が 1 フレームに合流して ⏎ /// パルスが見えなくても累積カウンタは差分が残るため取りこぼさない。 ⏎ /// driver（launcher_controller）は Task 4 で再検索トリガに組み込む（#532 SU6 Task 1）。 | | | | |
| B28 | 891-891 | /// #699 で当初の計画が落としていた経路（issue 本文も挙げていない）。 | | | | |
| B29 | 1035-1035 | /// view 種別ガード（#1039 決定 3）。`enter_folder` は行を差し替えないので `put_rows` を | | | | |
| B30 | 1122-1122 | /// 移設（`search_dispatch.rs` → ここ・#1039）。真理値表 4 件 + `should_flush_on_enter` | | | | |
| B31 | 1152-1152 | /// 移設（`unsettled_is_grounded_on_real_dispatch`・#1039）。sentinel（0 = in-flight なし）を | | | | |
| B32 | 1327-1328 | /// 移設で変わっていないこと。`search_dispatch.rs` の `accepts_only_the_latest_seq` と ⏎ /// 同じ値を返す（**移設は計測コードを壊してはならない**）。 | | | | |
| B33 | 1652-1652 | assert!(!s.accept_folder_result(t1)); // 旧 token は破棄 | | | | |
| B34 | 1718-1718 | // M1 は set_results の clamp のみで、打鍵後も旧 selected が残っていた（M3 で是正）。 | | | | |
| B35 | 1877-1877 | assert!(!s.accept_folder_result(t1)); // 旧 token は folder Some でも拒否 | | | | |
| B36 | 1904-1904 | // 複数回 bump がまとまっても 1 回の比較で拾う（repaint 合流パルス耐性・spec 決定 3） | | | | |
