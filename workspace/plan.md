# plan — issue #732「コメントガイドラインを参照するようにする」

## 目的

`docs/comment-guidelines.md` を「在るが届かない規範」から「`.rs` を触った瞬間に届く規範」へ変える。あわせて、issue が挙げた 2 つの改善（文途中の物理改行・訳語の自然さ）を条項として足し、**条項を実際に当てて**現存する違反を潰す。

## 受け入れ条件

1. `.rs` ファイルを読んだとき `docs/comment-guidelines.md` への経路が `<system-reminder>` として届く。**`snotra-egui-runtime` を含む 4 crate すべて**で届く。
2. 新 rule を足しても既存の crate router（`snotra-core.md` / `snotra-settings.md` / `src-tauri.md`）の配送が消えない。
3. `docs/comment-guidelines.md` に折返し条項と訳語条項が在り、どちらも**適用範囲（新規・触った箇所）**を明示している。
4. 数え上げ条項が、意図的な名指し（`search.rs:331` / `proof.rs:24`）を禁じない形に精緻化されている。
5. 行をまたぐコードスパン 5 件が 1 行に寄り、`grep` で識別子が引ける。
6. `npm run governance:check` と `cargo doc --workspace --no-deps --document-private-items` が green。
7. rules 面の面積が上限 12000 字を超えない。

## 決定事項（ユーザー合意が要るもの・実装判断ではない）

ルート `CLAUDE.md`「最重要ルール」2（エージェント設定の変更は合意してから）に該当するため、作業項目ではなく決定として分ける。

- **D1: `.claude/rules/comments.md` の新設**（Phase 2）。rules はチームの共有物。**→ 2026-08-08 ユーザー承認（「D1OK」）**
- **D5: `docs/comment-guidelines.md` を新 rule へ統合するか**。**→ 2026-08-08 ユーザー裁定で見送り（「統合見送り」）**。却下の根拠は 4 つ: (1) **機構が拒む**——正本は 5264 字で、統合すると rules 面は 15734 / 12000（**超過 3734 字・上限の 131%**）。上限を上げるのは火災報知器の無効化。(2) **先例が真逆**——`governance-docs.md`（当時 1677 字）を `.rs` の書き手へ届けることは「全 Rust 編集へこの rule 1 本ぶんを配送する費用を避けた」として 2026-08-04 に見送られている（`docs/adr/ADR-canonical-heading-references.md:34`）。5264 字はその 3.1 倍を毎回課税する。(3) **階梯を逆走する**——`docs` が `G-area-budget` の母集団外なのは「『その作業に入った者だけが読む面』への退去は #593 が推奨する経路であり、課税すれば登ってほしい階梯を登る側が罰せられる」ため（`scripts/governance-check.mjs:1046-1048`）。(4) **人間向け入口が壊れる**——`CONTRIBUTING.md:13` と `docs/development-principles.md:44`（SSOT 宣言）の指す先が `.claude/` 配下になる。**代替として提示し今回の範囲外とした 2 案**: 正本内で破られやすい条項を先頭へ寄せる／死んでいる `## TSDoc の様式` 節を整理する（フロントは #532 SU7 で撤去済み）
- **D2: 行またぎコードスパンの検知器を `governance-check` へ足すか**。**→ 2026-08-08 ユーザー裁定で見送り**。発火しうることは測れていた（現状 5 件・Phase 3 後は 0 件）が、**5 件は 8030 行に対する事故率であり、規範 1 行と Phase 3 の修正で足りると判断した**。**受容する残余**: 行またぎスパンの再発は誰も検知せず、次に誰かが grep で辿れないコメントを踏むまで分からない。
- **D4: `.claude/agents/code-reviewer.md` へコメント規約の観点を足す**（Phase 2 の 2 番目）。**→ 2026-08-08 ユーザー承認（「D4追加」）**。**独立導出が見つけた第二の配送穴**——`.claude/` 配下にコメント規約への参照は **0 件**（`grep -rn "comment-guidelines\|コメント規約\|コメントガイドライン" .claude/` が exit 1。主エージェントが再照合済み）。ルート `CLAUDE.md` は `/implement`「4b. code-reviewer エージェント」が実装後レビューを自動起動すると書いており、**rules だけ直すと「書く前に届く」経路はできても「書いた後に検算される」経路はできない**。エージェント設定ゆえ合意が要る。**→ 承認待ち**`search.rs:331` と `proof.rs:24` は「名指しが無いと誤読される」という理由を自ら述べている。**→ 2026-08-08 ユーザー裁定で「最小の介入」を承認**——名指しは残し、数だけ落とす。

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 | 対象 |
|---|---|---|
| `docs/comment-guidelines.md` | 変更（Phase 1） | 「第一原則」の**書かないもの**の 2 番目（数え上げ条項）／新節「日本語の折返し」／「言語（日英）」 |
| `.claude/rules/comments.md` | **新規**（Phase 2） | frontmatter `paths`（4 crate 分）＋「読む正本」＋「トリガー → 検査」 |
| `snotra-egui-runtime/src/proof.rs` | 変更（Phase 3） | 43-44 行のスパン、24 行の数え上げ |
| `src-tauri/src/egui_shell/results_window.rs` | 変更（Phase 3） | 216-217 行のスパン |
| `src-tauri/src/egui_shell/strings.rs` | 変更（Phase 3） | 48-49 行のスパン |
| `src-tauri/src/icon.rs` | 変更（Phase 3） | 754-755 行のスパン |
| `snotra-settings/src/style.rs` | 変更（Phase 3） | 102-103 行のスパン |
| `snotra-core/src/search/build.rs` | 変更（Phase 3） | 83 行の数え上げ |
| `snotra-core/src/search.rs` | 変更（Phase 3） | 331 行の数え上げ |
| `snotra-core/src/indexer.rs` | 変更（Phase 3c・**独立導出が追加**） | 811 行のインラインコメント（`derive_columns` 直下） |
| `src-tauri/src/egui_shell/font_stack.rs` | 変更（Phase 3c・**独立導出が追加**） | `//!` の 8-9 行（「被覆」2 件） |
| `.claude/agents/code-reviewer.md` | 変更（Phase 2・**D4 が Yes のとき**） | `## Phase 1: 実装検証` へ観点 1 行 |

