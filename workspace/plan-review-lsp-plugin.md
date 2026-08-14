# 独立導出による網羅性レビュー — #1083（project-scope LSP plugin）

作成: 2026-08-14 / レビュア: 独立導出枠（`plan.md` / `research.md` 非参照で開始）

---

## 0. 独立性の宣言（汚染の開示）— 先に読んでください

**このレビューの途中で `workspace/research.md` と `workspace/plan.md` の一部を意図せず読みました。**
`*.md` 全域への概念ラベル grep（ブリーフの仕事 4）に `workspace/` の除外を掛け忘れ、Grep ツールの
`output_mode: content` が両ファイルの行を返しました。

**汚染の線は明確です。** 以下はすべて grep より**前**に、`claude.exe` のバイナリ文字列と
リポジトリのソースから自分で導出しています（トランスクリプトの順序で確認できます）:

- `.lsp.json` のスキーマ（`command` / `args` / `extensionToLanguage` / `transport` / `env` /
  `initializationOptions` / `settings`）と、`.lsp.json` 単体で plugin と認識されること
- 公式 `rust-analyzer-lsp` が `.lsp.json` を持たず marketplace entry 側の `lspServers` で宣言していること
- `extraKnownMarketplaces` が repository `.claude/settings.json` 用の正規経路であること
- marketplace source に `{"source":"directory","path":...}` が在り、相対パスが project dir 基準で resolve されること
- 拡張子の二重ぶら下げが「先勝ち + warn」であること
- **第 1 章〜第 4 章の導出（ファイル集合・シンボル集合・検査が発火するか・偽になる散文）の全体**

**漏れ込んだもののうち、私が導出していなかったもの**（下の所見からは外し、⚠️ 未検証へ出典つきで置きました）:

| 漏れ込んだ内容 | 扱い |
|---|---|
| research.md 証拠 F「`rust-analyzer.toml` が Claude Code の RA に届いていない（実測）」 | ⚠️ に「leak・独立検証なし」として記載 |
| plan.md のスコープ裁定（今サイクルは `checkOnSave` まで・RA_LOG は PR チェックリスト送り） | ⚠️ に記載。所見の前提に使っていない |
| plan.md の受け入れ条件リスト・ファイル表の一部 | **比較の母集団としては数えないでください**（独立性が無い） |
| CLI `marketplace add` が絶対パスを書き込むこと | ⚠️ に記載 |

**leak 後に自分で一次証拠を取り直したもの**（"confirmed post-leak" と表記）:
Claude Code 側 per-server `diagnostics` フラグのスキーマ逐語、`claude plugin validate --strict` の挙動。

---

## 1. 触らざるを得ないファイル（導出結果・比較の母集団）

「新しい JSON を置いて `settings.json` を書き換える」だけでは**閉じません**。下表の ● は
「これが無いと issue の受け入れ条件を満たせない、または既存の機構が壊れる/沈黙する」ものです。

