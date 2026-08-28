# 調査 — issue #766: `.claude/agents/*.md` の frontmatter で `effort` が honored されるか

## issue の要約

Anthropic の "Prompting Claude Opus 5" が `low` / `medium` の effort を「品質を保ったままトークンとレイテンシを下げるコストの主レバー」として挙げている。PR #764 でスキル・エージェントへ適用しようとしたが、**`.claude/agents/*.md` の frontmatter で `effort` が honored されるか、およびそのキー名**が確定できなかった。

やること: `.claude/agents/code-reviewer.md`（`model: inherit` のみを持つ唯一の永続サブエージェント定義）で実測し、効くなら 3 フェーズに妥当な水準を決める。優先度は低いが実測は 1 回で済む。

## 計測環境

| 項目 | 値 | 取り方 |
|---|---|---|
| Claude Code | 2.1.250 | `claude --version` |
| 本体 | `~/.local/bin/claude`（226 MB の PE32+ 単一実行ファイル・JS バンドル同梱） | `which claude` / `file` |
| 永続サブエージェント定義 | `.claude/agents/code-reviewer.md` 1 枚のみ | `ls .claude/agents/` |

**issue が書かれた時点（2026-07-27 前後）とは版が違う。** 下の「issue の前提が古くなっている」はこの差から出る。

## 確定した事実（本体バイナリの一次読み取り）

抽出は `grep -aob <文字列> <本体>` でオフセットを取り、`dd` で近傍を読む方法による。**オフセットは版ごとに動くので再導出のための検索文字列を併記する**（数値は本セッションでの実測値であり、次の版では使えない）。

### F1. `.claude/agents/*.md` のローダは `effort` を読み、検証し、agent 設定へ載せる

`.md` ローダ（検索文字列: `has invalid effort`）は次の形を持つ。

```
let fe=r.effort, we = fe!==void 0 ? RA(fe) : void 0;
if (fe!==void 0 && we===void 0)
  n(`Agent file ${e} has invalid effort '${fe}'. Valid options: ${lg.join(", ")} or an integer`);
```

`Hin(e,".md")` を同じ関数が呼んでおり、`.md` ファイルのローダであることが読める。

**キー名は `effort` である。** `when-to-use` のようなケバブ別名は要らない（正規化表 `L` / `q`〈検索文字列: `"disable-model-invocation","user-invocable","effort"`〉が `-`/`_` 除去と小文字化で正準名へ寄せるので、`Effort` のような綴りも同じキーへ着地する）。

同じ形が**別の 2 経路**にもある。どれも返り値のオブジェクトへ `...effort!==void 0 && {effort}` を載せる。

- plugin agent のローダ（検索文字列: `Plugin agent file`）
- JSON / settings 由来の agent 定義（検索文字列: `Error parsing agent '`）

plugin 側だけは `permissionMode` / `hooks` / `mcpServers` を無視すると警告し、**「この水準の制御が要るなら `.claude/agents/` を使え」と誘導する**——`.claude/agents/` の方が広い集合を受けることが、この文言からも読める。

### F2. 有効値は `low` / `medium` / `high` / `xhigh` / `max` または整数

```
var lg = ["low","medium","high","xhigh","max"];
function RA(e){ /* 数値ならそのまま / 別名表 k で正準化 / 整数文字列も受ける */ }
```

**zod スキーマの `describe` テキストは 4 つしか挙げておらず（`xhigh` が無い）、`lg` と食い違う。** 有効値の正本は `lg` であって describe テキストではない。

### F3. agent frontmatter の zod スキーマは **shadow validator** であって、挙動のゲートではない

`aZt = { skill: oZt().strict(), agent: sZt().strict(), "output-style": iZt().strict() }` を使う `TE(e,t)` は `safeParse` の失敗時に**テレメトリだけ**を撃つ（`tengu_frontmatter_shadow_unknown_key` / `tengu_frontmatter_shadow_mismatch`。検索文字列: `frontmatter_shadow:`）。

**ゆえに「スキーマに載っている」ことは honored の証拠にならない。** 効くことの根拠は F1 のローダ側である。逆に、**スキーマに無いキーを書いてもエラーにならず黙ってテレメトリが飛ぶだけ**という含意もある。

`sZt`（agent 用スキーマ）が挙げるキーは宣言順に: `name` / `description` / `model` / `tools` / `disallowedTools` / `color` / `effort` / `permissionMode` / `mcpServers` / `hooks` / `maxTurns` / `skills` / `initialPrompt` / `memory` / `background` / `isolation` / `observer` / `observerMessage` / `observeSubagents` / `experimental`（`cacheTtl`）。

