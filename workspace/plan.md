# race:boundaries の追跡差分を設定非依存にし、照合で黙って落ちる形を止める（Refs #1300 の作業中に発見）

## 目的

`scripts/race-boundaries.mjs` の `git diff -U0 <mergeBase>` は porcelain であり、出力の形が利用者の git 設定と cwd で変わる。
一時リポジトリで本物を走らせて、次の 6 形が黙って母集団を欠くことを実測した（日本語名ファイル・サブディレクトリ実行）:

| 設定 | 結果 |
|---|---|
| `core.quotePath=true`（git 既定） | 日本語名ファイルが落ちる・exit 0 |
| `diff.relative=true`（サブディレクトリから） | cwd の外の変更が全部落ちる・exit 0 |
| `diff.mnemonicPrefix=true` | 名前が `w/…` に化け、日本語名が落ちる・exit 0 |
| `diff.noprefix=true` | 日本語名が落ちる・exit 0 |
| `color.ui=always` / `diff.external` | 追跡の変更が全部落ちる。未追跡が 1 件でもあれば exit 0 で ① 0 件 |

真因: 表示用出力を解き直していたこと、自己点検の数（変更ファイル N 件）が同じ壊れた経路から出ていたこと、
テストが本物の git でなく作者の想定（fake・手書き diff）を相手にしていたこと。

## 方針（ユーザー合意済み: A + B + C、D は ADR へ却下として記録）

- **A**: 追跡差分を plumbing `git -c core.quotePath=false diff-index -M -p -U0 <mergeBase>` で取る
- **B**: 別経路 `diff-index -M --numstat -z` のファイルごと追加行数と、patch から解いた `+` 行数を照合し、食い違えば exit 3
- **C**: 本物の git に上の 6 設定を注入し、日本語名ファイルとサブディレクトリ実行で道具全体を回すテスト
- **D（却下）**: `GIT_CONFIG_GLOBAL=/dev/null` + `GIT_CONFIG_NOSYSTEM=1` による隔離——リポジトリ local の設定は残り、`safe.directory` が消えて fatal を生む

## 受け入れ条件

- 上の 6 設定のそれぞれで、日本語名・サブディレクトリ外・通常のファイルの追加行がすべて母集団に入る（C で測る）
- patch の名前が numstat と一致しない、または追加行数が食い違うと exit 3 で止まり、0 件の報告を作らない
- 既存の挙動（行の集合）は変わらない——基準: このブランチで `main`〜`main~600` の 4 基準（作業ツリーを含む）について、旧 `git diff -U0` + 旧 `parseDiff` と新 `readTrackedRows` の行の集合が完全一致（巡 1 の修正後に測り直し済み）

## 変更ファイル

- `scripts/race-boundaries.mjs`: `parseNumstatZ`・`diffMismatches`・`readTrackedRows(git, base)` を足し、`main()` を差し替える。冒頭の契約欄と `main()` 内コメントを更新
- `scripts/race-boundaries.test.mjs`: 純関数のテスト・fake git による引数の固定・本物の git での敵対的設定テスト
- `docs/adr/ADR-race-boundaries-git-plumbing.md`: D と porcelain 継続（設定の列挙で塞ぐ形）の却下

## 不変条件と異常系

- 照合は追加・削除の両方の行数を見る。削除されたファイル（追加 0）は欠落を問わない。バイナリ扱いのソースは exit 3
- patch 側の名前が numstat に無い → 化けた名前（quote・接頭辞・色）。行数の不一致 → 行が落ちた
- hunk の本文は `@@` が宣言した行数だけ読む（`++ ` / `-- ` で始まる行をヘッダと誤らない）
- rename は numstat -z で `<add>\t<del>\t\0<old>\0<new>\0` の形（実測）。新しい名前で照合する
- 照合の相手は plumbing に限る——porcelain `diff --numstat -z` は `diff.relative` で同じだけ狭まり、黙って一致する（実測）

## 未確定（実装前に潰す）

- [x] `diff-index` は stat だけ変わったファイルを出すか — refresh を挟まない一時リポジトリで実測: patch にも numstat にも出ない。出ても追加 0 なので照合に影響しない。`update-index --refresh` は不要
- [x] rename・バイナリの出方 — 実測: `-M` で porcelain と同じ `+++ b/<new>`、numstat -z は上の形、バイナリは `Binary files … differ` と `-\t-`
- [x] plumbing は 6 設定のうちどれを無視するか — 実測: color・external・mnemonicPrefix・noprefix・relative を無視、quotePath だけが残る（`-c` で消す）
- [x] 旧と新で行の集合が一致するか — 受け入れ条件の 3 基準で完全一致

## 作業項目

- [x] Red: 純関数（`parseNumstatZ`・`diffMismatches`）のテストを no-op 実装に当てて落とす
- [x] Green: 純関数を実装
- [x] `readTrackedRows` を足し、fake git で引数の列を固定するテスト
- [x] `main()` を `readTrackedRows` へ差し替え、照合失敗を exit 3 にする。契約欄とコメントを更新
- [x] C: 本物の git で 6 設定 × 日本語名・サブディレクトリ外を回すテスト（旧実装で赤になることを確かめる）
- [x] ADR を起こす
- [x] 巡 1 レビュー対応: hunk の行数で読む `parseDiff`（M-1/M-2）・削除行数の照合（M-2）・バイナリ扱いのソースを exit 3（M-3）・exit 3 の e2e テスト（M-4）・setup の隔離と後始末（L-1）・rename と `++ ` 行の e2e（L-2）・文言（L-3/L-4/L-6/L-7/⚠-1/⚠-3）
- [x] 巡 2 レビュー対応: numstat の del を fixture と本物の git の書き換えで固定（Medium-A）・削除されたファイルを `--diff-filter=D` で知り照合から外す（Low-A・⚠️-A）・宣言より短い hunk を止める（Low-B）・context 行の fixture（Low-C）
- [x] 巡 3 レビュー対応: hunk の中の ` No newline` を単体と本物の git（改行で終わらないファイルの書き換え）で固定（Medium-B）・読み取り回数の文言（Low-D）・削除ファイル除外の前提をコメントへ（⚠️-C）

## 人間レビュー

- [x] 承認済み — 2026-10-25 / 問い: "この方針（A + B + C、D は ADR へ却下として記録）で計画を `workspace/plan.md` に起こし、実装へ進めてよろしいかしら。A の `update-index --refresh` の要否は、計画の未確定欄で実測してから決めます。" / 回答: "その方針で進めよう"

## セルフレビュー

- リスク: 通常（scripts の 1 道具・既存の挙動は基準で一致を確認）
- plan-review: 未実施（通常リスク）
- エージェント数: 0
- 要対処: なし
- 未検証: なし
- 散文の全件列挙: 未実施（通常リスク）