| # | ファイル | 変更の種類 | なぜ触らざるを得ないか（根拠） | 要否 |
|---|---|---|---|---|
| 1 | `.claude/settings.json` | 変更 | `enabledPlugins`（`settings.json:28-32`）で公式 plugin を `false` に、新 plugin を `true` に。`extraKnownMarketplaces` を新設 | ● |
| 2 | （新）plugin root の `.lsp.json` | 新規 | `initializationOptions` の唯一の宣言先候補（後述 R-9 で正本を 1 つに決める） | ● |
| 3 | （新）`<plugin>/.claude-plugin/plugin.json` | 新規 | plugin manifest。`name` が `enabledPlugins` のキー前半と一致する必要がある | ● |
| 4 | （新）`.claude-plugin/marketplace.json`（repo 内 marketplace） | 新規 | `extraKnownMarketplaces` の `directory` source が読む先。`name` は settings のキーと一致が**強制**される | ● |
| 5 | `.claude/hooks/post-edit.mjs` | 変更 | `CHECK_DEFINITION`（`post-edit.mjs:70-75`）へ新ファイルを足す。足さなければ**編集しても何も走らない沈黙**（`post-edit.mjs:11-13`） | ● |
| 6 | `.claude/hooks/post-edit.test.mjs` | 変更 | **カナリアと対でなければ「何も検証しない緑」**（`docs/hooks.md:69`・`post-edit.test.mjs:589-591`） | ● |
| 7 | `docs/hooks.md` | 変更 | `selectChecks` を変えたら発火一覧表を同じ変更で直す（`post-edit.mjs:122-124`）。G-hook-fires が CI で照合（`governance-check.mjs:931-1050`） | ● |
| 8 | `scripts/governance-check.mjs` | 変更 | 新検査 `G-lsp-config` の実装。**同時に G-stale-identifiers の語彙も供給する**（R-3） | ● |
| 9 | `scripts/governance-check.test.mjs` | 変更 | 新検査の赤経路テスト。既存 G-* はすべて対のテストを持つ | ● |
| 10 | `rust-analyzer.toml` | 変更 | `targetDir` 検算コメントの訂正（`rust-analyzer.toml:41-44` の偽の双条件） | ● |
| 11 | `.claude/rules/safety-nets.md` | 変更 | frontmatter `paths`（`safety-nets.md:2-12`）に新ファイルが**当たらない**＝配送されない。glob 追加は G-rules-globs の実在検査つき | ● |
| 12 | `AGENTS.md` | 変更 | 「条件別チェック」表のセーフティネット行が母集団の**正本**（ルート `CLAUDE.md`「最重要ルール 2」）。新ファイルをその射程へ入れる | ● |
| 13 | `docs/build-commands.md` | 変更 | カテゴリ A〜F のどれにも「plugin の JSON」が属さない（`build-commands.md:11/35/147/158`）。新検査を npm script 化するなら G-build-commands の照合対象 | ○ |
| 14 | ルート `CLAUDE.md` | 変更 | 「フック」節が hook 以外の agent 向け機構を持たない。LSP plugin の存在と「診断は権威でない」の 1 行を置くか判断が要る | ○ |
| 15 | `docs/architecture.md` | 変更 | 開発機構の横断説明。`G-ci-table` は workflow を変えたときのみ | △ |
| 16 | `.github/workflows/ci.yml` | 変更 | **`governance-check` job（`ci.yml:51-64`）に載せるなら変更不要**。別 job にするなら変更 | △ |
| 17 | `package.json` | 変更 | 新 npm script を作る場合のみ。編集すると `hook-selftest` が発火する | △ |
| 18 | `vitest.config.ts` | **触らない** | include を広げたくなるが不要（R-4 のとおりカナリアを既存 include 内へ置く） | — |
| 19 | `.vscode/settings.json` | **触らない** | 非対称の一方。**追跡されている**（`git ls-files` 実測。`.gitignore` に `.vscode/` が在るのに） | — |
| 20 | `SPEC.md` | **触らない** | 製品挙動を変えない（AGENTS.md 開発ワークフロー 1 の判定: バグでも仕様変更でもない chore） | — |
| 21 | `workspace/plan.md` | 変更 | 未チェック `- [ ]` が残ると `gh pr create` が block される（`docs/hooks.md:15`） | ● |

**触らないが影響を受けるもの**: `.claude/settings.local.json`（gitignore 済み・`.gitignore:19`）は
project 設定より優先度が高いので、ローカルで `enabledPlugins` を上書きしていると機械検査が緑でも
実効値が違う。**検査は「リポジトリの宣言」しか守れない**（⚠️ U-5）。

---

## 2. 触るシンボル・キー（導出結果）

