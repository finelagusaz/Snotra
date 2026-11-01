# #1334 2 巡目（sha 3f702e772fc5be8c4bab258b9ff03bd3d6235cb0）

align: exit 0。修正差分は f125249d..3f702e77（コミット 1 件）。触ったのは lsp-config.mjs・pre-bash.mjs・pre-bash.test.mjs のコメントだけで、あとは workspace/ に 2 ファイル。

## 1 巡目の指摘は解消したか（見つけたときと同じ方法で確かめた）

| 指摘 | 確かめ方 | 結果 |
|---|---|---|
| L1 序数の参照 | `grep "§[0-9]"`: 変更した 2 つの .mjs では 0 件。新しい正準形 `docs/development-principles.md`「構造的設計原則と強制の階梯」は 1 物理行に収まっている。`governance:check` は全 17 件 passed。着地しているかは、見出しを「…階梯X」に壊す変異で測った: G-heading-refs が `.claude/hooks/pre-bash.mjs:19 見出し参照が着地しない` で赤になった（照合に載っている）。`git checkout` で戻し、hash-object が HEAD の blob（6517eb3c）と一致した | 解消 |
| L2 当てはまらない #1088 の理由 | `grep "#1088\|ignore 対象"` lsp-config.mjs: 0 件 | 解消 |
| L3 落ちた細部 | 差分を目視した。旧 hooks.md の文言（checkOnSave: false・使用カウンタ 0・報せは無い）と意味が一致している | 解消 |
| W1 予算テストの限界 | コメントだけの変更（テストの本体は変わっていない）。「残り約 2 秒」= 30 − 28 で真 | 明記された（検知の限界そのものは残る。意図どおり） |
| W2 製品コードに限定 | 「製品コードでこの状態へ至るのは」。製品コードの呼び出し元が `decide` だけであることは 1 巡目に git grep で確認済み | 解消 |

## 修正差分が新しく持ち込んだもの

- ⚠️ N1（Low）pre-bash.mjs:19〜20 の括弧書き「一般則（検出は構造化された信号で行い、判定単位は構文的位置で定義する）」。heredoc・`\`・非 ASCII の判定が逸脱しているのは、項目 6 の派生則「判定単位は…構文的位置で定義する」だけである。項目 6 の本体「検出は構造化された信号で行い、テキストは証拠に留める」（exit code 等で判定する話）から逸脱しているわけではない。1 巡目までの「（構文的位置で判定単位を定義する）」より、逸脱の範囲が広く読める。直すなら括弧を派生則だけに戻す
- ⚠️ N2（範囲外の確認）workspace/ に verify-summary-round1.json（ローカルの worktree の絶対パスと log パスを含む）と review-1334-round1.md がコミットされた。要約ゲートの対象外（`workspace/` の接頭辞）なので PR ゲートには影響しない。squash の前に workspace/ が消える前提が崩れると、main の履歴に残る。中身はリポジトリのパスだけで、利用者のデータは無い
- このほか偽の記述、着地しない参照、意味の変化は見つからなかった

範囲外（今回の差分ではない既存の行）: pre-bash.test.mjs:105 の「§2.2:」も序数による参照である。

## 変異の当て直し（現在のアンカーで a1〜d2 をすべて）

基準 224/224。どの変異も、書き込みが当たったことをハッシュで確かめ、巻き戻しを sha256 で照合した（原本 1b966f20…）。最後に `git status` は clean（.delegate/ を除く）で、hash-object は 6517eb3c。

| 変異 | 結果 | 赤の理由 |
|---|---|---|
| a1 GIT_AT_CMD の先頭を緩める | 1 本赤 | AssertionError expected true to be false |
| a2 AT_CMD_POS を緩める | 4 本赤 | AssertionError |
| b workspace/ の接頭辞を緩める | 1 本赤 | AssertionError |
| c VERIFY 上限 40 秒 | 1 本赤 | expected 100000 to be less than 30000 |
| d1 heredoc の文言から Write ツールを消す | 1 本赤 | to contain 'Write ツール' |
| d2 noVerify の文言から理由を解消を消す | 1 本赤 | to contain '理由を解消' |

1 巡目と同じ結果。timeout やモジュール解決の失敗による赤は 0 件。
