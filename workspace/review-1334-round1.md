# #1334 レビューと変異注入（sha f125249ded18ecd248575efd550c3be83ce576b7）

align: exit 0、作業ツリーは clean。巻き戻し後の `git hash-object .claude/hooks/pre-bash.mjs` は `HEAD:` の blob（91ef9f84）と一致した。

## レビュー所見

- Critical: なし
- High: なし
- Medium: なし
- Low
  - L1 `pre-bash.mjs`:19 の「development-principles §6」は序数で指している。実際に指しているのは「構造的設計原則と強制の階梯」節の項目 6 で、文書の 6 番目の `##` 節（「ドキュメントとコードの分担」）ではない。旧 hooks.md も同じ書き方をしていたが、今回新しく書いた行である。`.claude/rules/governance-docs.md` は番号で指すことを避けるよう求めているので、見出し名で指すのが妥当
  - L2 `lsp-config.mjs`:34「gitignore 済みだが、実在検査は ignore 対象を免除するので参照してよい（#1088）」は、.md のガバナンス文書（G-references の走査元）に向けた理由だった。G-references は `.mjs` を走査しない（`governanceDocs` は .md だけを返す）ので、この行き先では理由が当てはまらない。偽ではないが、意味のない一文になっている
  - L3 移すときに落ちた細部: バイナリ欠落の項の「`checkOnSave: false` で切ってあるので、plugin の使用カウンタが 0 でも報せは無い」。RA の層の表と `.githooks/_lib.sh` の項は、research で「消す（5）」と裁定済み
- ⚠️（確信が持てないもの）
  - W1 予算テスト (c) は定数の積「2 本 × 2 種」を手で書いており、git 呼び出しの本数そのものは見ていない。readGitState か verifiedSummaryState に 3 本目の spawnSync を足しても緑のままになる。また 28 秒 < 30 秒の差 2 秒には、Node の起動と動的 import の時間が入っていない（ただし後者は今回入った問題ではない）
  - W2 `judgeCommandShape` の doc「この状態へ至るのは呼び出し側が `decide` へ platform を渡さないとき」。製品コードの呼び出し元は `decide` だけなので真である（`git grep` で確認）。export された関数を直接呼ぶ側（テスト）は数えていない

確認して問題がなかったもの:
- 生きた参照: `git grep "hooks\.md"` で ADR・`docs/superpowers/`・`workspace/` を除くと、残りは G-stale-identifiers.mjs:96（過去形）と同 test の fixture 名だけ。`docs/superpowers/` は `governanceDocs` が除外している。旧見出し（「実装契約」「RA インスタンスと hook」）を指す参照も 0 件
- 移した記述の意味: 判定単位 2 種（非 ASCII はコマンド全体、python はコマンド位置、とした修正は正確）、行継続、`.\` 相対形、区切りの共有の帰結、壊れ方 3 分類、残余 2 件は、元の hooks.md と意味が一致している。fail-closed（判定不能は block）、要約ゲートの順序と例外なし、plan.md ゲートのルート導出、動的 import、2 本 × 4 秒は、もともとコードの doc にある
- 新しく書かれた事実: settings.json の構造（`PreToolUse[0].hooks[0].timeout = 30`）、`rust-analyzer-lsp@claude-plugins-official: false`（「公式 plugin を無効化してある」）、ADR-ra-diagnostics-suppression が `diagnostics` キーを置かない理由を持つこと、ADR-command-shape-norms-in-hook:38 が「例外を一般則へ書き戻さない」理由を持つこと。いずれも実ファイルで確かめた

## 変異注入（`npx vitest run .claude/hooks/pre-bash.test.mjs`、基準は 224/224 緑）

ハーネス: `C:/Users/Eoh/AppData/Local/Temp/claude/C--workspace-Snotra/a17c39a2-4e6a-449b-9130-6025e7a7e3e5/scratchpad/mutate.mjs`。一致が 1 件であることを確かめてから書き込み、ディスク上の内容のハッシュが変異後と一致し原本と異なることを確かめ、テストを回し、巻き戻して sha256 を照合した。

| 変異 | 当たった | 結果 | 赤の理由 | 巻き戻し |
|---|---|---|---|---|
| a1 `GIT_AT_CMD` の先頭を `(?:^\|\s)` に | yes | 1 本赤: 新規の `echo git commit --no-verify` のみ | AssertionError expected true to be false | 一致 |
| a2 `AT_CMD_POS` を `(?:^\|\s)\s*` に | yes | 4 本赤（新規 1 本 + ラッパ経由 3 本） | AssertionError | 一致 |
| b `startsWith("workspace/")` → `"workspace"` | yes | 1 本赤: 新規の workspaceX テストのみ | AssertionError expected true to be false | 一致 |
| c `VERIFY_GIT_TIMEOUT_MS = 40_000` | yes | 1 本赤: 予算カナリアのみ | expected 100000 to be less than 30000 | 一致 |
| d1 heredoc の文言から「Write ツール」を消す | yes | 1 本赤: 拒否文言テスト | expected … to contain 'Write ツール' | 一致 |
| d2 noVerify の文言から「理由を解消」を消す | yes | 1 本赤: 拒否文言テスト | expected … to contain '理由を解消' | 一致 |

a1・b・c・d2 では、赤になったのは新しく足したテスト 1 本だけだった。足す前は、これらの変異は検知されずに緑のまま通っていたことになる。timeout やモジュール解決の失敗による赤は 0 件。
