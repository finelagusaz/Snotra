# 独立導出（ラウンド 2）— #831 台帳の母集団を永続化する

`gh issue view 831` とコード・規約だけから導出した。`workspace/plan.md` / `workspace/research.md` は読んでいない。

前提として採った裁定（ユーザー・2026-07-28）: **`init` が slug 集合をファイルへ永続化し、`verify` は引数なしでそれを読む。**

---

## 必要な変更集合

### 骨格の 3 裁定（これが決まらないと下表は書けない）

| # | 決定 | 却下側 |
|---|---|---|
| D1 | 台帳ファイルは **`workspace/plan-review/ledger.json`**（成果物と同居・拡張子は `.json`） | dir の外（`workspace/.plan-review-ledger.json` 等）／`.md` 名 |
| D2 | **SSOT は 2 つに割らない**——ファイル = *機械の母集団*、会話の表 = *カバレッジ論証*（レイヤー → 分割根拠 → plan.md を覆うか）。`init` が両者を溶接する唯一の瞬間 | 「会話の表と ledger.json が一致していること」を完全性の証拠にする |
| D3 | `verify --slug <x>` は **exit 2 で拒否**（黙って無視しない） | 後方互換で受理／黙って無視 |

### ファイル + シンボル

| ファイル | シンボル / 位置 | 変更 |
|---|---|---|
| `scripts/plan-review-ledger.mjs` | 冒頭の契約コメント（`:12`–`:18`） | 「呼び出し側が渡した**内容**は書かない／**識別子**（スラグ）は書く」へ書き分ける。`--write` / `--report-to` 禁止は残す。ADR を引用 |
| 〃 | 冒頭の契約コメント（`:20`） | 「スラグ 0 件は exit 2」をコマンド別に書き直す（init = 引数 0 件、verify = 台帳が読めない/空/壊れている） |
| 〃 | 冒頭の契約コメント（`:19` 決定性） | **時刻・乱数を判定に入れない**という既存の約束を維持することを明記（後述 R1 の却下理由） |
| 〃 | **新** `export const LEDGER_FILE = "ledger.json"` | 台帳ファイルの basename。拡張子が `.md` でないことが `readLedgerDir` の `*.md` 列挙からの構造的除外である旨をコメント |
| 〃 | **新** `export function serializeLedger(slugs)` | `{ "slugs": [...] }` の JSON 文字列（純関数） |
| 〃 | **新** `export function parseLedger(text)` | `{ slugs } \| { error }`（純関数・JSON 破損／`slugs` が配列でない／空配列を error へ） |
| 〃 | **新** `export function readLedger(dir, io = fs)` | 存在しない → `{ error: "台帳ファイルが無い（init を打っていない）" }`。`readLedgerDir` と同じ io 注入の形 |
| 〃 | **新** `export function writeLedger(dir, slugs, io = fs)` | `init` だけが呼ぶ |
| 〃 | `parseArgs`（`:65`–`:79`）と doc コメント（`:61`–`:64`） | 末尾に「`verify` に `--slug` が付いたら error」を追加（D3）。文言に移行先（引数なしの `verify`）を書く |
| 〃 | `validateSlugs`（`:85`）| 変更なし。ただし **verify 側でも読んだ slugs に適用**する（手編集された台帳の重複・形不正を exit 2 で止める） |
| 〃 | `readLedgerDir`（`:137`） | 変更なし（`.endsWith(".md")` が ledger.json を構造的に除外）。**なぜ拡張子が load-bearing か**を 1 行コメント |
| 〃 | `formatReport`（`:121`–`:134`） | 署名へ母集団の出所を足す（`formatReport(rows, ledgerPath)`）。見出し行に**読んだ台帳の絶対パス**を出す（§3・§6-3 の唯一の staleness 検出器）。**同じ変更で既存の呼び出し 3 点（`test.mjs:113, :119` と `main()`）を更新し、見出しにパスが載ることを assert する 1 本を足す**——既存テストは `toContain` なので、引数を渡し忘れると `母集団の出所: undefined` を印字したまま全件緑になる（この変更が塞ごうとしている形そのもの） |
| 〃 | `usage()`（`:154`–`:163`） | `verify` から `--slug` を消す。exit code の意味（0/1/2）を 3 行で書く |
| 〃 | `main()`（`:165`–`:194`） | init: `mkdirSync(dir)` の**直後**に `writeLedger`。verify: `readLedger` → `validateSlugs` → `classifyEntries`。読めない/空/壊れは **exit 2**（exit 0 でも 1 でもない） |
| `scripts/plan-review-ledger.test.mjs` | `describe("parseArgs")` の 1 本目（`:27`–`:32`） | **偽になる**（`verify --slug rust --slug docs` は現在 success を期待）。init 用へ書き換え |
| 〃 | 契約テスト（`:38`–`:43`）のコメント | 「内容の口は持たない」は残し、**識別子は書く**へ書き分け |
| 〃 | **新** `describe("verify の母集団")` | 【赤】`verify --slug x` が error / 台帳ファイル無し・空配列・JSON 破損・重複入り台帳がすべて error（＝ exit 2 経路）/【緑】`serializeLedger` → `parseLedger` の往復 |
| 〃 | **新** プロセス級テスト 1 本（`node scripts/plan-review-ledger.mjs` を `mkdtemp` の cwd で spawn） | **`--` 転送と実 fs 経路を一度は測る**（`RETROSPECTIVE.md:15` の教訓）。危険性は「特に危ない箇所 H3」 |
| `.claude/skills/plan-review/SKILL.md` | Step 3 のコマンド（`:137`） | `npm run plan:ledger -- verify`（`--slug` を消す） |
| 〃 | Step 2 の台帳段落（`:27`–`:31`） | **D2 を 1 文で書く**——会話の表はカバレッジ論証、`init` が母集団を凍結する、verify は打ち直さない |
| 〃 | Step 2 の init 段落（`:33`–`:39`） | 「判定の中身」の列挙へ「母集団の永続化（打ち直す機会を消す）」を足す。正本は script 契約 + テスト、のままにする |
| 〃 | Step 3（`:140` 付近） | **exit 2 の意味を書く**——「台帳が無い/壊れている＝ `init` を通っていない。verify を打ち直すのではなく Step 2 からやり直す」 |
| 〃 | `Write` を持たない理由（`:89`）と `allowed-tools`（`:4`–`:9`） | **理由文に台帳の改竄耐性を足す**（最重要の間接参照。根拠 §1） |
| 〃 | 受容する残余（`:96`） | スカウトは `ledger.json` も書ける——残余の射程が成果物から**母集団**へ広がったことを明記 |
| 〃 | 新設 ADR への引用 1 箇所 | G-adr-citations の母集団は `.md` のガバナンス文書のみ（`governance-check.mjs:1037`–`1045`, `:1443`）。**script のコメントから引いても照合されない** |
| `.claude/skills/start-issue/SKILL.md` | Step 3 の上書き列挙（`:58`） | `workspace/plan-review/` の中身が「スカウトの成果物」だけでなくなる。1 語（台帳を含む）を足す |
| 〃 | 5a の収束原則（`:149`）| 「持続する面は `plan.md` だけ」は **D1 を採る限り真のまま**（ledger は `init` の削除で毎ラウンド死ぬ）。**dir の外に置いた瞬間に偽になる**——D1 の可否を判定する述語なので、注記として残すか、少なくとも実装時に読み直す |
| `docs/adr/ADR-ledger-population-persistence.md` | **新規**（heading = stem・`ADR-<slug>.md` 形） | issue が「未決」と名指しした論点（識別子 vs 内容）の裁定と、却下案 R1–R5。`AGENTS.md`「ドキュメント参照」の「否定の知識が生じた決定のみ」に合致。**先行決定 2 本を引用して差分を書く**（`docs/adr/ADR-race-check-population-tooling.md` = 同じ軸の先例／`docs/check-skill-skeleton-design.md`「必須 1 — 母集団」= アンカー型の規範。§10） |
| `package.json` | — | **変更なし**（`plan:ledger` の script 名も値も変わらない） |
| `.claude/rules/safety-nets.md` | frontmatter `paths`（`:1`–`:9`） | **提案（要ユーザー合意）**: `scripts/plan-review-ledger.mjs` を追加。根拠 §5 |

