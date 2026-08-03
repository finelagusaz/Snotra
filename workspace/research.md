# research — #891 腐り検出器 G-stale-identifiers の射程拡大（#819 案 B）

## issue の要約

`G-stale-identifiers`（規範の散文に残る「現行語彙に無い識別子」の検出器）は、母集団が `.claude/(skills|rules|agents)/**.md` + `SPEC.md`、述語が camelCase 限定である。この射程では **#819 も #825 も機構で捕まらず**、どちらも `/plan-review` の独立導出が付随して見つけた。母集団に `docs/**.md`（`superpowers/` と `adr/` を除く）とルート規範文書を足し、述語に SCREAMING_SNAKE を足し、語彙源に `.yml` を足す。

issue 本文が自己完結した計画を持つが、**着手時に測り直すことを本文自身が要求している**。以下はその実測結果である。

## 関連ファイル・シンボル（grep で実在確認済み）

| 対象 | 位置 | 役割 |
|---|---|---|
| `VOCAB_SOURCE_EXT` | `scripts/governance-check.mjs:1419` | 現行語彙の正本になるソース拡張子 |
| `VOCAB_TEST_FILE` | 同 `:1423` | 語彙源から外すテストコード（ファイル名の形） |
| `STALE_EXTRA_DOCS` | 同 `:1425` | 静的リテラルの検査対象（現在 `["SPEC.md"]`） |
| `STALE_IDENT` | 同 `:1427` | camelCase 述語 |
| `EXTERNAL_CMD_LINE` | 同 `:1429` | 外部ツールのコマンド行を構造的に外す |
| `staleIdentifierDocs` | 同 `:1435` | `.claude/**` の母集団（**0 件検知の対象**） |
| `staleIdentifierTargets` | 同 `:1441` | 検査対象 = 上 + `STALE_EXTRA_DOCS` |
| `currentVocabulary` | 同 `:1447` | 語彙の組み立て（`#` コメント分岐は `ps1|toml`） |
| `scanStaleIdentifiers` | 同 `:1459` | 走査本体。`checked` を返す |
| `buildChecks` / `runAll` | 同 `:1681` / `:1717` | 登録表と 0 件検知（`ctx.staleDocs.length === 0` は `:1725`） |
| G-stale-identifiers のテスト | `scripts/governance-check.test.mjs:873-978` | 本体 13 件 + 配線 3 件 |
| `governanceDocs` | `scripts/governance-check.mjs:1198` | **`docs/adr/` を含む**（非対称の相手） |

## 実測（2026-08-03・`1681627` = #892 マージ後の HEAD）

**稼働中のガードは触っていない**——述語・母集団・語彙源を scratchpad の複製へコピーして変異させた（`.claude/rules/safety-nets.md`「複製に変異を当てる」）。母集団は**関数の戻り値を印字して**組み立てた（`AGENTS.md`「列挙も SSOT のツール自身に問う」）。

### 母集団

- `.claude/**`（`staleIdentifierDocs`）: **24 本**
- `docs/**.md` − `superpowers/` − `adr/`: **7 本**（`architecture` / `build-commands` / `check-skill-skeleton-design` / `comment-guidelines` / `design/2026-05-31-coherence-staleset` / `development-principles` / `hooks`）
- 新静的: `snotra-settings/SETTINGS-DESIGN.md` / ルート `CLAUDE.md` / ルート `AGENTS.md`
- 合計 **35 本**

### セル

| セル | 照合 | finding | 真の腐り | 外部語彙 |
|---|---|---|---|---|
| ベースライン（現行） | 1 | 0 | 0 | 0 |
| D-+E（`.yml` 無し） | 77 | 11 | 7 | 4 |
| **★採用: D-+E + 語彙源に `.yml`** | **77** | **9** | **7** | **2** |

**issue の表は 2 箇所ずれていた**——どちらも導出値で、実測が正す:

- **照合 43 → 77**。issue の 43 は新静的 3 本（照合 35 件を寄付する）を数えていない
- **真の腐り 8 → 7**。8 は `G12_NO_LAUNCHER_READ` を含む #825 マージ前の値。B12 が別途書いていた「#825 の PR 後は 7 件 / 相異なる識別子 5 個」のほうが正しく、**実測と完全一致した**

### finding 9 件の内訳

| 位置 | 識別子 | 分類 |
|---|---|---|
| `docs/development-principles.md:39` | `shouldShowResults` | 真の腐り |
| 同 `:78` ×2 | `viewKind()` `interpKind()` | 真の腐り |
| 同 `:81` | `assertNever` | 真の腐り |
| 同 `:83` | `viewKind()` | 真の腐り |
| 同 `:84` ×2 | `isInstantPrefix` `interpKind` | 真の腐り |
| 同 `:128` | `backgroundThrottlingPolicy` | **外部語彙**（`tauri.conf.json` の、在ってはならないキー） |
| `docs/hooks.md:67` | `CLAUDE_PROJECT_DIR` | **外部語彙**（harness の環境変数） |

### per-file の照合内訳（B2 の論拠）

| 群 | 照合 | fail-closed |
|---|---|---|
| `.claude/**`（24 本） | **1** | あり（`runAll` の `staleDocs.length === 0`） |
| `SPEC.md` | 6 | あり（静的リテラル → 読めなければ「母集団の欠落」） |
| 新静的 3 本 | 35 | **静的リテラルへ入れれば無償で付く** |
| `docs/**`（7 本） | 35 | **無い**（グロブ由来ゆえ 0 件で沈黙する） |

上位: `SETTINGS-DESIGN.md` 31 / `development-principles.md` 11 / `build-commands.md` 10 / `hooks.md` 10 / `SPEC.md` 6 / ルート `CLAUDE.md` 4 / `design/…` 2。**ルート `AGENTS.md` は 0 件**。

