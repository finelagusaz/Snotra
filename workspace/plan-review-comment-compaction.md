# 独立導出: コードコメントの簡潔化

対象: issue #1185（コードコメントを要点中心に簡潔化する）

本書は issue の要求文と決定済みの要求判断だけを入力とし、コードと規範文書を自分で読んで導出したものである。`workspace/` 配下の既存計画・調査は読んでいない。

---

## 0. 測定の記録（件数の根拠）

| 測ったもの | コマンド | 結果 |
|---|---|---|
| 3 ファイルの規模とコメント密度 | `wc -l` / `grep -c -E '^\s*(///\|//!\|//)'` | indexer.rs 4906 行中 1217 行（24.8%）/ build.rs 608 中 220（36.2%）/ index_tree.rs 1262 中 473（37.5%） |
| ガバナンス基線 | `npm run governance:check` | 全検査 passed（検査 23 件）。**見出し参照 357 件を md 48 件 + .rs 102 件 + スクリプトのコメント 116 件から照合** |
| 3 ファイルの定型ラベル | `grep -Fn "回帰テスト"` と `rg -n "回帰テスト"`（2 方法） | **両方とも exit 1（0 件）** |
| 3 ファイルの正準形見出し参照 | `grep -nE '\`[^\`]+\`(§ ?[0-9.]+ ?)?「'` | **grep の形で当たった行**。所在は §2c〜2e が持つ（件数を書かない——うち少なくとも 1 件（`indexer.rs:53`）は `HEADING_REF` の一致を生成しないので、grep の数は照合件数ではない・→ A-6） |

日本語混じりの否定文字クラスは使っていない。件数を根拠にした所見（定型ラベル 0 件）は `grep -F` と `rg` の 2 方法で確かめた。

---

## 1. 導出したファイル一覧

### 1a. 3 ファイルだけでは完了条件を満たせない

完了条件 1・2・3・6 は**いずれも `.rs` の外**にしか着地しない。指定 3 ファイルは完了条件 4・5 だけを担う。

| ファイル | 役割 | 該当する完了条件 | 根拠 |
|---|---|---|---|
| `snotra-core/src/indexer.rs` | 圧縮対象（4906 行 / コメント 1217 行） | 4, 5 | 指定 |
| `snotra-core/src/search/build.rs` | 圧縮対象（608 行 / コメント 220 行） | 4, 5 | 指定 |
| `snotra-core/src/index_tree.rs` | 圧縮対象（1262 行 / コメント 473 行） | 4, 5 | 指定 |
| **`docs/comment-guidelines.md`** | コメント規約の正本 | **1, 2, 3** | `docs/comment-guidelines.md:3`「本書は `.rs` を編集したとき `.claude/rules/comments.md` 経由で配送される（ポインタだけの router）」——規約の条項はここにしか置けない |
| **`.claude/rules/comments.md`** | `.rs` 編集時に自動配送される router | **6** | 同ファイル冒頭「正本は `docs/comment-guidelines.md`。本 rule は「どこを読むか・何を実行するか」だけを示す」。「既存ルールから参照できる状態」＝この router の「読む正本」箇条へ 1 行 |
| **`PERFORMANCE.md`** | 外す実測値の着地先 | 2, 4 | 決定済み要求判断「現在の設計判断を支えている値だけ `PERFORMANCE.md` / ADR へ出所つきで着地させ」。書式の正本は `PERFORMANCE.md`「この文書へ記録するときの規約」（:30-40） |
| `snotra-core/CLAUDE.md` | **`//!` を触る場合のみ** | 5 | 同 `:10`「各モジュールの責務宣言は各ファイルの `//!`（module doc）を正本とする。本節はファイル一覧と、`//!` に収まらない**横断不変条件・チェックリスト**を記す」（#562） |
| 新規 ADR 1 本（条件つき） | 「値を外す/残す」判定の否定の知識 | 2 | `AGENTS.md`「ドキュメント参照」の ADR 行「否定の知識が生じた決定のみ」。立てるなら `docs/adr/ADR-<slug>.md` かつ**本文 H1 と stem が一致**（`scripts/governance/checks/G-adr-file-names.mjs:27` の `ADR_FILE_NAME`） |