---

## 根拠

### 1. 最重要の間接参照 — `allowed-tools` が新しい職務を黙って引き受ける

`ledger.json` が改竄されないのは、**オーケストレーターがそれを書く手段を持たないから**である。`.claude/skills/plan-review/SKILL.md:4`–`:9` は `Read` / `Glob` / `Agent` / `Bash(gh issue view *)` / `Bash(npm run plan:ledger *)` しか許していない——`Write`/`Edit` が無く、`Bash` も 2 つの glob に閉じているので `echo > ledger.json` が打てない。

しかし**その理由文（`:89`）は成果物の偽造しか説明していない**:

> **オーケストレーター自身は `Write` を持たない**（`allowed-tools` から意図的に外してある）。持たせると、返り値で受けた内容を自分で成果物ファイルへ転記でき、**実在確認も中身の検査も自作自演で通ってしまう**

この変更後、同じ 1 行が「母集団の偽造」も止めている。理由が書かれていない防御は、**将来 `Bash(*)` や `Write` を足す編集で黙って外れる**（`docs/development-principles.md:117`「コンパイラを持たない機構…漏れた消費者は false green で気づかれないまま残る」）。ここが今回の変更で最も静かに壊れうる箇所である。

### 2. 同居（D1）が fail-closed を作る — 母集団を書き換える唯一の口が証拠を破壊する