### `.yml` が寄付する新規語彙（9 語）

`GITHUB_ENV` `GITHUB_OUTPUT` `GITHUB_TOKEN` `TAG_NAME` `TAURI_SIGNING_PRIVATE_KEY` `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` `ddTHH` `ssZ` `yyyyMMddHHmm`

- 語彙源に足る `.yml` は 7 本（`.github/dependabot.yml` / `labels.yml` / `workflows/` 5 本）。**`.yaml` はリポジトリに 1 本も無い**（`git ls-files "*.yaml"` = 0）ので、`yml` だけで漏れない
- **`GITHUB_TOKEN` を免罪して finding が 11 → 9 になった**（`docs/build-commands.md:216` の 2 件）
- **後ろ 3 語は日付書式の断片である**（`yyyyMMdd'T'HHmm` 等が `'` で分断された残骸）。camelCase 述語に当たるため語彙のノイズになる——同名の識別子が文書に書かれれば誤って免罪する。今日 0 件で、受容する残余として記録する

### `\b` と `_` の挙動（SCREAMING_SNAKE 述語の前提）

JS の `\b` は `_` を単語構成文字として扱う。実測: 語彙 `const NO_LAUNCHER_READ = 1` に対し `\bNO_LAUNCHER_READ\b` = **true**、`\bG12_NO_LAUNCHER_READ\b` = **false**。部分一致で誤って免罪される経路は無い。

## 再利用できる既存パターン

- **静的リテラルは fail-closed を無償で持つ**——`STALE_EXTRA_DOCS` の `SPEC.md` が読めなければ `scanStaleIdentifiers` が「母集団の欠落」を出す。**新母集団のうち 3 本（固定パス）はここへ入れれば新しい機構が要らない**
- **0 件検知は `runAll` に 3 本並んでいる**（`ctx.docs` / `ctx.refDocs` / `ctx.staleDocs`）。4 本目を同じ形で足す
- **配線テストの形**は `governance-check.test.mjs:959` の `describe("G-stale-identifiers の配線 …")` にある（`buildChecks` から検査を引き当てて実行する）
- **赤フィクスチャは実在の欠陥にする**（同ファイル `:875`「実際に検出された `createObjectURL`」）

## 技術的制約

- **`STALE_EXTRA_DOCS` へ混ぜてはならないもの**: `staleIdentifierDocs`（= `.claude/**`）。混ぜると `runAll` の 0 件検知が永久に沈黙する（`:1432-1434` のコメントが SSOT）
- **免除注記の機構を設けない**（ファイル冒頭の契約）。除外リストではなく「行の形」「ファイル名の形」で外す
- **判定は決定的**（手元と CI で同じ）。gitignore 済みファイルを語彙源に入れてはならない（`.superpowers/` を走査から外した理由・#722）
- **テストコードは語彙を寄付しない**（`VOCAB_TEST_FILE`）
- `EXTERNAL_CMD_LINE` は `gh|npm|cargo|git|node|pwsh|npx` にしか当たらない——`CLAUDE_PROJECT_DIR` にコマンド行化は使えない（実測）

## 調査で判明した、計画を変える事実

### 1. B12 の処方が変わる——**現行の等価物が実在する**

issue は「歴史として散文化するか、現行の等価物へ差し替える」と両論併記していたが、実測すると **2 軸導出は Rust へ生き残っており、識別子名だけが TS 期のまま取り残されていた**。

| 散文の語 | 現行の等価物 | 位置 |
|---|---|---|
| `viewKind()` | `view_kind()` / `ViewKind`（Results / Folder / Tool） | `src-tauri/src/egui_shell/search_state.rs:10` |
| `interpKind` | `interpret()` / `QueryIntent`（Plain / Command / Instant） | 同 `:18`, `:35` |
| `isInstantPrefix` | `is_instant_prefix()`（doc に「instant 検出の SSOT」と明記） | 同 `:30` |
| `assertNever` | Rust の網羅 `match`（コンパイラが直接検出する） | 言語機能 |
| `shouldShowResults` | **無い**——`layout.rs:536` が「`show_results` へ潰していた」のを分解済み | — |

**これは検出器が守るべき腐りそのものである**（原理が生きたまま名前だけ死んだ形）。4 個は差し替え、`shouldShowResults` だけ散文化する。

### 2. `docs/design/` は「日付を持つ」が「もう成り立たないことを書く場所」ではない

`docs/design/2026-05-31-coherence-staleset.md` は `status: Agreed` で、**`docs/architecture.md:99` が「詳細は」と現在形で指している**（`:210` にも索引がある）。`docs/superpowers/`（#589 で非規範化）や `docs/adr/`（却下案＝もう存在しない案）とは性質が違う。→ 未確定 U1 で決める。

### 3. 既存テスト 2 件が新しい形と衝突する

- `:944` `staleIdentifierTargets(withProse).sort()` が `[".claude/rules/b.md", "SPEC.md"]` と一致することを固定している。`STALE_EXTRA_DOCS` を増やすと落ちる
- `:930` `staleIdentifierDocs` が `docs/d.md` を除くことを固定している。**これは新関数を別に足す限り真のまま**（`staleIdentifierDocs` の意味は変えない）

### 4. `runAll` の証跡文字列が動く

`散文の識別子 ${ctx.stale} 件を ${ctx.staleTargets.length} 文書から照合` が **1 件 / 25 文書 → 77 件 / 35 文書**へ動く。アサーションは無い（証跡は印字のみ）が、B6 の対象に含める。

## 未解決の疑問

`workspace/plan.md`「未確定」へ送る。