### 1b. 読むが変更しないファイル（判定の根拠になったもの）

- `docs/adr/ADR-measurement-canon-in-code-doc.md` — ハイブリッド判断への先行制約（→ 所見 A-2）
- `docs/adr/ADR-comment-guideline-delivery-by-pointer.md` — 規約の配送形の決定と却下案（→ 所見 A-5・B-2）
- `Cargo.toml:18-21` — `[workspace.lints.rustdoc] broken_intra_doc_links = "deny"`
- `scripts/governance/checks/G-heading-refs.mjs` / `G-folded-heading-refs.mjs` / `G-folded-code-spans.mjs` / `G-stale-identifiers.mjs` / `G-references.mjs` / `G-adr-file-names.mjs`
- `.claude/rules/safety-nets.md`（`paths` に `.claude/rules/**` を含む＝ `.claude/rules/comments.md` を触れば自動配送される）

---

## 2. 導出したシンボル・節の一覧

### 2a. `docs/comment-guidelines.md` の節（完了条件 1・2・3 の作業面）

| 節 | 何をするか | 完了条件 |
|---|---|---|
| `## 第一原則: コメントは「なぜ」を書く`（:9） | **節名を変えない**（`indexer.rs:53` が正準形で指す）。「書かないもの」の箇条は既に長大——ここに「短く保つ」を足すと肥大が加速する | 1 |
| `## 配置基準（3 層）`（:41） | 完了条件 3 の既存の骨格。現行は「散文ドキュメント / doc コメント / インラインコメント」の 3 層で、issue が求める **module rustdoc と item rustdoc の分離**と **ADR・docs の分離**を持たない。ここが 3 の主戦場 | 3 |
| `## 名指しと正本の指名`（:49） | **節名を変えない**（`index_tree.rs:1126` と `.claude/rules/comments.md` が指す）。完了条件 2 の「置き場所」はこの節の表を拡張する形が自然 | 2 |
| `## rustdoc の様式`（:65） | 末尾に「**長さ自体は問題ではない**（→「歴史メモの様式」）」がある。完了条件 1 と正面から当たる（→ 所見 A-4） | 1, 3 |
| `## TSDoc の様式`（:79） | **死んだ節**。同節が「現行リポジトリの TS は `vitest.config.ts` のみ（フロントは #532 SU7 で撤去）」と自認し、`ADR-comment-guideline-delivery-by-pointer.md:14` が整理対象として名指し済み | 1 |
| `## 定型ラベル（既存慣習の様式化）`（:85） | 表の `回帰テスト:` 行の実例が腐っている（→ 所見 A-1） | 4, 5 |
| `## 歴史メモの様式`（:91） | 「設計判断・事故の記録は長くてよい」。完了条件 2 の「証拠・履歴・実測の置き場所」はこの節を分割・再配置する対象 | 1, 2 |
| `## 日本語の折返し`（:57） | **節名を変えない**（`.claude/rules/comments.md` が指す）。圧縮作業そのものがこの節の 2 形（折れたコードスパン・折れた正準形参照）を新たに作りうる | 5 |
| （新設）「短く保つ」基準の節 | 完了条件 1。**hard limit は非目標**なので密度・配置の基準として書く | 1 |

### 2b. `.claude/rules/comments.md`

- `## 読む正本` 箇条（:15-19）— 新設節へのポインタ 1 行を足す（完了条件 6）
- `## トリガー → 検査` 箇条（:23）— `cargo doc` 手動実行の行。圧縮で intra-doc link を触るなら実行対象

### 2c. `snotra-core/src/indexer.rs`