| シンボル / キー | 所在 | 何をするか |
|---|---|---|
| `CHECK_DEFINITION` | `post-edit.mjs:70-75` | 新 JSON の rel パスを追加（`.claude/settings.json` と同格） |
| `selectChecks` | `post-edit.mjs:125-156` | 上記 Set 経由で `hook-selftest` を発火。**分岐を足すなら 147 行の条件** |
| `BUDGETS` | `post-edit.mjs:35-48` | **新しい検査 id を作る場合のみ**追加。id 再利用なら不要 |
| `buildCommand` | `post-edit.mjs:285-332` | 同上。新 id には `case` と `repro` が要る |
| `validateSettings` | `post-edit.mjs:338-347` | `.claude/settings.json` の JSON 妥当性のみ。**キーの中身は見ない**——ここを拡張するか、G 側で見るかの判断点 |
| `REPRESENTATIVE_EDITS` | `post-edit.test.mjs:659-669` | BUDGETS 完全性カナリアの代表パス。新 id を作るなら追加 |
| （新）`G-lsp-config` | `governance-check.mjs` の `checks` 配列（`1857-1875`） | 新検査の登録点 |
| （新）`checkLspConfig` / `LSP_CONFIG_PATH` 等 | `governance-check.mjs` | `CLIPPY_TOML`（`494`）と同じ静的パス定数の型。読めなければ「母集団の欠落」で赤（`613` が手本） |
| `paths`（frontmatter） | `.claude/rules/safety-nets.md:2-12` | glob 追加。G-rules-globs が「実在ファイルに 1 件以上マッチ」を要求（`governance-check.mjs:790-810`） |
| `enabledPlugins` | `.claude/settings.json:28-32` | `rust-analyzer-lsp@claude-plugins-official: false` / 新 plugin `true` |
| `extraKnownMarketplaces` | `.claude/settings.json`（新設） | `{"<name>": {"source": {"source":"directory","path":"."}}}` |
| `lspServers` | plugin manifest / marketplace entry | **`.lsp.json` を上書きする**（R-9）。使わないなら**書かないことを検査する** |
| `initializationOptions` | `.lsp.json` のサーバ設定 | RA へ `rust-analyzer.` prefix を外した設定 tree で渡る |
| `checkOnSave` / `diagnostics.enable` | `initializationOptions` の中 | 抑制の実体。**この 2 キーが検査対象** |
| `extensionToLanguage` | `.lsp.json` のサーバ設定 | `{".rs": "rust"}`。**必須・1 件以上**（zod refine 実測） |
| `command` | `.lsp.json` のサーバ設定 | `"rust-analyzer"`。**スペース不可**（zod refine 逐語: "Command should not contain spaces. Use args array for arguments."）。PATH 実在を実測: `/c/Users/Eoh/.cargo/bin/rust-analyzer` / `1.97.1 (8bab26f4 2026-07-14)` |
| `[cargo] targetDir` | `rust-analyzer.toml:45-46` | 値は変えない。**上のコメント 41-44 行を訂正する** |

---

## 3. 要対処

### R-1. `CHECK_DEFINITION` へ足すだけでは「何も検証しない緑」になる — カナリアが対で要る

`post-edit.mjs:66-69` が明文で禁じています:

> カナリアの無いファイルをここに足してはならない — 何も検証しない緑になる。

`hook-selftest` の実体は `vitestSpec(".claude/hooks")`（`post-edit.mjs:326`）＝
`vitest run .claude/hooks` なので、**`.claude/hooks/**/*.test.mjs` に置いたテストしか走りません**。
既存の対の手本が `post-edit.test.mjs:593-604`（vitest.config.ts ドリフト検出）と
`606-616`（package.json ドリフト検出）です。同じ形で `.lsp.json` / `settings.json` の
キーを読むカナリアを足してください。

### R-2. `selectChecks` を触ったら `docs/hooks.md` の発火一覧を**同じ変更で**直す。順序に制約がある

`post-edit.mjs:122-124` が明示し、G-hook-fires（`governance-check.mjs:931-1050`）が CI で縛ります。
書式規則（`docs/hooks.md:44`）が判定に効きます:

- 代表パス列は**バッククォート括りの実在する具体パス 1 件**（glob 不可・**実在も検査する**・`governance-check.mjs:1008` 付近）
- 検査 id 列のバッククォートは検査 id だけ（空集合は `（なし）`）
- 表の走査は最初の空行まで

→ **ファイルを作る前に表へ行を足すと、実在検査で赤になります。** 実装順は「JSON を作る →
`selectChecks` を変える → `docs/hooks.md` の行を足す」に固定してください。

### R-3. `npm run governance:check` が G-stale-identifiers で落ちる（受け入れ条件の最後の項に直撃）

