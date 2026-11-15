# #1041 計画の差分レビュー（前回レビュー後の差分のみ）

## 要対処

### D1. 剪定の tmp→rename は `binfmt::save_bytes` の手順を写してはならない（差分 3）
- `snotra-core/src/binfmt.rs` の `save_bytes`（`fs::write(tmp)` → `fs::remove_file(path)` → `fs::rename(tmp, path)`）は、remove と rename の間で落ちると本体が無い。計画の「途中終了で全行を失わない」は、この手順を写すと偽になる。`prune_in` は `remove_file` を挟まず `rename` だけで置換する（Windows の std の rename は既存先を置き換える。推測: MoveFileExW の REPLACE_EXISTING。実測はしていない）。計画の `prune_in` の記述に「remove を挟まない」を明記する。さもないと実装者は「既存の保存手順」に揃えて R3 を再現する。
- rename が失敗する場合（`Get-Content -Tail` などが FILE_SHARE_DELETE なしで開いている・AV）は `prune_in` が握り潰して、ファイルが伸び続ける。伸びる側の害は「`last_completed_dirs` が毎回ファイル全体を読む」だけ。許容でよいが、計画の「異常系」に 1 行書く（現状は「ロック」とだけある）。tmp の名前は固定にして、失敗時に残った tmp は次回の `write` が上書きする形にする（`with_extension("jsonl.tmp")` 相当）。
- 剪定の入力に末尾 `\n` なしの最終行がある場合の数え方（部分行を 1 行と数えて残すか）を決めておく。残す場合、書き直した後の末尾が `\n` で終わらないので、`append_in` の先頭 `\n` 補完に依存する。テスト表の「末尾が `\n` で終わらないファイル」は `append_in` だけを見ている。`prune_in` を通した後の `append_in` も 1 ケース足す。

### D2. `scan_dirs` / `scan_expected` の書き込み順の保証は、メモリ順序を指定しないと空文言（差分 2・6）
- 決定 10 は「`dirs = 0` → `expected` の順」「`finish_index_build` で戻す」を設計の根拠にしているが、計画は atomic の `Ordering` を書いていない。決定 7 は `scan_all` の `fetch_add` を `Relaxed` と書いている（これは単調加算なので妥当）。一方 `state.rs` の既存フィールドは `indexing` / `index_build_started` / `index_generation` のすべてが `SeqCst`（`state.rs` の `is_indexing` ほか）。`begin_scan_progress` / `finish_index_build` / `scan_progress` が `Relaxed` で実装されると、順序の記述は別スレッドでは成り立たない（R6 の残余の主張が偽になる）。計画に「`begin_scan_progress` / `scan_progress` / 戻しは `SeqCst`（既存の 3 つと揃える）」と書く。`fetch_add` だけ `Relaxed` と、理由（単調加算で順序に依らない）を添える。

### D3. 実機の確認「未完走」の手順は `start` の対を作れない場合がある（差分 9）
- 「`/s` の数秒後にプロセスを終了 → `start` だけが残る」は、`drain_index` の 1 反復の走査が数秒より短い環境（ユーザーの標本は 29〜83 秒だが、検証用プロファイルでは短い）では完走する。計画の確認項目は走査が数秒より長いことを前提にしている。検証用プロファイル（`SNOTRA_CONFIG_DIR`）で行う場合は大きな根を 1 つ足す手順を書く。確認の側の話であり、実装は変わらない。

## 軽微

