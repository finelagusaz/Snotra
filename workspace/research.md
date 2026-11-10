# 調査: #1409 egui_shell のコメントの経緯を、いま守っている約束の形へ書き直す（フェーズ 3/3）

## issue の要約

- `src-tauri/src/egui_shell/` のコメントに、egui 移行期（#532・#646・#671 前後）の経緯が残っている。「〜へ移設」「以前は〜」「PR A′」「codex #11」「#752 F2」のように、その作業を知らないと読めない
- あるべき姿: 経緯は、いま守っている約束の形へ書き直す。経緯そのものは `#NNN` で辿れれば足りる。約束にできないもの（作業記録）は消す。コードは変えない（トークン列の比較などで確かめる）
- フェーズ 1（#1407・規約を縮める）の規約に沿う。急がない
- **範囲の裁定（2026-11-10・ユーザー）**: `scripts/smoke-egui.ps1` は別 PR に分ける。この PR は `egui_shell/` の `.rs` だけを扱う（AskUserQuestion の回答「別 PR に分ける (Recommended)」）。理由: `scripts/**` はセーフティネット（`.claude/rules/safety-nets.md`）で、同じ PR に入れると全体が「マージは合意してから」・高リスクになる。open の #1413・#1414 が同じファイルのコードに触る

## 前提の裏取り

- #1404（smoke-egui.ps1 の整理）は CLOSED。その PR #1412（0db290a8）は HEAD の祖先である（`git merge-base --is-ancestor` で確認）
- フェーズ 2（#1408・PR #1411・692e8871）は egui_shell の 8 ファイルに触った。参照の削減であり、経緯の書き直しはしていない
- issue の件数（view.rs 58 行など）は 3a98f16c での測定で、式が記録されていない。再現はしない。以下は自分の式による

## 母集団

- `git ls-files src-tauri/src/egui_shell` = 23 ファイル（`launcher_controller/activation/tests.rs` を含む。`launcher_controller/*.rs` の glob では取りこぼす）
- 候補の抽出: scratchpad の `candidates.mjs`（行の `//` 以降に、変化の語り・作業単位名の正規表現が当たる行）。2026-11-10・HEAD 0265463c で 464 行（`.rs` のみ。smoke-egui.ps1 の 27 行を除く）
  - 上位: view.rs 84・window_coordinator.rs 76・mod.rs 47・layout.rs 46・search_state.rs 34・results_view.rs 33・results_window.rs 20・activation/tests.rs 16・activation.rs 15・lifecycle.rs 14・icon_textures.rs 11・launcher_controller.rs 10
- 標本（等間隔 20 行）での判定: 書き直し・縮める対象 16・残す 4（例: `activation/tests.rs:787`「移した先でゲートが落ちる退行」は変異の説明・`results_view.rs:995` は現在の式の帰結の可能性）。適合率はおよそ 8 割。取りこぼしの形: 「〜した結果」「〜の名残」など式に無い語り
- 単位は行ではなくコメントブロック（段落）である。1 ブロックに候補行が複数あることが多い

## 経緯の形（標本から）

1. **作業単位の副ラベル**: `#532 SU4`・`#646 PR2 決定 6`・`#752 F2`・`#671 PR D`・`#700 state-check 発見 B`・`(Task 8)`・`SU6 spec 決定 2`。`#NNN` が無いもの: `PR A′`（`view.rs:594`）・`SU5 要石`（`window_coordinator.rs:1434`）・`spec 決定 3`（`view.rs:654`）・`codex #11` 型
   - 名前は `docs/superpowers/plans/` と `specs/` で引ける（例: `PR A′` = `docs/superpowers/plans/2026-07-25-pr-a-prime-results-window-newtype.md`）。issue 番号は計画書の見出しから引く