`main()` の `init`（`:179`–`:189`）は `rmSync` → `mkdirSync` の順である。台帳をこの dir の中に置くと:

- 母集団を小さく書き換える唯一の手段は `init` を打ち直すことであり、**それは全成果物を消す**。直後の `verify` は全件不着 → exit 1。つまり **issue が挙げた「母集団が黙って縮む」形は構造的に作れなくなる**（縮めると赤くなる）。
- 台帳だけ古い／成果物だけ古い、という**部分生存が起きない**。削除 1 回で母集団と成果物の新鮮性が同時に揃うのは `SKILL.md:114` が既に述べている設計理由であり、台帳を同居させることはその理由の**適用範囲を広げるだけ**で、新しい原理を持ち込まない。

判断の向きは `docs/adr/ADR-plan-ownership-boundary.md` の却下案 6 と同型である——「同一性が不明なとき、あるものとして進む」は却下、誤りの代償が非対称だから。台帳が読めない/壊れている場合に **exit 0 でも exit 1 でもなく exit 2**（母集団不明）へ倒すのはその系である。exit 1（不着）へ倒すと「起動したのに届かなかった」と区別できなくなり、SKILL の再起動手順（`:142`）が的外れな回復を促す。

### 3. D3 — `verify --slug` を黙って無視してはならない

`.claude/skills/plan-review/SKILL.md:137` の**文字列そのもの**を静的に検査する機構は存在しない:

- `governance-check.mjs` の `checkBuildCommands`（`:390`）が npm script の実在を照合するのは `docs/build-commands.md` **だけ**であり、`plan:ledger` はそこに記載が無い（`grep -rn "plan:ledger" docs/` → `docs/superpowers/` の 2 本のみ）。
- `selectChecks`（`.claude/hooks/post-edit.mjs:118`–`:143`）は `.claude/skills/**` にも `scripts/*.mjs` にも検査を割り当てない。**沈黙は「何も走らなかった」**（ルート `CLAUDE.md`「フック」）。

したがって **SKILL 本文と CLI の乖離を捕まえる検出器は「実行したときの exit 2」しか無い**。`--slug` を受理・無視する実装にすると、旧文字列が SKILL に残ったままでも緑で通り、issue の穴が「打ち直しに依存する」から「打ち直しても効かないのに誰も気づかない」へ移動するだけになる。

### 4. この変更で偽になる既存の記述（網羅）

| 箇所 | 現在の記述 | 変更後 |
|---|---|---|
| `scripts/plan-review-ledger.mjs:13` | 「**呼び出し側が渡した内容を、このスクリプトは決して書かない。** 受け取るのはスラグだけで、`init` はディレクトリを作り、`verify` は読んで報告する」 | 字義的に**偽**。`init` はスラグを書く。識別子/内容の書き分けが要る（issue が「未決」と名指しした論点そのもの） |
| 同 `:20` | 「スラグ 0 件は exit 2」 | verify にはもうスラグ引数が無い。コマンド別へ |
| 同 `:62`–`:64` | 「argv からスラグ列を取り出す。`--slug <name>` の繰り返しのみを受ける」 | verify では受けない |
| 同 `:157`–`:161`（usage） | `verify --slug <name> ...` | 偽 |
| `scripts/plan-review-ledger.test.mjs:28` | `parseArgs(["verify","--slug","rust","--slug","docs"])` が success | **テストが偽になる**（赤で落ちる。これは望ましい形——移行漏れ検出器として働く） |
| 同 `:38`–`:43` のコメント | 「内容を受け取る口を持たない（契約: 呼び出し側の内容を書かない）」 | 半分だけ真。書き分けが要る |
| `.claude/skills/plan-review/SKILL.md:137` | `verify --slug <slug1> ...` | 偽 |
| 同 `:89` / `:96` | `Write` 非保持の理由・受容する残余 | 真だが**不完全**（§1・§7） |
| `.claude/skills/start-issue/SKILL.md:58` | `workspace/plan-review/`（…スカウトと Step 2b の成果物。ファイル名が毎回変わるため列挙せず） | 台帳が加わる。「成果物」だけを名指す記述は狭くなる |
| 同 `:149` | 「持続する面は `plan.md` だけ（`workspace/plan-review/` は毎ラウンド削除され…）」 | **D1 なら真のまま／D1 を外すと偽**。この一文が置き場の可否を判定する述語になっている |
| `docs/superpowers/specs/2026-07-28-plan-review-loop-design.md:50, :129` | 同趣旨（`rm -rf` で毎ラウンド消える） | **書き換えない**（履歴資料。§6） |
| `RETROSPECTIVE.md:15` | `npm run plan:ledger -- init` の文字列を実行した記録 | **真のまま**（init の形は変わらない） |