`VOCAB_SOURCE_EXT = /\.(rs|ts|tsx|mjs|ps1|toml|yml)$/`（`governance-check.mjs:1518`）に
**`.json` は入りません**（同 1504-1508 の「受容する残余」が理由を持つ）。
一方で検査対象は `docs/**`・`.claude/{skills,rules,agents}/**.md`・
`STALE_EXTRA_DOCS = ["SPEC.md","CLAUDE.md","AGENTS.md","snotra-settings/SETTINGS-DESIGN.md"]`
（`governance-check.mjs:1531, 1563-1583`）で、camelCase 述語 `STALE_IDENT`（`1532`）に当たります。

**新しく文書へ書く camelCase のうち、現行語彙に無いもの**:

| 識別子 | 現行語彙の有無 | 供給元 |
|---|---|---|
| `targetDir` | **在る** | `rust-analyzer.toml:46` の非コメント行（`.toml` は語彙源。`#` コメントは除去される） |
| `checkOnSave` | **無い** | — |
| `initializationOptions` | **無い** | — |
| `extensionToLanguage` | **無い** | — |
| `lspServers` | **無い** | — |
| `extraKnownMarketplaces` | **無い** | — |
| `enabledPlugins` | **無い**（`.claude/settings.json` は `.json` ＝非語彙源） | — |

**対処は 2 通りで、片方は罠です。**

- ○ **機械検査の実装を `scripts/governance-check.mjs`（非 test の `.mjs` ＝語彙源）へ置き、
  キー名を文字列リテラルで書く。** それだけで上の識別子が現行語彙に入り、文書側の記述が免罪されます。
  **これが「検査をどの層に置くか」の決定要因の 1 つです**（責務の議論だけでは出てこない）。
- ✕ **カナリアを `*.test.mjs` にだけ書く形は語彙を供給しません。**
  `VOCAB_TEST_FILE = /\.test\.(mjs|ts|tsx)$/`（`governance-check.mjs:1522`）が外します。
  R-1 のカナリアだけで済ませると、文書へキー名を書いた瞬間に `governance:check` が赤になります。

先例として `${CLAUDE_PROJECT_DIR}` はこの残余を避けるために**文書側の記述を書き換えて**外しています
（`governance-check.mjs:1504-1508`）。同じ逃げ方（バッククォートを外す・日本語で書く）も可能ですが、
規範の可読性を下げるので推奨しません。

### R-4. `npm test` の include に plugin ディレクトリは入らない — 「置いたつもりで誰も走らせない」経路

`vitest.config.ts:9-13` の include は `.claude/hooks/**`・`.githooks/**`・`scripts/**` の 3 本だけです。
plugin を `.claude/plugins/snotra-lsp/` 等へ置いてテストを同居させると、
**hook-selftest でも CI の `npm test`（`ci.yml:45`, `ci.yml:155`）でも走りません**。
→ 検査コードは `scripts/`、カナリアは `.claude/hooks/` に置く。**`vitest.config.ts` は触らない**
（触ると `post-edit.test.mjs:597-603` のカナリアも同時に直す必要が出て、変更面が広がるだけです）。

### R-5. 「抑制設定が消えた」の検知器は `.lsp.json` のキーだけでは不足 — 二重 LSP を防いでいるのは settings の 1 行

受け入れ条件は 2 つの独立した事実を要求しています。

1. `.lsp.json` の `checkOnSave=false` / `diagnostics.enable=false` が消えていないこと
2. 公式 plugin と二重にぶら下がっていないこと

**(2) を防いでいるのは `.claude/settings.json` の
`"rust-analyzer-lsp@claude-plugins-official": false` という 1 行だけ**であり、これが
`true` に戻っても `.lsp.json` は無傷です。しかも二重時の挙動は**先勝ち + warn**にすぎません
（`claude.exe` 逐語: `LSP: extension ${i} already handled by "${P[0]}"; "${A}" will not be used for ${i} files`）
＝ **沈黙に近い失敗**で、順序が保証されないので「どちらが勝つか」も不定です。

→ 新検査は **`.lsp.json` の 2 キー + `enabledPlugins` の 2 エントリをペアで**見てください。
`.claude/rules/safety-nets.md:40` の「検出器のカバー範囲は、欠落のパターンごとに検算する」が
まさにこの形（足が複数あるとき、どの足が欠けても赤くなるとは限らない）を名指ししています。

### R-6. 名前が 3 か所で一致していないと**沈黙で load されない**