### F4. effort は 3 状態の解決値として持ち回される

`{kind:"inherit"} / {kind:"default"} / {kind:"level", value}` の凍結オブジェクトと、`X5(e)`（未定義 → `default`）・`H(e)`（未定義 → `inherit`）の 2 つの包み方がある（検索文字列: `kind:"inherit"`）。env `CLAUDE_CODE_EFFORT_LEVEL` を読む経路（`sC()`）と、settings の `modelSettings[].effortLevel` を読むモデル別の解決もある。

## issue の前提が古くなっている

**issue の「確定した事実」のうち 1 つは 2.1.250 では偽である。**

issue は「`SKILL.md` の frontmatter に `effort` は書けない。キー集合は 7 つで、完全な並びを抽出したため不在が言える」と書く。しかし 2.1.250 の skill スキーマ `oZt` は基底 `rZt` を extend しており、**その基底が `effort` を持つ**（検索文字列: `Thinking effort for the model`）。基底のキーは `name` / `description` / `model` / `allowed-tools` / `disallowed-tools` / `argument-hint` / `arguments` / `disable-model-invocation` / `user-invocable` / **`effort`** / `shell` / `version`。

**この訂正は issue の結論を変えない**（issue が求めたのは agent 側の確定であり、そちらは F1 で肯定に決まる）。ただし**メモリ `skill-frontmatter-has-no-effort-key` が偽になっている**ので、その訂正が要る。

⚠ 注意: skill 側も F3 と同じ理屈で、スキーマに載っていることは honored を意味しない。skill 側の honored は本 issue の射程外なので、**確定できたのは「不在という過去の主張が偽になった」ところまでである**。

## 確定した事実（続き — 敵対的調査のあとに足した分）

### F5. agent 定義の `effort` は、model の上書きと同じ配列へ積まれる（自分で再確認した）

サブエージェント spawn の経路（近傍に `r.options.agentDefinitions.activeAgents` / `agentContext` / `systemPrompt` / `CLAUDE_CODE_ENABLE_APPEND_SUBAGENT_PROMPT` が並ぶ）に次がある。検索文字列: `kind:"effort"`。

```
Di = [ {kind:"model", mainLoopModel: hn},
       ...e.effort !== void 0 ? [{kind:"effort", effort: e.effort}] : [] ]
```

`e` は agent 定義である。**model の上書きと同じ形・同じ配列に載る**ことが、この 1 行の意味である。同じ `{kind:"effort"}` を Skill ツールの経路も `contextLayers` へ push している（検索文字列: `SkillTool returning`）。

⚠ **この配列が最終的に API のペイロードへどう変換されるかは追っていない。** ゆえに「届く」は**model の上書きと同じ経路に載る**という形の主張であって、末端の直接トレースではない。

### F6. skill 側もローダが `effort` を読んで伝播させる

敵対枠がスキルの実ローダを追い、`effort` を読んで検証し最終オブジェクトへ載せることを実測した。**F3 の但し書き（スキーマは根拠にならない）を skill 側でも越えている**ので、「`SKILL.md` に `effort` は書けない」は 2.1.250 では**ローダ水準でも**偽である。

## 測定環境（実測。ここが計画の前提を決める）

**baseline は「未設定」ではない。**

| 場所 | 値 | 取り方 |
|---|---|---|
| `~/.claude/settings.json:130` | `"effortLevel": "high"` | `grep -nE "effort\|modelSettings\|ultracode"` |
| `.claude/settings.json` / `.claude/settings.local.json` | 該当なし | 同上 |
| env `CLAUDE_CODE_EFFORT_LEVEL` | 未設定 | `env \| grep -iE "^CLAUDE.*EFFORT"` |
| env `CLAUDE_EFFORT` | `high` | 同上 |

**`CLAUDE_EFFORT` は上書きの入力ではなく、現ターンの解決済み effort を hook へ渡す出力である**（敵対枠の所見。`CLAUDE_CODE_EFFORT_LEVEL` とは別物で、取り違えると誤った警鐘になるところだった）。

この 2 つが計画に効く。

- `code-reviewer.md` は現在 `effort` キーを持たないので、その agent は `{kind:"level"}` を積まない。**比較の baseline は `high` であって「既定」ではない**——「効果なし」と読み違える余地がここにある。
- **`CLAUDE_EFFORT` が「解決済み effort の出力」なら、サブエージェント自身に `env` を読ませれば、そのサブエージェントの解決値が 1 起動で読める。** トークン数の比較より決定的である（⚠ サブエージェントの Bash にも同じ変数が同じ意味で入るかは未確認）。