計 11〜12 ファイル（新規 1・変更 10〜11）。`scripts/governance-check.mjs` / `scripts/governance-check.test.mjs` / `docs/build-commands.md` は **D2 見送りにより対象外**。

`SPEC.md` の更新は**不要**——挙動・状態遷移・永続形式のいずれも変えない（コメントと規範だけ）。

## 実装順序

Phase 1 → 2 → 3。**順序は入れ替えられない**:

- **1 が 2 より先**: 新 rule は `docs/comment-guidelines.md`「日本語の折返し」を正準形で指す。`headingRefDocs`（`scripts/governance-check.mjs:1363`）は `docs/superpowers/` と `workspace/` と `docs/adr/` を除く**全 `.md` を母集団に取る**ため `.claude/rules/comments.md` も含まれ、節が無いまま rule を作ると `G-heading-refs` が赤になる（**節を先に作り、それを指すポインタを後に作る**）。
- **3 が 1 より後**: 条項が確定しないと「何に寄せるか」が決まらない。

---

## Phase 1 — ガイドラインを更新する

### 1a. 数え上げ条項の精緻化

`docs/comment-guidelines.md:21` の「**数えず列挙せず、正本をリンクで指す**」を、次の 3 段へ置き換える（**腐るのは数であって名指しではない**という区別を入れる）。

- **数は書かない**（「経路は 2 つ」「呼び出し元は 3 箇所」）——分岐を 1 本足すたびに散文だけが嘘になり、**誰も検査しない**
- **名指しは書いてよい。ただし `` [`Type::method`] `` の intra-doc link 形で書く**——`cargo doc` が着地を検査する（素の散文名は検査されない）
- **列挙が「正本の代わり」なら書かない。列挙が「それが無いと誤読される」ための証拠なら残す**——実例は `snotra-core/src/search.rs` の `recent_history`（「全件走査が毎回の窓表示に乗る」という誤読を 2 度招いたため、頻度ではなく呼び出し元を名指しした）

- [ ] 上記へ書き換える。**模範例 `Engine::new_from_cache` の記述は消さない**（「版の番号を書かない」と宣言しつつ正本を名指しする形は、精緻化後の条項の模範例としてそのまま生きる）

### 1b. 新節「日本語の折返し」