`claude.exe` 逐語: *"Marketplace name. Must match the extraKnownMarketplaces key (enforced);
the synthetic manifest is written under this name."*

一致が要るのは:

1. `.claude-plugin/marketplace.json` の `name`
2. `.claude/settings.json` の `extraKnownMarketplaces` のキー
3. `.claude/settings.json` の `enabledPlugins` のキー接尾辞（`<plugin>@<marketplace>`）

＋ plugin 側で `.claude-plugin/plugin.json` の `name` と marketplace entry の `name`、
`enabledPlugins` のキー前半。**この一致は機械検査に載せる価値があります**——
ずれると LSP が上がらないだけで、エラーは debug log にしか出ません。

### R-7. `.claude/rules/safety-nets.md` の `paths` に当たらない ＝ 配送されない

現行の `paths`（`safety-nets.md:2-12`）は
`.claude/hooks/**` / `.githooks/**` / `.claude/settings.json` / `.github/workflows/**` /
`.claude/rules/**` / `.claude/skills/**` / `scripts/*.mjs` / `scripts/*.ps1` / `scripts/lib/**`。
plugin の `.lsp.json` / `marketplace.json` / `plugin.json` は**どれにも当たりません**。

glob を足すときは **G-rules-globs（`governance-check.mjs:790-810`）が
「実在ファイルに 1 件以上マッチ」を要求する**ので、ファイル作成と同じコミットで足してください
（先に glob だけ入れると赤）。

### R-8. `AGENTS.md`「条件別チェック」のセーフティネット行が母集団の正本 — 更新しないと射程外に落ちる

ルート `CLAUDE.md`「最重要ルール 2」:

> 母集団は `AGENTS.md`「条件別チェック（トリガー → 参照先）」のセーフティネット行が正本であり、**規範文書を含む**

plugin の設定は**エージェントの LSP 挙動を決めるチームの共有物**です。行の参照先
（`.claude/rules/safety-nets.md`）へ paths で入れるか、行の記述を更新するかのどちらかが要ります。
放置すると「セーフティネットの変更は合意してから」という規範の射程から新ファイルだけが外れます。

### R-9. 宣言箇所が 3 つあり、**`.lsp.json` は manifest に負ける** — 正本を 1 か所に定めて検査もそこを読む

`claude.exe` の `cJt` を読んだ実測:

```
.lsp.json を読んで n に入れる
　↓
if (e.manifest.lspServers) { ... Object.assign(n, s) }   ← manifest が .lsp.json を上書きする
```

さらに **marketplace entry 側でも `lspServers` を宣言できます**（公式 `rust-analyzer-lsp` が実際にそう。
`~/.claude/plugins/cache/.../rust-analyzer-lsp/1.0.0/` には `LICENSE` と `README.md` しか無く、
宣言は `marketplace.json` の entry に在ります）。

→ AGENTS.md「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」がそのまま当たります。
**正本を 1 つ選び、機械検査は同じファイルを読み、残り 2 か所に `lspServers` が**
**書かれていないことも検査する**（書かれると黙って勝つため）。issue の提案は `.lsp.json` ですが、
**優先度が最も低い場所**である点は計画で明示的に引き受けてください。

### R-10. `rust-analyzer.toml` の訂正は 41-44 行の双条件そのもの

```
# 効果が実在するかはこの設定自身が測る。…
# `target/rust-analyzer/` が出来て育てば回しており競合も実在した、出来なければ
# 回しておらず**この設定も `checkOnSave = false` も買う意味が無い**、と判る。
```
（`rust-analyzer.toml:41-44`）

issue が名指しした「強すぎる推論」です。訂正時の注意 2 点:

- 直前の 13-14 行に **#1082 の訂正注記**（「workspace 水準だけ」からの訂正）が在ります。
  同じ節に 2 つの訂正注記が並ぶので、**日付つきで並べる**か、片方を畳むか決めてください。
- **コメントに `checkOnSave` と書いても G-stale-identifiers の語彙にはなりません**
  （`.toml` の `#` コメントは `currentVocabulary` で除去される・`governance-check.mjs:1594-1600` 付近）。
  R-3 の対処が別途要ります。

### R-11. RA_LOG による実効設定の実測は PR 前に閉じられない — PR 本文チェックリストへ送る

