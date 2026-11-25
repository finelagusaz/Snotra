# 計画レビュー: issue #1526（独立レビュー・観点 A の正しさ／検知器と B の並行性・順序）

## 要対処

1. 【A・文書の偽化】計画の変更ファイル表は、`skip_name` を全モードにしたことで偽になる現在形の記述のうち、`scoring.rs` の直上コメントと `tests/path.rs` の 2 本しか挙げていない。次も「Fuzzy だけ」「部分列」と書いており、同じ変更で直す必要がある（`governance-docs.md`「自分の変更が偽にした記述」）。
   - `snotra-core/src/search.rs:158-163`（`any_name_has_path_sep` の doc。「部分列として存在しえない」「name/file_name の Fuzzy スコアリングを飛ばせる」）
   - `snotra-core/src/query.rs:68-76`（`contains_path_sep` の doc。「name の Fuzzy スコアリングを飛ばしてよいのは…」）
   - `snotra-core/src/search/query_plan.rs:40`（`norm_query_has_path_sep` の doc。「name の Fuzzy マッチが使う needle」）
   - `snotra-core/src/search/tests/path.rs:639-646,666,694`（テストの doc とメッセージ。飛ばすのは Fuzzy だけという前提で書いてある）
   - `snotra-core/src/search.rs:200-215`（`caches_candidates` の doc。「`skip_name` のコメントが条件つきで記録している」。Fuzzy 条件を外すと「外延的に一致」の適用範囲が全モードへ広がる）
   - `snotra-core/src/search/build.rs:346,357` と `PERFORMANCE.md` の #1057 節の見出し（「name の Fuzzy スコアリングを行わない」）。見出しの名前は旧名のまま引用されているので、直すなら `git grep` で参照を数える
   修正案は、計画の「変更するファイル」にこれらを足すか、`git grep "Fuzzy スコアリング"` の結果を母集団として「在る前提で書いている出現」を直す、と明記することである。
2. 【A・論証の書き換え】Prefix/Substring の論証は「部分列」ではなく「接頭辞・連続一致」である。needle が区切りを含み、`lower_name`・`lower_file_name` が区切りを含まなければ、一致しえない。この論証の前提は、`build.rs:368` の `any_name_has_path_sep` が `names_blob`・`lower_names.blob()`・`lower_file_names.blob()` の 3 本を舐めていることに依存する。
   - 照合が読む文字列は `entry_view`（`scoring.rs` の `entry_view`）が返す 2 本だけで、どちらもこの 3 本のいずれかの部分文字列である。**全モードで同じ前提が成り立つ**ことは確認した。
   - Prefix の `starts_with(plan.norm_query)`（`scoring.rs:285-286`）と Substring の `Finder::new(norm_query)`（`query_plan.rs:138-139`）は、どちらも `norm_query` を needle にしており、ガード `norm_query_has_path_sep` と同じ値を見る。
   - 新しいコメントには「名前側は正規化後（`lower_*`）の測定」まで書くこと。生の表示名だけを測っていると読まれると、`¥` や正規化が区切りを作る版で論証が崩れる。
3. 【A・検知器の限界】計画は「実索引の突き合わせ（3 モード）」を A の検知器として挙げているが、実索引は区切りを含む名前が 0 件（テスト自身が `assert!(!optimized.any_name_has_path_sep)`）なので、全モードで「飛ばした結果＝飛ばさない結果」になるのは前提から自明である。このテストは**前提（実データに区切りの名前が無い）の確認**であって、変異 1 を落とさない。変異 1/2 を落とすのは合成 fixture の 1 本だけである。計画の「テスト方針」はその役割分担を書き分けること（検知器は合成 1 本）。

## 軽微

1. 【変異の設計は発火する・確認済み】`path.rs` の fixture（`make_entry("c:\\tool", "D:\\other\\app.exe")`・クエリ `c:\tool`）は、全モードで名前マッチだけが成り立つ。
   - Substring: `find("c:\tool")` が `lower_name` に当たる。Prefix: `starts_with` が当たる。パスは `d:\other\app.exe` に当たらない。`has_dot` は偽（`c:\tool` に `.` が無い）。
   - 変異 1（`!any_name_has_path_sep` を外す）: 3 モードとも `name_score = None` → パスも不一致 → 0 件で落ちる。計画どおり。
   - 変異 2（`mode != Fuzzy && norm_query_has_path_sep`）: Fuzzy の回は `skip_name = false` で通り、Substring/Prefix の回は `any_name_has_path_sep = true`（fixture の表示名が区切りを含む）なのに飛ばして 0 件になり落ちる。新しい分岐へ届く。計画どおり。
   - テストをモードのループで書く場合は、失敗メッセージにモードを入れること（どの回で落ちたかが変異 2 の切り分けになる）。