### 5. 実装後、これが壊れたことに誰がどう気づくか（検出器の所在）

| 壊れ方 | 検出器 | 状態 |
|---|---|---|
| 判定関数の退行 | `vitest`（`npm test`）— CI `.github/workflows/ci.yml:39, :116` | **あり**。ただし **PostToolUse hook は走らない**（`selectChecks` が `scripts/*.mjs` に検査を割り当てない）→ 実装中の沈黙は合格ではない。手で `npm test` を打つのが手順に要る |
| SKILL の文字列と CLI の乖離 | **静的検出器なし**。実行時 exit 2 のみ（→ D3 が必須の理由） | **穴（受容）** |
| ADR の名前・見出し不一致 | `G-adr-file-names`（`governance-check.mjs:1393`） | あり |
| ADR が誰からも引用されない | **なし**（`G-adr-citations` は引用側しか見ない）。`.mjs` コメントからの引用は母集団外（`governanceDocs` は `.md` のみ・`:1037`） | **穴** → `.md` から 1 箇所引く |
| SKILL 内の見出し参照の腐り | `G-heading-refs`（母集団は `headingRefDocs`・`:1053`。`.claude/skills/**/SKILL.md` を含む） | あり |
| 台帳ファイル形式の消費者漏れ | 消費者は `verify` 1 つだけ（コンパイラなし機構だが依存者が 1）| 小 |
| **この機構自体を編集する人へのフォールトインジェクション義務の配送** | `.claude/rules/safety-nets.md` の `paths` は `.claude/skills/**` と `scripts/governance-check.mjs` を含むが **`scripts/plan-review-ledger.mjs` を含まない**（`:1`–`:9`） | **穴**。SKILL 側を触れば配送されるので今回は届くが、**次にスクリプトだけを直す人には届かない** |

最後の行は「セーフティネットを新設・変更したら safety-nets.md を読む」という規範の**配送漏れ**であり、`paths` に `scripts/plan-review-ledger.mjs` を足せば塞がる（`G-rules-globs` が glob の実在を照合する）。ただしこれは `.claude/rules/` の変更＝エージェント設定であり、ルート `CLAUDE.md`「最重要ルール 2」により**ユーザー合意が要る**。導出としては「穴が在る・塞ぎ方はこれ・合意が要る」までを述べ、必須の変更としては数えない。

### 6. ラウンド／サイクルをまたいで台帳が生き残る経路

D1（同居）を採ると、生き残る経路は **1 クラスに収束する**: **`init` を通らずに `verify` が走る**。入り口は 3 つあるが、いずれも同じクラスである。

1. **`init` の打ち忘れ・中断後の再開**——前ラウンドの dir がそのまま残り、台帳も成果物も揃っているので `verify` は緑。ただし**今日も同じ穴が在る**（スラグを打ち直せば古い成果物が実在判定を通る）。永続化はこの穴を広げも狭めもしない。
2. **git 復元**——`/start-issue`「Step 6」の `git add workspace/`（`:172`）が `ledger.json` をコミットへ載せるため、ブランチ切替・`git checkout` で**母集団ごと**復元されうる。これは 1 の入り口違いであって独立の穴ではない（`init` を通っていない状態で `verify` を打つ、という同じ形）。
3. **別 cwd / worktree での `verify`**——`path.resolve(process.cwd(), LEDGER_DIR)`（`:177`）は cwd 依存。**現在**は「全件不着 exit 1」（起動失敗と区別できない）だが、**変更後は「台帳が無い → exit 2」**になり、意味が正しくなる。**改善**である。

一方 **D1 を外して dir の外に置くと、新しい穴が開く**: `rm -rf` を生き延びた古い台帳 ＋ 新品の空 dir → 毎ラウンド全件不着（false red）か、さらに悪く「古い台帳 ＋ 別経路で残った古い成果物」で false green。加えて `/start-issue`「5a」の「持続する面は `plan.md` だけ」（`:149`）が偽になり、収束判定の前提が黙って変わる。**同居は美観ではなく、この 2 つを同時に殺すために要る。**