LSP は**起動時に初期化**されるため、`initializationOptions` を変えた効果はセッション再起動後にしか測れません。
一方 `gh pr create` は `workspace/plan.md` の未チェック `- [ ]` で block されます（`docs/hooks.md:15`）。
これは `.claude/rules/safety-nets.md:41` が記録した循環（「CI の実測は PR が在って初めて行える」）と**同型**です。
→ 計画の検証項目ではなく **PR 本文のチェックリストへ**置いてください。

### R-12. 検査の層は「hook + governance の 2 枚」を推奨する（片方ずつ穴が違う）

| 層 | カバーする | カバーしない |
|---|---|---|
| `CHECK_DEFINITION` + `.claude/hooks/` のカナリア | 編集した瞬間に赤。`npm test` 経由で CI（ubuntu + windows 両方） | **ファイル削除**（Edit\|Write matcher に届かない・`post-edit.mjs:166-168` が同型の残余を記録）／`skip-ci` ラベル付き PR（`ci.yml:26-28`, `67-69` の guard） |
| `governance-check.mjs` の `G-lsp-config` | `skip-ci` でも常時実行（`ci.yml:47-50` が意図的に guard を付けない）。削除も検出。**R-3 の語彙も供給** | 編集した瞬間には走らない（hook に配線が無い） |

**id の選択**: `CHECK_DEFINITION` へ paths を足して `hook-selftest` を再利用する形が最小です
（`BUDGETS` / `buildCommand` / `docs/hooks.md` の新しい行が要らず、既存行 `docs/hooks.md:55` の
補足列にパスを足すだけで済む）。新しい検査 id を作る道もありますが、
**`BUDGETS` + `buildCommand` の `case` + `repro` + `docs/hooks.md` の新行 + `REPRESENTATIVE_EDITS`
の 5 点セット**が同時に要ります（`post-edit.test.mjs:653-682` の完全性カナリアが強制）。

### R-13. 「検査・検証手段を新設する」トリガーが立つ — `docs/development-principles.md` の死角の表を通す

`AGENTS.md:59` 近傍の条件別チェック表に
「**検査・検証手段を新設する、またはどの手段で保証するか決める** →
`docs/development-principles.md`「検証の層と、層と層の隙間」」の行が在ります。
`G-lsp-config` の新設はこのトリガーに当たります。同節の表（`development-principles.md:153-164`）を
**実際に見て確かめた結果、LSP diagnostics を層として数えている行はありません**（訂正不要）。
ただし同節の要求「(2) その出力を消費する層まで届いているか」は今回そのまま効きます——
**機械検査が守るのはファイルの内容であって、稼働中の RA の実効値ではない**（⚠️ U-5 / U-6）。
表に行を足すかどうかは計画側の判断ですが、**足すなら「見ないもの」の列を空にしないこと**。

---

## 4. 軽微

- **M-1** `docs/superpowers/plans/2026-07-09-hook-responsibility-layers.md:1106` に
  `"rust-analyzer-lsp@claude-plugins-official": true` が逐語で残ります。`docs/superpowers/` は
  governance の走査元から外れている（`governance-check.mjs:1358`）ので赤にはなりませんが、
  「settings.json の現行値」として読まれうる歴史資料です。直さなくてよい。
- **M-2** `.vscode/settings.json` は `.gitignore:14` に `.vscode/` が在るのに**追跡されています**
  （`git ls-files` 実測）。「VS Code 側は変えない」を文書で主張するとき、
  この非対称（＝チームで共有される設定）を前提に書いてください。
  中身は `rust-analyzer.cargo.allTargets: false` / `check.allTargets: false` /
  `cachePriming.enable: false` / `lru.capacity: 192` で、`rust-analyzer.toml:18-20` の
  「重い設定はここに書かない」という記述と整合しています（訂正不要）。
- **M-3** `docs/build-commands.md` のカテゴリ A〜F に「plugin の JSON」が属しません
  （`11` / `35` / `147` / `158` 行）。カテゴリ F は `*.md`・rules・skills・workflow が対象。
  新検査を npm script 化するなら G-build-commands（`governance-check.mjs:649-680`）の照合対象になります。
