# ADR-scan-log-start-order-by-source-text: `drain_index` の `start` 行の順序は、振る舞いではなくソーステキストで固定する

## 文脈

`src-tauri/src/scan_log.rs` の `//!`「2 行 1 組」は、構築 1 回につき走査の**前**に `start` 行を書くことを定める。`start` だけあって `end` が無い行が、完走しなかった構築を表すからである。

#1014 で、`indexing::drain_index` の `scan_log::begin_in` を `indexer::rebuild_and_save` の後ろへ移す変異を当てた。既存の 339 本はすべて緑のままだった。`begin_in` の内部の順序を縛るテストはあったが、呼び出し順を縛るテストは無かった。

issue 本文（背景再スキャンの計器を対象にしていた頃）は、走査を別スレッドで走らせ、走査中にログを poll して `start` 行の有無を見る形を想定していた。

## 決定

`indexing.rs` の `mod tests` に置いたソーステキスト検査 `drain_index_appends_start_before_the_scan` で固定する。既存の `top_level_fn_body` で本体を切り出し、コメントを除いたコードで 2 つの綴りがちょうど 1 回ずつ、この順に現れることを assert する。縛るのはテキストの順序であって実行順ではない。受容した残余はテストの doc が持つ。

## 検討した代替案と却下理由

- **走査中に poll する振る舞いテスト（issue 本文の形）**: 却下。理由は 3 つで、どれも単独で成立する。
  1. `drain_index` は `&AppHandle`（= `AppHandle<Wry>`）を取る。tauri 2.12.5 の `test` feature にある `mock_app` を使うには、`tauri` の dev-dependency に `test` を足し、`drain_index` を `R: Runtime` へ一般化しなければならない。テストのために製品コードのシグネチャを変えることになる
  2. 保存先は `Config::config_dir()` が引き、`SNOTRA_CONFIG_DIR` はプロセス全体の env である。並列に走る他のテストと衝突させずに差し替える手段が要る。差し替えを忘れると、実ユーザーの `scan-log.jsonl` に書き込む
  3. 走査を差し替える口が無い。CI の走査根は合成した一時ディレクトリで、走査はマイクロ秒で終わるので、poll と走査が時間で競合する
- **反復の本体を `AppHandle` 抜きの関数へ切り出し、走査をクロージャで注入する**: 却下。費用が便益を上回る。
  - 便益: 注入なので時間に依存せず、実行順まで測れる。`drain_index` が切り出しを迂回して `begin_in` を直接呼ぶ経路も、「切り出した関数を呼ぶ」ことを別のソーステキスト検査で縛れば塞げる
  - 費用: 反復は `begin_scan_progress`・`build_index_from_material`・`complete_index_drain` を通して `AppState` に絡んでいる。そのため切り出した関数は引数が多くなり、`drain_index` を組み替える変更になる。しかも迂回を塞ぐためのソーステキスト検査は、結局残る
- **`begin_in` が証人の値を返し、走査がそれを引数に取る（型で順序を強制する）**: 却下。走査（`indexer::rebuild_and_save`）は `snotra-core` にあり、`scan_log` は `src-tauri` にある。証人を走査の引数にすると、`snotra-core` の API に記録の都合が漏れる。保存先が引けないと `start` 行は書かれないので、証人は「記録なし」も表さねばならない。また、型で守れるのは「呼んだこと」だけで、呼ぶ位置の意味は守れない。この点は `ADR-source-text-probes-not-lifted-to-types` と同じである
- **CI で変異の赤を実測し、連続 N 回の実行で flaky でないことを確かめる（issue の受け入れ）**: ユーザーの裁定で課さなかった。入力はコンパイル時の `include_str!` だけで、時間・I/O・スレッドに依存しない。環境の差は改行コードだけなので、`indexing.rs` を CRLF にしたローカルの変異（素の実装は緑・変異は赤）で代えた

## 帰結

- 実行順の退行は、この検査では捕まらない（受容した残余の正本はテストの doc）。それを捕まえたくなったら、反復の切り出しの費用を測り直すところから始める
- 母集団の切り出しは `ADR-source-text-probe-helper-locality` に従い、サイト局所の helper を使う