**受容する残余（1 文で書けるべきもの）**: 「`init` を通らずに `verify` が走った回」は依然として緑になりうる。機構は `init` の**実行**を強制しない（`/plan-review` を起動しない自由と同じクラス）。唯一の実効的な抑止は、スカウトへ渡す絶対パスが `init` の stdout からしか得られないこと（`:186`–`:187`）である。

### 7. 「`init` が書く」が静かに変える前提

1. **契約の字義が偽になる**（§4）。issue の言うとおり、**決めずに実装すると契約が黙って緩む**。ADR が要るのはここ——「スラグは内容ではなく識別子だから書いてよい」という線引きを、次に `--summary` や `--layer-reason` を足したくなった人が読める形で残す。
2. **`allowed-tools` が改竄耐性を担う**（§1）。
3. **受容する残余の射程が広がる**（`SKILL.md:96`）。スカウトは `general-purpose`＝全ツール型ゆえ `ledger.json` を書き換えられる。プロンプト契約（`:87`「書いてよいのは割り当てられた 1 ファイルだけ」）が母集団まで守ることになった。
4. **ラウンド内に「持続する面」が 1 つ増える**——セッション中断後、会話コンテキストが巻き戻っても母集団が復元できる（`CLAUDE.md`「長時間の委譲タスクは中断を前提に設計する」に沿う良性の性質）。`/start-issue:149` の「持続する面は plan.md だけ」は**ラウンドをまたぐ持続**の話なので両立するが、**読者にはそう読めない**——だから注記が要る。
5. **PR の差分に母集団が載る**（`git add workspace/`）。レビューで「何体を起動したか」が見えるのは利点。復元経路（§6-2）はその裏面。
6. **スクリプトが「読むだけ」でなくなる**ため、テストで実 CLI を回したくなる誘因が増える。既に `rmSync` を持つので blast radius 自体は増えないが、**回す動機が増えることが危険**（H3）。

### 8. 同名・別概念 / 同概念・別名

**列挙の方法**（結論だけでなく手順を残す）: シンボル grep（`plan:ledger` / `plan-review-ledger` / `plan-review/`）に加えて、**概念ラベルの全域スイープ**を別に打った——`台帳` / `母集団` / `スラグ` / `plan-review`（末尾スラッシュなし）／ `ledger`（大小無視）を `node_modules .git target workspace` 除外で全ファイルへ。**シンボル grep では出なかったファイルが 6 本出た**（下の ★）。

**同名・別概念（巻き込んではならない）**

| 語 | 別概念の在処 |
|---|---|
| `verify` | `package.json` の npm script `verify` = `cargo check --workspace && npm test`。`plan:ledger verify` とは無関係 |
| slug | `docs/adr/ADR-<slug>.md` の命名規約（`.claude/rules/governance-docs.md`、`G-adr-file-names` の `ADR_FILE_NAME`）。レイヤースラグとは別。★ `.claude/skills/health-check/SKILL.md:36` の `<project-slug>`（harness のメモリ領域）も別 |
| **ledger** | ★ **`snotra-core` の `index_stale` ledger**（`snotra-core/CLAUDE.md:14`・`docs/architecture.md:99`・`snotra-core/src/engine.rs`）。コヒーレンシ判断の台帳であり本件と無関係。`grep -i ledger` の主要ヒットはこちら |
| レジャー / 台帳 | `.gitignore` の `.superpowers/`（「subagent-driven-development の進捗レジャー」）。別機構 |
| 母集団 | `governance-check.mjs` の各 `G-*` が言う「母集団の欠落」・`/race-check` の並行境界母集団（`scripts/race-boundaries.mjs`）。同じ語・別対象 |

**同概念・別名（間接参照。シンボル grep では届かない）**

