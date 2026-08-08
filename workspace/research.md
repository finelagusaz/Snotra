# research — issue #732「コメントガイドラインを参照するようにする」

## issue の要約

**ペイン**: `docs/comment-guidelines.md` を策定したが参照されているように見えない。

**あるべき姿**: (1) ガイドラインに沿ってコメントが書かれること、(2) ガイドライン自体がより良く更新されること。

**たとえば**（issue 本文）: コード・git・issue・SPEC から読み取れないものを書く / ドリフトしにくくする / 可読性（改行位置を禁則処理にあわせるか、文途中の物理改行をなくす）/ 使われている単語をより自然に。

**issue コメント（2026-08-08・#977 の一次証拠）が残した穴**（この issue の中心）:

> 条項を足しても、`docs/comment-guidelines.md` は自動配送されない。`.claude/rules/` は対象ファイルを触ると配送されるが、現在どの rule も `docs/comment-guidelines.md` を指していない。（…）**`.rs` を編集した瞬間に届く経路は無い**。（…）**条項の存在は、それが読まれることを意味しない。**

## 一次証拠（すべて本サイクルで実測）

### A. 配送の穴は実在する

`docs/comment-guidelines.md` への参照は 5 か所（`grep -rn "comment-guidelines"`・node_modules と target を除く）:

| 参照元 | 種別 |
|---|---|
| `AGENTS.md:16`「ドキュメント参照」 | 散文（人が辿る） |
| `CONTRIBUTING.md:13` | 散文 |
| `docs/development-principles.md:44` | 散文 |
| `RETROSPECTIVE.md:33` | サイクル記録 |
| `.superpowers/sdd/plan/audit-universals.md:181` / `docs/superpowers/plans/…` | 過去の作業記録 |

**`.claude/rules/` からの参照は 0 件**。`.rs` に一致する rule は 3 枚（`snotra-core.md` / `snotra-settings.md` / `src-tauri.md`）あるが、どれも「読む正本」の一覧に `docs/comment-guidelines.md` を含まない。

**`snotra-egui-runtime/**/*.rs`（12 ファイル）に一致する rule は 1 枚も無い**（`.claude/rules/*.md` の frontmatter を全 7 枚読んで確認）。ゆえに crate 別 router へ 1 行ずつ足す方式では、この crate だけ永久に届かない。

### B. rules の配送は **重なる**（`STACKING`・サブエージェントで実測）

新 rule が既存の crate router を隠さないかは、`G-rules-globs` からは分からない——当の検査が自ら「harness の配送判定の再現ではなく『マッチ 0 件の検知』に限定した近似」と宣言している（`scripts/governance-check.mjs:728-731`）。当セッションでは rule 本文を既に読み込んでおり重複排除で観測できないため、**新鮮な context のサブエージェント 1 体**で測った（成果物: scratchpad の `rules-delivery-measurement.md`）。

- `snotra-core/src/search/scoring.rs` の Read 直後、**glob が一致する 2 枚が同時に配送された**（`snotra-core.md` = `snotra-core/**/*.rs` と `snotra-core-search.md` = `snotra-core/src/search/**/*.rs`）。加えて `snotra-core/CLAUDE.md`。
- **blanket-dump 仮説は独立に排除済み**: rule は 7 枚あるが届いたのは 2 枚で、残る 5 枚は届かなかった。選択性は別の対象ファイル（`.claude/rules/*.md` の Read → `safety-nets.md` 1 枚だけ）でも再現した。
- 重複排除は**ファイル単位**で効く（既配送の rule は再送されない）。

**この 1 標本が保証するのは「2 一致 → 2 配送」である。**3 枚以上の同時一致、および bare `**/*.rs`（先頭にディレクトリを持たない glob）の挙動は未測定——ゆえに新 rule の `paths` は**既存 rule と字面が同型の、crate 名で始まる glob**だけを使う。

### C. 「文途中の物理改行」— issue の either/or は片方が既に満たされている

`.rs` 全 96 ファイル・コメント行 8030 行を測った（scratchpad の `measure-comment-wrap.mjs` / `measure-comment-wrap2.mjs`）。

| 観測 | 値 |
|---|---|
| 日本語コメント行 / 英語のみ | 7289 / 741 |
| 日本語行の表示幅 | p50=86 / p90=96 / p99=105 / max=151 |
| 文途中の物理改行 | 日本語 2719 行（37.3%）・英語 170 行（22.9%） |
| **禁則違反候補**（行末が開き括弧・行頭が閉じ括弧や句読点） | **0 件** |

