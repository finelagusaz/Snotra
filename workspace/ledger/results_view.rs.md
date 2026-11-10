# 台帳: src-tauri/src/egui_shell/results_view.rs（基準 0265463c・候補 63 行・37 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-7 | //! 結果リストウィンドウ（"results"）の egui view（#646 PR2）。main（SearchWindowView）が発行する ⏎ //! RowsSnapshot を描くだけの従属 view——検索状態の所有者は main のまま（一方向データフロー・ ⏎ //! spec 決定 5）。クリックは ResultsShared.clicked へ積んで main を wake する（遅延 dispatch）。 ⏎ //! ウィンドウの可視性・サイズ・位置の driver は main 側（hidden ウィンドウは update() が走らないため）。 ⏎ //! hide は外部（`hide_egui_main` / main の `drive_results_window`）が所有する。runtime に ⏎ //! view 側から hide する API は無い（`RuntimeFrame::hide_window` は #671 サイクル PR A で削除。 ⏎ //! 対称の `close_window` も #660 の `snotra-egui-mvp` 撤去に伴い削除）。 | | | | |
| B2 | 19-19 | /// main が毎フレーム発行する描画用スナップショット（spec 決定 5）。 | | | | |
| B3 | 35-35 | /// の後継・Fix 3）。`selected` の値だけでは「打鍵で結果が丸ごと変わったが selected は | | | | |
| B4 | 39-39 | /// main の `search_debounce` が**予約を持っていない**かどうか（= 非 armed）。旧実装（Task 5 移設前）の `!self.search_debounce.is_armed()`（[`crate::egui_shell::layout::Debouncer::is_armed`] の**否定**）ゲート（連打中は icon worker を積まない・perf 最適化）の後継——`ResultsView` は `search_debounce` を持てないため、main が代わりに live 値を snapshot に載せて運ぶ（#532 SU4 の系譜・controller fix 依頼）。Default=false（PartialEq 比較にも自然に入る。armed → 非 armed の遷移だけで snapshot 差分が生まれ wake が 1 回走るのは「打鍵停止後にアイコン要求を起こす」という意図どおりの信号）。 | | | | |
| B5 | 100-100 | /// （#699 備考が「意図的か見落としかコードから読めない」と問うていた点の結論）。 | | | | |
| B6 | 166-168 | /// 直近に scroll_to_me した選択 index。選択変化時のみ scroll するための gate（#632・Task 4）。 ⏎ last_scrolled_selected: Option<usize>, ⏎ /// 直近に観測した snapshot の世代番号（#632 reviewer Important 3 の後継・Fix 3）。 | | | | |
| B7 | 172-173 | /// path→TextureHandle（セッション内保持。可視集合に頭打ち・#532 SU4。Task 5 で view.rs から ⏎ /// 本 view へ移設——TextureHandle はウィンドウの Context 従属のため、行描画と同じウィンドウに置く）。 | | | | |
| B8 | 184-184 | /// request_icons_for_results 呼び出し（wanted 収集）で自然に収束する（Task 5 申し送り）。 | | | | |
| B9 | 206-206 | /// 渡された行の未取得アイコンを worker に積む（[`RowsSnapshot::input_idle`] 相当）。**`rows` は結果全件ではなく `layout::icon_prefetch_range` で絞った viewport 範囲である**——呼び出し側がそのフレームで実際に描いた `ScrollArea` の状態から導く（絞る理由と実測は同関数の doc）。ゆえに**描画より後に呼ぶ**（範囲がスクロール状態に依存するため）。in-flight（icon_pending）の path は除外し、抽出中の repaint（マウス移動・カーソル点滅等）による同一 path 集合への重複 spawn を防ぐ（thread pileup 対策）。spawn した path は icon_pending へ積み、drain（Loaded/Missing）で remove する。wanted が空なら insert も spawn もしない。Task 5 で view.rs から移設し、入力を `self.state.results()` から `snapshot.rows` へ変更した（本 view は snapshot を描くだけで検索状態を持たないため）。 | | | | |
| B10 | 213-213 | // #673 spec 決定 4: **ここで config を読み直さない。** 呼び出し側（update()）が | | | | |
| B11 | 238-240 | /// 現結果集合の未取得アイコンを別スレッドで抽出し IconMsg を channel へ送る（SU4）。folder の ⏎ /// per-nav thread パターン踏襲。token は載せない（staleness はアイコンキーで無害）。 ⏎ /// show_icons=false 時は呼ばない（呼び出し側でガード）。Task 5 で view.rs から移設 | | | | |
| B12 | 286-286 | /// 持ち越さず瞬時に選択行へ、選択移動（世代不変・↑↓）は従来のアニメーションで寄せる。 | | | | |
| B13 | 317-318 | /// 2 行表示(#646 決定 9): 上段名前・下段パス。行高は Metrics::row_height(呼び出し側注入)。 ⏎ /// `show_icons=false` はアイコン slot 自体を畳む（skip でなくレイアウト変更・#532 SU4 Task 6） | | | | |
| B14 | 347-347 | // block:"nearest" parity・Center だと中央維持で早期スクロール・#532 SU6.5） | | | | |
| B15 | 363-363 | // スクロールバー・ピッチ）とスクロール要求は上で済んでおり、以降は「見えるものを描く」 | | | | |
| B16 | 369-369 | // 約 0.8us で、1 フレームは rows=8 で 13us・200 で 156us・1000 で 807us だった。 | | | | |
| B17 | 414-416 | // 2 行表示(#646 決定 9): 上段 = 名前(全幅・末尾省略)、下段 = パス(全幅・左寄せ・ ⏎ // 幅超過時のみ中間省略)。#632 の「name 60% 制限 + path 右寄せ + 実測幅の重なり回避」は ⏎ // 2 行化で name と path が幅を取り合わなくなったため廃止。 | | | | |
| B18 | 505-505 | // （実測: `visible_rows` 8 で 0.38 行・20 で 1.38 行はみ出していた）。 | | | | |
| B19 | 540-540 | /// Task 7 に集約・コントローラ決定）。Task 7 の視覚スモークで jp_font が 📁📄 を描けると | | | | |
| B20 | 574-574 | // 外部 wake 用の ctx 登録はここに無い（#671 PR D・main の setup と同じ理由）。 | | | | |
| B21 | 578-580 | // テーマ値は 1 フレーム 1 読み（#673 spec 決定 4）。**早期 return より前**で取る—— ⏎ // 背景（clear color）は rows が空でも塗られるからである。旧実装はここが空 rows の ⏎ // early return より後にあり「描かないフレームで lock を取らない」ことを理由にしていたが、 | | | | |
| B22 | 587-587 | // （&mut self）呼び出しと `shared` の同時生存を両立させる（Task 5 で挿入した icon | | | | |
| B23 | 596-598 | // #632 の不変条件「再表示後に確実に一度 scroll し直す」の実ゲート。旧実装は main ⏎ // 側の resetForShow（reset_pending 消費）でこの gate を直接クリアしていたが、 ⏎ // 移設後は本 view が gate の唯一の所有者ゆえここでリセットする——hide/非表示の | | | | |
| B24 | 614-615 | // ——worker（spawn_icon_load）は ColorImage を送るだけで load_texture は呼ばない。Task 5 で ⏎ // main（view.rs）から本 view へ移設: TextureHandle はウィンドウの Context 従属のため、行描画と | | | | |
| B25 | 643-643 | // #632 reviewer Important 3 の後継（Fix 3）: 結果集合が総入れ替えされた | | | | |
| B26 | 674-679 | // SU4 決定 A）。Task 5 で main（view.rs）から本 view へ移設。この一連の処理は ⏎ // update() 冒頭の早期 return（rows 空 = 非表示）を通過したフレーム、すなわち results ⏎ // 可視中にしか走らない——旧実装（main 側）は show_results の真偽に関係なく毎フレーム ⏎ // 走っていた（隠れていても先読み）。reindexing 明けの再表示でアイコンが一瞬 ⏎ // placeholder → 追いつく体感になりうるが、クラッシュ・不整合ではなく hidden 中の ⏎ // update 停止（SU5 要石）と整合する自然な帰結として受容する（plan-review rev-egui | | | | |
| B27 | 688-689 | // 毎フレームやっていた頃は、可視集合の構築だけで結果件数ぶんの `String` 確保を ⏎ // 払っていた（`result_limit` は既定 200・設定次第で 1000）。 | | | | |
| B28 | 699-701 | // snapshot.input_idle は旧 view.rs の `!self.search_debounce.is_armed()` ゲート（連打中は ⏎ // icon worker を積まない・perf 最適化）の後継（#532 SU4 の系譜）。main の search_debounce は ⏎ // ResultsView から参照できないため、live 値を snapshot 経由で運ぶ（Task 5 concern 2 の fix）。 | | | | |
| B29 | 705-705 | // 絞る前は画面に出る 8 個のために 200 回シェルへ問い合わせていた | | | | |
| B30 | 729-729 | // クリック逆流(決定 5): 共有スロットへ積み、main を起こして起動処理させる。 | | | | |
| B31 | 755-755 | // これまで検査が無かった。#646 PR2 から続いた「設定した件数がウィンドウに収まらない」欠陥は、 | | | | |
| B32 | 818-818 | /// **撤去条件**: 結果の行を仮想化しない方針（全行を描いて clip に任せる）を変える変更と同じ PR で消すか、 | | | | |
| B33 | 859-860 | /// 回帰テスト: 行を `Vec` で持っていた頃は、受け取りのたびに行数ぶんの `String` を ⏎ /// 複製していた。型を戻すと 2 つの snapshot が別々のバッファを指し、これが落ちる。 | | | | |
| B34 | 878-879 | /// 回帰テスト: 発行が `&[SearchResult]` から `Arc` を作っていた頃は、↑↓ のたびに行数ぶんの ⏎ /// `String` を複製していた。挙動は変わらない退行なので、見るのはポインタだけである。 | | | | |
| B35 | 947-952 | /// 固定していたことが、この検査の盲点だった——34.92 は丸めが**下**へ向かう側の値で、 ⏎ /// 上へ向かう 13/41 の font_size（12・26〜37）を見逃させていた。3 種の故障注入で ⏎ /// 発火を確かめておきながら、実装に生きていた 4 つ目の破れに対しては盲目だったのである。 ⏎ /// ⏎ /// **等式で測れるのは `results_window_height` が丸めてから掛けるようになったからである** ⏎ /// （案 3）。以前は余りが出るため挟み込みでしか書けなかった。 | | | | |
| B36 | 995-996 | // `results_window_height` も掛ける前に `round_ui` するようになったため、差は ⏎ // 丸め済みどうしの引き算で表せる（案 3 より前はウィンドウ側だけが生の値だった）。 | | | | |
| B37 | 1071-1071 | // size*0.55 概算値だった旧シグネチャの名残で 11.0 を使うが、意味は「1 文字の | | | | |