- 「配送欄」`SKILL.md:142, :153, :165` — `verify` の出力の貼り先
- 「独立レビュー不成立」`SKILL.md:130, :144` / `start-issue:145, :155` — 不着の言い換え。**収束判定の入力**になっている
- 「新鮮性は『上書き』ではなく…削除が保証する」`start-issue:58` — `init` の `rm -rf` の間接参照
- 「持続する面」`start-issue:149` / `docs/superpowers/specs/2026-07-28-plan-review-loop-design.md:50` — D1 の可否を判定する述語
- 「検査の入力集合を、具体対象で検算する」`.claude/rules/safety-nets.md` — 母集団の一般則。今回の変更はこの節の実例になる
- 「台帳の全エントリを起動する」`SKILL.md:41` — 母集団の**利用**側
- ★ **「配送台帳」`.claude/skills/norm-review/SKILL.md:80`** — 「`K = 捕まえた + 素通りした + 対照も捕まえた + 不成立` が立たない報告は…（`/plan-review`「Step 3 — 結果の統合と報告」の配送台帳と同じ理由）」。**別スキルが本件の台帳を規範の根拠に引いている**。命題（母集団を全件書く）は変更後も真——編集不要だが、Step 3 の見出しを動かせば参照が腐る
- ★ **`.claude/skills/norm-review/SKILL.md:48`** — 「このスキルは `Edit` を持たない（`/plan-review` が `Write` を外しているのと同じ理由で、測る側が測られる文書を書き換えないため）」。**§1 で拡張する理由文の下流コピーが別スキルに在る**。要旨（測る側が測られる面を書かない）は台帳へも素直に延びるので編集不要だが、`SKILL.md:89` を書き換える人がここを知らないと、片方だけが更新された理由文になる
- ★ `/plan-review` の見出しへの**正準形参照は 17 件**（`` `/plan-review`「…」 `` の全件を `wc -l` で数えた値。`docs/superpowers/` と `workspace/` を除いた分が `G-heading-refs` の母集団・`governance-check.mjs:1053`–`:1057`）。**内訳のファイル別列挙は `head` で切った出力から起こしたので、ここには載せない**——切った時点で母集団は推測に戻る（`docs/development-principles.md:115`）。実装時に切らずに再列挙すること。件数だけで十分な用途（**見出しを動かさない**理由の量的根拠）に留める

### 9. `docs/development-principles.md`「列挙の完全性」3 クラスの当て込み（`:113`–`:117`）

- **glob の意味論をツール自身に問う**:
  (a) `readLedgerDir` の `.endsWith(".md")` — ledger.json を除外するのは**この 1 行**。台帳を `.md` 名にすると命名逸脱行になって毎回 exit 1（fail-closed だが不可解）。拡張子が load-bearing。
  (b) `allowed-tools` の `Bash(npm run plan:ledger *)` — **末尾 `*` が `-- verify`（追加引数なし）に当たるかは未実測**。`RETROSPECTIVE.md:15` は「SKILL に書いた文字列そのものは一度も走っていなかった」という同型の失敗の記録である。受け入れ条件へ入れる。
  (c) `git add workspace/` はディレクトリ列挙なので `.json` も自動で載る（意図した挙動として確認する）。
  (d) `governanceDocs`（`:1037`）・`headingRefDocs`（`:1053`）はどちらも `.md` のみ。**`.mjs` の冒頭コメントに書いた参照はどの検査の母集団にも入らない**（既に `:8` が ADR を引用しているが未照合）。
  (e) `.claude/rules/safety-nets.md` の `paths` glob（§5）。
- **序数参照**: `/plan-review`「Step 2」「Step 2b」「Step 3」・`/start-issue`「5a」「5b」は多数から正準形で参照され `G-heading-refs` が照合する。さらに `scripts/governance-check.test.mjs:686` の fixture が `## Step 2b — 独立導出 + 差分` を文字列で固定している。**見出しの新設・改名・番号のずらしを行わない**（今回の変更は既存節の中で閉じる）。**exit code も序数的識別子**である——`2` に「母集団が確定できない」という意味を一本化し、usage に書く（今の `2` は「引数の形が不正」）。
- **コンパイラを持たない機構**: CLI 引数の形（消費者は SKILL 2 箇所・test・usage・`RETROSPECTIVE.md:15`）、`allowed-tools` の glob、npm script 名、`ledger.json` のスキーマ。**すべて grep + 本導出で数え上げた**。移行漏れは compile-fail では出ない。

### 10. 先行決定 2 本 — ADR はこれらとの差分で書く（概念スイープで発見）

シンボル grep には出ないが、**「母集団をどう定めるか」を既に裁定した文書が 2 本ある**。引かずに新 ADR を書くと、却下理由が宙に浮くか、既存の裁定と黙って食い違う。

1. **`docs/check-skill-skeleton-design.md`「必須 1 — 母集団」**（`.claude/rules/safety-nets.md` が SSOT と名指ししている）
   - `:36`「**母集団を得る手順が決定的でないなら、その検査は結論を持たない。**」——**issue #831 はこの命題の実例**である。「エージェントが打ち直す」は手順ではない。
   - `:40`–`:46` アンカー型の表: **「列挙アンカー…この型を母集団に採らない——列挙は入口の手がかりに留め、母集団は構造物か差分から取る」**。現行の台帳は列挙アンカーそのものであり、**規範に照らして既に非推奨の型**だった。
   - `:50` の差分アンカーの正しい形——「(1) 差分を決定的に取得（リビジョン明示）→ (2) その中の候補を列挙 → **(3) 以降の母集団は候補集合**」——が、**本変更の形と一致する**（`init` = 決定的な取得と凍結、`verify` 以降は候補集合を読むだけ）。裁定の正当化はここから引くのが最も強い。
   - falsify するか: `:102`–`:104` の採点表は `-check` 6 スキルが母集団（`:119`「6 つは母集団が互いに素」）で `/plan-review` を含まない。**偽になる記述は無い**。
