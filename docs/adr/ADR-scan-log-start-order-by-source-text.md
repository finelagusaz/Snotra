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
  3. 走査を差し替える口が無いので、CI の合成した根（走査はマイクロ秒で終わる）では、poll と走査が時間で競合する
- **反復の本体を `AppHandle` 抜きの関数へ切り出し、走査をクロージャで注入する**: 却下。注入なので時間には依存しない。しかし `drain_index` の構造を変えるうえに、`drain_index` が切り出した関数を使わずに `begin_in` を直接呼ぶ経路は捕まえられない。ソーステキスト検査は、その経路（本体の中の並べ替え）を捕まえる
- **CI で変異の赤を実測し、連続 N 回の実行で flaky でないことを確かめる（issue の受け入れ）**: ユーザーの裁定で課さなかった。入力はコンパイル時の `include_str!` だけで、時間・I/O・スレッドに依存しない。環境の差は改行コードだけなので、`indexing.rs` を CRLF にしたローカルの変異（素の実装は緑・変異は赤）で代えた

## 帰結

- 実行順の退行（走査の前に作ったクロージャを後で呼ぶ形など）は、この検査では捕まらない。それを捕まえたくなったら、上の 1 番目の却下理由（製品のシグネチャを変える費用）を測り直すところから始める
- 母集団の切り出しは `ADR-source-text-probe-helper-locality` に従い、サイト局所の helper を使う