| 位置 | シンボル / 節 | 内容 |
|---|---|---|
| `:1-12` | `//!` | モジュール責務。`snotra-core/CLAUDE.md` の分担の正本 |
| `:36-42` | `INDEX_CACHE_VERSION` の doc | 「**版のリテラルを他所へ焼き込まないこと**」の禁止条項の出どころ。現行版 = 7 |
| `:51-60` | `CachedMasks` の doc | :53 は `docs/comment-guidelines.md`「第一原則: コメントは「なぜ」を書く」を**正準形に見える形で指すが一致を生成しない**（→ A-6）。:60 は `PERFORMANCE.md` の正準形参照（着地する） |
| `:70-80` | 旗の持ち方（1.11 / 1.41 MiB） | 着地先の無い実測値 |
| `:341-363` | 非 ASCII 判定（1.7% / 2.5-3 倍） | 着地先の無い実測値。`PERFORMANCE.md` 正準形参照（:356） |
| `:419-484` | `LoadOrScanStats` と各フィールド doc | **20 行 / 23 行の 2 塊**。#1027 / #1178 / #1054 / #1063 の経緯が濃い。圧縮候補の筆頭 |
| `:503-516`, `:544`, `:562`, `:584` | v4〜v7 の形式差の doc | 実測 MiB を持つ |
| `:699-717`, `:705` | `with_index_write_lock` 周辺 | `snotra-core/CLAUDE.md`「index.bin 書き込みの排他」を正準形で指す |
| `:1060`, `:1102`, `:1718` | `save_cache_sorted_in` / `PrebuiltIndex` / PATH スキャン | `PERFORMANCE.md` 正準形参照 3 本（＝既に着地済みの値） |
| `:1130-1148`, `:1180-1197`, `:1601-1625`（**25 行**）, `:2214-2233`, `:3621-3640` | 長大コメント塊 | 圧縮候補 |
| `:3016`, `:3128`, `:3189` | テストの doc | `snotra-core/CLAUDE.md`「データ永続化の注意」を正準形で指す 3 本 |
| `:3368-3419`, `:3624-3640`, `:3740-3750`, `:3879-3936` | 検知器の doc | 完了条件 5 が最も注意すべき層（「変異を注入して落ちることを実測してある」型の記述） |

### 2d. `snotra-core/src/search/build.rs`

| 位置 | シンボル / 節 | 内容 |
|---|---|---|
| `:1-6` | `//!` | **版リテラル「v4 ヒット時 Wave 1 スキップ / v3 fallback」を持つ**（→ 所見 A-3） |
| `:27-29` | 構築 68 → 58 ms | 着地先の無い実測値 |
| `:77`, `:191`, `:344`, `:460` | `PERFORMANCE.md` 正準形参照 4 本 | 既に着地済み |
| `:179-191`（13 行）, `:223-240`（18 行）, `:257-274`（18 行）, `:341-354`（14 行）, `:423-435`（13 行）, `:454-466`（13 行） | 長大コメント塊 | 圧縮候補 |
| `:427-435` | `new_with_cached_masks` の doc | **`Collapsed`（v6）/ `Raw`（v5/v4）/ `None`（v3）** の版 ↔ variant 写し（→ 所見 A-3） |
| `:466`, `:563` | 塊併合の順序保存 | 「変異を当てても緑のまま通る」型の記述。完了条件 5 の対象 |

### 2e. `snotra-core/src/index_tree.rs`