- **M-4** `claude plugin validate <path> --strict` が実在します（`--help` 実測・confirmed post-leak）:
  *"Treat warnings as errors (exit 1). Use in CI to fail on unrecognized fields, missing metadata…"*。
  CI に載せる案は魅力的ですが、**ubuntu runner に claude CLI は無い**ので導入コストが乗ります。
  `G-lsp-config` を自前で書けば依存ゼロで済みます（`governance-check` job は「依存ゼロ・数秒」で
  常時実行という設計・`ci.yml:47-50`）。
- **M-5** `G-area-instrument`（`governance-check.mjs:1060-1165`）は合否を持たない計器です。
  ルート `CLAUDE.md` / `AGENTS.md` へ足すと常時ロード面の字数が増えるだけで赤にはなりません。
- **M-6** `.claude/settings.json` の編集は `hook-selftest` を自動発火します（`post-edit.mjs:70-75, 147-149`）が、
  `validateSettings`（`338-347`）が見るのは **JSON として parse できるかだけ**です。キーの中身は誰も見ていません。
- **M-7** `makeSnapshot` の除外（`governance-check.mjs:40-41`）は
  `.git` / `node_modules` / `target` / `dist` と `workspace` / `.claude/worktrees` / `.superpowers`。
  plugin をどこへ置いても（repo ルートでも `.claude/` 配下でも）走査対象に入ります。本レビュー自身も
  `workspace/` 配下なので governance の対象外です。
- **M-8** `.lsp.json` の `env` フィールドが実在します（zod 逐語: *"Environment variables to set when
  starting the server"*）。`RA_LOG=rust_analyzer=info` の実測はここへ一時的に置く形で取れます。
  ただし `${...}` 展開の対象で、`CLAUDE_PLUGIN_ROOT` / `CLAUDE_PLUGIN_DATA` / `CLAUDE_PROJECT_DIR`
  だけは展開から除外されます（バイナリ実測）。
- **F-1（偽になる散文の走査結果・陰性）** 概念ラベル（`rust-analyzer` / `LSP` / `findReferences` /
  `checkOnSave` / `marketplace`）で `.claude/**`・ルート `CLAUDE.md`・`AGENTS.md`・`SPEC.md`・
  `PERFORMANCE.md`・`CONTRIBUTING.md`・`docs/**` を走査した結果、**偽になる記述は
  `rust-analyzer.toml:41-44`（R-10）以外に見つかりませんでした**。
  `AGENTS.md:59` の「呼び出し元は LSP ツールの findReferences で列挙する」は
  **今回の変更後も真のまま**です（navigation は保つのが issue の方針）。
  `.claude/skills/*/SKILL.md` と `.claude/agents/code-reviewer.md` に LSP 前提の記述はありません
  （`診断` のヒットは cargo の診断サマリーで無関係）。
  **これは「探した範囲での不在」であり全称否定ではありません**——`.rs` のコメント・
  `docs/adr/`・`docs/superpowers/` は走査していません（⚠️ U-11）。
- **M-9** LSP サーバ設定には `startupTimeout` / `shutdownTimeout` / `restartOnCrash` /
  `maxRestarts` / `workspaceFolder` / `settings`（`workspace/didChangeConfiguration` 経由）も在ります
  （confirmed post-leak）。今回は使わないと思いますが、**`settings` は `initializationOptions` と
  役割が重なる**ので、計画で「使わない」と明示しておくと後の混乱が減ります。

---

## 5. ⚠️ 未検証（見たが確信が持てない／測っていない）

- **⚠️ U-1** 相対パスの `directory` marketplace が **fresh clone で本当に再現するか**。
  バイナリの該当関数は「`source` が `directory`/`file` かつ絶対パスでないとき project dir 基準で
  `path.resolve` する」と読めましたが、**実行して確かめていません**（marketplace 登録は状態変更ゆえ
  ブリーフで禁止）。計画側で実測してください。
- **⚠️ U-2** marketplace が cache へコピーされた後、plugin entry の相対 `source`（`./plugins/x` 形）が
  **元ディレクトリと cache のどちらを基準に解決されるか**。公式 marketplace は cache 側に plugin 実体が
  ありますが、`directory` source でも同じ経路を通るかは未確認。ここが違うと
  「repo を編集しても cache の古い `.lsp.json` が使われる」という**沈黙する乖離**になります。
