# Retrospective — #532 Phase 2 SU5（updater + 通知 primitive + 起動 async 化）

## よかったこと

### spec 段階のマルチパースペクティブレビューが設計の根幹を 3 回覆した
brainstorm ドラフトに 3 レンズ subagent（並行性/parity/状態機械）+ codex 敵対探索を当て、実装前に (1)「hide は起動完了待ち」というドラフトの根幹前提が WebView2 実コードで反証され（launching ガードは打鍵のみ）、(2) codex の「世代 token 必要」が per-launch channel の rx 所有で不要と実測反証され、(3)「保存優先」の出所調査が SPEC・コード・issue 申し送りの三者食い違いを暴いた。実装後に発覚していれば作り直しだった 3 件が、spec の文面修正で済んだ。多レンズは「収束点で盲点を暴く」（#536 の教訓）が SU5 でも成立。

### 一次資料が reviewer の Important を 2 件取り下げさせた
Task 3（flush 後 selected=0 の主張）と Task 4（Tool 失敗時の「退行」）で、reviewer の Important に対し実装者が WebView2 実コード・spec 本文という一次資料で反証し、いずれも「コメント是正のみ」「現状維持」で決着した。「reviewer の指摘も実装者の反論も鵜呑みにせず、一次資料で接地させてから裁定する」往復が、誤った fix（parity 逸脱の reset 追加・不要なメニュー復元実装）を 2 回防いだ。

### plan 工程の plugin 一次調査が「決着不能な未決」を構造的解に変えた
spec が未決とした保存順序を、実装計画の冒頭で tauri-plugin-updater のローカルソース（`updater.rs:865` の `std::process::exit(0)`）に問うて決着した。副産物として「現行 WebView2 経路は update 時に終了保存が一度も走っていない」既存 gap を発見し、`on_before_exit` hook 登録という「保存が構造的に保証される」解へ到達した。文書間の食い違い（SPEC vs ロードマップ申し送り）は、実装を読むまで裁定しないのが正しかった。

### 検証専任タスクが spec の要石を決着させ、実バグも捕捉した
Task 10 を検証専任に切り、spec が「実装時スモークで決着」と明記した hidden 中 update() の挙動を一時プローブで実測（走らない・backstop 有効）。さらに回帰スモークが toast dismiss 後の stale 表示（wake 忘れ）という**レビュー 2 段をすり抜けた実バグ**を捕捉した。「テストで書けない項目を実装時スモークとして計画に明記する」→「検証タスクがそれを実行する」の分業が機能した。

---

## 伸びしろ

### rustdoc の intra-doc link 誤解釈で CI が赤になった（検査カバレッジの隙間）
doc コメントに UI ラベルを `[今すぐ更新]` と角括弧で書き、rustdoc がリンク構文と解釈して CI rust-check（cargo doc・deny 設定）が赤になった（4 箇所・マージ前に発覚）。**cargo doc は PostToolUse hook にも task review のテスト実行にも含まれず、CI でのみ発火する**——「hook 沈黙=合格」の射程外にある検査の存在を、書く時点の様式で防ぐしかない。`docs/comment-guidelines.md` rustdoc 様式に「非リンク角括弧は backtick で包む」を追記して塞いだ。

### イベント駆動 runtime の wake 忘れが 3 度目の同型で再発した
toast action（クリックの遅延 dispatch）後に `request_repaint` を呼ばず、dismiss 後の stale 表示が残った。folder ロード（SU3 M2）・icon worker（SU4）で 2 度学び view.rs にコメントまである同型パターンの 3 度目——「状態を変えたら起こす」が**サイトごとの注意**に留まり、横断不変条件として文書化されていなかった。`src-tauri/CLAUDE.md` の egui_shell 節に wake 不変条件（+ hidden 中 update() 非走行の実測）を明記して昇格させた。残余の暗黙 wake 1 箇所（start_launch）は #648 で自己完結化する。

### 計画内コードの「わかったつもり」が review 往復を 2 回消費した
plan の完全コード方式は転写ミスを消す一方、(1) flush 後 selected のコメントが実挙動（clamp）と矛盾、(2) Tool 失敗時の run_search no-op 見落とし、(3) toast ボタンの `next_auto_id` 二重取得と、**計画コード自体の欠陥**が 3 件レビューで出た。いずれも「WebView2 の該当経路を計画時に読んで確認したか」で防げた——**計画に書くコードの parity 主張は、書く時点で該当実装の該当行を読んで裏取りする**（spec の parity 検証と同じ規律を plan のコード片にも適用する）。