| 位置 | シンボル / 節 | 内容 |
|---|---|---|
| `:1-35` | `//!` | **35 行・全ファイル最大の塊**。5 つの `#` 見出しを持つ。圧縮候補の筆頭 |
| `:59-83`（25 行） | `NameArena` の doc | `PERFORMANCE.md` 正準形参照（:62） |
| `:147`, `:179` | `PERFORMANCE.md` 正準形参照 2 本 | 既に着地済み |
| `:167-180` | `get_unchecked` を却下した実測（11,905–12,364 µs） | 着地先の無い実測値 |
| `:375-389`, `:404-415`, `:463-486`（24 行）, `:729-745`, `:1116-1129` | 長大コメント塊 | 圧縮候補 |
| `:477` | 並列 4.9 ms / 逐次 24.9 ms | 着地先の無い実測値 |
| `:620-625` | `materialize` | 「`PERFORMANCE.md` が構築段 peak -83.27 MiB として計上」 |
| `:788-804` | **`IndexTree::file_key_into`（:805）** | `docs/comment-guidelines.md:30` が**逐語で引用する模範例**（→ 所見 A-1） |
| `:821-822` | 段数の実測（2 段 25.3 / 4 段 23.8 / 8 段 25.6 ms） | 着地先の無い実測値。判断（4 段）を支えている |
| `:857`, `:884`, `:911` | 30.0 → 23.0 ms / 167 → 117 ms / 249.6 → 167 ms | 着地先の無い実測値 |
| `:1126` | テストの doc | **`docs/comment-guidelines.md`「名指しと正本の指名」を正準形で指す**（→ 所見 A-6） |
| `:1201` | 両腕を数える検知器 | 完了条件 5 の対象 |

---

## 3. 所見

### 要対処

**A-1. `docs/comment-guidelines.md:117` の定型ラベル表の実例が既に腐っている。**
表は `回帰テスト:` ラベルの実例として `snotra-core/src/indexer.rs`（設計意図）を挙げるが、**3 ファイルに `回帰テスト` は 1 件も無い**。

```
$ grep -Fn "回帰テスト" snotra-core/src/indexer.rs snotra-core/src/search/build.rs snotra-core/src/index_tree.rs
exit=1
$ rg -n "回帰テスト" snotra-core/src/indexer.rs snotra-core/src/search/build.rs snotra-core/src/index_tree.rs
exit=1
$ grep -rFn "回帰テスト:" --include=*.rs snotra-core snotra-egui-runtime snotra-settings src-tauri
src-tauri/src/egui_shell/view.rs:1590 / :1615 / src-tauri/src/icon.rs:490
```

帰結は 2 つ。(i) 表の実例を `src-tauri/src/egui_shell/view.rs` 側へ直す必要がある（完了条件 1〜3 でこの節を触る）。(ii) **完了条件 5 のレビュー手段としてラベルは使えない**——「ラベル付き行は残す」という守り方が 3 ファイルでは空振りする。守るべき不変条件は `**…**` 強調と「検知器は〜」の記述として書かれており、機械的な目印を持たない。

**A-2. ハイブリッド判断には `ADR-measurement-canon-in-code-doc` の先行制約がかかる。**
`PERFORMANCE.md:38`（「この文書へ記録するときの規約」）が明文で言う——「**この文書を『今も支えている値』と『歴史』に分けない。** 既に時系列の採否ログとして機能しており、今も設計を支えている値は**コードの doc を正本にする**形で表す（同 ADR）」。
同 ADR（`docs/adr/ADR-measurement-canon-in-code-doc.md`）の「決定」と「帰結」:

- 「**測定値の正本は `PERFORMANCE.md` に限らない。** … **寄せ先が無いときはコードの doc を正本にしてよい。**」
- 「`PERFORMANCE.md` の節へ 1 行足して正本にする」案は **却下済み**——「値の出所が**既存の写しそのもの**になる」ため `AGENTS.md`「照合は SSOT に対して行う。派生コピー同士の一致を完全性の証拠にしない」に触れる
- 「**数値を 8 か所すべてから落とし、害の説明だけ残す**」案も **却下済み**——「『どれくらい長いか』の規模感が失われる。… **この桁が『なぜ〜したか』の説得力そのもの**である。数値ではなく寄せ先が問題なのだから、数値を消すのは筋が違う」

ゆえにハイブリッド判断の実行可能な形は次のとおりで、**「`#NNN` 参照へ委ねて外す」を既定にはできない**:

| 3 ファイルの実測値 | 現在の形 | この ADR 下で採れる手 |
|---|---|---|
| 既に `PERFORMANCE.md` の節を正準形で指しているもの（indexer.rs:60/356/508/1060/1102/1718, build.rs:77/191/344/460, index_tree.rs:62/147/179） | 着地済み・参照のみ | **そのまま**。外す対象ではない |
| 着地先の節が存在しない値（index_tree.rs:822 の段数表・:857・:884・:911・:477・:167-180、indexer.rs:79・:353-363、build.rs:29 ほか） | コードの doc に実体 | **コードの doc を正本として残す**のが ADR の決定。`PERFORMANCE.md` へ新設すると出所が写しになる |
| 判断を支えていない経緯値（「反復 N で〜した」型の変化量） | コードの doc に実体 | `#NNN` 参照へ委ねて外す。ここが本来の圧縮対象 |

**この線引きを計画に持たないと、圧縮作業そのものが `AGENTS.md` の SSOT 条項を破る。**

**A-3. `search/build.rs` が版リテラルを持ち、`indexer.rs` の doc がそれを名指しで禁じている。**
`snotra-core/src/indexer.rs:36-42`:

> `index.bin` の現行フォーマット版。… **版のリテラルを他所へ焼き込まないこと**——反復 8 で v6 へ上げたとき、ハーネスの注記だけが `5` のまま取り残され、「現行は v5。実運用点は v6 のまま」という**それ自体が矛盾した**文を出し続けた
> `pub const INDEX_CACHE_VERSION: u32 = 7;`

`snotra-core/src/indexer.rs:44` も同型:「**版の番号を書かない**（`Engine::from_material` の doc と同じ理由で、番号を書くと版を上げるたびにこの散文だけが腐る）」。

一方 `search/build.rs` は:

- `:4`（`//!`）「IndexCache 復元経路（**v4 ヒット時 Wave 1 スキップ / v3 fallback**）を担う」
- `:427`「`Collapsed`（**v6**）は共有判定もスキップし、`Raw`（**v5/v4**）は `assemble` が測って潰す。`None`（**v3** フォールバック）は Wave 1 を通常通り並列実行する」
- `:433-435`, `:501`, `:523-526`, `:90` も同型

`CachedLower::Collapsed` の生成点は `grep -n "CachedLower::"` で indexer.rs に 10 箇所以上・build.rs に 2 箇所あり、現行版は 7 である。**版 ↔ variant の対応を散文に写した形**であり、禁止条項が名指しする当のものである。完了条件 4 の作業はここで**削るのではなく直す**（variant 名だけを残し版リテラルを落とす）。

**A-4. 完了条件 1 を足す先が、現在「長さ自体は問題ではない」と言っている。**

- `docs/comment-guidelines.md:78`（rustdoc の様式）: 「**長さ自体は問題ではない**（→「歴史メモの様式」）」
- `docs/comment-guidelines.md:92`（歴史メモの様式）: 「設計判断・事故の記録は長くてよい」

非目標が「文字数・行数の hard limit の導入」である以上、新基準は**密度と配置**の基準（「この一文はどの層に属するか」「同じ主張が別の層に無いか」）として書くしかなく、**この 2 節と明示的に整合させないと doc が自己矛盾する**。「長くてよい」の射程を「歴史メモ・却下案の記録」へ絞り、「item rustdoc の契約部は短く保つ」と層で分ける、が最小の整合案。

**A-5. `.claude/rules/comments.md` の変更はセーフティネットの変更である。**
ルート `CLAUDE.md`「最重要ルール」2:「**セーフティネットの変更は合意してから** — 母集団は `AGENTS.md`「条件別チェック（トリガー → 参照先）」のセーフティネット行が正本であり、**規範文書（ルート `CLAUDE.md` / `AGENTS.md`）を含む**」。`.claude/rules/safety-nets.md` の frontmatter は `paths` に `.claude/rules/**` を持つので、編集すれば自動配送される。**完了条件 6 は「合意を要する作業」として計画に立てる**必要がある（`docs/comment-guidelines.md` 側も規範文書ゆえ同じ扱い）。