2. **移設の記録**: 「#949 で `update()` から移設」（view.rs:12・571・639・794・823）・「#749 で `ResultsWindow` へ移設」（view.rs:117・590）・「`font_stack` へ移設」（view.rs:1566）
3. **旧実装との対比**: 「旧実装の inner_size() 読みは…race の片翼だった」（view.rs:141）・「以前は `interactive` が 2 項…」（view.rs:714）・「かつてここに…があった」（view.rs:883）・「かつてはバー高固定で…」（window_coordinator.rs:359）・「旧実装は暗黙連鎖で…」（activation.rs:196）
4. **誤検出（経緯ではない）**: 「旧色」「旧幅」「旧 state」（フレーム内の前の値）・`回帰テスト:` が守る性質と出自・日付つきの実測・変異の説明

## 規約（正本: `docs/comment-guidelines.md`）が認める形

- 「経緯は、コミットメッセージと PR 本文に書く。コメントには `#NNN` だけを残す」（第一原則）
- 過去の変更は「経緯そのものは書かず、`#NNN` で辿れる形にする」。根拠は「過去の行為（〜した）・日付つきの実測・正本への参照」なら古くならない（書かないもの）
- 「コメントに残すのは、その経緯が現在の形へ課している制約だけ」（歴史メモの様式）
- 却下案の比較は ADR へ（`///` の面の降ろす先）
- `回帰テスト:` ラベルは守る性質と出自の `#NNN` を書く（テストコードのコメント）
- コードスパンを行またぎさせない（日本語の折返し）
- 適用範囲の但し書き: 「既存コメントの一括書き直しは求めない」。#1409 は issue としてそれを明示的に行う（`ADR-comment-guidelines-one-question` の受容する残余が #1409 を名指し）

## 関連ファイル・制約

- コメント同士・文書からの参照:
  - `docs/adr/ADR-blur-grace-single-field-state-machine.md:24` が `view.rs` の `read_pre_widget_input` の「旧・段 14〜20 相当の位置」を引き、**段番号を振り直さない**決定の根拠にしている。段番号はコード側の番号付け（`frame_stages`）であり、コメントの書き換えで振り直してはならない。ADR は凍結（`ADR-adr-frozen-history`）。コメントから「旧・段」の語句を消すと、ADR の引用先が消える（受容する残余にするか、語句を残すかを決める）
  - `view.rs:672` が同じファイル内の別コメント（`:407`）の語句を引いている。egui_shell 内のコメント間の逐語引用は、書き換えの単位を揃える必要がある
  - `scripts/manual-smoke.ps1` が `#752 F2` を引く（範囲外・触らない）
  - `PR A′`・`#646 PR2`・`#532 SU` は `SPEC.md`・`docs/architecture.md`・ADR・workflow にも出る（範囲外・触らない）
- コメントだけの変更の確かめ方:
  - 「変更行がすべて `//` で始まる」は不十分（文字列の継続行・コードの純削除を見逃す・`ADR-comment-pointer-trim-scope`）
  - #1411 は「コメントを落とした Rust のトークン列」の比較器を使ったが、リポジトリに置いていない。今回も一時的な足場として作る
  - `cargo doc` はバッククォートの欠けを当てにならない形でしか捕まえない。`#[cfg(test)]` の下の doc は検査しない（`.claude/rules/comments.md`）。追加行のバッククォートが偶数であることで確かめる
  - `broken_intra_doc_links` は deny。書き直しで `` [`X`] `` を足すなら着地を `cargo doc` で確かめる
- 再利用できる既存のパターン: #1411 の手順（候補を 1 件ずつ判定・残す形を ADR に記録・トークン比較・バッククォート偶数）

## 敵対的調査（3b）の所見と採否

出力は `workspace/adversarial-1409.txt`。

**壊せた項目（すべて採用）**

