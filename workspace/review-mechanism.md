# レビュー: セーフティネットの機構

レンズ = セーフティネットの機構。読み取り専用で実施し、稼働中のガード（`.githooks/` / `.claude/hooks/` / `scripts/governance-check.mjs`）には一切触れていない。測定はすべて proxy snapshot（本物の関数を import し、変異は複製側へ当てる）で行った。

**calibrate 済み**: proxy の scan 実装が production の `scanStaleIdentifiers` とベースラインで一致することを先に確認した（`照合 1 / finding 0` が両者で一致）。以降の数値はこの写しで測っている。

測定スクリプト（scratchpad・リポジトリ外）:
`C:/Users/Eoh/AppData/Local/Temp/claude/C--workspace-Snotra/2b2184b6-6635-4cc7-862f-51ae76be5b70/scratchpad/{measure-vocab,measure-amnesty,proxy-scan,proxy-failclosed,proxy-phaseA}.mjs`

**計画の測定表は再現した。** D-+E = 照合 43 / finding 12、採用案（+ 語彙 `.yml`/`.json`）= 照合 43 / finding 9。真の腐り 9 件はいずれの段でも不変。計画の一次証拠は正しい。以下は、その表が測っていない軸の話である。

---

## fail-closed 経路の列挙

### 実装レベルの検算 — 「`STALE_EXTRA_DOCS` の経路を使う」では守れない

計画 B1 の**理由づけは正しい**。`docs/**` を `staleIdentifierDocs` へ混ぜると `runAll:1725` の `ctx.staleDocs.length === 0` が長さで埋まり、`.claude/**` の消滅検知が永久に沈黙する（`scripts/governance-check.mjs:1431-1434` の doc コメントと `governance-check.test.mjs:944-952` がその形を固定している）。B1 はこれを保っている。

**欠けているのは、新しい母集団**自身**の fail-closed である。** `STALE_EXTRA_DOCS` が fail-closed たりうるのは、それが `["SPEC.md"]` という**静的なリテラル**だからである——実在を問わず targets へ入り、読めなければ `scanStaleIdentifiers:1470-1472` が「母集団の欠落」を出す（`:1439-1440` の doc がその意図を明記）。`docs/**.md` は **snapshot 由来の glob** であり、0 件になれば 0 個を寄付して終わる。**アサーションが存在しない。**

proxy で実測した（`proxy-failclosed.mjs`。`docs/` が丸ごと消えた snapshot に、計画どおり glob 派生を targets へ足した実装を当てる）:

```
staleIdentifierDocs 件数: 24 (→ runAll の 0 件検知は鳴らない)
targets 件数: 25 vs 通常 32
finding: 0 / 照合: 1 → ★緑（沈黙）★
```

**exit 0 で緑になる。** 検出器は拡大前の射程へ黙って戻り、誰も鳴らない。

### 「`docs/**` が 1 枚も無くなったときは誰が鳴るのか？」への答え: **誰も鳴らない**

`runAll` の既存 3 つの 0 件検知はいずれも代替にならない。

- `ctx.docs`（`governanceDocs`・`:1198-1207`）は `docs/**` を含むが `CLAUDE.md` / `AGENTS.md` / `SPEC.md` / `.claude/rules/**` / `.claude/skills/**` も含む → 非空のまま
- `ctx.refDocs`（`headingRefDocs`・`:1214-1218`）は全 `.md` → 非空のまま
- `ctx.staleDocs`（`.claude/**`）→ 24 件で非空のまま

**しかも `checked` の内訳が、この沈黙を見えにくくする。** 拡大後述語での per-file 内訳を測った（`proxy-phaseA.mjs`）:

| 母集団 | 文書数 | 照合件数 |
|---|---|---|
| `.claude/**`（既存・fail-closed で守られている側） | 24 | **1** |
| `SPEC.md` | 1 | 6 |
| `docs/**` − superpowers,adr（**新設・守られていない側**） | 7 | **36** |
| 合計 | 32 | 43 |

