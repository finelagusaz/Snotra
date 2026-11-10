# Phase 2 委譲の共通指示（#1409）

あなたは `src-tauri/src/egui_shell/` のコメントの経緯を、いま守っている約束の形へ書き直す実装者である。**コードは 1 文字も変えない。コメントだけを変える。**

## 最初にやること（自分の worktree の cwd で、git は 1 コマンドずつ単発に打つ。`cd … &&` や `;` で繋がない）

1. `git checkout -B p2-<群名> e28a2499`（本体ブランチ `chore/egui-shell-comment-history` の HEAD。作業ツリーと食い違ったら `git show e28a2499:<path>` が正本）
2. 次を読む（相対パスはこの worktree の中）:
   - `workspace/plan.md`（とくに「分類規則」「Phase 1 の追補」「不変条件と異常系」「受け入れ条件」）
   - `workspace/ledger/results_window.rs.md`（主エージェントが試行した実例。分類と処置後の文の粒度の手本）
   - `docs/comment-guidelines.md`（規約の正本。「第一原則」「歴史メモの様式」「テストコードのコメント」「日本語の折返し」「名指しと正本の指名」）
   - 担当ファイルの台帳 `workspace/ledger/<file>.md`（候補ブロックの雛形。行番号は基準 0265463c のもの）

## 進め方

- 担当ファイルの台帳の**全ブロック**に、分類（残す／縮める／書き直す／消す）・処置後の文（要旨）・根拠を書く。式の誤検出（過去形の語りで当たったが経緯ではないもの）は「残す」と書けば足りる
- 台帳に無くても、同じファイルで読んでいて見つけた経緯（作業単位名・移設の記録・旧実装との対比）は、台帳の末尾に `X1, X2…` として足して同じく処理する
- **元のコメントに無い主張を足さない。** 足すなら根拠欄にコードの位置（`file:シンボル`）を書き、grep で確かめる
- 作業単位名の番号: `PR A′`/`PR A`→#671・`SU<n>`/`SU5 要石`/`codex #<n>`→#532（副ラベルは消す）・`Task <n>`→消す・`spec 決定 3`（view.rs）→#802。それ以外は `git log --format='%h %s' -S '<語句>' -- <file>` の最古コミットの PR 番号
- 回帰検出器の宣言（「この行は〜の回帰検出器になる」）は消さず、検出する性質を現在形で書く
- 「旧・段 14/34」（view.rs・hide_request.rs・lifecycle.rs）は段番号の欠番の説明として**残す**。段番号を振り直さない
- 担当ファイルの外のコメントの語句を引いている（「」で引用している）箇所は変えず、範囲外メモに書く。担当の外のファイルは編集しない
- **needle の綴りをコメントに足さない・消さない**: `.controller.indexing()`・`read_visible_rows(`・`let input_idle = !self.controller.is_search_armed();`・`if snapshot.input_idle {`・`self.indexing()`・`read_config(`・`plain_results_hidden(`・`results_area_collapsed(`・`if crate::egui_shell::should_flush_on_enter(`・`fn on_enter(`・`fn activate_or_execute(`・`fn shift_activate(`・`forbidden(`
- コードスパン（バッククォート）を行またぎさせない。追加行のバッククォート数は偶数
- `///` のブロックを丸ごと消すときは、直後のアイテムの doc の帰属が動かないことを確かめる
- 規約のほかの違反（指すだけの参照など）は直さず、範囲外メモへ

## 検証（コミットの前に毎回）

- `node workspace/tools/rust-tokens.mjs 0265463c` → `23 files, 0 differ`
- `node workspace/tools/needles.mjs 0265463c` → `0 differ`
- `cargo fmt --all -- --check`
- 追加行のバッククォート数: `git diff -U0 e28a2499 -- src-tauri | grep '^+[^+]' | awk -F'`' 'NF%2==0'` が空
- doc コメントを触ったので `cargo doc -p snotra --no-deps --document-private-items` が警告 0
- `cargo test -p snotra -q`（ソーステキスト検査がコメントを読むので必須）

## コミット

- 担当ファイル 1 本ごと（大きいファイルは前半・後半で分けてよい）に、検証が緑になったらコミットする（途中で止まっても成果が残るように）。メッセージ例: `refactor(egui_shell): <file> のコメントの経緯を、いま守っている約束の形へ書き直す (#1409)`。末尾に `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
- コミットメッセージは Write ツールでファイルに書き、`git commit -F <path>` で渡す（heredoc は hook が拒む）
- `workspace/plan.md` は編集しない（主エージェントが消し込む）。台帳は編集してコミットに含める

## 返り値

- 最後のコミットの sha とブランチ名
- ファイルごとの分類の件数（残す／縮める／書き直す／消す）
- 根拠を確かめきれなかった書き直し・判断に迷ったブロック（⚠️ を付けて列挙）
- 範囲外メモの要旨