### M1. `save` に `derive_columns`（マスク導出）が入る（差分 1）
- `save_cache_sorted` は `save_cache_sorted_in` を呼び、その中で `derive_columns(entries)` を `bf.save` の前に行う（`cache.rs` の `save_cache_sorted_in`）。したがって `save_ms` は「ファイル書き込み」ではなく「ツリー構築 + 導出 + 書き込み」である。cache-miss 分岐の `cache_save`（`cache.rs` の `cache_save_started`〜`elapsed`）と同じ区間なので比較はそろう。ただし計画の doc 案（「保存先が引けない分岐ではツリーの構築だけの時間になる」）は、通常分岐が導出を含むことを書いていない。`RebuildStats::save` の doc に「ツリー構築・派生データの導出・書き込みを含む」と 1 句。issue の標本（`index.bin` の mtime までの時間）との比較は `scan + sort + save` で正しい。
- 差分 1 の主問い: `config_dir` が `None` の分岐。`save_cache_sorted` は `Config::config_dir()` を内側で自前に引く。`drain_index` は別に 1 回引く。同じ関数・同じ env だが 2 回の呼び出しは独立で、`config_dir` は env を読む（`snotra-core/src/config/location.rs` の `config_dir`）。プロセス実行中に env は動かないので、両者が食い違うのは実用上ありえない。矛盾は無い。ただし「記録を書かない分岐」と「`save` を測る分岐」が別の呼び出しで決まることは、`RebuildStats` の doc に書いておくと、将来の読み手が一致を前提に `save_ms: null` を探さずに済む。
- `with_index_write_lock` の範囲: 現行 `rebuild_and_save` はロックの内側で `scan_all` → `sort_entries_canonical` → `compute_config_hash` → `save_cache_sorted`。`scan_and_sort_timed` は `INDEX_WRITE_LOCK` を取らない（doc に明記・`cache.rs` の `scan_and_sort_timed` の doc）ので、`with_index_write_lock(|| { scan_and_sort_timed(..); compute_config_hash(..); save_cache_sorted(..) })` と書けば範囲は同じ。cache-miss 分岐の書き方（`cache.rs` の `with_index_write_lock(|| { let Scanned{..} = scan_and_sort_timed(..); ...})`）が前例。`compute_config_hash` の位置は、ロックの内側に残すこと（現行どおり）を計画に 1 句。`Scanned` の `scan:` shorthand を避ける規約（#1178）にも従う。

### M2. `Scanned` と `RebuildStats` の `scan` 名の衝突（差分 5 付近）
- `rebuild_and_save(scan: &[ScanPath], ..)` は引数名 `scan` と構造体フィールド `scan` が衝突する。cache-miss 分岐と同じ #1178 の規約（shorthand を使わない）が `RebuildStats { scan: scan_took, .. }` にも要る。実装時に気づく程度だが、cache.rs の他の箇所が全てこの注意を書いている。

### M3. `startup::to_ms` の公開（差分 4）
- `startup.rs` の `to_ms` の doc は「丸めはこの 1 か所だけで起きる」「除数を変異させて検知器が落ちることを測れるように除算を書いた」（`startup.rs` の `to_ms` の doc）。`pub(crate)` にして `scan_log.rs` が使っても、テスト `to_ms_truncates_toward_zero` と変異の意図は壊れない。一方「この 1 か所」はモジュール内（起動計器の中）の主張から、crate 内の 2 計器の主張へ変わる。真のまま保たれるが、doc の「この 1 か所」の射程を 1 句直す（計画は doc に共有を書くとしている。同じ段落を読み直す）。
- `mod startup;` は無条件（`main.rs:19`）で `cfg` に守られていないので、`scan_log.rs` から参照できる。
- `startup.rs` の `//!` は「起動の計測コード」を名乗る。`scan_log.rs` が他モジュールの丸めを借りる依存の向き（スキャン記録 → 起動計器）は軽い逆転。`to_ms` を移す案は計画の「写さない」を損ねないが、移す価値は低い。受容でよい。