- **⚠️ U-3** **worktree でのキャッシュ名衝突**。directory source のキャッシュ名は
  `path.basename(path)` から作られます（バイナリ実測）。`.claude/worktrees/agent-xxxx` を
  project dir とした場合、`path: "."` の basename は worktree 名になりますが、
  **repo root を指す形だと basename が同じ `Snotra` になりうる**。衝突時の挙動は未検証。
  このリポジトリはサブエージェント委譲で worktree を常用する（ルート `CLAUDE.md`）ので、
  実害が出るなら早く出ます。
- **⚠️ U-4** `settings`（`workspace/didChangeConfiguration`）が `initializationOptions` を
  後から上書きしうるか。スキーマに両方在ることは実測しましたが、RA 側でどちらが勝つかは未検証。
- **⚠️ U-5** `.lsp.json` の編集がセッション中に反映されるか。`.claude/settings.json` は
  file watcher が即拾う（`docs/hooks.md:68` 実測）が、LSP サーバの再起動を伴うかは未確認。
  **反映が再起動時のみなら、機械検査が守れるのは「ファイルの内容」までで、
  「稼働中のサーバの実効値」は守れません**——doc にこの限界を書くとき全称表現にしないこと。
- **⚠️ U-6** CI で LSP の実効値は測れません（Claude Code の LSP は CI に無い）。
  機械検査の保証範囲は「JSON が parse でき、意図したキーが意図した値である」まで。
  **これは私の推論であって実測ではありません**（CI で claude CLI を動かす経路を探していない）。
- **⚠️ U-7** `diagnostics` の抑制には**層が 2 つ**あります（confirmed post-leak・逐語）:
  Claude Code 側の per-server `diagnostics: boolean` — *"Whether to push publishDiagnostics into the
  agent context after edits. Set to false to keep LSP navigation (goToDefinition, hover, etc.) but
  suppress automatic diagnostic injection. Defaults to true."* と、
  issue が提案する RA 側の `initializationOptions.diagnostics.enable=false`（RA が計算しない）。
  **2 層あることは実測しましたが、どちらを選ぶべきかは検証していません。**
  前者なら navigation が保たれることを docstring が名言する一方、後者は `unlinked-file` 等の
  検出手段ごと消えます。
- **⚠️ U-8** 公式 plugin を `false` にした状態で、reconciler がいつ marketplace を同期するか＝
  **clone 直後の初回セッションで LSP が上がるまでの経路**。未検証。
- **⚠️ U-9** `governance-check.mjs` を `.claude/hooks/*.test.mjs` から `import` して
  カナリアに使う案の可否。`isMain` ガード（`governance-check.mjs` 末尾）が在るので import 安全に見えますが、
  実際に import して測っていません。
- **⚠️ U-10 — leak 由来（私の導出物ではありません。母集団として数えないでください）**:
  - research.md 証拠 F「`rust-analyzer.toml` は Claude Code の RA インスタンスに届いていない（実測）」。
    **もしこれが正しいなら R-9 と R-10 の意味が変わります**（`rust-analyzer.toml` の
    `workspace.symbol.search` も届いていないことになり、plugin へ移す動機が増える）。
    私は独立に検証していません。
  - plan.md のスコープ裁定（今サイクルは `checkOnSave` まで／`RA_LOG` は PR チェックリスト送り）。
    偶然ですが R-11 の私の導出と一致します。
  - CLI `claude plugin marketplace add` が入力を `path.resolve` して**絶対パス**で書き込むこと。
    私は CLI を実行していないので未確認ですが、事実なら「CLI を使わず `.claude/settings.json` へ
    相対パスで手書きする」が正しい形になります。
- **⚠️ U-11** 時間の都合で見ていないもの: `docs/architecture.md` 全文、`PERFORMANCE.md`、
  `CONTRIBUTING.md`、`docs/adr/` 各本文（凍結された歴史ゆえ訂正不要のはずですが、
  現行値として引かれていないかは未確認）、`.claude/skills/*/SKILL.md` の**全文**
  （grep では LSP 関連の記述ゼロを確認済み・下の F-1 参照）。