1. **コメントは、テストの入力として読まれている**（調査に無かった）。トークン列の比較は必要条件にすぎない。
   - `view.rs` の `assert_read_once_in_this_file` は、`include_str!("view.rs")` の全文について `matches(needle).count() == 1` を見る。呼び出しは 2 か所。
   - `icon_textures.rs` のテストは、`view.rs` と `results_view.rs` の全文に対して `contains` を見る。
   - `launcher_controller/activation/tests.rs` の `sources()` は、`launcher_controller/*.rs` を実行時に読み、`owners_of` / `contains` で判定する。
   - コメントに needle と同じ綴りを足すと、件数の検査は赤になる。`contains` の検査は、将来ゲートを消しても黙って緑のまま残る。needle だけをコメントから消しても、通るテストは変わらない。
   - 自分で確かめた: `view.rs:1418`・`icon_textures.rs:427,433` の `include_str!` は実在する。
   - 採用: 計画に「needle の出現数が、ファイルごとに基準と変わらない」検査を足す。
2. **取りこぼしの主な形は、過去形の語り**（〜ていた／〜だった・当時・従来・時期）。「した結果」「名残」は少数だった（実例: `layout.rs:26` など）。
   - 採用: 候補の式に、この形を足す。
3. **`#NNN` の無い名前の多くは、issue まで引けない。**
   - `PR A′` → #671
   - `SU5 要石`・`codex #11` → 親の #532 まで
   - `Task 8` → SU5 plan の内部番号
   - `spec 決定 3` → 9c64c09d（#802）。config-background-color-design の決定 3
   - 採用: 引けたものは番号へ置き換え、引けないラベルは消す。
4. **外の文書が写している箇所が、調査の 2 件のほかにもある。**
   - `SPEC.md:198` と `layout.rs:123`
   - `ADR-show-path-derives-bar-rect:52` と `window_coordinator.rs:1209`
   - `config_watcher.rs:247` と `view.rs:1561` が、互いに「移設した」と書いている
   - コメントを変えて外が壊れる向きは、`ADR-blur-grace` だけである。採用: 外は触らず、ADR に「受容する残余」として書く。
5. **誤検出の列挙が誤っていた。**
   - 「旧色」「旧幅」は、式に当たらない。
   - 旧幅は hide を跨いだ値で、フレーム内の値ではない。
   - `view.rs:264`・`:1807` の旧色は、過去の不具合の語り（経緯）である。
   - 本当の誤検出は「旧 state」「旧 token」。「旧 」の形には、経緯（旧 `results_should_show`・旧 WebView2 など）が約 25 行ある。
   - 採用: 誤検出は語ではなく 1 件ずつ判定する。
6. **「旧・段 14/34」は 5 か所にある**（`view.rs:407,669,672`・`hide_request.rs:86`・`lifecycle.rs:280`）。採用: 5 か所とも段番号の欠番の説明として「残す」。

**壊せなかった項目**

- 件数（464 / 491）と、母集団の 23 ファイル
- 適合率 8 割前後（独立標本では約 85%）
- 「`//` で始まる行」では不十分であること（文字列中に `//!` の fixture もある）
- リポジトリに比較器が無いこと
- ADR-blur-grace の引用
- `manual-smoke.ps1` の `#752 F2`
- 「旧 state」「旧 token」が経緯でないこと

**⚠️（未測定）**: 取りこぼしの総数・ブロック数・行をまたぐ作業単位名・言い換えで外が引く形・書き直しでソーステキスト検査が黙って緑になるか。計画では、台帳の生成と needle の検査で受ける。

## 未解決の疑問

- 「旧・段 14〜20 相当の位置」を消すか（ADR の引用先が消える）
- `#NNN` の無い作業単位名（`PR A′`・`SU5 要石`・`spec 決定 3`・`Task 8`）の issue 番号を引けるか
- 「旧実装は〜の race の片翼だった」型のうち、現在の制約（「config を正本にすると cross-thread writer が無い」）へ書き直せる情報と、却下案の比較（ADR 行き）とをどう分けるか。新しい ADR を作るか
- 候補ブロックの総数（行ではなく段落の数）