- [ ] `## rustdoc の様式` の後（`## TSDoc の様式` の前）へ新節 `## 日本語の折返し` を置く。**見出し名はこの字面のとおりにする**（Phase 2 の rule がこの名前で着地する）。内容:
  - **文途中で物理改行を入れない**（1 段落 1 行）。**適用は新規に書くコメントと、その変更で触った段落だけである**（既存の一括書き直しは本書 5 行目のとおりスコープ外）
  - 根拠 1: **折返しは `grep` を壊す**。`.claude/rules/` が #588 で規範化した「シンボル名で grep して辿る」が、行をまたいだコードスパンでは効かない（2026-08-08 実測 5 件。`grep -rn "current_thread().id() == context.main_thread_id" --include=*.rs .` は 0 件を返す）
  - 根拠 2: **折返しは機械が持たない**。`rustfmt.toml` は不在で、rustfmt の `wrap_comments` は既定 false（nightly 限定）。手で折るので一文直すと段落全体が再折返しになり diff が膨らむ
  - **禁則処理の規則は置かない**——`.rs` のコメント 8030 行で禁則違反候補は **0 件**（2026-08-08 実測）。既に守られているものに規範を足すと、面積だけ増えて読者の注意を奪う
  - **表・箇条書き・コードフェンスは対象外**（構造が改行を要求する）
  - **コードスパンを行またぎさせない**をこの節へ含める（`## rustdoc の様式` の角括弧の項の隣ではなく——原因が折返しであり、機序が違う）

### 1c. 訳語の判定基準（「言語（日英）」節）

- [ ] `## 言語（日英）` へ次を足す。**repo が自足する形で書く**（人間の寄稿者と、global 設定を持たないエージェントの両方に効かせるため）
  - **訳語は「その分野の日本語話者が実際に使う語」を選ぶ。誤りは例外なく「原語の字面をそのまま日本語へ置き換える」方向で起きる**（漢語化も和語直訳も同じ誤り）。判定は 2 つ——**(A) 誤配属**: その訳語が標準語である分野を名指せて、それが今の分野でなければ誤り。**(B) 造語**: その語で検索して同じ文脈の技術文書が出ないなら誤り
  - **迷ったら訳さない側へ倒す**（カタカナか原語のまま書く）。逆向きの誤り（カタカナにしすぎ）は観測されていない
  - **語ではなく語義ごとに判定する**（同じ語が文脈によって正しくも誤りにもなる）
  - **射程（これを書かないと条項が嘘になる）**: 本条項は**新しく訳語を選ぶときの判定**であり、既存の定着語を置き換える指示ではない。**定着の判定は「その語がこのリポジトリの `SPEC.md` か各 `CLAUDE.md` で既に使われているか」**とし、使われているならそれがこのリポジトリの語である。守るのは**同一ファイル内で語を混在させないこと**（`## 言語（日英）` の日英混在条項と同型）。適用は**新規に書くコメントと、その変更で触った箇所**

**射程規則を置く根拠（実測）**: 「窓」は `.rs` コメントに 290 行あるだけでなく、**リポジトリ自身の規範文書に定着している**——`src-tauri/CLAUDE.md` 43 対 `ウィンドウ` 15、`snotra-egui-runtime/CLAUDE.md` 13 対 2、`docs/architecture.md` 8 対 8（2026-08-08 実測）。カタカナを要求する条項を書くと**その瞬間に自分の `CLAUDE.md` が違反になる**（`AGENTS.md`「検証の作法」の「実装より強い主張になった瞬間に嘘になる」）。射程規則は次の 2 件も機械的に解く: **「被覆」は規範文書に 1 件も無い**（`.rs` の font_stack.rs 2 行だけ・`.md` のヒットは `.superpowers/` の履歴資料のみ）ので条項の対象＝Phase 3c で直す。**「畳み込み」は `snotra-core/CLAUDE.md` が規範として使っている**（「畳み込み比較を別実装で書き起こしてはならない」）ので定着語＝対象外。

**写しの評価**: ユーザーの global `CLAUDE.md` に同趣旨があるが、それは repo 外の個人設定であり参照できない。SSOT を repo 内に持たせる判断は 2026-08-08 のユーザー裁定（「repo 固有の条項として新設する」）。**観測された誤りの一覧表は写さない**——判定基準（A・B）と射程だけを置き、置換辞書は書かない（辞書は語ごとの一括置換として誤用されるうえ、repo 側で保守できない）

**`G-stale-identifiers` の制約**: 条項本文に**実在しない camelCase / SCREAMING_SNAKE の識別子をバッククォートで書かない**（`STALE_IDENT` / `STALE_SNAKE_IDENT`・`scripts/governance-check.mjs:1505-1512`。母集団に `docs/comment-guidelines.md` が入る）。snake_case（`wrap_comments`）と `Type::method` 形は述語の外なので安全。

- [ ] Phase 1 の完了時点で `npm run governance:check` を実行する（節を足したので `G-heading-refs` の既存参照元が壊れていないこと・`G-stale-identifiers` が条項本文で赤くならないこと）。**`cargo doc` はこの Phase では不要**（`.rs` を触っていない）

---

## Phase 2 — 配送の穴を塞ぐ

- [ ] `.claude/rules/comments.md` を新規作成する。**内容は下記のとおり**（要約を置かず正本を指すだけ。既存 router 3 枚と同型。**664 字と実測済み**）