### M4. `SCAN_PROGRESS_REPAINT` の 500 ms と `src-tauri/CLAUDE.md` の既存規範（差分 6）
- `src-tauri/CLAUDE.md` の「期限を待つ状態（armed）は、条件が成立するか解除されるまで毎フレーム残余を再要求する」（#711）と、`snotra-egui-runtime/CLAUDE.md`「予約は単一スロット・フレームの到来を約束しない」に対し、計画の決定 9 は**毎フレーム再要求**する形なので両者と整合する。割り込まれて予約が畳まれても、割り込んだフレームが再度 500 ms を撃つ。止まるのは構築中案内が出ないフレームだけ。矛盾は無い。
- 食い違うのは「再要求してよいのは時間経過で解消する不成立だけ」（同ファイルの該当行）の 1 点で、計画はそれを例外として足す。例外の文言は「固定間隔・`ZERO` にしない・止まる条件を表示条件に結びつける」の 3 点に加えて、**「毎フレーム再要求する（armed の規範を満たす）」を明記**する。さもないと例外が armed の規範の緩和と読める。
- `lifecycle.rs` の局所規約（`BlurGrace` ほか）との関係: grep で見た `request_repaint_after` の呼び出し元は `activation.rs`・`frame_stages.rs`・`hide_request.rs`・`search_flow.rs` で、いずれも「残余」を撃つ。500 ms 固定は唯一の「残余ではない間隔」。規約の衝突ではなく新種なので、`SCAN_PROGRESS_REPAINT` の doc に「残余ではなく固定間隔である理由（値の変化に期限が無い）」を 1 句。
- 追加の観察: 初回起動の `AppState::new(.., initial_indexing = true)` でビルドスレッドがまだ無い期間も、`overlay_kind` は Indexing を返すので 2 fps の再描画が発生する（可視・Results のときだけ）。ビルドが起動しない異常（スレッド生成失敗は `finish_index_build` で戻るので到達しない）が無ければ有限。計画は「初回起動で `is_indexing()` が真なのにビルド未開始の期間」を既に挙げている。許容でよい。
- 計画の「`INDEXING_COMPLETE` の wake が来て、そのフレームで止まる」は、`egui_shell/mod.rs` の `register_config_wake_listeners` が `INDEXING_COMPLETE` を購読しているので正しい（`mod.rs:519-530`）。`notify_indexing_complete` は `finish_index_build` の後に emit される（`indexing.rs:69-70`）ので、フレームが走る時点で `indexing == false`。

### M5. `finish_index_build` の呼び出し元（差分 2）
- 全呼び出し元は 2 つ: `indexing.rs:69`（`catch_unwind` の後・正常と unwind の両方）と `indexing.rs:100`（spawn 失敗）。ほかに製品コードの呼び出しは無い（grep: `finish_index_build` の出現は `indexing.rs`・`state.rs` のテストと `view.rs` のコメントだけ）。戻しはどちらでも `finish_index_build` の中で起きる。release の `panic = "abort"` では戻らないがプロセスごと終わる。計画の `/symmetric-check` の記述と一致する。
- `AppState` の構造体リテラル（`AppState {`）は `state.rs` の `new` だけ（grep）。`commands/system.rs` の `test_state` も `AppState::new` を通る。2 フィールドの追加でコンパイルが壊れる箇所は 1 つ。
- 規約: `state.rs` の `indexing` フィールドの doc は「書くのは `new` / `try_begin_index_build` / `finish_index_build` だけ」と書き手を列挙する。新しい 2 フィールドの doc は、この列挙の写しにせず、`begin_scan_progress` / `finish_index_build` / `scan_dirs_counter` 経由の `scan_all`（加算）を名指しする（`AppState::new` も初期化の書き手）。`indexing` の doc を直す必要は無い（別のフィールド）。

### M6. `last_completed_dirs` が `Some(0)` を返しうる（差分 2・3）
- 決定 8 は `dirs` が非負整数の最初の `end` 行を採る。`dirs == 0` の完走（根が全て読めない）なら `begin_in` は `Some(0)` を返し、`start` 行の `expected` に `0` が書かれる。一方 `scan_progress` は `scan_expected == 0` を `None` と扱う（決定 10）。記録の中の `0` と状態の `None` が食い違うが、表示の結果は同じ（フォルダ数だけ）で、`null` と `0` を分ける不変条件は「分母が無い `start` の `expected` は `null`」なので破れていない。受容でよいが、「読み手は 0 を `None` として返す」とそろえるほうが単純（`begin_in` の返り値と `scan_expected` が同じ意味になる）。選ぶなら決めて書く。