## 敵対的調査（3b）の採否

5 命題すべて「壊せなかった」。採った所見と理由。

| 所見 | 採否 | 理由 |
|---|---|---|
| `.claude/agents` ディレクトリの走査経路まで特定（研究が名指ししていなかった呼び出し元） | 採用 | 命題 1 の根拠が「`.md` を読む関数」から「`.claude/agents` の `.md` を読む関数」へ強くなる |
| zod の戻り値がカンマ演算子で明示的に捨てられている | 採用 | F3 の直接証拠。ロード可否は生 frontmatter の `description` 有無だけで決まる |
| skill の実ローダが `effort` を読み伝播させる | 採用（F6） | 調査が「射程外」としていた部分を埋めた |
| `~/.claude/settings.json` のトップレベル `effortLevel: high` | **採用・自分で再測した** | 計画の baseline を決める。静的読みだけだった所見を `grep -n` で裏取りした |
| `CLAUDE_EFFORT` は上書きではなく出力 | 採用 | 取り違えれば「環境が汚染されている」という誤った結論になっていた |
| `RA` / `w` の識別子がバンドル内で無関係な別関数として再利用されている | 採用（方法論として） | **テキストの近さだけで同定すると誤る**。以後の読みで呼び出し元の意味を確かめる根拠にする |

## 残る不明点（計画で潰す）

1. **agent frontmatter の `effort` が、そのサブエージェントの解決 effort を実際に動かすか。** F5 で「model の上書きと同じ配列に載る」ところまでは確定したが、末端は追えていない（敵対枠も「追えなかった」と明記した）。
2. **サブエージェントの Bash 環境に `CLAUDE_EFFORT` が、そのサブエージェント自身の解決値として入るか。** 入れば 1 起動で決定的に測れる。入らなければトークン数の比較へ落ちる（baseline が `high` なので `low` との差は出るはずだが、課題の難易度に左右される弱い観測量である）。
3. **agent 定義はセッション中に再読み込みされるか。** ローダ側に `r.agents ??= (async()=>{...})()` の形のメモ化が見える（検索文字列: `agents??=`）。キャッシュされるなら、**測定のために書き換えたファイルが同一セッションでは効かない**——計器が測る枝と変更が触る枝が違う形そのものである。

## 再利用できる既存パターン

- **不正値のエラーメッセージが決定的な 1 回プローブになる。** `effort: bogus` を書けば `Agent file ... has invalid effort 'bogus'. Valid options: low, medium, high, xhigh, max or an integer` が出るはずで、**出れば「そのキーが読まれている」ことが 1 起動で確定する**（`lg` の 5 値も同時に画面で裏取りできる）。挙動の差を測るより先にこれを打つ。
- メモリ `measure-whether-detector-can-fire`: 検出器を確定する前に注入する変異を書き下ろす。ここでは「不正値」「有効値」「キー無し」の 3 条件が変異に当たる。
- メモリ `adversarial-frame-must-question-the-measurement-environment`: 測定環境そのものを疑う。本件では**版**（2.1.250）と**設定の上書き**（env `CLAUDE_CODE_EFFORT_LEVEL`・settings の `modelSettings[].effortLevel`・`/config` の effort）が agent 側の値を覆いうる。

## 技術的制約

- **`.claude/agents/` の定義は 1 枚しか無い**（`code-reviewer.md`）。これはチームの共有物であり、`AGENTS.md`「条件別チェック」のセーフティネット行に当たる——**変更には合意が要る**（ルート `CLAUDE.md` 最重要ルール 2）。
- `code-reviewer` を起動するのは `/implement`「3b. 委譲へ渡すもの」である。ルート `CLAUDE.md` は 3 フェーズ（実装検証 / 計画判断・SPEC.md 同期 / パフォーマンス）と書く。
- 本体バイナリは 226 MB あり、`grep -aob` + `dd` 以外の読み方は現実的でない。**出力が大きいと Bash がファイルへ退避してプレビューだけを返す**（メモリ `bash-output-preview-hides-the-hit`）ので、検索は必ず件数を絞る。

## 未解決の疑問

1. 実リクエストへ届くことをどう観測するか。トークン数は課題の難易度に左右されるので単独では弱い。
2. 効くとして、`code-reviewer` の 3 フェーズに単一の水準を当てるのが妥当か（フェーズごとに変える手段は frontmatter に無い）。
3. issue の「効くなら水準を決める」は、セーフティネットの変更に当たる。**実測の報告と、`code-reviewer.md` を実際に変えるかの判断は分ける**べきではないか。