**守られているのは照合 1 件を寄付する母集団で、守られていないのが 36 件を寄付する母集団である。** B10 が 8 件を是正した後、`docs/**` が生きている証拠は証跡行の `照合 43 件` という数字だけになり、それを機械で確かめる面はどこにも無い。

### 0 件になりうる経路の表

| 0 件になりうる経路 | 現行 | 拡大後 | 計画は塞いでいるか |
|---|---|---|---|
| `.claude/skills\|rules\|agents/**.md` が全消滅 | `runAll:1725` が鳴る | 同じく鳴る | **塞いでいる**（B1 の理由づけどおり・B8 で検算予定） |
| `SPEC.md` が消える／読めない | `scanStaleIdentifiers:1471` が鳴る（静的リテラルゆえ） | 同じ | 塞がっている（既存） |
| **`docs/` が改名・移動される**（`documentation/` 等） | 該当なし | **沈黙（実測 finding 0 / 照合 1 / exit 0）** | **塞いでいない** |
| **`WALK_EXCLUDE_NAMES` / `WALK_EXCLUDE_PREFIXES` に `docs` 相当が入る** | 該当なし | **沈黙**（snapshot に現れない） | **塞いでいない** |
| **glob 述語が腐る**（`docs/**.md` の正規表現を後日の改修が壊す） | 該当なし | **沈黙** | **塞いでいない** |
| **配線が戻される**（後日の改修が `docs/**` を targets から外す） | 該当なし | **沈黙**（B10 完了後は finding 0 で緑 ↔ 緑） | **塞いでいない**（後述） |
| 除外述語 `docs/(superpowers\|adr)/` が広がりすぎる（例: `docs/` 全体に当たる形へ壊れる） | 該当なし | **沈黙** | 塞いでいない |
| 語彙が空になる（`VOCAB_SOURCE_EXT` が 1 件も当たらない） | finding が**全件**になる＝赤 | 同じ | 塞がっている（fail-open ではない） |

**配線が戻される経路は、既存設計が既に一度直面して機構で答えている。** `governance-check.test.mjs:955-958` のコメントがそれである——「`staleTargets` を `staleDocs` へ戻しても実リポジトリの finding は 0 / 照合 1 のまま変わらないため、dogfood テストも証跡の印字も気づけない」。ゆえに `describe("G-stale-identifiers の配線 …")`（`:959-978`）が置かれた。**同じ論証が `docs/**` にそのまま当たるのに、計画 B6 は「既存テストを改訂する」としか書いておらず、新母集団に対応する配線テストの新設を項目にしていない。**

### 推奨（この節の要対処）

1. `runAll` へ **`docs/**` サブ母集団の 0 件検知を足す**（既存 `staleDocs.length === 0` と対称の 1 行）。`buildChecks` の sink へ glob の結果を出す
2. `governance-check.test.mjs` の `describe("… の配線")` と**同じ形の describe を `docs/**` について新設する**（フィクスチャの `docs/x.md` に赤の識別子を置き、`buildChecks` 経由で finding になることを固定する）。B7〜B9 は実装時の 1 回きりの測定であって、これの代わりにはならない

---

## 語彙源拡大の副作用（実測）

### 実際に語彙へ入るファイルの全件（`measure-vocab.mjs` / `measure-amnesty.mjs`）

`.json` / `.yml` を `VOCAB_SOURCE_EXT` へ足すと **19 ファイル・約 380 KB** が語彙へ入る。新たに語彙へ加わる識別子は **camelCase 151 個 / SCREAMING_SNAKE 7 個**。内訳:

| ファイル | サイズ | 新規 camel | 新規 SCREAM | 性質 |
|---|---|---|---|---|
| `src-tauri/gen/schemas/desktop-schema.json` | 119 KB | 15 | 0 | **生成物**（`gen/`）。`anyOf` `allOf` `oneOf` `uniqueItems` `macOS` と、JSON の `\n` エスケープが次語へ癒着した `nThe` `nIt` `nThis` `nIf` `nOn` `nMust` `nBy` |
| `src-tauri/gen/schemas/windows-schema.json` | 119 KB | 15 | 0 | 同上 |
| `src-tauri/gen/schemas/acl-manifests.json` | 68 KB | 1 | 0 | **生成物**。`nThe` |
| `package-lock.json` | 49 KB | **118** | 0 | **依存メタデータ**。大半が integrity ハッシュの base64 断片（`yByuxyS7BlSNRDOMLMlROYtjYdIAuBmJssVz1UJDSeYxLrdizhXCFYhedC5bqd` 等）。`npm install` のたびに中身が入れ替わる |
| `.github/workflows/*.yml`（5 本） | 17 KB | 3 | 7 | `GITHUB_TOKEN` `GITHUB_OUTPUT` `GITHUB_ENV` `TAG_NAME` `TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)`。**計画が消したい偽陽性 2 件の出所** |
| `.claude/settings.json` | 655 B | 1 | **1** | `CLAUDE_PROJECT_DIR`。**計画が消したい偽陽性 1 件の出所**。`enabledPlugins` も入る |
| `src-tauri/tauri.conf.json` | 952 B | 6 | 0 | 手書き設定。`productName` `externalBin` `installMode` に加え **minisign 公開鍵の base64 塊 1 個** |
| `package.json` | 1.2 KB | 2 | 0 | 手書き。`hooksPath` `devDependencies` |
| `.vscode/settings.json` / `extensions.json` | 1.1 KB | 6 | 0 | エディタ設定。`watcherExclude` `procMacro` `cachePriming` `unwantedRecommendations` |
| **`.claude/settings.local.json`** | 1.2 KB | 0 | 0 | **gitignore 済み**（`.gitignore:18`・実測） |
| **`test-results/.last-run.json`** | 45 B | **1**（`failedTests`） | 0 | **gitignore 済み**（`.gitignore:34`）。**内容がテストの実行結果で変わる** |
| `.github/labels.yml` / `dependabot.yml` | 1.5 KB | 0 | 0 | — |
| `src-tauri/gen/schemas/capabilities.json` | 2 B | 0 | 0 | 生成物（空） |

`*.test.json` は今日 0 件（実測）、`.yaml`（`.yml` でない）も 0 件（実測）。

### (a) 検出器自身のテストフィクスチャが語彙を寄付する経路 — **B9 の探し方では見つからない形で既に成立している**

計画 B9 は `VOCAB_TEST_FILE = /\.test\.(mjs|ts|tsx)$/` が `.json` を採らないことから `*.test.json` を心配している。**その形は今日 0 件で、危険なのは別の形である。**

`test-results/.last-run.json` は Playwright 系のテスト実行成果物で、`failedTests` を語彙へ寄付する（実測）。これは `VOCAB_TEST_FILE` のファイル名述語には当たらないが、**「テストコードは語彙を寄付しない」という不変条件（`:1391-1393`・`governance-check.test.mjs:899-903` が固定）を実質的に破る**。しかも:

- **gitignore 済みゆえ、手元と CI で語彙が違う。** `.superpowers/` を `WALK_EXCLUDE_PREFIXES` へ入れた理由（`:34-35`「gitignore 済みゆえ CI のチェックアウトには存在しない——走査に含めると同じコマンドが手元と CI で別の母集団を見る（#722）」）と**同型の欠陥を、語彙側に新設する**
- **内容がテストの実行結果で変わる。** ファイル冒頭の契約（`:12`）「決定的（ネットワーク・時刻・環境変数に非依存）」に正面から反する

`.claude/settings.local.json` も同じ gitignore 経路に乗る（今日たまたま新規語彙 0 個だが、性質は同じ）。

### (b) 巨大な生成物が語彙に混じる経路 — **成立する。ただし利益は 0 と測れた**