2. 【`needs_fn_score` の `!skip_name` は変異では落ちない】`has_dot` のクエリで `!skip_name` を外しても、file name は区切りを持たない（`any_name_has_path_sep` が `lower_file_names.blob()` も測るため、区切りを持つ file name は前提を落とす）ので、結果は変わらない。同値変異であり、計画の「変異に使わない」の判断は妥当。ただしコメントの「**ここを閉じないと削減が半分になる**」は性能の主張で、検知器が無い。測定済みの額（A の表）が既に両方を含むなら足りる。
3. 【incremental cache・問題なし】`can_reuse` は `prev_mode == Some(mode)` で全モードが別扱いで、`caches_candidates` は `!has_path_sep`。パスクエリは候補を集めず再利用もしない。`skip_name` は `Some`/`None` の判定結果を変えない（証明）ので、`local_matches` への記録の有無も変わらない。順序の tie-break は `ScoredEntry::Ord` で、スコアが同じなので変わらない。
4. 【kana 経路・問題なし】`kana_score` は `primary_score.is_none()` で試す。`skip_name` と非 skip で `primary_score` は同じ（どちらも `None`）なので、kana の結果も変わらない。`kana_query` は `norm_query` が ASCII で ASCII のアルファベットが残らないときだけ `Some` で、区切りを含むクエリでも `Some` になりうるが、`skip_name` の有無とは独立。実索引のテストは `new_with_migemo(.., false)` で kana を建てないので、kana との組み合わせは合成テストで 1 回（`SearchEngine::new` は migemo 有効）通るだけ。足りないと思うなら、合成テストに migemo 有効の回を足せるが、論証上は不要。
5. 【B・`t0` の移動】計画どおり安全。
   - `t0` は `trace_main("hotkey:listener_enter")` の後でも `on_event_loop` の post の前なので、marshalling の hop を含める意味（`main.rs:556` のコメント）は保たれる。
   - 世代の採番（`hotkey_generation.fetch_add`）・可視の読み・`plan_hotkey`・`ShowAfterAltRelease` の `thread::spawn` は、すべて post されたクロージャの内側（`main.rs:557-606`）にあり、`t0` の取得位置とは無関係。`ShowAfterAltRelease` の `Instant::now()`（`main.rs:602`）も変わらない。
   - 早期 return（設定プロセス起動中・`main.rs:544-548`）は `t0` を使わないので、`trace` の後ろへ移しても分岐は変わらない。
   - `trace()` の `seq` は `AtomicU64::fetch_add`（`trace.rs`）で、`listener_enter` の seq が post 前に採番される順序は保たれる。
   - 製品（`SNOTRA_TRACE` 無効）は `trace()` が即 return するので、`t0` の取得位置が変わっても測れる差は無い。
6. 【B・`ime_control` の trace の移動】`scripts/` は読まない。`ime_control`・`listener_enter` の grep は 0 件（`scripts/` 全体で確認）。`SnotraTraceInvariants.psm1` が `egui_show:done` を使うのは main の可視状態の遷移（H1 の区間）だけで、区間の中で見るイベント名は `egui_results:show` などに限られる。`smoke-egui.ps1` の `Invoke-AndWaitNextEvent` と `Test-SnotraNoTraceEventInWindow` も、イベント名で絞り、`seq` の窓で見る。`ime_control` が `done` の後ろに出ても、次の区間へ紛れ込んでも、判定の対象外のイベント名なので結果は変わらない。
7. 【B・`ime_control` の移動の書き方】trace を `done` の後へ移すとき、送信の成否を `bool` で持ち回す形になる（`if let` 連鎖の内側で trace を呼べなくなる）。**IME の送信そのもの（`b.send_command`）の位置は動かさないこと**。「前面化より後」の依存（`window_coordinator.rs:467-476` のコメント）は送信の位置にしか掛からないので、trace の位置は無関係。`bridge.lock()` のガード `b` は `if let` のスコープの終わりで落ちるので、`done` の後で trace を呼ぶときにロックを持ち越さない。
8. 【B・`ms` の意味の変化】`ms` から `ime_control` の trace 書き込みが消える一方、`send_command`・`read_config`・`bridge.lock()` は `ms` に残る。計画の「`ms` − 区間内の trace」の帯に下がる、という確認は妥当。
9. 【B・`heap_trace`】`heap-trace` feature 有効時は、各 trace 行にヒープ標本が付く（`trace.rs`）。`egui_show:done` の標本は、`ime_control` の行より前に採られる形に変わる（`ime_control` の行自体の `json!({})` の確保が `done` の標本に入らなくなる）。段階の標本は `done` の行のものなので意味は変わらないが、前後の比較が要る計測（`heap-trace` を使う `PERFORMANCE.md` の記録）には、移動の前後で標本の条件が違うことを書く。

## 未検証

1. ⚠️ A の「Prefix は退行しない」の根拠は、計画の表（`3/10 が閾値超え`）のみ。コード上は、Prefix の `starts_with` は `lower_name.len()` 比較の一手で、飛ばす分岐が増えるだけなので退行の機序は見当たらない。ただし測定は一時版（`mode == Fuzzy` を外した版）の p50 で、本実装の版（コメントだけ違う）と同一のコードかは確かめていない。
2. ⚠️ `¥` の正規化が区切りに触る将来の版（`nucleo` の版上げ等）では、`has_path_sep` と `norm_query_has_path_sep` が食い違う。計画は「変異に使わない」としているが、Substring/Prefix でも同じ食い違いが起きる。コメントには「全モードで外延的に一致」と書き、前提の適用範囲を（モード別ではなく）1 か所にまとめること。
3. ⚠️ 計画が挙げる B の確認（専用プロファイルでの 25 回測定）と smoke（カテゴリ C）は、実機・実行でしか測れない。このレビューでは実行していない。`ShowAfterAltRelease` の経路では `listener_enter` は区間に入らず、`ime_control` だけが対象になる点を、確認の期待値（Alt 経路 13.2 ms 前後）が反映している。
4. 実索引・実 config を使うテストは実行していない（依頼の範囲外）。
