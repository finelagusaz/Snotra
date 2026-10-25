# ADR-race-boundaries-git-plumbing: race:boundaries の追跡差分を plumbing で取り、別経路の numstat と照合する

## 文脈

`scripts/race-boundaries.mjs` は追跡ファイルの差分行を porcelain の `git diff -U0 <mergeBase>` から解いていた。#1300 の作業中に、本物の git に設定を 1 つずつ注入した一時リポジトリ（日本語名ファイル・サブディレクトリ実行）で測ると、`core.quotePath=true`（git の既定）・`diff.relative`・`diff.mnemonicPrefix`・`diff.noprefix`・`color.ui=always`・`diff.external` の 6 形とも行が黙って落ち、多くは exit 0 で報告した。開発機の `~/.gitconfig` が `quotepath false` だったため表に出ず、fake と手書き diff のテストは git の実際を見ていなかった。

## 決定

追跡差分は `git -c core.quotePath=false diff-index -M -p -U0` で取り、同じ plumbing の `--numstat -z` が出すファイルごとの追加・削除行数と照合する。食い違えば exit 3。hunk の本文は `@@` が宣言した行数だけ読む。削除されたファイルの行は `--- a/<path>` に帰属させ、入れるかは `--include-removed` だけが決める。本物の git に 6 設定を注入するテストを置く（global / system の設定は外して git の既定から始める）。

## 検討した代替案と却下理由

- **porcelain のまま、形を変える設定を引数で 1 つずつ打ち消す**（`--no-color --no-ext-diff --no-relative --src-prefix=a/ --dst-prefix=b/`）: 却下。塞いだ設定の列挙は git の版や新しい設定で古びる。plumbing は測った 6 形のうち quotePath 以外を性質として無視する。
- **git の設定を環境変数で隔離する**（`GIT_CONFIG_GLOBAL=/dev/null` + `GIT_CONFIG_NOSYSTEM=1`）: 却下。リポジトリ local の設定は残るうえ、global に置いた `safe.directory` も消え、所有者の異なるリポジトリでは git が拒む（git の仕様。この形は測っていない。テストでの隔離は、一時リポジトリが自分の所有ゆえ当たらない）。
- **照合を置かず、`parseDiff` が知らない形のヘッダを受け取ったら throw する**: 却下。色や外部 diff でヘッダそのものが読めない形では、throw する行が現れず、行は黙って落ちる。落ちたことを知るには別経路の数が要る。
- **hunk の本文を接頭辞（`+++ ` / `--- `）で見分け続ける**: 却下。`++ ` で始まる追加行は patch で `+++ …` になり、ファイル名が切り替わった（照合が exit 3 で止めた。照合が無かった旧実装では黙って別ファイルへ帰属した）。`-- ` で始まる削除行は、追加行だけの照合を素通りして落ちた。どちらも委譲レビューが本物の git で実測。
- **照合の相手を porcelain の `git diff --numstat -z` にする**: 却下。`diff.relative` で patch と同じだけ狭まり、黙って一致した（実測）。2 経路は同じ設定に引きずられないときだけ独立である。
- **`-M` を付けない**（rename を削除と追加に分ける）: 却下。移動しただけのファイルの全行が母集団へ入り、旧 `git diff`（rename 検出が既定で有効）と行の集合が変わる。`-M` 付きの新から削除されたファイルの行を除くと、旧 `git diff -U0` と、このリポジトリの 4 基準（`main`〜`main~600`・作業ツリーを含む）で行の集合が完全一致した。
- **削除されたファイルの行を捨て続ける**（#806 の導入時からの形）: 却下。`--include-removed` は廃止の検査のための指定なのに、ファイルごと消した worker が映らず、削除だけのブランチは空の母集団として「基準が誤っている」と誤診された（セルフレビューが実測）。理由の記録は見つからなかった。
- **削除されたファイルを 3 本目の `diff-index --diff-filter=D` で知り、照合から外す**（一時期の形）: 却下。patch に `--- a/<path>` と `Binary files a/<path> and /dev/null differ` が既にある。git の呼び出しが増え、読み取りの間に作業ツリーが動く窓が広がるだけだった。

## 帰結

- 照合が食い違うと、母集団を欠いた報告の代わりに exit 3 と食い違いの一覧が出る。
- 削除されていないバイナリ扱いのソース（`.gitattributes` の `-diff` 等）も exit 3 になる。旧実装はこれを exit 0・0 件で黙って落としていた（委譲レビューが実測）。
- `--include-removed` の母集団に、削除されたファイルの行が加わる（4 基準で最大 6978 行。既定の母集団は変わらない）。
- quotePath を外しても `"` `\` 制御文字を含むパスは quote されたままであり、その形は照合が exit 3 で止める（Windows では作れないので本物の git では測っていない）。
- patch と numstat を別々の git 呼び出しで読むので、その間に作業ツリーが動くと exit 3 になる。再実行で消える。