**禁則処理は de facto で既に守られている**ため、issue の「改行位置を日本語の禁則処理にあわせるか」の枝は**手を入れる余地が無い**。生きているのは「文途中の物理改行をなくす」枝だけである。

`rustfmt.toml` は**不在**（`ls -a` で確認）。rustfmt の `wrap_comments` は既定 false かつ nightly 限定なので、**この折返しは機械が持たない完全な手作業**である。

### D. 折返しが実際に壊しているもの — 行をまたぐコードスパン **5 件**

奇数バッククォート行は 12 行あるが、その実体は **5 件**である（各件が開き行と閉じ行の 2 行を出し、残る 2 行は ```` ``` ```` の fence 区切り。fence を除いて数え直した — scratchpad の `measure-split-span.mjs`）:

| 位置 | 分断された識別子 |
|---|---|
| `snotra-egui-runtime/src/proof.rs:43-44` | `current_thread().id() == context.main_thread_id` |
| `src-tauri/src/egui_shell/results_window.rs:216-217` | `expected ResultsScale, found MainScale` |
| `src-tauri/src/egui_shell/strings.rs:48-49` | `t("search.placeholder.folder", { dir: fs.currentDir })` |
| `src-tauri/src/icon.rs:754-755` | `cargo test -p snotra --release icon_pipeline_cost_probe -- --ignored --nocapture` |
| `snotra-settings/src/style.rs:102-103` | `ScrollArea::vertical().auto_shrink([false,false]).scroll_source(drag:false)` |

**害は「rustdoc のレンダリングが壊れる」ではない**——CommonMark の inline code span は soft line break を跨げるので描画は正しい。**実際の害は grep 不能である**（実証: `grep -rn "current_thread().id() == context.main_thread_id" --include=*.rs .` → **0 件**）。これは `.claude/rules/snotra-core.md` 他が #588 で規範化した「位置はファイル名で断定せず**見出し名・シンボル名で grep** して辿る」を、折返しが正面から無効化していることを意味する。

### E. #977 で足した条項は、隣のファイルで既に破られている

`docs/comment-guidelines.md:21` が禁じるのは「コードが決めている構造の事実——経路の**数**、分岐の**列挙**、**呼び出し元**、**到達可能性**」で、「**数えず列挙せず、正本をリンクで指す**」と書いてある。語彙依存の regex で `.rs` を走査すると 10 件ヒットし、分類すると**本物は 3 件**（残りは「経路が 1 つでもあれば」型の条件節や「一箇所に保つ」型の規範で、数え上げではない。**regex は語彙依存ゆえこの 3 件が全部だとは言えない**）:

1. **`snotra-core/src/search/build.rs:83`** — 「通る経路は **2 つ**（`new_from_tree` と、`new_with_cached_masks` の v3 フォールバック腕）」。条項が禁じる型そのもの。`git log -S` で **`4c96fef`（2026-08-08 12:46・反復 10）** に書かれたと確定し、条項を足した `4e424b7`（#977・同日 17:06）の**祖先**である——つまり**条項を書いていたその日の 4 時間前に、1 ファイル隣で同じ型が生まれていた**。条項は既存を書き直さない設計なので、これは仕組みどおりに取り残された。
2. **`snotra-core/src/search.rs:331`** — 「**呼び出し元は 2 つで、どちらも明示の操作である**——`/r` スラッシュコマンドとトレイの履歴メニュー」。
3. **`snotra-egui-runtime/src/proof.rs:24`** — 「構築点は **2 つだけ**である: `RuntimeFrame`（フレームの中）と `on_event_loop`（marshalling したタスクの中）。**3 つ目を足すときは、その経路が本当にイベントループ上かを一次証拠で示すこと。**」

**2 と 3 は条項と意図が衝突している。**`search.rs:331` は直後に自らこう書いている——「この一行が『全件走査が毎回の窓表示に乗る』という誤読を **2 度**招いたので、頻度を推測させない形にしてある——**頻度を書くなら呼び出し元を名指しする。**」。`proof.rs:24` の列挙も「3 つ目を足すな」という将来の変更者向け規範を運んでいる。**どちらも「正本の代わりの写し」ではなく「その名指しが無いと誤読される」ための証拠である。**

ゆえに条項の側に精緻化が要る: **腐るのは数であって名指しではない**。`` [`Type::method`] `` の intra-doc link 形なら `cargo doc` が着地を検査するが、数（「2 つ」）は誰も検査せず分岐を 1 本足すたびに嘘になる。条項の模範例 `Engine::new_from_cache`（`snotra-core/src/engine.rs:134`）自身が「**版の番号を書かない**」と宣言しつつ正本を**名指ししている**——現行の「数えず列挙せず」という文面は、この模範例とも整合していない。

### F. #977 の取り残しは解消済み（確認のみ）

issue コメントが挙げた「`indexer.rs` に入れた相互参照の取り違え」は**既に直っている**（`snotra-core/src/indexer.rs:58` は正しく `Engine::new_from_cache` を指す。`grep -rn "版の番号を書かない"` で 2 件とも確認）。本 issue での作業は不要。

### G. 訳語の判定基準は repo 内に存在しない

`grep -rn "訳語\|カタカナ\|用語の選"` で repo 内のヒットは `SPEC.md:109`（機能説明の「カタカナ名対応」）のみ。訳語の選び方（誤配属・造語の 2 判定）は**ユーザーの global `CLAUDE.md`（`C:\Users\Eoh\.claude\CLAUDE.md`）にしかなく、リポジトリは自足していない**。人間の寄稿者にも、global 設定を持たないエージェントにも届かない。

## 関連ファイル・シンボル（すべて grep で実在を確認）

| パス | 役割 |
|---|---|
| `docs/comment-guidelines.md` | 本 issue の対象規範（96 行・9 節） |
| `.claude/rules/` 全 7 枚 | 配送機構。`paths` frontmatter が SSOT |
| `.claude/rules/safety-nets.md` | セーフティネット改修時の運用手順（`.claude/rules/**` に一致＝本作業で自動配送される） |
| `scripts/governance-check.mjs` | 検査 20 種（`G-references` … `G-rules-globs` 等）。1860 行 |
| `scripts/governance-check.test.mjs` | 上の単体テスト |
| `AGENTS.md`「条件別チェック（トリガー → 参照先）」 | トリガー表。散文側の写し禁止行を持つ |
| `snotra-core/src/search/build.rs:83`, `snotra-core/src/search.rs:331`, `snotra-egui-runtime/src/proof.rs:24` | E の 3 件 |
| D の表の 5 ファイル | 行またぎコードスパン |

## 再利用できる既存パターン

- **ルーター型 rule**: `snotra-core.md` / `snotra-settings.md` / `src-tauri.md` はいずれも「事実の正本は〜。本 rule は『どこを読むか・何を実行するか』だけを示す（**要約を置かない**）」という同一の型を持つ。新 rule もこれに倣えば、規範本文の写しを作らずに済む。
- **`G-heading-refs` の正準形** `` `<対象>`「<見出し>」 ``（`.claude/rules/governance-docs.md`）: 新 rule から `docs/comment-guidelines.md` の節を指すときこの形で書けば、見出しの着地が CI で照合される。
- **`docs/check-skill-skeleton-design.md`**: check 系スキルに必須なのは母集団と費用対称性の 2 つだけ（検知器を足す場合の設計基準）。

## 技術的制約

- **`.claude/rules/` と `docs/comment-guidelines.md` の変更は、どちらもセーフティネットの改修である**——ルート `CLAUDE.md`「最重要ルール」2 により、Claude が単独で決めてはならない。`.claude/rules/safety-nets.md` の運用手順（フォールトインジェクションは**複製に当て、稼働中のガードを弱めない**）が適用される。
- **規範そのものへフォールトインジェクションを当てる専用手順は置かない**（`/norm-review` は判別力ゼロと測られ 2026-08-07 に廃止・`docs/adr/ADR-retire-norm-review.md`）。規範の妥当性は「条項を実際に使う作業」と独立レビューが事後に暴くものとして扱う。
- **`cargo doc` は PostToolUse hook で発火しない**（CI の rust-check のみ）。doc コメントを触るので手動実行が必須。
- **既存コメントの一括書き直しは `docs/comment-guidelines.md` 自身がスコープ外と宣言している**（同ファイル 5 行目）。ゆえに新条項の適用対象は「新規・触った箇所」に限る旨を条項に書かないと、実装しない範囲を約束する文面になる。
- **数え上げの検知は機械化に向かない**: E の regex は 10 件中 7 件が偽陽性で、語を変えれば容易にすり抜ける。行またぎコードスパン（D）は逆に決定的に測れる（バッククォートの偶奇 + fence 状態）。

## 未解決の疑問（→ plan.md の未確定欄で潰す）

- bare `**/*.rs` glob が harness に配送されるかは未測定（回避策を採るので実装には影響しない。新 crate 追加時の取りこぼしという残余だけが残る）。
- 行またぎコードスパンの検知器を `governance-check` へ足すかは**ユーザー裁定**（新規セーフティネット）。