### M7. `too_many_arguments`（差分 7・確認済み）
- 既定のしきい値は 7 で、8 個で立つ。scratch の crate で 7 引数・8 引数の関数を `cargo clippy` にかけ、`this function has too many arguments (8/7)` が 8 引数の関数にだけ出ることを確かめた。
- `scan_directory_with_extensions` は現在 6 引数（`dir`・`extensions`・`include_folders`・`show_hidden_system`・`entries`・`dedup`）なので、`dirs` を足して 7。立たない。
- しきい値の変更・lint レベルの変更: `clippy.toml` は `src-tauri/clippy.toml` の 1 つだけで、`disallowed-methods` だけを持つ（`too-many-arguments-threshold` は無い）。ルート `Cargo.toml` の `[workspace.lints.clippy]` は `disallowed_methods = "deny"` だけ。CI は `cargo clippy --workspace --all-targets -- -D warnings`（`ci.yml:144`）で `pedantic` は無い。計画の「リポジトリに `clippy.toml` によるしきい値の変更は無い」は正しいが、**根拠の文言は「`src-tauri/clippy.toml` にだけ `disallowed-methods` があり、しきい値は無い」と直す**（「`clippy.toml` が無い」と誤読される）。
- 既存の `#[allow(clippy::too_many_arguments)]` が 4 箇所（`cache/breakdown.rs`・`raster.rs`・`results_view.rs` ×2）ある。将来 8 引数になる最初の機会（`dirs` の隣にもう 1 つ足す変更）で必要になる点だけ、`scan_directory_with_extensions` の doc に書いておくと次の変更者が気づく。

## 未検証

- U-a. `std::fs::rename` が Windows で既存先を置換する挙動、および他プロセスの共有モードに起因する失敗の現れ方は、このレビューでは実測していない（Rust の std は `MoveFileExW` に `MOVEFILE_REPLACE_EXISTING` を渡す、という記憶による）。実機の確認（Phase 2 の末尾）か、`prune_in` のテスト（既存の `scan-log.jsonl` がある状態で剪定）で見る。
- U-b. `append_in` の「末尾が `\n` かを調べてから 1 回の `write_all`」の実装形（`read(true).append(true)` で開いて末尾 1 バイトを読む形か、`metadata` と別 open か）を、このレビューでは確かめていない。計画は形を決めていない。実装時に、検査と追記の間に他プロセスの追記が割り込む窓がある（計画の「2 プロセス同時は受容」の範囲内）。
- U-c. `src-tauri/src/egui_shell/view.rs` の `Indexing` の arm（`view.rs` 約 951 行）で `ctx` がその位置から使えるか（クロージャの借用）は確かめていない。
- U-d. 差分 8（テスト表）のうち、「凍結の `end` 行リテラル」と `end_line` の往復を別に置く方針は、`/persistence-check` の趣旨に沿うと読んだが、実装前なので出力の形は未確認。
- U-e. `docs/architecture.md:105`・`docs/build-commands.md:180`・`location.rs` の 7・44 行のほか、`SPEC.md` §13.2 の「用途別ファイル分割」の列挙（`SPEC.md:745-749`）は `scan-log.jsonl` を載せない設計（§13.4 を別に立てる）で、計画の判断と整合する。ただし `.claude/skills/persistence-check/SKILL.md:18` の「on-disk 永続化（`index.bin` / `config.toml` / `history.bin` / `window.bin`）」は 4 ファイルの列挙で、`scan-log.jsonl` を足しても偽にはならない（列挙は例示）。計画の変更対象に入っていないが、触らない判断で妥当と読んだ。
