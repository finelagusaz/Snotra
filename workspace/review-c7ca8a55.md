# code-review round 2: c7ca8a55 (fix of 58765a39)

Critical 0 / High 0 / Medium 0 / Low 2 (warn)

## 解消の検算（58765a39..c7ca8a55）
- M1 解消: isListedInPlan doc と SKILL 項目 7 が「計画が一度も名指していない」と意味を明記。
- L1 解消: GREP_FLAGS/GREP_PATHSPEC から runGrep 引数と見出しを導出。引数順は旧版と同一（grep -I --untracked -n -z -F --no-color -e 語 -- . :!workspace）。実走の見出しで確認。
- L2 解消: 測定値を帰結へ。
- L3 解消: テンプレート内の 4 連バックスラッシュは正規表現上の 1 文字。実測: x+BS+CLAUDE.md -> false、-CLAUDE.md -> false（- は範囲にならず literal）。
- L4 解消: 数字列判定。空欄でテストが throw を確認。
- L5 受容: 主の木に .delegate/ は無い（受容に同意）。
- L6 解消: 0 件の原因の断定を削除し「でも 0 になる」へ弱めた。
- W1 解消: 過去形の実測へ弱めた。

## 新所見
- N1 Low (warn) build-commands.md の「Git Bash では起きない」は現在形の全称で、根拠は #1300 の 1 環境だけ。「Git Bash では起きなかった」へ。
- N2 Low 末尾 lookahead へ足したバックスラッシュ（変異 g2）を赤にするテストが無い。ファイルパスの直後に区切りが続く入力は実在しにくく実害はほぼ無いが、検査されない分岐。テストを 1 件足すか、末尾側は戻す。

## 変異注入（35 件 baseline green・復元ハッシュ一致 3e2c7d7b）
g L3 巻き戻し（前方クラスから BS 除去）: 検知 AssertionError（区切りテスト）
g2 末尾 lookahead から BS 除去: 非検知（N2）
h L4 巻き戻し（Number(parts[1])）: 検知 AssertionError（空欄テスト）
i 末尾 lookahead 削除: 検知 AssertionError（a.mdx テスト）