`src-tauri/gen/schemas/*.json`（306 KB・追跡されている）と `package-lock.json`（49 KB）が語彙へ入る。`package-lock.json` の 118 個は大半が integrity ハッシュの base64 断片で、**`npm install` のたびに丸ごと入れ替わる**——無関係な依存更新が、文書の識別子に対する判定を動かしうる面が生まれる。

**この 4 ファイルが計画の目的に寄与するかを測った**（`proxy-scan.mjs` の対照セル）:

| 語彙源 | finding | 消えた偽陽性 |
|---|---|---|
| 現行のまま | 12 | — |
| `.yml` だけ足す | 10 | `GITHUB_TOKEN` ×2 |
| **`.yml` + 手書きの `.json` 3 本のみ**（`.claude/settings.json` / `package.json` / `tauri.conf.json`） | **9** | + `CLAUDE_PROJECT_DIR` |
| `.yml` + `.json` 全部（計画の採用案） | **9** | 同上 |

**採用案と narrow 案は完全に同じ 9 件を出す。** 生成物・lockfile・gitignore 済みファイルの 380 KB 中 375 KB は、**測定上ひとつの利益も生んでいない**。

`.json` がまったく不要なわけではない——`docs/hooks.md:67` の `CLAUDE_PROJECT_DIR` は `.claude/settings.json` にしか無く、`.yml` だけでは消えない（実測 10 件）。必要なのは `.json` という**拡張子**ではなく、**手書きの設定ファイル**である。

### (c) `tauri.conf.json` の設定キーが腐りを免罪する経路 — 成立するが、これは意図どおり

`tauri.conf.json` は手書きの production 設定であり、`productName` / `installMode` 等が語彙へ入るのは「現に動いている実装」の定義に合う。ここは問題ない。ただし minisign 公開鍵の base64 塊 1 個が語彙へ入るのは副産物である。

### 判定の根拠は ADR ではなく、検出器自身が名乗る原則である

`scripts/governance-check.mjs:1387-1388` はこう書いている——**「この母集団を狭める 2 つは、どちらも同じ 1 つの原則から出ている——語彙を寄付してよいのは『現に動いている実装』だけである」**。

生成された ACL スキーマ・依存の lockfile・gitignore 済みのローカル設定・テスト実行の成果物は、そのどれでもない。**narrow 案はこの原則を守り、かつ同じ 9 件を出す。** 拡張子だけで採る案は、原則を破って何も買っていない。

### 未指定の実装詳細: `.json` はどちらのコメント除去へ回るか

`currentVocabulary:1453` は二分岐である（`/\.(ps1|toml)$/` なら `#` 除去、それ以外は `stripRustComments`）。計画 B3 は `.yml` を `#` 側へ回すと書くが、**`.json` については何も書いていない**——既定では `stripRustComments` に落ち、`"resolved": "https://registry.npmjs.org/…"` の `//` 以降が行末まで消える。JSON はコメント構文を持たないので、正しくは**どちらの除去も通さない第 3 の枝**である。偽陰性側へは倒れない（語彙が減る＝検出が厳しくなる）ため無害だが、恣意的な挙動が仕様として残る。narrow 案を採るなら、この枝は明示的に書くこと。

---

## フォールトインジェクション計画の穴

