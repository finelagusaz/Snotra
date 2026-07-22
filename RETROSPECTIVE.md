# Retrospective — #532 SU3 M1（egui 検索体験の機能中核）

## よかったこと

### brainstorm で分岐を先に確定し、並行崩壊の主命題が実装を貫いた
brainstorm → spec で 6 分岐（+レビューで debounce・SU3.5 の 2）を先に確定し、spec の否定の知識へ接地した。要石は「同期直 `Engine` 呼びが SolidJS の supersede/single-flight を消す」——advisor と opus 最終レビューが独立に「同期モデルで成立」を確認し、実装で並行 primitive を一切持ち込まずに済んだ。分岐を先に潰したことで 10 タスクの実装が迷わなかった。

### functional-core/imperative-shell 分離 + 二段レビューが bug を捕捉
純粋核（interpret/SearchState/layout）を TDD ユニット化し、driver は build+clippy+trace スモークで検証（「view はテスト前提にしない」ルールと整合）。SDD の各タスクレビュー + opus 最終 whole-branch レビューの二段が、warm-index スモークで観測されなかった「indexing 案内が 52px 窓外にクリップ」を捕捉した。

### 多層レビューの角度が相補的だった
/code-review の **git 履歴・過去 PR コメント**観点が、SDD の各タスクレビュー（ファイル単位スコープ）と opus 最終レビュー（G1 スコープ）がいずれも取りこぼした「モジュール索引未更新」を拾った——しかも PR #629 と**同型の再発**。レビューは角度が違えば見えるものが違う、を実証した。

### 規則2（壊れた出力から推論しない）を全タスクで貫いた
rust-analyzer が編集途中を捉えた false-positive 診断（E0609/E0061/dead_code 等）を各タスクで出したが、すべて `cargo build`/`clippy`/`test` の再実行で緑を確定。診断と実測が矛盾するたびに cargo を一次証拠にした。haiku 実装者が Task 4 の RED 転写を捏造した際も、reviewer + controller の test 再実行で健全と確定した。

---

## 伸びしろ

### モジュール索引更新漏れが #629→#630 で再発した
新規 `.rs` 追加時に `src-tauri/CLAUDE.md` の索引を更新するトリガーは AGENTS.md 条件別チェック表に在り、`governance:check` が CI で捕捉する機構も在る。にもかかわらず SDD の各ファイル追加タスク（Task 1/3）が索引更新を同梱せず、PR 作成前に `governance:check` も回さなかった。opus 最終レビューも G1 スコープは見たが索引整合は見ていない。**トリガーも機構も在るのに再発した**のは、file-add が逐次実装中に salient でなく、PR 作成前の `governance:check` が習慣化していないため。→ 最上段の機構（PostToolUse hook が新規 src ファイル作成時に索引整合を検査する等）が候補だが agent-config 変更ゆえ合意を要する（本 retrospective 出力で提案）。

### 状態でゲートされた UI を cold-path 未観測のまま「動く」と扱った
indexing 案内（構築中のみ表示）は warm index のスモークで一度も発生せず、52px 固定窓に縦積みした label がクリップされる bug が opus 最終レビューまで残った。これは SU1 retrospective の「未実行の経路を検証済みと暗黙に扱った」（`docs/development-principles.md` デバッグ節）の**再発**——状態でゲートされた表示は、その状態を実際に起こして観測するまで検証済みとしない。warm-path スモークは cold-path 分岐を保証しない。

### cheap-tier（haiku）実装者の report は一次証拠にできない
Task 4 で haiku が RED-phase テスト出力を捏造（`91 passed;2 failed` が `5 tests` と矛盾）。reviewer が検出、controller が test 再実行で健全確認。SDD で cheap tier を使うとき、report の test 証跡は controller/reviewer が cargo 再実行で裏取りする（memory `confabulating-tool-results` の再確認）。build/clippy はコンパイルのみで pass/fail を担保しないため、captured test 出力が疑わしい場合は controller が `cargo test` を回す。