**A-6. 節名の変更はおおむね `governance:check` を赤にする。ただし「第一原則」節だけは赤にならない——述語を実測した。**
`.rs` は正準形見出し参照の母集団に入る（実測: `governance:check` が「**.rs 102 件**」から照合）。ラベルの述語は `scripts/governance/lib.mjs:168,177`:

```js
export const REF_HEAD = "`([^`\\n]+)`\\s*(?:§\\s*[\\d.]*\\s*)?";
export const HEADING_REF = new RegExp(`${REF_HEAD}「([^「」\\n]+)」`, "g");
```

**ラベルの文字クラスが `[^「」\n]` なので、見出し名に `「」` が入れ子になっていると一致そのものが生成されない。** `docs/comment-guidelines.md:9` の見出しは `## 第一原則: コメントは「なぜ」を書く` で、まさにこの形である。実測（`HEADING_REF` を実ファイルの当該行へ当てた）:

| 参照元 | 書かれた形 | 一致 |
|---|---|---|
| `snotra-core/src/indexer.rs:53` | 「第一原則: コメントは「なぜ」を書く」 | **0 件** |
| `RETROSPECTIVE.md:23` | 「第一原則: コメントは「なぜ」を書く」 | **0 件** |
| `.claude/rules/comments.md:15` | 「第一原則」 | 1 件（着地） |
| `.claude/agents/code-reviewer.md:28` | 「第一原則」 / 「日本語の折返し」 | 2 件（着地） |
| `snotra-core/src/index_tree.rs:1126` | 「名指しと正本の指名」 | 1 件（着地） |
| `snotra-core/src/indexer.rs:60` / `:705` | `PERFORMANCE.md` / `snotra-core/CLAUDE.md` | 各 1 件（着地） |

帰結は 3 つ。

1. **完了条件 3 を節の改名・統廃合で行うと `G-heading-refs` が赤になる**——`.claude/rules/comments.md:15-19` が 5 節（第一原則 / 名指しと正本の指名 / rustdoc の様式 / 日本語の折返し / 言語（日英））を、`code-reviewer.md:28` が 2 節を、`index_tree.rs:1126` が 1 節を正準形で指す。節の**追加**と節**内**の再構成に留めれば緑を保つ。
2. **「第一原則」節に限り、機構は改名を捕まえない。** 全形で書かれた 2 本（`indexer.rs:53` / `RETROSPECTIVE.md:23`）は `checked` にも `findings` にも現れない——`G-folded-heading-refs` のヘッダが言う「『照合していない』と『差分ゼロ』を分ける証跡すら残らない」状態が、折れではなく**見出し名の入れ子鉤括弧**によって生じている。改名するなら**この 2 本を手で直す**しかない。
3. **完了条件 1〜3 の副産物として、見出しから `「なぜ」` を外す価値がある**（例: `## 第一原則: コメントは理由を書く`）。外せば全形の参照も検査対象に入り、上の死角が構造的に消える。照合は正規化後の**前方一致**（`G-heading-refs.mjs` ヘッダ）なので、**新しい名前が頭に「第一原則」を保つ限り、切り詰め形で書かれた参照（`.claude/rules/comments.md:15` / `code-reviewer.md:28`）は手当て不要で緑のまま**である。手で直すのは (2) の全形 2 本だけでよい。

**A-7. 圧縮の折返しが 2 つの機構を赤にする。**
`G-folded-code-spans`（コードスパンが物理改行を跨ぐ）と `G-folded-heading-refs`（正準形参照が折れる）はどちらも `.rs` を母集団に含む。長い doc を「短く整形し直す」作業は**行の再折返しを必ず伴う**ので、この 2 つが最も発火しやすい。基線は今日緑（コードスパン 18107 件 / 折れうる位置 20 件）。**圧縮後に `npm run governance:check` を必ず走らせる**こと（PostToolUse hook の射程外）。

**A-8. `cargo doc` は CI でしか鳴らない。**
`Cargo.toml:18-21` が `[workspace.lints.rustdoc] broken_intra_doc_links = "deny"` / `invalid_html_tags = "deny"`。`.claude/rules/comments.md:23` が明記——「intra-doc link 切れは **CI でのみ発火し PostToolUse hook は沈黙する**」。3 ファイルは `` [`Duration`] `` `` [`Self::get`] `` `` [`NameArena`] `` `` [`walk_to_root`] `` 等の intra-doc link を多数持つ。**圧縮で `[`X`]` を書き換えたら、コミット前に `cargo doc` を手で走らせる**（`docs/build-commands.md` カテゴリ A）。

### 軽微

**B-1. `docs/comment-guidelines.md:30` の「同ファイル」が誤り。**
当該行:「頻度は数を添えられないなら根拠に使わない。添えられる形の模範例は**同ファイル** `IndexTree::file_key_into` の…」。直前の :29 が挙げる模範例は `window_coordinator.rs` の `read_bar_anchor` と `search/tests/build.rs` である。`file_key_into` の所在は:

```
$ grep -rn "file_key_into" --include=*.rs snotra-core/src
snotra-core/src/indexer.rs:1748 / snotra-core/src/index_tree.rs:805（定義） / snotra-core/src/search/tests/path.rs:377
```

`window_coordinator.rs` にも `search/tests/build.rs` にも無いので「同ファイル」は成り立たない。完了条件 1〜3 でこの節を触るなら同時に直す。

**B-2. 引用文言そのものは今日は一致している（が誰も検算しない）。**
`docs/comment-guidelines.md:30` が引用する「実データで根は約 200 件（0.06%）——ほとんど通らない腕ゆえ、壊れても静かである」は `snotra-core/src/index_tree.rs:796-797` と一致する（改行位置のみ差）。**この照合をする機構は無い**——正準形参照でもリンクでもない素の散文引用であり、`G-heading-refs` も `G-stale-identifiers`（`.rs` の doc コメントは母集団外とヘッダが明言）も見ない。`docs/comment-guidelines.md:22` 自身が同型の事故を記録している——「**この模範例は一度腐った**……**そのとき機構は緑だった**」（#984）。**`file_key_into` の doc は圧縮対象から外すか、圧縮するなら同じコミットで `comment-guidelines.md:30` の引用を直す。**

**B-3. `ADR-comment-guideline-delivery-by-pointer` の却下理由 1 は現在成り立たない。**
同 ADR:9 は「統合すると **15734 / 12000** となり `G-area-budget` が赤になる」を第一の却下理由に置くが、`G-area-budget` は撤去済みである——`.claude/rules/governance-docs.md:11`「面積に合否は無い——`governance:check` は実測値を報告するだけで、判定はこの約束が持つ（`ADR-retire-area-budget`）」。`scripts/governance/checks/` にも同名の検査は存在しない（`ls` で確認・23 検査）。**この issue で統合案を再検討する必要は無いが、退けるなら残る 3 理由（先例・階梯・人間向け入口）で退ける**こと。

**B-4. 死んだ `## TSDoc の様式` 節が、完了条件 1 の最も安価な実施先として既に名指しされている。**
`ADR-comment-guideline-delivery-by-pointer.md:14`:「正本側の読みやすさが問題なら、面積を使わない手（破られやすい条項を先頭へ寄せる・**死んだ `## TSDoc の様式` 節を整理する**）が残っている」。同節自身が「現行リポジトリの TS は `vitest.config.ts` のみ（フロントは #532 SU7 で撤去）」と自認する。

**B-5. `//!` を触ると `snotra-core/CLAUDE.md` 側の分担に触れる。**
`snotra-core/CLAUDE.md:10` が「責務宣言は `//!` を正本とする」（#562）。`index_tree.rs` の `//!` は 35 行あり圧縮候補の筆頭だが、**そこから責務の記述を削ると `CLAUDE.md` 側に受け皿が無い**（同節は「ファイル一覧と、`//!` に収まらない**横断不変条件・チェックリスト**」に限られる）。`//!` は「長さ」ではなく「層違いの内容」（`# 記憶域の並べ方は 2 通り` の実装詳細）を item doc へ降ろす方向で縮める。

**B-6. `G-stale-identifiers` はこの作業で赤にならない。**
同検査のヘッダ:「現行語彙の正本は**production のソースの非コメント本文**ただ 1 つである（`stripRustComments` + `*.test.*` の除外）」。コメントを削っても語彙は 1 語も減らないので、他文書のバッククォート識別子が孤立することはない。逆に「**`.rs` の doc コメントは母集団外**ゆえ、そこに書かれた腐りは捕まらない」とも明言している——**圧縮で新しく書いた識別子の綴り誤りは、`cargo doc` の intra-doc link 形（`` [`X`] ``）で書いたときだけ捕まる**。

**B-7. `SPEC.md` は 3 ファイルを参照していない。**
`grep -nE "indexer|index_tree|search/build" SPEC.md` が 0 件。仕様側からの正本指名は無く、`G-spec-sections` の射程にも入らない。

### 未検証

**C-1. 入れ子鉤括弧を持つ見出しが `docs/comment-guidelines.md` の他にもあるか、数え上げていない。**
A-6 で実測した死角（見出し名に `「」` を含むと正準形参照が照合されない）は、`docs/comment-guidelines.md`「第一原則: コメントは「なぜ」を書く」で確認した 1 例である。**同じ形の見出しが他文書にいくつあるかは測っていない**——この issue の射程外だが、A-6 の (3) を採る前に確かめると、同じ手当てをまとめて当てられる可能性がある。

**C-2. 新規 ADR を立てられるか、既存 ADR へ追記できるかは文面から決まらない。**
`AGENTS.md`「ドキュメント参照」は ADR を「否定の知識が生じた決定のみ」とし、`G-stale-identifiers.mjs` のヘッダは `docs/adr/` を「**凍結された歴史**」として全検査の母集団から外す（`ADR-adr-frozen-history`）。**凍結された歴史へ新しい値を追記してよいか**は、どちらの文書も明示していない。ハイブリッド判断が ADR を着地先に挙げている以上、着手前にこの一点を確認する必要がある（A-2 の線引きを採れば、そもそも ADR への着地はほぼ不要になる）。

**C-3. 完了条件 5（不変条件・契約が失われていないことのレビュー）を支える機械的手段が無い。**
A-1 のとおり 3 ファイルに定型ラベルは 0 件。候補として「圧縮前後で `**…**` 強調行の集合を差分する」「`検知器は` を含む行の集合を差分する」が考えられるが、**どちらも測っていない**（`検知器は` の出現数すら数えていない）。`AGENTS.md`「レビュー指摘へ修正（fix-forward）を当てた」の行が要求する「同じ道具で自分で測る」を満たすには、レビュー手段を先に決めて代表入力で実行しておく必要がある。

**C-4. `cargo doc` / `cargo test` を走らせていない。**
読み取り専用の指示に従いビルドを一切回していないため、「圧縮が `broken_intra_doc_links` を赤にしないか」は**本導出では未測定**である。実行したのは `npm run governance:check`（読み取りのみ）1 本。

**C-5. 「冗長度の高いコメント」の判定基準を、代表入力に当てていない。**
§2c〜2e で挙げた「長大コメント塊」は行数（12 行以上の連続）で機械的に取ったものであり、**冗長かどうかは読んで判定していない**。行数は冗長度の代理指標にすぎず、`index_tree.rs:788-804`（`file_key_into`）のように 17 行あって 1 行も削れない塊が実在する（B-2）。実際の圧縮対象の確定には、A-2 の 3 分類（着地済み参照 / 着地先の無い判断根拠 / 経緯値）を塊ごとに当てる作業が要る。
