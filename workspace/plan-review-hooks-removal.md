# plan-review: #1334 hooks.md 撤去（観点 A 削除の網羅 / 観点 B 参照の付け替え）

## 実測の要点
- 撤去状態を複製（git archive HEAD から docs/hooks.md を除いた scratchpad の sim/）で governance:check に掛けた結果は 8 件。内訳は CLAUDE.md:19・lsp-config.mjs:8/121・AGENTS.md:23・development-principles.md:78/89/93 で、計画の書き換え対象と過不足なく一致した。ADR・G-stale-identifiers.mjs:96（.mjs の注釈）は検査に現れない（後者は計画が手で拾っている）。
- 観点 A の「3既」を pre-bash.mjs / lsp-config.mjs で読み直した: A1 A2 A4 A5 A6 B1 B2 B3 B4a B4d B4f C3 C7 は主張どおりコメントに内容がある（先頭コメント 1-30、AT_CMD_POS、FLAG、節コメント、usesHeredoc doc、BACKSLASH_PATH、needsPyEncoding doc、gitSegments doc、SHAPE_REMEDY doc、readPlanState doc、VERIFY_GIT_TIMEOUT_MS doc、main() 動的 import、invokedDirectly）。テストの名指しも実在（pre-bash.test.mjs:260,553,736,780,831,929,1001,1154）。

## 要対処
1. **件数「全 23 条項」は表と合わない。** research.md の行は A=6、B=11（B0,B1,B2,B3,B4a-g）、C=9 で 26 行。件数行の 12+4+3+4=23 は誤り。PR 本文の台帳は「行 ID ごと」と書き、数は載せない（数え上げは腐る）。あわせて hooks.md の導入（1-7 行: PostToolUse 撤去注記と CLAUDE.md「フック」節・development-principles・safety-nets.md への指し先 3 本）が表に行を持たない。「消してよい（指し先は CLAUDE.md 側に既にある）」と台帳に 1 行立てる。
2. **テスト②（git 上限の予算）は定数が非 export。** `GIT_TIMEOUT_MS`（pre-bash.mjs:46 付近）・`VERIFY_GIT_TIMEOUT_MS` は export されず、テストも import していない（pre-bash.test.mjs:9-31）。計画の pre-bash.mjs 行に export 追加の記述が無く、ソース読みで取るのかも未定。どちらかを決めて pre-bash.mjs の行へ書く（export なら「挙動変更なし」と、検証の層の付き合わせ）。予算は 4s×2 + 10s×2 = 28s < 30s（settings.json:10）で余裕 2 秒。「未満」の比較であることを明記する。
3. **テスト③は既存テストと重複する。** pre-bash.test.mjs:1001「拒否文言は代わりの手段を含む」が既に 5 件へ assert している。弱いのは noVerify の `人間専用`（代替手段の語ではない。代替は「feature ブランチと PR」）で、heredoc も `git commit -F` 1 語。新設ではなくこの it を強める、と計画に書く（/dry-check）。
4. **CLAUDE.md:19 の行き先が「判定の一覧」を持たない。** 文は「判定の一覧と改修時の契約は…」だが、先頭コメントにあるのは契約（fail-closed）とゲート(1)-(4)で、5 判定の一覧は節コメント（pre-bash.mjs:127 付近）と `judgeCommandShape` / `SHAPE_REMEDY` にある。指し先を「先頭コメントと `judgeCommandShape`」にするか、文を「改修時の契約は先頭コメント」へ縮める。
5. **development-principles.md の書き換え指示が選択式で未確定。** :78 は hooks.md の半分だけ削り、AGENTS.md の例が残るので「反例が実在する」は偽にならない（差し替え不要と決める）。:89 の行を消すと「射程の宣言と逸脱の明記の対」を名乗る役割行が表から無くなる。:79 の (2) は一般形なので残してよいが、その実例の所在は pre-bash.mjs 先頭の A3 注記になる、と一言添えるか、行ごと消すと割り切るかを決める。:93 は「常時ロード面自身の行」へ。
6. **lsp-config.mjs:8 は自己参照になる。** 「沈黙しない枝は hooks.md が正本」を外すだけでは、分類を同じ doc の中へ移した後に「正本は上」を言う文になる。:8 の文を移した C6 の表への内向きの指しにする（:121 は C8 の節を指す）。

## 軽微
- B2: 偽の「第 4 引数」「経路は 1 本だけ」は `judgeCommandShape` の doc（pre-bash.mjs:280 付近）。署名は `(command, platform)` で、options を受けるのは `decide`。直す文は 2 関数に分けて書く（判定関数は値、decide は options）。
- C1（RA は navigation の道具で検証の権威ではない）は、LSP の状態を検証の根拠にしかける人が読む場所に置かれず、lsp-config.mjs を編集する人にしか届かない。裁定は争わないが、`.claude/lsp/snotra-rust-lsp/.claude-plugin/plugin.json` の description 等に 1 行の入口を置けるか確かめる価値がある（⚠️ 到達性の懸念・未実測）。
- hooks.md:101 の「settings.local.json は gitignore 済みだが実在検査は ignore 対象を免除するので参照してよい（#1088）」と、:100 の「古い枝から作った worktree は公式 plugin へ素直に落ちる」・:99 の実測の形（サーバ名だけ変えたら登録はメインツリー側）は、計画の C8/C9 移設に明記が無い。台帳へ行を立て、移すか捨てるかを決める。
- 撤去の語彙の数え直し（`git grep`）は、`.claude/worktrees/agent-verify-chore-refine-hooks-doc/` の古い複製（gitignore 下）に旧 hooks.md がある。git grep では見えず問題にならないが、手で grep すると誤読する。
- ADR 5 本が「分担の正本は docs/hooks.md」と現在形で書く（ADR-claude-code-ra-lsp-plugin-delivery.md:16 など）。凍結ゆえ触らない方針に同意するが、PR 本文に「ADR は宛先を失うが凍結」と 1 行残すと追える。

## 未検証
- 計画の移設後の `governance:check` 緑（上の sim は撤去のみ。書き換え後は実走が要る）。
- 変異 M1-M4 と、安いテスト 3 種の変異による赤（委譲先の仕事）。再測定していない。
- C6/C8/C9 の移設文面そのもの（計画にあるのは指示まで）。
- lsp-config.mjs の先頭 doc 肥大後の comment-guidelines 適合。