```markdown
---
paths:
  - "snotra-core/**/*.rs"
  - "snotra-egui-runtime/**/*.rs"
  - "snotra-settings/**/*.rs"
  - "src-tauri/**/*.rs"
---

# コメントの書き方（ルーター）

正本は `docs/comment-guidelines.md`。本 rule は「どこを読むか・何を実行するか」だけを示す（要約を置かない）。

## 読む正本

- 何を書き、何を書かないか: `docs/comment-guidelines.md`「第一原則」
- `///` / `//!` の様式・見出し構造: `docs/comment-guidelines.md`「rustdoc の様式」
- 改行位置と折返し: `docs/comment-guidelines.md`「日本語の折返し」
- 日英の選択と訳語: `docs/comment-guidelines.md`「言語（日英）」

## トリガー → 検査

- doc コメント（`///` / `//!`）を追加・変更したら `cargo doc --workspace --no-deps --document-private-items` を手で走らせる（intra-doc link 切れは **CI でのみ発火し PostToolUse hook は沈黙する**・`docs/build-commands.md`「変更後の検証チェックリスト」）
```

**glob の形の根拠**: 4 本すべて既存 rule と字面が同型（crate 名で始まり `/**/*.rs` で終わる）。bare `**/*.rs` は harness の配送を測っていないので**使わない**——外すと rule が 1 度も届かない静かな失敗になる。

**参照を「同「…」」と略さない根拠**: `G-heading-refs` の `HEADING_REF` はバッククォート付きの `*.md` 対象が `「` の直前に在ることを要求する。「同」で略すと 3 本が**検査の視界から消える**（略さない形で実測: 5 本すべてが「検査される」側に入る）。`第一原則: コメントは「なぜ」を書く` は入れ子の `「」` ゆえ全文は書けないが、照合が `anchor.startsWith(normAnchor(label))` の**前方一致**（`scripts/governance-check.mjs:1237`）なので `「第一原則」` で着地する。

- [ ] `npm run governance:check` を実行し、`G-rules-globs`（4 本すべてが実在ファイルにマッチ）・`G-heading-refs`（5 本の参照が着地）・`G-area-budget`（rules 面 ≤ 12000 字）が green であることを確認する。**面積は 10470 → 11134 / 12000（余裕 866 字）と実測済み**。超えたら本文を削る（上限を上げない）
- [ ] **配送の実測**——新鮮な context（新セッションかサブエージェント。現セッションは重複排除で観測できない）で、**この順に 2 枚読む**:
  1. `snotra-core/src/` の `.rs` を 1 枚 → **`snotra-core.md` と `comments.md` の両方**が届くこと（これが受け入れ条件 2 の受領証であり、同時に「機構が生きている」対照になる）
  2. `snotra-egui-runtime/src/` の `.rs` を 1 枚 → **`comments.md` が届く**こと（今まで 1 枚も rule が無かった crate）
  - **切り分けの規則**: 1 で `snotra-core.md` が届き `comments.md` が届かないときだけ、glob かファイルの側に原因がある。**両方届かないなら harness が起動時に `.claude/rules/` を読んでおりセッション途中の新ファイルを見ていない**（この場合 rule は正しいので直さない——新セッションで測り直す）

- [ ] `.claude/agents/code-reviewer.md` の `## Phase 1: 実装検証` へ観点を 1 行足す（**D4 承認済み**）。**内容は「コメント規約（`docs/comment-guidelines.md`）に照らす」ではなく、条項のうち機械が見ないものを名指しする**——`docs/comment-guidelines.md`「第一原則」の**書かないもの**（コードが決めている構造の事実）に当たる記述が新規コメントに無いか。**要約を書かず正本を指す**（`.claude/agents/` は `G-area-budget` の母集団外だが、サブエージェント起動ごとに必ず読まれる面である）

---

## Phase 3 — 条項を実際に当てる

### 3a. 行をまたぐコードスパン 5 件

- [ ] `snotra-egui-runtime/src/proof.rs:43-44` — `current_thread().id() == context.main_thread_id`
- [ ] `src-tauri/src/egui_shell/results_window.rs:216-217` — `expected ResultsScale, found MainScale`
- [ ] `src-tauri/src/egui_shell/strings.rs:48-49` — `t("search.placeholder.folder", { dir: fs.currentDir })`
- [ ] `src-tauri/src/icon.rs:754-755` — `cargo test -p snotra --release icon_pipeline_cost_probe -- --ignored --nocapture`
- [ ] `snotra-settings/src/style.rs:102-103` — `ScrollArea::vertical().auto_shrink([false,false]).scroll_source(drag:false)`

**やり方**: そのコードスパンが 1 行に収まるよう改行位置を動かす。**文言は変えない**（変えると「何を直したか」が読めなくなる）。段落全体を 1 行へ寄せるのは、その段落に触るときだけでよい（1b の適用範囲どおり）。

- [ ] 修正後、`node <scratchpad>/measure-split-span.mjs` を再実行して **0 件**を確認する

### 3b. 散文の数え上げ 3 件（D3 の合意が前提）

- [ ] `snotra-core/src/search/build.rs:83` — 「通る経路は 2 つ（`new_from_tree` と、`new_with_cached_masks` の v3 フォールバック腕）」。**数を落とし、名指しを intra-doc link 形へ**（`` [`SearchEngine::new_from_tree`] `` 等）。直後の「1 本に寄せてあるのは〜」の理由文は残す
- [ ] `snotra-core/src/search.rs:331` — 「**呼び出し元は 2 つで、どちらも明示の操作である**」→ 数を落とし「**呼び出し元はどちらも明示の操作である**」。`/r` とトレイの名指しは**残す**（直後の「頻度を書くなら呼び出し元を名指しする」がその理由の正本）
- [ ] `snotra-egui-runtime/src/proof.rs:24` — 「構築点は 2 つだけである: …」→ 数を落とす。「**3 つ目**を足すときは」→「**足すときは**」（数を消したので序数も外す）

### 3c. 独立導出が見つけた 2 件（本レビューで追加・根拠は主エージェントが再照合済み）

- [ ] `snotra-core/src/indexer.rs:811` — `// マスクを計算してキャッシュに含める。起動時に SearchEngine::new_with_cached_masks() がマスク再計算をスキップできるようにする。`。**呼び出し元＋到達可能性の写しであり、しかも反復 11 の事実より狭い**（`derive_columns` は保存経路と cache-miss 経路の両方が通る。正本は `snotra-core/CLAUDE.md` の「記録側が潰したものを、cache-miss の枝がそのまま索引の表現に使う」と `LoadOrScanResult::cached_masks` の doc）。**消費者を数え直すのではなく、正本を指す形へ書き換える**（1a の条項どおり）
- [ ] `src-tauri/src/egui_shell/font_stack.rs:8-9` — 「CJK 非被覆なら」「解決し**被覆するなら**」の**「被覆」2 件を「カバー」へ**。font coverage の語義で、規範文書に 1 件も定着していない（`.rs` 全体でこの 2 行だけ・1c の射程規則の対象）。**issue の「単語をより自然に」に対する唯一の実適用**である

- [ ] `cargo doc --workspace --no-deps --document-private-items` を実行する（**doc コメントを触ったので必須。PostToolUse hook は沈黙する**）

**やらないこと（どちらも受容する残余）**:

- **数え上げの検知器は置かない**。`research.md` E の regex は 10 件中 7 件が偽陽性で、語を変えればすり抜ける——**機械化に向かないと測れた**
- **行またぎスパンの検知器も置かない**（D2 見送り）。こちらは機械化できると測れていたが、規範 1 行と本 Phase の修正で足りると裁定した。**残余は「再発を誰も検知しない」こと**

**計測に使う足場は repo へ入れない**——`measure-split-span.mjs` 等はすべて scratchpad に置いたままにする。repo に足さないので撤去条件を持つ必要が無い（`AGENTS.md`「条件別チェック（トリガー → 参照先）」の一時的な足場の行）。

---

## 不変条件と異常系

- **既存 rule の配送を消さない**（受け入れ条件 2）。実測済みの `STACKING` に依拠する。**検知**: Phase 2 の配送実測の 1 番目で、`snotra-core.md` と `comments.md` が**両方**届くことを見る
- **rules 面の面積上限を上げない**（12000 字）。超えたら新 rule の本文を削る。**検知**: `G-area-budget`
- **新 rule に規範本文の要約を置かない**（写しになる）。既存 router 3 枚と同じ「正本は〜。要約を置かない」の型を守る
- **`docs/comment-guidelines.md` の既存節名を変えない**（既存 4 参照元と `G-heading-refs` が着地している）。**足すだけ**にする
- **新設する節名は `## 日本語の折返し` の字面で固定する**（Phase 2 の rule がこの名前を指す）。変えるなら rule 側も同じ変更で直す
- **異常系**: bare `**/*.rs` を使わない設計なので「rule が 1 度も届かない」失敗は起きない。ただし**新 crate を追加したとき `comments.md` の `paths` に足す必要がある**——`G-rules-globs` は「パターンが 1 件以上にマッチするか」だけを見て逆向き（全 `.rs` が覆われているか）を見ないため、**これは受容する残余である**（検知器を置くなら D2 とは別の判断が要る）

## テスト方針と検証コマンド

| 対象 | コマンド / 手段 |
|---|---|
| ガバナンス文書・rules・面積・見出し着地 | `npm run governance:check`（Phase 1 完了時・Phase 2 完了時・Phase 3 完了時） |
| doc コメントの intra-doc link | `cargo doc --workspace --no-deps --document-private-items`（Phase 3 の後。**hook は沈黙するので手動必須**） |
| コメント変更が製品コードを壊していないこと | `cargo check --workspace`（PostToolUse hook が `.rs` 編集で発火する分に加えて） |
| 行またぎスパン 0 件 | `node <scratchpad>/measure-split-span.mjs`（fence を除いて件数で数える版） |
| 配送（受け入れ条件 1・2） | 新鮮な context で 2 枚読む（Phase 2 の最終項目。切り分けの規則つき） |

## SPEC.md・関連文書の更新要否

- `SPEC.md`: **不要**（挙動・永続形式・状態遷移を変えない）
- `AGENTS.md`「ドキュメント参照」: **不要**（`docs/comment-guidelines.md` への行は既に在る）
- `AGENTS.md`「条件別チェック（トリガー → 参照先）」: **不要**。「各言語ファイルを編集 → `.claude/rules/`」の行が既に新 rule を覆う（rule 名を列挙していないので写しが増えない）
- `docs/build-commands.md`: **不要**（D2 見送りにより検査を足さないため、検査列挙は変わらない）
- `RETROSPECTIVE.md`: 本サイクルの `/retrospective` が扱う（この計画の範囲外）

---

## 未確定（実装前に潰す）

- [x] bare `**/*.rs` glob が harness に配送されるか — **測らない方針で解消**。既存 rule と字面が同型の crate 名始まり glob を 4 本並べる（測っていない形に依存しない）。代償は「新 crate 追加時の取りこぼし」で、不変条件節へ受容する残余として明記した
- [x] rules が重なって配送されるか（新 rule が既存 router を隠さないか） — **実測で解消: `STACKING`**。新鮮な context のサブエージェントが `snotra-core/src/search/scoring.rs` の Read で 2 枚同時配送を観測し、blanket-dump 仮説も独立に排除（`research.md` B）
- [x] 新 rule を足す面積の余地があるか — **実測で解消**。現在 rules 面 10470 / 12000 字。Phase 2 の rule 本文を実際に書いて数えたところ **664 字**で、足すと **11134 / 12000（余裕 866 字）**
- [x] `第一原則: コメントは「なぜ」を書く` を正準形で参照できるか — **実測で解消**。入れ子の `「」` ゆえ全文は書けないが、照合が前方一致（`scripts/governance-check.mjs:1237`）なので `「第一原則」` で着地する。**さらに 4 本すべてを略さず書いたときに `HEADING_REF` が 5 件とも捕まえることを実測した**
- [x] `.claude/rules/*.md` が `G-heading-refs` の母集団に入るか（入るなら節を作る前に rule を作れない） — **実測で解消: 入る**。`headingRefDocs`（`scripts/governance-check.mjs:1363`）は `docs/superpowers/` と `workspace/` と `docs/adr/` を除く全 `.md`。ゆえに **Phase 1（節の新設）を Phase 2（rule の新設）より先**に置いた
- [x] #977 の取り残し（`indexer.rs` の相互参照の取り違え）が現存するか — **解消済みと確認**（`snotra-core/src/indexer.rs:58` は正しく `Engine::new_from_cache` を指す）。本 issue での作業は無い
- [x] **D2（行またぎコードスパンの検知器を置くか）** — **2026-08-08 ユーザー裁定で見送り**。Phase 4 を削除し、受容する残余（再発を誰も検知しない）を決定事項欄と Phase 3 の「やらないこと」へ明記した
- [x] **D3（意図的な 2 件のコメントに手を入れるか）** — **2026-08-08 ユーザー裁定で「最小の介入」を承認**。名指しは残し数だけ落とす（Phase 3b のとおり）
- [x] 訳語条項が「窓」290 行と規範文書 56 件を違反にしてしまわないか — **実測で解消**。`src-tauri/CLAUDE.md` 43 対 15 等、リポジトリ自身の規範文書に定着していると測れたので、**射程規則**（定着の判定は `SPEC.md` / 各 `CLAUDE.md` での使用・守るのは同一ファイル内の混在禁止）を 1c へ入れた。同じ規則が「被覆」（対象）と「畳み込み」（対象外）も機械的に分ける
- [x] **D4（`.claude/agents/code-reviewer.md` へコメント規約の観点を足すか）** — **2026-08-08 ユーザー承認で解消**（「D4追加」）。Phase 2 の作業項目を無条件化した
- [x] **D5（正本を新 rule へ統合するか）** — **2026-08-08 ユーザー裁定で見送り**（「統合見送り」）。却下の根拠 4 件は決定事項欄に記録（統合すると rules 面 15734 / 12000）

---

## セルフレビュー

主エージェント自身の照合（`/start-issue` Step 5a の 5 項目）:

1. **issue の全要件に作業項目が対応する**: 配送の穴 → Phase 2 / ドリフトしにくくする → Phase 1a（条項の精緻化）/ 可読性（物理改行）→ Phase 1b + Phase 3a / 単語をより自然に → Phase 1c / 「ガイドラインに沿って書かれること」→ Phase 3。**issue の「禁則処理にあわせる」枝だけは作業項目を持たない**——実測 0 件で既に満たされており、その旨を Phase 1b で条項に明記する
2. **境界条件と検証**: 表・箇条書き・コードフェンス（折返し条項の対象外）→ 条項に明記し、`measure-split-span.mjs` が fence を除いて数える（D2 見送りにより CI 側の検知は無い＝受容する残余）/ 面積上限 → `G-area-budget` / 見出し参照の入れ子 `「」` と「同」略記 → 実測で解消 / 節と rule の前後関係 → 実装順序節で固定 / 新 crate 追加 → 受容する残余として明記
3. **新しい状態・リソースの正常/失敗/破棄経路**: 新設するのは静的な文書 1 枚のみ。プロセス・ハンドル・フラグを持たない
4. **より単純な既存パターンで置き換えられないか**: 検討した代案は「既存 4 rule へ 1 行ずつ足す」。**却下**——`snotra-egui-runtime` に rule が無いため 5 か所になり、`AGENTS.md`「条件別チェック（トリガー → 参照先）」の写し禁止行に正面から反する。単一 rule が可能なのは `STACKING` を実測したため
5. **壊してはならない不変条件に検知手段がある**: 既存 router の配送 → Phase 2 の配送実測（両方届くことを見る）/ 面積 → `G-area-budget` / 節名の着地 → `G-heading-refs` / intra-doc link → `cargo doc`

該当する check スキル（`AGENTS.md`「条件別チェック（トリガー → 参照先）」から）:

- セーフティネット（rules）を新設 → `.claude/rules/safety-nets.md`（本ブランチの作業で自動配送済み。フォールトインジェクションは複製に当てる方針を Phase 4 へ反映）
- ガバナンス文書を変更 → `npm run governance:check`（検証コマンド表に在り）
- 文書に事実の写しを増やす変更 → 正本 1 か所（新 rule は要約を置かない・訳語条項は事例表を写さない）
- `/persistence-check` `/race-check` `/state-check` `/symmetric-check` `/dry-check` → **非該当**（永続形式・並行性・UI 状態・対称ペア・新規関数のいずれも触らない。D2 見送りにより新規関数は 1 つも足さないので `/dry-check` も該当しない）

- リスク: **高**（rules・ガバナンス文書の変更 = `/plan-review`「リスク判定」の該当条件）
- plan-review: **独立レビュー1体・実施済み**（`/plan-review`「Step 2b」＝独立導出による網羅性レビュー。2026-08-08 ユーザー指示「念のため plan-review いってみよう」）。**枠組みに Step 2b を選んだ理由**: 本計画の残余リスクは「配送が届かないコードパスの取りこぼし」＝列挙の穴であり、計画の分解を前提に読む Step 2 は同じ盲点を継承する。結果は「plan-review 結果」節（**要対処 5 件を反映し、うち 1 件は配送穴がもう 1 つ在るという発見だった**——枠組みを選んだ狙いが当たった）
- エージェント数: 2（配送の実測 1 体 ＋ plan-review Step 2b 1 体。どちらもユーザー合意のうえ起動）
- 要対処: advisor の指摘を 2 巡で計 6 件反映——1 巡目 (1) 配送 glob の意味論を実装前に測る、(2)「12 件」を件数へ数え直す（→ 5 件）、(3) 折返し条項に適用範囲を書く。2 巡目 (4) Phase 順序の逆転（節が無いまま rule を作ると `G-heading-refs` が赤）、(5)「同「…」」略記で 3 本が検査の視界から消える、(6) 配送実測が 2 つの失敗モードを切り分けられない（→ 対照読みと切り分け規則を追加）。加えて **plan-review の要対処 5 件**（同節に記載）
- 未検証: 配送の実測（Phase 2 の最終項目）は**新 rule を作った後でしか測れない**ため実装フェーズに残る。`--deep` は不要と判断（ガバナンス文書の移動・圧縮・分割をしないため）

---

## plan-review 結果

- リスク: **高**
- レビュー方式: **独立導出1体**（`/plan-review`「Step 2b」。`plan.md` / `research.md` を読ませず、issue の WHAT だけを渡してコードから導出させた。成果物: `workspace/plan-review-comment-guidelines-delivery.md`）
- エージェント数: 1（本レビュー）／通算 2（配送の実測を含む）

### 要対処（すべて主エージェントが根拠を再照合した）

- **配送穴は 1 つでなく 2 つある** — `.claude/agents/code-reviewer.md` にもコメント規約への参照が無い（`.claude/` 全体で 0 件・`grep` が exit 1 で再照合）。**→ D4 として決定事項と未確定へ追加**。rules だけ直すと「書く前に届く」経路はできても「書いた後に検算される」経路はできない
- **`snotra-core/src/indexer.rs:811` が現存違反** — 呼び出し元＋到達可能性の写しであり、反復 11 の事実より狭い（逐語で再照合）。**→ Phase 3c へ追加**
- **`src-tauri/src/egui_shell/font_stack.rs:8-9` の「被覆」2 件** — `.rs` 全体でこの 2 行だけ（`grep` で再照合）。**→ Phase 3c へ追加**。issue の「単語をより自然に」に対する唯一の実適用
- **訳語条項に射程が無いと嘘になる** — 「窓」は `.rs` 290 行だけでなく**リポジトリ自身の規範文書に定着**（`src-tauri/CLAUDE.md` 43 対 15 等を自分で実測）。**→ 1c へ射程規則を追加**。同じ規則が「被覆」と「畳み込み」の可否も分ける
- **`G-stale-identifiers` の制約** — 条項本文に実在しない camelCase / SCREAMING_SNAKE を書くと赤（母集団に `docs/comment-guidelines.md` が入る）。**→ 1c の末尾へ制約として記録**

### 軽微

- 独立導出は `docs/comment-guidelines.md:7` の見出しの**改題**も選択肢に挙げたが、本計画は前方一致による `「第一原則」` 参照で解いており（実測済み）、改題は不要。`RETROSPECTIVE.md:33` の未照合参照は本 issue の射程外
- `CONTRIBUTING.md:13` と `AGENTS.md:16` の括弧内要約（`rustdoc / TSDoc の様式・粒度`）が条項追加後に実装より狭くなる、という指摘。**括弧内は例示であり全称を主張していない**ため降格した
- `.claude/rules/snotra-core-search.md` への二重課税の懸念は、本計画が単一 rule で足すため成立しない

### 未検証

- **`N つ` 系 35 件のうち何件が条項の対象か**は、独立導出も「4 件程度」と幅を持たせた。本計画は Phase 3b で 3 件を名指しし、残りは「一意性の不変条件」「事故の予防」として**触らない**方針を 1a の条項本文で線引きする。全 35 件の分類は行っていない（**分類しないまま条項を書く危険は 1a の 3 段目が引き受ける**）
- **長い設計判断コメントを 1 行へ寄せたときの diff の読みやすさ**は測っていない。1b の適用範囲（新規・触った段落のみ）が影響を限定する
- 独立導出が挙げた「窓 290 行の語義内訳（未分類 167）」は、射程規則により**分類しなくても条項が成立する**形にしたため測っていない

### 判断

- 実装着手: **可**（D1・D4 承認、D2・D5 見送り、D3 は「最小の介入」で承認。未確定欄は空）

---

## 人間レビュー

- [x] 承認済み — 2026-08-08 / 問い: "**統合を見送る**ことにご同意いただけましたら、`workspace/` をコミットして `/implement` へ渡せる状態です。" / 回答: "統合見送りで /implement"

決定の内訳（すべてユーザー発言の逐語）:

| 決定 | 問い（要旨） | 回答（逐語） |
|---|---|---|
| D2 見送り・D3 最小の介入・plan-review 実行 | 「D2: 検知器を足しますか／D3: 手を入れてよろしいですか／plan-review を起動してよろしいですか」 | "D2見送り、D3最小の介入で、念のため plan-review いってみよう" |
| D1 承認・D4 承認 | 「D4: `code-reviewer.md` へ観点を 1 行足しますか／あわせて D1 と計画全体のご承認を」 | "D4追加、D1OK。既存のコメントガイドラインはcomments.mdに統合する？" |
| D5 見送り・実装着手 | 「統合を見送ることにご同意いただけましたら、`workspace/` をコミットして `/implement` へ渡せる状態です」 | "統合見送りで /implement" |