「複製に変異を当てる」原則自体は守られている（B7〜B9 が proxy snapshot 上と明記され、`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」に沿う）。穴は 4 つ。

### 1. B7 に逆向きが無い（`safety-nets.md`「検査の入力集合を、具体対象で検算する」は両方向を要求する）

B8 には「`docs/adr/` の種は捕まらないことも両方向で確認する」と明記があるが、**B7（述語）には順方向しか書かれていない**。SCREAMING_SNAKE 述語には固有の偽陽性リスクがある——語彙に在る SCREAMING_SNAKE（`CLEAR_COLOR` / `NO_LAUNCHER_READ` / `AREA_BUDGET` 等）が鳴らないことを対で測る必要がある。**これは絵空事ではない**: Phase A の A7 が書く文には `` `CLEAR_COLOR` `` が含まれ、それは Phase B が新たに監視する `docs/development-principles.md` の中にある（後述の測定では緑と確認できたが、B7 が両方向で書かれていればこの確認は計画の中に在ったはずである）。

### 2. B7 と B8 が同じ種で述語と母集団を同時に測っている

B7 の文面は「`docs/` 配下の文書へ架空の SCREAMING_SNAKE 識別子を種として蒔き」——これは述語と新母集団を**同時に**変異させる。失敗したとき、述語が効かないのか母集団が届いていないのかを切り分けられない。**B7 の種は既存母集団（`.claude/rules/*.md`）へ蒔き、B8 で初めて `docs/**` へ移すこと。**

### 3. 種が「捕まえると自称するもの」になっていない（この repo の作法から外れている）

`governance-check.test.mjs:874-875` は赤フィクスチャの作法を明記している——**「守りたい対象 = #736 の同クラス。…赤フィクスチャは実際に検出された `createObjectURL`」**。実際に起きた欠陥を種にするのがこの repo の形である。

B7/B8 の「架空の SCREAMING_SNAKE 識別子」はこの作法から外れる。**実在の種が手元にある**: `G12_NO_LAUNCHER_READ`（`docs/development-principles.md:71`）は、SCREAMING_SNAKE 述語が捕まえると自称するものの**実測された唯一の実例**であり、Phase A が消す語である。これを赤フィクスチャに据えるのが正しい（緑の対には `NO_LAUNCHER_READ` を使えば、同じ形で「語彙に在れば鳴らない」逆向きも同時に固定できる）。

### 4. B7〜B9 が「一度きりの測定」であって、機構になっていない

これが最も重い。`governance-check.test.mjs:955-958` が既に論証しているとおり、**この検出器は配線を戻されても実リポジトリでは緑のままである**。B10 が 8 件を是正した後、`docs/**` を targets から外す改修は finding 0 → finding 0 で、証跡行の数字（43 → 7）以外に痕跡を残さない。B7〜B9 は実装時に 1 回走る手順であって、後日の退行を捕まえる面ではない。**「fail-closed 経路の列挙」節の推奨 2（配線テストの新設）と同じ結論に落ちる。**

### B9 の設計そのものについて

B9 の前半（「B7 の種を `.yml`/`.json` へ書いた場合に免罪されることを確認し、受容する残余として明記する」）は、**穴が在ることを測って文書化する**手順である。それ自体は正しいが、上で測ったとおり**実際に免罪を配っているファイルの一覧を取る手順が無い**。B9 は「実際にどのファイルが語彙へ入るか」の列挙（本レビューの表）を含むべきで、`*.test.json` の有無だけを見ると `test-results/.last-run.json` を取り逃がす。

---

## ADR・既存契約との矛盾

| 契約・記録 | 拡大は矛盾するか | 根拠 |
|---|---|---|
| **「免除注記の機構は設けない」**（`:14` 冒頭契約） | **矛盾しない** | 計画は偽陽性を除外リストではなく語彙源の拡大（構造）で消しており、ADR 却下 2 の作法と同じ。`docs/superpowers/` `docs/adr/` の除外は finding ごとの免除注記ではなく**母集団の定義**であり、`staleIdentifierDocs` が既にパスで母集団を切っているのと同種 |
| **「語彙を寄付してよいのは『現に動いている実装』だけである」**（`:1387-1388`） | **`.json` 一括は矛盾する** | 生成物 306 KB・lockfile 49 KB・gitignore 済み 2 本が語彙へ入る。narrow 案なら矛盾しない（測定上等価） |
| **「テストコードは語彙を寄付しない」**（`:1391-1393`・test:899-903） | **矛盾する** | `test-results/.last-run.json` が `failedTests` を寄付する（実測） |
| **「決定的（ネットワーク・時刻・環境変数に非依存）」**（`:12`） | **矛盾する** | gitignore 済み 2 本 + テスト実行結果依存のファイルが語彙に入り、手元と CI で母集団が違う（#722 と同型） |
| **「`.json` は語彙源ではない」を受容する残余として記録**（`:1413`） | 解消してよい | 残余の解消であって却下理由の反故ではない。ただし**解消の仕方**が上 3 行に触れる |
| ADR **却下 3**「単語 1 つの識別子も対象にする」 | 矛盾しない | SCREAMING_SNAKE 述語は `_` を 1 つ以上要求する形（`/^([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)…/`）で、「こぶを 1 つ以上要求する」camelCase 述語と同じ構造。単語 1 つ（`GLOB` `INFO`）は入らない |
| ADR **却下 5**「ソースのコメントも語彙に入れる」 | 矛盾しない | `.yml` の `#` 除去は同じ向きの措置。ただし `.json` の枝が未指定（前節） |
| ADR「その後」節の**残る残余**「述語は camelCase しか見ない」 | 一部解消 | SCREAMING_SNAKE を足すことで狭まる。ADR 追記でこの残余の更新が要る（B12 の書く内容に**この項目が挙がっていない**——(a)〜(e) のどれにも当たらない） |
| `.claude/rules/governance-docs.md`「既に消滅した節の名前を正準形で書かない」 | 整合 | B10 が「歴史として書くならバッククォートを外す」を選ぶのは同じ形の措置 |

### 母集団の形について 1 点

`governanceDocs`（`:1198-1207`）は **`docs/adr/` を含む**（除外は `docs/superpowers/` のみ）。`headingRefDocs`（`:1214-1218`）は `docs/superpowers/` と `workspace/` のみ除外。つまり **「ADR は歴史ゆえ母集団外」はリポジトリ全体の不変条件ではなく、この述語に固有の判断である。** 拡大後は母集団の定義が 4 つ（`governanceDocs` / `headingRefDocs` / `staleIdentifierDocs` / 新しい `docs/**` glob）になり、`docs/adr/` の扱いだけが検査ごとに違う。**矛盾ではないが、ADR 追記（B12 の (b)）で「なぜこの述語だけ ADR を外すのか」を、G-references が ADR を見ていることと並べて書かないと、次に読む人が誤って統一しにいく。**

### 潜在: `docs/design/` は同じ性質を持つが除外されない

新母集団 7 件の中に `docs/design/2026-05-31-coherence-staleset.md` が入る（実測。照合 2 件・finding 0）。冒頭は `status: Agreed` / `date: 2026-05-31` の日付スラグつき設計メモで、**`docs/adr/` と同じ「決定当時の語彙を歴史として残す」性質**を持つ。今日は 0 件なので測定表には現れないが、除外がディレクトリで切られている一方、性質はディレクトリと一致していない。B12 で「除外の基準は歴史資料であること」と書くなら、`docs/design/` がその基準に当たるのに外れていないことを、意識的な受容として書くか除外へ足すこと。

---

## Phase A の判断の検算

### 「`scripts/governance-check.mjs` のコメントだけを変える変更は判定に影響しない」— **結論は正しいが、述べられた根拠は成り立たない**

- **G-stale-identifiers への影響は無い。** `currentVocabulary` は `.mjs` を語彙源に採るが `stripRustComments` を通す（`:1453` → `:1299-1301`）ので、`//` コメントの語は語彙へ入らない。A4 の差し替えは語彙を動かさない
- **しかし「コメントは判定の外」は偽である。** `adrCitationDocs`（`:1642-1648`）は `/\.(rs|mjs)$/ && !docs/ && !.test.mjs` を母集団に含む——**`scripts/governance-check.mjs` 自身が G-adr-citations の検査対象であり、コメント本文が読まれる**。A4 の新しい文には ADR 短縮引用が無いので今回は無害だが、**「コメントだから判定に影響しない」という一般命題を根拠に据えるのは誤りである**。同ファイル `:903` のコメントが `docs/adr/**` と `scripts/governance-check.mjs` を rules の配送対象へ足した経緯を書いているのは、まさに「検査を足す人へ規範を届ける」ためである
- **G-area-budget への影響は無い**（`ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]`・`:824`）。A7 が `docs/development-principles.md` を長くしても面積予算には算入されない。**A6 が触る `snotra-egui-runtime/CLAUDE.md` も算入されない**（ALWAYS_LOADED はルートの 2 本のみ）

**ゆえに「Phase A はフォールトインジェクション不要」という結論自体は支持する**（判定述語も母集団も変えない）。ただし計画 165 行目の根拠は「コメントしか変えないから」ではなく「**`checkConfigFieldReachability` と `NO_LAUNCHER_READ` 表という判定の実体を変えないから**」と書くべきである。A13（`git diff` で表と判定関数に差分が無いことを確認）が実質的にこれを担保している。

### Phase A → Phase B の相互作用を測った（計画が測っていない軸）

**Phase A は `docs/development-principles.md` へ新しい SCREAMING_SNAKE / camelCase を書き込み、その文書は Phase B が新たに監視する。** 計画の「9 → 8 になる想定」は検算されていなかったので測った（`proxy-phaseA.mjs`。A7 の差し替え文と A8 の `G12_` 除去を proxy 上の同ファイルへ当て、Phase B の採用射程で走らせた）:

```
Phase A 適用前:  finding 9 / 照合 43
Phase A 適用後:  finding 8 / 照合 42
Phase A が新たに生んだ finding: なし
```

**計画の想定どおりである。** A7 が書く `` `CLEAR_COLOR` `` は SCREAMING_SNAKE 述語に当たるが `renderer.rs` の `pub const` に実在するため緑、`` `check:colors` `` はどちらの述語にも当たらない、`` `[visual].background_color` `` は `.` を含むので `:1478` で落ちる。**Phase A の文面は Phase B を赤にしない。**

### Phase A の他の検査への当たり

- **G-heading-refs**: A6・A7 が書く正準形 `` `docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」 `` の見出しは実在する（`docs/build-commands.md:69` の `#### `。実測）。A9 が意味の側だけを確認すればよいという計画の判断は正しい
- **G-references**: A7 が書く `engine.rs` / `snotra-egui-runtime` 等はパス参照ではなく語であり、A1/A2 の rustdoc は `governanceDocs`（`.md` のみ）の外——計画の不変条件表の「rustdoc 内のパス参照は機械照合されない」は正しい

---

## 要対処（深刻度順）

### 深刻度 1（着手前に計画へ反映すべき・これが無いと拡大は黙って戻せる）

1. **新母集団 `docs/**` に fail-closed が無い。** `runAll` へ既存 `ctx.staleDocs.length === 0` と対称の 0 件検知を足す。実測: `docs/` が消えた proxy で `finding 0 / 照合 1 / exit 0`（緑）。既存の 3 つの 0 件検知はどれも代替にならない（`ctx.docs` も `ctx.refDocs` も他の母集団で埋まる）
2. **新母集団の配線テストが計画に無い。** `governance-check.test.mjs:959` の `describe("G-stale-identifiers の配線 …")` と同じ形を `docs/**` について新設する。同ファイル `:955-958` の論証（「配線を戻しても実リポジトリでは緑のままで、dogfood も証跡も気づけない」）が新母集団にそのまま当たる。B7〜B9 は 1 回きりの測定であってこの代わりにならない
3. **`VOCAB_SOURCE_EXT` への `.json` 一括追加を、手書きの設定ファイル 3 本へ絞る。** 測定上まったく等価（どちらも finding 9・偽陽性 0）でありながら、一括案は (i) 検出器自身が名乗る原則「語彙を寄付してよいのは『現に動いている実装』だけ」（`:1387-1388`）、(ii)「テストコードは語彙を寄付しない」（`test-results/.last-run.json` が `failedTests` を寄付）、(iii) ファイル冒頭の「決定的」契約（gitignore 済み 2 本で手元と CI の母集団が割れる・#722 と同型）の 3 つを破る。`.yml` の追加は問題なし

### 深刻度 2（実装時に必ず直す）

4. **B7 に逆向きの検算が無い。** 語彙に在る SCREAMING_SNAKE（`CLEAR_COLOR` / `NO_LAUNCHER_READ`）が鳴らないことを対で測る
5. **B7 と B8 が述語と母集団を同時に変異させている。** B7 の種は既存母集団（`.claude/rules/*.md`）へ蒔いて述語だけを切り分け、B8 で `docs/**` へ移す
6. **種を実在の欠陥にする。** `G12_NO_LAUNCHER_READ`（Phase A が消す語・SCREAMING_SNAKE 述語が捕まえる唯一の実測例）を赤フィクスチャに、`NO_LAUNCHER_READ` を緑の対にする。`governance-check.test.mjs:874-875` の作法（「赤フィクスチャは実際に検出された `createObjectURL`」）に揃う
7. **B9 は「実際に語彙へ入るファイルの列挙」を手順に含める。** `*.test.json` の有無だけを見ると `test-results/.last-run.json` を取り逃がす（本レビューの表を出発点にできる）
8. **`.json` を通すコメント除去の枝が未指定。** 既定では `stripRustComments` に落ち `https://` 以降が消える。JSON はコメント構文を持たないので、除去を通さない第 3 の枝として明示する

### 深刻度 3（計画の文言・ADR 追記で処理する）

9. **Phase A の「フォールトインジェクション不要」の根拠を書き直す。** 「コメントしか変えないから」は偽（`adrCitationDocs` が `scripts/governance-check.mjs` を含み、コメント本文を読む）。正しい根拠は「`checkConfigFieldReachability` と `NO_LAUNCHER_READ` 表を変えないから」で、A13 が既にそれを担保している
10. **B12 の書く内容に「camelCase しか見ない残余の更新」が抜けている。** ADR「その後」節の「残る残余」に SCREAMING_SNAKE の追加を反映する項目を (a)〜(e) へ足す
11. **B12 (b) に、G-references（`governanceDocs`）が `docs/adr/` を**含む**ことを併記する。** 「ADR は歴史ゆえ母集団外」はこの述語に固有の判断であって repo 全体の不変条件ではない。書かないと次の人が統一しにいく
12. **`docs/design/` の扱いを意識的に決める。** `docs/design/2026-05-31-coherence-staleset.md`（`status: Agreed` / 日付スラグ）は新母集団に入るが、`docs/adr/` を外した理由（歴史資料）と同じ性質を持つ。今日は finding 0（照合 2）なので測定表には現れない。除外へ足すか、受容する残余として書く

### 支持する判断（変更不要）

- **B1 の「`STALE_EXTRA_DOCS` の経路を使い、`staleIdentifierDocs` へは入れない」は正しい。** 既存の fail-closed を保つ理由づけに誤りは無い——足りないのは新母集団側の対称な検知である
- **`docs/adr/` の除外は正しい。** 逆向きで実測した: ADR を母集団へ入れると照合 64 / finding 28 で、うち 20 件が `ADR-stale-identifier-detector-scope.md` 自身の却下記録・失効記録（`folderState` `resetForShow` `createObjectURL` `alwaysOnTop` 等）。**ADR がその ADR 自身を赤にする**
- **M（モジュール `CLAUDE.md`）を採らない判断は正しい。** 真 1 : 偽 3 で、偽 3 が外部 API 語彙（Win32 / tao / TTC）という性質の説明も測定と整合する
- **偽陽性を除外リストではなく語彙源の拡大（構造）で消す方針は、冒頭契約「免除注記の機構は設けない」と ADR 却下 2 の作法に合う**
- **計画の測定表は再現できた**（D-+E = 43/12、採用案 = 43/9）。着手前ゲートの一次証拠に誤りは無い