2. **`docs/adr/ADR-race-check-population-tooling.md`** — `/race-check` の母集団取得を `scripts/race-boundaries.mjs` へ機構化した際の否定の知識。**同じ軸の先例**であり、しかも `:24` が本導出の E3（報告に母集団の出所を印字する）を先に一般化している:
   > **ツールが両方を同時に解く**——常に ①〜⑧ の全種を 0 件行つきで印字するため、**証拠は義務ではなく出力そのものになり、費用が結論に依存しなくなる**

   新 ADR は「あちらは母集団の**取得**をツール化した／こちらは母集団の**保持**をツール化した」という差分を明示し、`条項を足す案`（＝ SKILL 本文に「打ち直すな」と書く案）が**同じ理由で却下される**ことを引き継ぐ（規範を厚くしても、忠実な読者の負荷が増えるだけで打ち間違いは減らない）。これは R2 の却下理由を規範側から補強する。

---

## 却下した案（ADR に書くべき否定の知識）

- **R1: 台帳に生成時刻を持たせ、`verify` が「plan.md より古い」「成果物が台帳より古い」を見る** — 却下。`scripts/plan-review-ledger.mjs:19` が明示している「決定的（ネットワーク・**時刻**に非依存）」を破る。issue の射程は**打ち直し**であって**打ち忘れ**ではない。§6 の残余として受容する方が、機構の性質を保てる。
- **R2: `verify` が `--slug` を後方互換で受理し、台帳と一致すれば通す** — 却下。母集団が 2 つになり、「派生コピー同士の一致を完全性の証拠にしない」（`AGENTS.md`「検証の作法」）に正面から反する。打ち直す機会を消すという裁定の目的そのものを失う。
- **R2′: 機構を変えず、SKILL 本文へ「スラグを打ち直すな／台帳と照合せよ」という条項を足す** — 却下。`docs/adr/ADR-race-check-population-tooling.md`（同じ軸の先例）が「条項を残す案・足す案」を却下した理由がそのまま当たる——**規範を厚くしても忠実な読者の負荷が増えるだけで、打ち間違いは減らない**。同 ADR の「証拠は義務ではなく出力そのものになる」形（＝ E3）を採る。
- **R3: 台帳を `workspace/plan-review/` の外に置く** — 却下。§6 の後段（false red / 部分生存 / `start-issue:149` が偽になる）。
- **R4: 台帳を `.md` で置く** — 却下。`readLedgerDir` の命名逸脱行になり、毎回 exit 1。
- **R5: `plan:ledger:verify` のような npm script を新設して引数を消す** — 却下。`package.json` の面が増えるだけで、`--` 転送の問題は既に解決している（`RETROSPECTIVE.md:15` で実測済み）。

## 触らない（根拠付き）

- `package.json` — script 名も値も不変。`plan:ledger` は `node scripts/plan-review-ledger.mjs` のまま。
- `docs/build-commands.md` — `plan:ledger` の記載が無い（grep 0 件）。`G-build-commands` の母集団に入っていないので、足すと**新しい同期義務**が生まれる。足さない。
- `AGENTS.md` / ルート `CLAUDE.md` — **台帳・`plan:ledger` への言及は 0 件**（両ファイルにあるのは `/plan-review`「Step 2b」への行き先参照だけ・`AGENTS.md:58, :61`）。`G-area-budget` の常時ロード面に余裕が小さく、書き足す理由が無い。
- `RETROSPECTIVE.md:15` — `init` の文字列は不変。記述は真のまま。
- `docs/superpowers/plans/2026-07-27-*.md` / `specs/2026-07-28-*.md` — **履歴資料**。`governanceDocs:1041` と `headingRefDocs:1055` の両方が `docs/superpowers/` を明示的に除外している（意図的に不検査）。`:50` `:129` の記述は当時の事実として残す。
- `.claude/hooks/**` / `.githooks/**` / `.github/workflows/**` — `plan:ledger` への言及 0 件。CI は `npm test` と `governance-check` を既に回している。
- `scripts/governance-check.mjs` — 新しい `G-*` 検査は提案しない（SKILL 文字列の静的照合は「skill 本文の CLI 呼び出しを parse する」という新しい母集団を作ることになり、費用対効果が合わない。exit 2 で足りる）。したがって `.claude/skills/health-check/references/mechanized-checks.md`（`G-*` の一覧）も**変更なし**（`plan-review` / 台帳への言及 0 件を実測）。
- `.claude/skills/norm-review/SKILL.md`（`:48`, `:80`）・`.claude/skills/implement/SKILL.md:114`・`.claude/skills/retrospective/SKILL.md:64`・`.claude/skills/race-check/SKILL.md:14`・`.claude/skills/health-check/SKILL.md:51`・`.claude/agents/code-reviewer.md` — **概念スイープで拾ったが、いずれも命題が変更後も真**（§8 ★）。ただし `/plan-review` の見出しを動かせば全部が腐る。
- `docs/check-skill-skeleton-design.md` — **引用はするが編集しない**（§10-1。偽になる記述が無いことを採点表まで確認済み）。

