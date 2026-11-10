# 台帳: src-tauri/src/egui_shell/search_state.rs（基準 0265463c・候補 43 行・36 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! egui 検索ウィンドウの純粋状態核（#532 SU3）。query/選択/結果と 2 軸モード導出・遷移を | 縮める | `（#532）` | `SU3` は #532 の副ラベル |  |
| B2 | 13-13 | /// 到達可能化・#532 SU3 M2、Tool は SU3.5 で到達可能化・#532 SU3.5）。 | 消す | 括弧（Folder は M2 で到達可能化…Tool は SU3.5 で到達可能化…）を消し、「Results/Folder/Tool の 3 段ラダー（#532）。」 | 到達可能化の時期は作業記録 |  |
| B3 | 78-78 | /// フォルダ展開モードの退避/復元単位（`Option<FolderFrame>`・#532 SU3 M2）。深掘りは push でなく | 縮める | 「（`Option<FolderFrame>`・#532）」 | `SU3 M2` は副ラベル |  |
| B4 | 86-86 | /// 突入した時点で [`SearchState::is_unsettled`] が真だったか（#1079）。**復帰させる行が | 残す |  | 誤検出（突入時点の値の説明） |  |
| B5 | 106-106 | /// ツール選択モードの退避/復元単位（#532 SU3.5・SolidJS ToolSelectionFrame parity）。 | 縮める | 「（#532・SolidJS ToolSelectionFrame parity）」 | `SU3.5` は副ラベル |  |
| B6 | 112-112 | /// tool_frame() 越しに読む（`shift_activate` / `execute_tool_selected`・#532 SU3.5 Task 3）。 | 縮める | 「（`shift_activate` / `execute_tool_selected`・#532）」 | `SU3.5 Task 3` は副ラベル。両関数が `tool_frame()` を読む（`launcher_controller/activation.rs`） | 呼び出し元の列挙（「書かないもの」）。`view.rs:766` も `tool_frame()` を読むので列挙は網羅でない |
| B7 | 144-144 | /// driver（launcher_controller）が消費する（#532 SU3 M3 Task 2）。 | 縮める | `（#532）` | `SU3 M3 Task 2` は副ラベル。`SlashCmd` は `launcher_controller/activation.rs`・`search_flow.rs` が消費する（grep で確認） | 呼び出し元の名指し（「書かないもの」） |
| B8 | 155-155 | /// driver（launcher_controller）が消費する（#532 SU3 M3 Task 2）。 | 縮める | `（#532）` | B7 と同じ。`find_slash_command` は `search_flow.rs` が呼ぶ | B7 と同じ。`mod.rs:82` も `#532 SU3 M3 Task 2` を持つ（群 C） |
| B9 | 167-167 | /// M2 で追加・#532 SU3 M2）と tool 軸（tool・SU3.5 で追加・#532 SU3.5）を持つ。 | 縮める | 「folder 軸（folder / folder_filter / folder_gen）と tool 軸（tool）を持つ（#532）。」 | 追加の時期（M2・SU3.5）は作業記録 |  |
| B10 | 178-179 | /// `results` が総入れ替えされるたびに進む世代（#699。旧 `view.rs` の ⏎ /// `snapshot_generation` をここへ移した）。 | 縮める | `（#699）` | 「旧 `view.rs` の `snapshot_generation` をここへ移した」は移設の記録 |  |
| B11 | 269-269 | /// 書く規約だった頃は 11 箇所の手書きと 3 箇所の漏れがあった。 | 書き直す | 「呼び出し側が [`SearchDispatch::invalidate`] を書く規約では漏れる（実測は `ADR-row-replacement-choke-point`）。」 | 「11 箇所の手書きと 3 箇所の漏れ」は同 ADR の「文脈」が持つ（grep で確認） |  |
| B12 | 303-303 | /// setSelected(0) parity・#532 SU3 M3）。 | 縮める | 「（SolidJS の毎打鍵 setSelected(0) parity・#532）」 | `SU3 M3` は副ラベル |  |
| B13 | 393-393 | /// 遷移先を返し続ける。**これは意図した挙動である**——「どこへ移ったか」を行の入れ替わりより | 残す |  | 誤検出（「意図した挙動である」は現在の宣言） |  |
| B14 | 412-412 | /// getter としては未消費（#532 SU3 M2 Task 3）。 | 縮める | `（#532）` | `SU3 M2 Task 3` は副ラベル |  |
| B15 | 430-430 | /// `shift_activate` が呼ぶ（#532 SU3.5 Task 3）。 | 縮める | `（#532）` | `SU3.5 Task 3` は副ラベル。`shift_activate` が `enter_tool` を呼ぶ（`activation.rs`） |  |
| B16 | 463-463 | /// driver（`shift_activate` / `execute_tool_selected`）が読む（#532 SU3.5 Task 3）。 | 縮める | `（#532）` | B15 と同じ | B6 と同じ（`view.rs:766` も読む） |
| B17 | 490-492 | /// #1039 当時のこの doc は [`SearchState::accept_worker_rows`] の view ガードが捨てる段を並びへ ⏎ /// 含めていたが、`invalidate` は届いたかを見ずに pending を落とすので**遅着の有無によらず成立する** ⏎ /// （2026-08-14 に検知器で実測）。当時「稀」を受容の根拠の一つに置けたのは、この過剰仕様のためである。 | 書き直す | 「**worker の結果が folder 中に届くことはこの並びの要件ではない。** `invalidate` は届いたかを見ずに pending を落とすので、**遅着の有無によらず成立する**（2026-08-14 に検知器で実測・#1079）。」 | 日付つきの実測と現在の主張は残す。「#1079 の issue 本文と #1039 当時のこの doc は…含めていた」「当時「稀」を受容の根拠に置けたのは…」は作業記録 |  |
| B18 | 503-503 | /// 乗せる」は現在のコードに当たらない**——`run_search_with` の Plain 分岐は #1004 の worker 化以降 | 縮める | 「…Plain 分岐は**同期 `engine.search` を含まない**（#1004。…）」 | 「#1004 の worker 化以降」の時期の語りだけを落とす | 続く「2026-08-27 の分割で変わった」は日付つきの前提の変化で、今も再検討時の注意として効くので残した |
| B19 | 508-508 | /// 費用に数えていたが、#1076 でその読みが `read_config` へ移り消えた。** | 消す＋書き直す | 「`run_search` 入口の `instant_prefix` が…#1076 でその読みが `read_config` へ移り消えた。」を消す。直前の「doc が名指していたもの」→「issue 本文が名指したもの」 | 過去の費用の数え方の訂正記録。「doc が名指していた」は過去のこの doc を指す（作業記録）ので、同じ段落の主語（#1079 の issue 本文）へ揃えた |  |
| B20 | 547-548 | // hide を跨いだ in-flight もここで失効する（#1039。以前は呼び出し側の ⏎ // `consume_reset_pending` が別に撃っていた）。 | 縮める | `（#1039）` | 「以前は呼び出し側の `consume_reset_pending` が別に撃っていた」は旧実装の描写 |  |
| B21 | 603-606 | /// 自由関数からここへ移設）。[`should_flush_on_enter`] の第 3 引数がこれである。 ⏎ /// ⏎ /// **`armed` だけでは足りない。** 同期実装の頃は `armed == false` が「反映済み」を含意して ⏎ /// いたが、worker 化（#1004）で trailing 発火の直後は必ず「`armed == false` かつ in-flight | 縮める＋書き直す | 「（#1038・#1039）」・「**`armed` だけでは足りない。** trailing 発火の直後は必ず「`armed == false` かつ in-flight あり」になるので、`armed == false` は「反映済み」を含意しない（#1004）。」 | 「#1039 で `search_dispatch.rs` の自由関数から移設」は移設の記録。「同期実装の頃は含意していた」は旧実装の描写で、現在の主張（含意しない）は元の文にある | `docs/architecture.md:239` が「含意が壊れた経緯は同 doc」とこの doc を指す。経緯（同期実装の頃は含意していた）を落としたので、指す先の「経緯」が無くなった（現在の理由「trailing 直後は armed == false かつ in-flight あり」は残る）。範囲外のファイルなので直していない |
| B22 | 625-625 | /// である。#1039 が規約から機構へ移したのも in-flight の失効に限る。 | 書き直す | 「機構が守るのも in-flight の失効に限る（#1039）。」 | 「規約から機構へ移した」の変化の語りを現在形へ。`put_rows` が `dispatch.invalidate()` を呼ぶ |  |
| B23 | 632-632 | /// いま取り残しが起きないのは `Debouncer` ごと捨てているからであり、フィールドだけを移した瞬間に | 残す |  | 誤検出（「いま取り残しが起きないのは」は現在の理由） |  |
| B24 | 636-636 | /// フレームで `poll` が「隠れていた時間」を経過として読み、その場で trailing を撃つ。**同型の | 残す |  | 誤検出（「同型の残余」） |  |
| B25 | 673-673 | /// **`unsettled` は「最終クエリの結果がまだ行へ反映されていないか」である**（#1038）。その中身と経緯は [`SearchState::is_unsettled`] の doc が正本である（#1039 で `search_dispatch.rs` の自由関数から移設）。 | 縮める | 「その中身は [`SearchState::is_unsettled`] の doc が正本である（#1039）。」 | 「経緯」は B21 で落としたので同時に直す（同じファイルの相互参照）。「`search_dispatch.rs` の自由関数から移設」は移設の記録 | 同じ段落の「unsettled になるのは Results∧Plain 経路のみ（…）」は経路の列挙（「書かないもの」） |
| B26 | 729-729 | /// §4.7 表示ゲート（#633・SU6 spec 決定 3）: 再インデックス中は plain 結果のみ隠す。 | 縮める | `（#633）` | `SU6 spec 決定 3` は `git log -S` → 04612f60（SU6・#651・「#633 表示ゲート/世代カウンタ」）。#633 が既に名指されている |  |
| B27 | 744-747 | /// #633 世代トリガ（SU6 spec 決定 3）: index build 完了で bump される世代が last-seen と ⏎ /// 異なれば再検索。bool エッジ検出と違い、started/complete の repaint が 1 フレームに合流して ⏎ /// パルスが見えなくても累積カウンタは差分が残るため取りこぼさない。 ⏎ /// driver（launcher_controller）は Task 4 で再検索トリガに組み込む（#532 SU6 Task 1）。 | 縮める＋消す | 「#633 世代トリガ: …」。4 行目「driver（launcher_controller）は Task 4 で再検索トリガに組み込む（#532 SU6 Task 1）。」を消す | B26 と同じ。4 行目は未実装の計画の形の作業記録 |  |
| B28 | 891-891 | /// #699 で当初の計画が落としていた経路（issue 本文も挙げていない）。 | 書き直す | 「回帰テスト: `set_results` を通らない経路でも世代が進む（#699）。」 | `enter_tool` は `set_results` を通らず `put_rows` を直接呼ぶ（`search_state.rs:SearchState::enter_tool`）。「当初の計画が落としていた」は作業記録 |  |
| B29 | 1035-1035 | /// view 種別ガード（#1039 決定 3）。`enter_folder` は行を差し替えないので `put_rows` を | 縮める | `（#1039）` | `決定 3` は #1039 の副ラベル |  |
| B30 | 1122-1122 | /// 移設（`search_dispatch.rs` → ここ・#1039）。真理値表 4 件 + `should_flush_on_enter` | 消す | 「移設（`search_dispatch.rs` → ここ・#1039）。」を消す。残りは元のまま | 移設の記録 |  |
| B31 | 1152-1152 | /// 移設（`unsettled_is_grounded_on_real_dispatch`・#1039）。sentinel（0 = in-flight なし）を | 消す | 「移設（`unsettled_is_grounded_on_real_dispatch`・#1039）。」を消す。残りは元のまま | 移設の記録 |  |
| B32 | 1327-1328 | /// 移設で変わっていないこと。`search_dispatch.rs` の `accepts_only_the_latest_seq` と ⏎ /// 同じ値を返す（**移設は計測コードを壊してはならない**）。 | 書き直す | 「計時 trace（…）の材料が、`SearchState` 越しに呼んでも `search_dispatch.rs` の `accepts_only_the_latest_seq` と同じ値を返すこと（**計測の区間を変えない**・#1039）。」 | 回帰検出器の宣言を、検出する性質（同じ値を返す）の現在形で残す。`accepts_only_the_latest_seq` は `search_dispatch.rs` に在る（grep で確認） |  |
| B33 | 1652-1652 | assert!(!s.accept_folder_result(t1)); // 旧 token は破棄 | 残す |  | 「旧 token」は同じ処理の前の値 |  |
| B34 | 1718-1718 | // M1 は set_results の clamp のみで、打鍵後も旧 selected が残っていた（M3 で是正）。 | 書き直す | 「回帰テスト: `set_results` の clamp だけでは、打鍵後も前の selected が残る（#532）。」 | M1/M3 は #532 の副ラベル。元の主張（clamp のみでは残る）を現在形へ |  |
| B35 | 1877-1877 | assert!(!s.accept_folder_result(t1)); // 旧 token は folder Some でも拒否 | 残す |  | B33 と同じ |  |
| B36 | 1904-1904 | // 複数回 bump がまとまっても 1 回の比較で拾う（repaint 合流パルス耐性・spec 決定 3） | 縮める | 「（repaint 合流パルス耐性・#633）」 | B26 と同じ |  |
| X1 | 125 | `///` 「（instant/command 解除段は M3 で確定済み・prefix hot-change は別軸）」 | 縮める | 「（prefix hot-change は別軸）」 | 「M3 で確定済み」は作業記録 |  |
| X2 | 477 | `///` 「Escape ラダー（M2）:」 | 縮める | 「Escape ラダー:」 | `M2` は #532 の副ラベル |  |
| X3 | 733 | `///` 「（#637 finding 0）」 | 縮める | `（#637）` | `finding 0` は副ラベル |  |