---

## 特に危ないと考える箇所

- **H1（設計を変える）— 台帳が 2 つになる**: `SKILL.md:27` は「台帳を**会話へ表として出力してから**起動する」と要求している。ファイル台帳を足すと**同じ名前の物が 2 つ**になる。D2 を明文化しないと、「表と ledger.json が一致していること」を完全性の根拠にする読み方が生まれ、**issue の失敗クラス（派生コピー同士の一致）を 1 段上で再生産する**。会話の表にしか無い情報（分割根拠・plan.md を覆うかの検算・`SKILL.md:29`）はファイルに入らないので、両者は役割が違う——そう書く。
- **H2 — `allowed-tools` の理由文**（§1）。書き忘れると、防御が理由なしで立っている状態になる。
- **H3 — 実 CLI テストの破壊力**: `init` は `rmSync(path.resolve(process.cwd(), LEDGER_DIR), { recursive: true, force: true })`。テストを**子プロセスの `cwd:` を `mkdtemp` へ向けて** spawn すること（`process.chdir()` は vitest がプロセスを共有するため禁止）。加えて、削除前に「解決先がリポジトリルート配下の本物か」を assert する。リポジトリルートで `init` を打つと**進行中のレビュー成果物が消える**。
- **H4 — `verify` の 3 値化**: exit 0 / 1 / 2 の意味が「実在 / 不着 / 母集団不明」になる。`SKILL.md:142` の再起動手順は exit 1 用であり、exit 2 で再起動すると**同じ穴に落ち続ける**（台帳が無いのだから何度やっても無い）。Step 3 に exit 2 の行き先を書かないと、機構は正しいのに手順が的外れになる。
- **H5′ — `formatReport` の引数追加が、それ自体 false green を作る**: 既存の 3 呼び出し点を更新し忘れると `母集団の出所: undefined` を印字したまま全テストが緑になる（既存の assert は `toContain`）。**同じ変更で呼び出し点を全部移し、パスが載ることを 1 本 assert する**（`AGENTS.md`「条件別チェック」の「関数・型を…改名／導入 → 呼び出し元を grep」・新旧を 1 タスクに束ねる）。
- **H5 — 空の `slugs: []` を書けてしまう**: `init` は `validateSlugs` を先に通すので通常は起きないが、**`parseLedger` 側でも空配列を error にする**（多層。`validateSlugs:87` と同じ理由——0 件中 0 件実在で自動成立する）。

## 受け入れ条件（実装者がこれを満たすまで完了ではない）

1. `npm test` を**手で**実行する（PostToolUse hook は `scripts/*.mjs` に検査を割り当てない＝沈黙は合格ではない）。
2. **SKILL に書いた文字列そのもの**を一時ディレクトリを cwd にして 1 度実行する: `npm run plan:ledger -- init --slug a --slug b` と `npm run plan:ledger -- verify`。`--` 転送と `Bash(npm run plan:ledger *)` の glob 一致を実測する（`RETROSPECTIVE.md:15`）。
3. フォールトインジェクション 4 本（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」・**稼働中のガードを弱めず、一時 cwd の複製に変異を当てる**）:
   (a) `init --slug a --slug b` → `a.md` だけ書く → `verify` が exit 1 で `b` を不着と言う（＝ issue の再現ケースが**打ち直し無しで**赤くなる）
   (b) `verify --slug a` → exit 2（移行文言つき）
   (c) `ledger.json` を壊す / 消す → exit 2（exit 0 でも 1 でもない）
   (d) `ledger.json` を `{"slugs":[]}` にする → exit 2
4. `node scripts/governance-check.mjs` が緑（新 ADR の名前・見出し・引用の照合を含む）。
