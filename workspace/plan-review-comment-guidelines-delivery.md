# 独立導出 — issue #732

`workspace/plan.md` / `workspace/research.md` は読んでいない（`workspace/` 配下は一切開いていない）。以下はコードと規範だけから独立に導出したもの。

## 対象 issue

**#732「chore: コメントガイドラインを参照するようにする」**（OPEN・label なし・コメント 1 件）

本文の要求（逐語の要点）: ペイン =「コメントガイドラインを策定したが参照されているように見えない」。あるべき姿 =「コメントガイドラインにそってコメントが書かれること」「コメントガイドラインがより良いものとして更新されること」。たとえば =「コードやgitのコメント、issueやSPECから読み取れないものを書く」「ドリフトしにくくする」「コメントの可読性をあげる（改行位置を日本語の禁則処理あわせるか、文途中の物理改行をなくす / 使われている単語をより自然に）」。

コメント（#977 サイクルの一次証拠）が明示している残り穴: **「条項を足しても `docs/comment-guidelines.md` は自動配送されない」**。私の実測もこれを裏づけた（下記）。

---

## 導出した変更ファイルとシンボル

| ファイル | 触る理由 | 対象シンボル・行・節名 |
|---|---|---|
| `docs/comment-guidelines.md` | 条項の正本。折返し（文中の物理改行を入れない）と語の自然さ（repo 固有条項）を足す先はここ 1 か所（`docs/development-principles.md:44` が「コメント自体の書式・粒度・定型ラベルは `docs/comment-guidelines.md` を SSOT とする」と明示） | **既存節へ足すのが安い**: `## 言語（日英）`（L78–83）。同節 L82 が既に **`（新規分から適用。既存は書き直さない）` という適用範囲の先例**を持つ。折返しは可読性の話なので `## rustdoc の様式`（L38）か新節。**新節を作るなら見出し名が下流参照の契約になる**（下記「機械的な検査からの制約」） |
| `docs/comment-guidelines.md` | 見出し `## 第一原則: コメントは「なぜ」を書く`（L7）は**入れ子の `「」` のせいで正準形参照が機械照合されない**（実測。下記）。ここへ新条項の参照を集めるなら改題が前提 | L7 の見出し文字列。改題した場合の参照元は `RETROSPECTIVE.md:33` の 1 件だけ（`git grep -n "第一原則" -- . \| grep -v '^workspace/'` = 3 hit、うち参照 1・定義 1・散文 1） |
| `docs/comment-guidelines.md` | L5 のスコープ宣言（`既存コメントの一括書き直しは本書のスコープ外`）が、新条項でも**射程を持つことを明示**しないと 2733 件 + 290 件の潜在違反を即座に作る（実測値は下記） | L5 の一文、および新条項の「適用範囲」1 文 |
| `.claude/rules/snotra-core.md` | `.rs` 編集時に自動配送される唯一の機構。`paths: snotra-core/**/*.rs`。現在どの rule も comment-guidelines を指していない（`git grep -c "comment-guidelines" -- .claude/` = **0 件**、exit 1） | `## 読む正本（snotra-core/CLAUDE.md の該当節）` または `## トリガー → 検査` に 1 行。現状 882 字 |
| `.claude/rules/src-tauri.md` | 同上。`paths: src-tauri/**/*.rs` | 同構造の節。現状 1729 字 |
| `.claude/rules/snotra-settings.md` | 同上。`paths: snotra-settings/**/*.rs` | 同構造の節。現状 819 字 |
| **`.claude/rules/snotra-egui-runtime.md`（新規）** または **全 `.rs` を覆う 1 枚** | **`snotra-egui-runtime/` の 12 件の `.rs` はどの rule の `paths` にも入らない**（実測: .rs 96 件中 84 件がカバー・未カバー 12 件はすべてこの crate）。既存 3 枚に足すだけでは 12.5% が届かないまま残る | 新規なら frontmatter `paths: ["snotra-egui-runtime/**/*.rs"]` +「読む正本 = `snotra-egui-runtime/CLAUDE.md`」。**代替**: `paths: ["**/*.rs"]` の薄い 1 枚に comment-guidelines のポインタだけを載せれば 96/96 を 1 枚で覆える（面積も最小）。どちらを採るかは判断が要る → ⚠️ 節 |
| `.claude/agents/code-reviewer.md` | **第二の配送穴**。ルート `CLAUDE.md` が「`/implement`「4b. code-reviewer エージェント」が自動で起動する」と書く実装後レビューが、コメント規約を 1 度も参照していない（`git grep -rn "comment-guidelines\|コメント規約\|コメントガイドライン" .claude/` = **0 件**）。issue の (a)「ガイドラインに沿って書かれること」に最も直接効く | `## Phase 1: 実装検証`（L20）または `## Phase 2: 計画判断の検証`（L33）配下。既存見出しは `2a`〜`2f` + `## Phase 3: パフォーマンス検証`（L115）+ `## 出力フォーマット`（L127） |
| `AGENTS.md` | `## 条件別チェック（トリガー → 参照先）` 表が「変更の種類 → 参照先」の索引。コメントを書く/直す局面の行が無い。ただし**常時ロード面の余裕が最小**（下記） | `## 条件別チェック（トリガー → 参照先）` の表に 1 行。`## ドキュメント参照` L16 は既にポインタを持つ（`docs/comment-guidelines.md`） |
| `snotra-core/src/search/build.rs` | **現存違反（最も強い）**。L83 `通る経路は 2 つ（new_from_tree と、new_with_cached_masks の v3 フォールバック腕）。` は #977 で腐った「通る経路は 3 つ」と**ほぼ逐語で同形**。事実は現在真（`grep -rn wave1_from_tree --include=*.rs .` → 呼び出しは build.rs:387 と build.rs:484 の 2 件）ゆえ**誤りは形であって内容ではない** | `wave1_from_tree` の doc（L80–85）。L84–85 の「1 本に寄せてある理由」は不変条件ゆえ残す |
| `snotra-core/src/search.rs` | **違反ではなく条項の境界事例**（当初「現存違反」と書いたが、doc ブロック全文を読んで撤回した）。L331 は「呼び出し元」を名指しするが、L337 が `**頻度を書くなら呼び出し元を名指しする。**` と**意図と根拠**（`誤読を 2 度招いた`）を明示している。条項の「過去の事故」（L15）に該当する形なので、**条項に境界が要ることの証拠**として扱う | `SearchEngine::recent_history` の doc（L331–342）。L334 / L336 は「窓」の語を含むので (b) 側の対象 |
| `snotra-core/src/indexer.rs` | 現存違反 2 種の複合。L811 `// マスクを計算してキャッシュに含める。起動時に SearchEngine::new_with_cached_masks() がマスク再計算をスキップできるようにする。` は (i) **呼び出し元・到達可能性の写し**、(ii) **反復 11 の事実と食い違う**（cache-miss も同じ入口を通るので「起動時に」は狭い。正本は `engine.rs:127–131` の `new_from_cache` doc と `indexer.rs:51–59` の `CachedMasks` doc） | `derive_columns`（L810）直下のインラインコメント |
| `src-tauri/src/egui_shell/font_stack.rs` | 語の自然さの現存違反。L8 `CJK 非被覆なら`・L9 `解決し**被覆するなら**` — font coverage の意味で「被覆」を使っている。**リポジトリ内で「被覆」を使う `.rs` コメントはこの 2 行だけ**（96 件の `.rs` のコメント行 8861 行を走査して 2 件） | `//!` 冒頭（L8–9） |

### 触らないと判断したもの（根拠つき）

- `CONTRIBUTING.md:13` — 既に `[docs/comment-guidelines.md](docs/comment-guidelines.md)` を指すポインタが在る（人間向け入口）。新条項は正本側に足るので変更不要。ただし括弧内が `rustdoc / TSDoc の様式・粒度` に閉じているため、折返し・語の自然さを足したあと**この要約が実装より狭くなる**（→ ⚠️）
- `.claude/rules/snotra-core-search.md` — `paths` が `snotra-core/src/search.rs` + `search/**/*.rs`（.rs 15 件）で `snotra-core.md`（33 件）の**真部分集合**。同じポインタを両方に置くと同じ面で二重課税になる
- `.claude/rules/spec.md` / `governance-docs.md` / `safety-nets.md` — `paths` に `.rs` を 1 件も含まない
- `SPEC.md` — 挙動を変えないのでこの issue では同期不要（`AGENTS.md`「バグか仕様変更かを判定する」の判定）

---

## 実装順序の依存関係

1. **見出しの改題は、それを指す参照と同じコミットで入れる（改題するなら）。** `docs/comment-guidelines.md:7` の `第一原則: コメントは「なぜ」を書く` は入れ子 `「」` のため **G-heading-refs も G-near-heading-refs も照合 0 件**（実測。次節）。既存参照は `RETROSPECTIVE.md:33` の 1 件のみ。**先に参照だけを増やすと、増やした分がすべて未検査のまま積む。**
2. **新しい rule ファイルは `paths` glob が実在ファイルに 1 件以上マッチする状態で入れる。** G-rules-globs は 0 件マッチを finding にする（`scripts/governance-check.mjs:783`）。`snotra-egui-runtime/**/*.rs` は 12 件マッチするので条件を満たす（実測）。
3. **G-area-budget の検算は rule 編集をすべて終えてから 1 回**（`npm run governance:check`）。ファイルごとに測ると「rules 合計」の判定を通らない。`AGENTS.md` にも行を足すなら**常時ロード面の方が余裕が小さい**（1365 字 vs rules 1530 字）ので、そちらを後回しにしない。
4. **`.rs` のコメント修正は、`.rs` へ正準形参照を書くかどうかで検査経路が変わる。** `.rs` 96 件は G-heading-refs の**ソースの腕**（`headingRefSourceDocs`・`governance-check.mjs:1398`）に載る。一方 PostToolUse hook の沈黙は fmt / clippy / test の合格であって**見出し参照の着地を含まない**（`.claude/rules/governance-docs.md` の最終項が明示）。ゆえに `.rs` へ参照を書くなら**手元で `npm run governance:check` を回すまで検査されない**。
5. **`docs/comment-guidelines.md` の編集は PostToolUse hook が何も走らせない**（`docs/hooks.md:57`「`*.md` … は**何も走らない**——沈黙は「合格」ではない」）。沈黙を合格と読まないこと。

先にやらないと壊れるもの以外（`.rs` の 3 ファイルのコメント修正・code-reviewer への観点追加）に順序依存は無い。

---

## 機械的な検査からの制約（検査 ID・母集団・実測値）

ベースライン（`npm run governance:check` 実行・2026-08-08）:

```
governance:check — 全検査 passed（検査 19 件 / 対象文書 34 件 / rules 7 件 / skills 12 件 /
恒久規範 常時ロード 14135/15500 字・rules 10470/12000 字 / 見出し参照 168 件を md 46 件 + .rs 96 件から照合 /
workspace member 4 件の lints opt-in / clippy 禁止 7 件 / 散文の識別子 70 件を 32 文書から照合 /
近傍の見出し参照 12 件 / ADR 37 本の名前 / ADR の短縮引用 197 件）
```

### G-area-budget（面積の上限）— 最も先に赤くなりうる

- 上限は `AREA_BUDGET = { alwaysLoaded: 15500, rules: 12000 }`（`scripts/governance-check.mjs:1060`）。指標は**コードポイント数・CR 除く**（同 `countChars`・L1063）。
- **常時ロード面**の母集団 = `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]`（L1052）+ **`disable-model-invocation: true` を除く skill の `description` 1 行**（`skillDescriptionArea`・L1087）。実測 **14135 / 15500 → 余裕 1365 字**。
- **rules 面**の母集団 = `^\.claude/rules/[^/]+\.md$`（L1128）。実測 **10470 / 12000 → 余裕 1530 字**。内訳（`tr -d '\r'` + コードポイント数え・合計が evidence の 10470 と一致することを検算済み）:

  | ファイル | 字数 |
  |---|---|
  | `safety-nets.md` | 3462 |
  | `governance-docs.md` | 1819 |
  | `src-tauri.md` | 1729 |
  | `snotra-core-search.md` | 1495 |
  | `snotra-core.md` | 882 |
  | `snotra-settings.md` | 819 |
  | `spec.md` | 264 |
  | **合計** | **10470** |

- 帰結: 既存 3 枚へ 1 行（実測サイズの類似行で 50–60 字）ずつ足す ≈ 180 字は余裕内。**`snotra-settings.md` 相当（819 字）の新規 router を 1 枚足しても 819 + 180 ≈ 999 < 1530 で収まる**が、余裕の 2/3 を 1 回で使う。`**/*.rs` の薄い 1 枚（100–200 字想定）なら消費は最小。
- **`docs/comment-guidelines.md` 自身は G-area-budget の母集団外**（`docs` は対象外・L1046–1048 のコメントが明言）。条項をここへ厚く書いても面積では赤くならない。

### G-heading-refs / G-near-heading-refs（見出し参照の照合）— **実測で穴を 1 つ確認**

`scripts/governance-check.mjs` の実物を import して代表入力で測った（推測ではない）。走査元は md 46 件 + `.rs` 96 件、正準形は `` `<対象>`「<見出し>」 ``（`HEADING_REF`・L1174）、照合は `**`/バッククォート/`「」`/空白を除去したあとの**前方一致**（`normAnchor`・L1185）。

| 入力 | G-heading-refs | G-near-heading-refs |
|---|---|---|
| ``条項を `docs/comment-guidelines.md`「第一原則: コメントは「なぜ」を書く」へ追加した。``（= `RETROSPECTIVE.md:33` の現物） | **照合 0 件 / finding 0 件** | **照合 0 件 / finding 0 件** |
| ``` `docs/comment-guidelines.md`「第一原則: コメントは」 ``` | 照合 1 件 / finding 0 件（前方一致で着地） | 照合 0 |
| ``` `docs/comment-guidelines.md`「rustdoc の様式」 ``` | 照合 1 件 / finding 0 件 | 照合 0 |
| ``` `docs/comment-guidelines.md`「存在しない節」 ``` | 照合 1 件 / **finding 1 件**（着地しない） | 照合 0 |
| ``` `docs/comment-guidelines.md` の「rustdoc の様式」 ```（助詞挟み） | 照合 0 | 照合 1 件 / **finding 1 件**（正準形へ直せと言う） |

**結論**: 見出しに `「」` を含むと `HEADING_REF` の `「([^「」\n]+)」` が当たらず、`NEAR_REF` も gap 上限 8 字（`NEAR_REF_GAP`・L1282）を超えるため**どちらの検査にも載らない**。`docs/comment-guidelines.md` の現アンカー 33 件のうちこの形は `第一原則: コメントは「なぜ」を書く` 1 件（`collectAnchors` の実出力で確認）。**新設する節名に `「」` を入れると同じ穴を増やす。**

制約（新節を作るとき）:
- 節名は**下流参照の契約**になる。参照側は「正準形 + 前方一致」なので、後置の括弧注記（`（…）`）を足すのは安全だが**先頭を変えると参照が着地しなくなる**。
- 参照は**必ずバッククォートを閉じた直後に `「`**（`§` + 節番号と空白のみ挟める）。助詞 1 つ挟むと G-near-heading-refs が「正準形でない」と赤にする。

### G-references（参照実在）

母集団 = `governanceDocs`（`CLAUDE.md` / `AGENTS.md` / `CONTRIBUTING.md` / `SPEC.md` / `docs/**` から `superpowers`・`adr` を除く / 4 crate の `CLAUDE.md` / `.claude/rules/*.md` / `.claude/skills/*/SKILL.md` — L1339–1348、実測 34 件）。**`docs/comment-guidelines.md` はこの母集団に入る**。バッククォート内のパス様参照とリンクの実在を見るので、`.claude/rules/*.md` から `docs/comment-guidelines.md` を書くのは解決する（`exists` は文書ディレクトリ基準 → リポジトリルート → suffix 一意の順・L164–177）。

### G-stale-identifiers（散文に残る腐った識別子）— **測って安全域を確定した**

- 母集団に `docs/comment-guidelines.md`（`staleIdentifierGuideDocs`・L1527）と **`.claude/agents/code-reviewer.md`**（`staleIdentifierDocs`・L1516、実測で含まれることを確認）が入る。
- 述語は**バッククォート内の camelCase（こぶ 1 つ以上）と SCREAMING_SNAKE（`_` 1 つ以上）だけ**（`STALE_IDENT` / `STALE_SNAKE_IDENT`・L1505–1512）。現行語彙は **production ソースの非コメント本文**（`VOCAB_SOURCE_EXT = /\.(rs|ts|tsx|mjs|ps1|toml|yml)$/`・L1492、`stripRustComments` 適用）。
- 代表入力で実測（照合 7 件 / finding 3 件）:
  - 通る: `` `AREA_BUDGET` ``（`governance-check.mjs` に `export const` で在る）・`` `selectChecks` ``・`` `collectAnchors` ``・`` `stripRustComments` ``
  - **赤くなる**: `` `wrapComments` `` / `` `commentWidth` `` / `` `MAX_COMMENT_WIDTH` ``（実装に無い camelCase / SCREAMING_SNAKE）
  - **述語の外**（照合されない）: `wrap_comments`・`max_width`（snake_case）・`SearchEngine`（PascalCase）・`Engine::new_from_cache`（`::` 付き）
- 現状 `docs/comment-guidelines.md` は**照合 0 件 / finding 0 件**（バッククォート内 camelCase / SCREAMING_SNAKE を 1 つも持たない）。
- 帰結: **条項本文で rustfmt の設定キーや架空の検査名を camelCase / SCREAMING_SNAKE で書かない。** 書くなら実在する識別子（`AREA_BUDGET` 等）に限る。

### G-rules-globs

`.claude/rules/*.md` の frontmatter `- "<glob>"` が実在ファイルに 1 件以上マッチすることを見る（L769–789）。**`globToRegex` は harness の配送判定の再現ではなく「マッチ 0 件の検知」に限定した近似**とファイル自身が宣言している（L730）。新規 rule を足すなら glob が 0 件にならないことだけを満たせばよい。

### 検査されないもの（受容する残余）

- **PostToolUse hook は `*.md` と `.claude/rules/**` に何も割り当てない**（`docs/hooks.md:57`）。`docs/comment-guidelines.md` を編集したときの沈黙は「何も走らなかった」。
- **`.rs` の hook の沈黙は fmt / clippy / test の合格であって見出し参照の着地を含まない**（`.claude/rules/governance-docs.md` 最終項）。
- **行またぎコードスパンの検知器は新設しない**（ユーザー確定事項）ので、条項の遵守を機械で見る手段はこの issue の成果物には無い。
- **rustfmt はコメントを折り返さない**: `rustfmt.toml` はリポジトリに存在せず（`ls` で確認）、既定 `wrap_comments = false`。実測でコメント行の最長は 197 字（`src-tauri/src/egui_shell/results_window.rs`）で CI の fmt を通っている。**折返し方針は formatter と衝突しない**。

---

## 現存違反の実測（issue の具体例に照らして）

走査対象: `git ls-files '*.rs'` = **96 件**、コメント行（`///` / `//!` / `//` / `*` 始まり）**8861 行**。走査スクリプトは自分で書いて実行した。

### (a) 文途中の物理改行 — **2733 件 / 87 ファイル**

判定: コメントブロック内で、行末が `。: ： 、 ） 」 】 - | =>` 以外で終わり、次行もコメントで日本語/識別子から続く行（表・箇条書きの導入は除外）。上位:

| ファイル | 件数 |
|---|---|
| `src-tauri/src/egui_shell/view.rs` | 316 |
| `snotra-core/src/indexer.rs` | 271 |
| `src-tauri/src/egui_shell/window_coordinator.rs` | 187 |
| `src-tauri/src/egui_shell/layout.rs` | 156 |
| `src-tauri/src/egui_shell/launcher_controller.rs` | 144 |
| `src-tauri/src/egui_shell/results_view.rs` | 144 |
| （以下 81 ファイル） | … |

**これは修正リストではない。** 2733 件は「適用範囲を明示しない条項が、コミットした瞬間に 2733 件の潜在違反を作る」ことの証拠である。リポジトリには既に先例が 2 つある — `docs/comment-guidelines.md:5`（`既存コメントの一括書き直しは本書のスコープ外——既存コメントは通常の変更でそのコードに触れたときに本書へ寄せれば足りる`）と同 L82（日英混在の条項が `新規分から適用。既存は書き直さない`）。**新条項には同等の射程宣言が要る**（要件であって、確信の持てない所見ではない）。

### (b) 使われている単語をより自然に

`.rs` コメント 8861 行に対する実測:

| 語 | 件数 | 所見 |
|---|---|---|
| 窓（GUI / 時間の両義） | **290 行** | 対する `ウィンドウ` は **28 行**。**13 ファイルで両方が混在**（最悪は `src-tauri/src/egui_shell/window_coordinator.rs` 窓 41 / ウィンドウ 3、`layout.rs` 窓 39 / ウィンドウ 3、`mod.rs` 窓 26 / ウィンドウ 1）。`layout.rs:1` は同じ 1 行で `検索ウィンドウの純粋レイアウト…results 窓の可視性` と両方使う |
| 畳み込み | 11 行 | 語義が 2 つに割れる。`snotra-core/src/opener.rs:205` / `:779`（重複ターゲットを畳み込む = 正しい日本語）と `snotra-core/src/indexer.rs:652/690/695/699/702/828`（digest の fold） |
| 写像 | 8 行 | `query.rs:87` / `search.rs:121` の `恒等写像` は数学の標準語で適切。`indexer.rs:1370-1371/1388` の `文字単位の写像` は「変換」の方が自然 |
| 被覆 | **2 行** | `src-tauri/src/egui_shell/font_stack.rs:8`（`CJK 非被覆なら`）と `:9`（`解決し**被覆するなら**`）。font coverage の意味。**リポジトリ全体でこの 2 行だけ**なので、修正の代価が最小で効果が明確 |
| 「キャッシュが冷えた/温まった」 | 10 行（候補） | 正規表現の当たりで、実際に比喩を述語へ活用した形かは個別確認が要る（→ ⚠️） |

`窓` 290 行の語義内訳をマーカーで排他分類した実測: **GUI 語義のみ 107 / 競合・時間の窓のみ 14 / 両マーカー 2 / 未分類 167**（未分類のサンプル 8 行はすべて GUI 語義寄り: `メイン窓と結果窓の隙間`・`窓を開くたび`・`hidden な窓でも走る` 等）。**GUI 語義が支配的だが、正確な内訳は未測定**（→ ⚠️）。

**リポジトリに既存の訳語規範は無い**: `git grep -n "訳語\|カタカナ" -- '*.md'`（workspace 除く）は `SPEC.md:109` の 1 hit のみで、それは migemo の仕様説明であって規範ではない。ゆえに「repo 固有条項として新設する」は**既存条項の移設ではなく真の新設**である。

### (c) コードから読み取れないものを書く（#977 で足した条項の現存違反）

**この 3 件は「今ある条項」の違反なので、適用範囲の議論が要らない。** `docs/comment-guidelines.md:21` が名指しするのは「経路の**数**、分岐の**列挙**、**呼び出し元**、**到達可能性**」。

1. **`snotra-core/src/search/build.rs:83`** — `通る経路は 2 つ（new_from_tree と、new_with_cached_masks の v3 フォールバック腕）。` #977 で腐った「通る経路は 3 つ」と**ほぼ逐語で同形**。事実は現在真（呼び出しは build.rs:387 / :484 の 2 件）ゆえ**形の誤り**。続く L84–85 の「1 本に寄せてある理由」は不変条件なので残す。
2. **`snotra-core/src/indexer.rs:811`** — `// マスクを計算してキャッシュに含める。起動時に SearchEngine::new_with_cached_masks() がマスク再計算をスキップできるようにする。` 呼び出し元 + 到達可能性の写しであり、**さらに反復 11 の事実と食い違う**（cache-miss も同じ入口を通る。正本は `engine.rs:127–131` と `indexer.rs:51–59`）。
3. **`src-tauri/src/egui_shell/font_stack.rs:8–9`** — 上記 (b) の「被覆」2 件。

**この 3 件は確定違反だが、4 件目の候補は調べて撤回した。** `snotra-core/src/search.rs:331` の `**呼び出し元は 2 つで、どちらも明示の操作である**` は条項が名指しする「呼び出し元」を数えているが、**同じ doc ブロックの L337 が意図と根拠を明示している**——`この一行が「全件走査が毎回の窓表示に乗る」という誤読を 2 度招いたので、頻度を推測させない形にしてある——**頻度を書くなら呼び出し元を名指しする。**`。これは `docs/comment-guidelines.md:15`「過去の事故（実測値・再現条件・issue 番号つきの経緯）」が書く価値があると認めた型であり、**同じ文書の 2 条項が正面衝突している**。

なお当初この件を「事実は真」と書いたのは**測る対象を取り違えていた**（`Engine::recent_history` の呼び出し 2 件を数えたが、doc が乗るのは `SearchEngine::recent_history`・L343 で、その直接の呼び出しは `engine.rs:169` の 1 件）。コメントが名指しするのは推移的なユーザー操作 2 つ（`/r` スラッシュコマンドとトレイの履歴メニュー = `launcher_controller.rs:833` と `platform/tray.rs:34`）であり、**その水準では真**。この取り違えは `AGENTS.md`「主張は代理ではなく対象そのもので測ってから書く」の型そのものなので、記録として残す。

### 判定の分類規則（これを書かないと大量誤検出になる）

`N つ` 系の当たりは **35 件**あるが、**その大半は条項が禁じるものではない**。区別:

- **禁じられる = 現在の形の数え上げ**: `通る経路は 2 つ`（build.rs:83）・`出所は 2 つある`（indexer.rs:51）・`書く条件は 2 つある`（indexer.rs:1194）・`消費者は 2 つある`（layout.rs:325）
- **推奨される = 一意性の不変条件**（`docs/comment-guidelines.md:14`「不変条件（何が常に成り立つべきか、崩すと何が壊れるか）」に該当）: `保存先を導く経路はここ 1 つだけである`（config.rs:723）・`規則の定義はこの関数 1 つである`（indexer.rs:228）・`正規化キーを得る経路はここ 1 本である`（path_store.rs:363 / scoring.rs:66）・`判定式の正本はここ 1 か所である`（layout.rs:370）・`色のパーサは 1 本である`（visual.rs:18）
- **推奨される = 事故の予防として意図的に呼び出し元を名指しした形**（同 L15「過去の事故」に該当）: `呼び出し元は 2 つで、どちらも明示の操作である`（search.rs:331）——理由が同ブロック L337 に在り、`頻度を書くなら呼び出し元を名指しする` と自ら規則化している

「これが唯一の経路であり、そう保て」は条項が望む「なぜ」型である。**条項が禁じるのは今の姿を数えることであって、一意性を宣言することでも、事故の再発を防ぐために名指しすることでもない。** この線引きが条項本文に無いので、実装者が 35 件を一括で潰しにかかるリスクがある（→「見落とされやすいと考える点」）。

---

## 「参照されているように見えない」の実測（配送の母集団）

### `docs/comment-guidelines.md` を指す既存参照は 5 件（`git grep -n "comment-guidelines" -- .` から `^workspace/` を除いた全件）

| 参照元 | 種類 |
|---|---|
| `AGENTS.md:16` | `## ドキュメント参照` の索引行 |
| `CONTRIBUTING.md:13` | 人間向け入口 |
| `RETROSPECTIVE.md:33` | 履歴記述（この issue を名指し） |
| `docs/development-principles.md:44` | SSOT 宣言（「書式・粒度・定型ラベルは comment-guidelines.md を SSOT とする」） |
| `docs/superpowers/plans/2026-07-23-su3.5-tool-selection.md:17` | 履歴資料（#589 で非規範化） |

**`.claude/` 配下は 0 件**（`git grep -rn "comment-guidelines\|コメント規約\|コメントガイドライン" .claude/` → exit 1）。**`.rs` からも 0 件**（`git grep -n "comment-guidelines" -- '*.rs'` → exit 1）。

### `.rs` 編集時の自動配送（`.claude/rules/` の `paths` frontmatter）

母集団を repo 自身の glob 実装（`governance-check.mjs` の `globToRegex`）で数え上げた:

| rule | `paths` | マッチする `.rs` |
|---|---|---|
| `.claude/rules/snotra-core.md` | `snotra-core/**/*.rs` | 33 |
| `.claude/rules/snotra-core-search.md` | `snotra-core/src/search.rs` + `snotra-core/src/search/**/*.rs` | 15（上の真部分集合） |
| `.claude/rules/src-tauri.md` | `src-tauri/**/*.rs` | 35 |
| `.claude/rules/snotra-settings.md` | `snotra-settings/**/*.rs` | 16 |
| `.claude/rules/governance-docs.md` / `safety-nets.md` / `spec.md` | `.rs` を含まない | 0 |

**`.rs` 96 件中 84 件がいずれかの rule に覆われ、未カバーは 12 件 — すべて `snotra-egui-runtime/`**（`env.rs` / `ime.rs` / `input.rs` / `lib.rs` / `monitor.rs` / `proof.rs` / `raster.rs` / `renderer.rs` / `repaint.rs` / `runtime.rs` / `surface.rs` / `windows_ime.rs`）。この crate は `snotra-egui-runtime/CLAUDE.md` を持ち `Cargo.toml` の workspace member でもあるのに、rule ファイルが 1 枚も無い。

**この 84/96 は近似である。** `governance-check.mjs:730` が「harness の配送判定の再現ではなく『マッチ 0 件の検知』に限定した近似」と自ら宣言しており、実際に 2 つの glob 意味論が食い違う: `git ls-files 'src-tauri/**/*.rs'` は `src-tauri/build.rs` を**含めない**（34 件）が `globToRegex`（`**/` → `(?:.*/)?`）は**含める**（35 件）。同じ差が `snotra-settings/build.rs` にもある。**12 件未カバーという結論はどちらの意味論でも変わらない**（`snotra-egui-runtime` にマッチする glob が存在しないため）。

---

## ⚠️ 確信の持てない所見

1. **`snotra-egui-runtime` 専用 router を 1 枚建てるか、`**/*.rs` の薄い 1 枚で 96/96 を覆うか、判断がつかない。** 前者は既存 4 枚と構造が揃い `snotra-egui-runtime/CLAUDE.md` への router も同時に埋まる（この crate は今 rule 経由の入口を持たない）が、rules 面の余裕 1530 字の 2/3 を使う。後者は面積最小で全 `.rs` を覆うが、「crate ごとに 1 枚」という既存の分割規律を破り、同じポインタが 5 枚のうち 2 枚から二重に届く crate が生まれる。**どちらが良いかは repo の設計選好の問題で、私の測定では決着しない。**
2. **`窓` 290 行の語義内訳が確定していない。** マーカーによる排他分類で GUI 語義 107 / 競合・時間 14 / 両方 2 / **未分類 167**。未分類 167 のサンプル 8 行はすべて GUI 語義寄りだったが全数は見ていない。「GUI の窓 → ウィンドウ」型の条項は競合の窓（`lost-update 窓`・`finish 窓`・`release が届かない…窓`）へ誤爆しうるので、条項を書く前に**語義ごとに数え直すべき**。
3. **`窓` を repo 固有条項でどう扱うか、私には判断材料が足りない。** 290 対 28 という比は「`窓` がこの repo の既定語である」とも読める。ユーザーは「repo 外の個人設定を参照させない」と確定させたので条項は repo ローカルでなければならないが、**その条項が既定語をひっくり返す方向を採るのか、混在だけを禁じるのか（`docs/comment-guidelines.md:82` の日英混在条項と同型）は、この issue の本文からは決まらない。** 混在 13 ファイルという実測は後者を支持する材料になる。
4. **「キャッシュが冷えた/温まった」の 10 件が真の当たりか未確認。** 正規表現（`(?:キャッシュ|cache)[^。\n]{0,12}(?:冷え|温ま)` 等）の当たりで、実際に比喩を述語へ活用した形か個別に読んでいない。
5. **`indexer.rs` の `畳み込み` 6 件（digest の fold）を直すべきか判断がつかない。** `snotra-core/CLAUDE.md` の本文自身が `畳み込み比較を別実装で書き起こしてはならない` を規範として持ち、`snotra-core/src/indexer.rs:230` がそれを逐語で写している。**条項を作ると規範文書側も同時に赤くなる関係**にあり、コードコメントだけ直すと文書とコードで語が割れる。
6. **`docs/comment-guidelines.md:7` の見出しを改題すべきか判断がつかない。** 改題すれば正準形参照が機械照合の保護下に入る（照合 0 → 1）が、L7 は本書の第一原則の名であり、`「なぜ」` を外すと語義が弱まる。**代替として `「第一原則: コメントは」` までで参照を切る**手がある（実測で前方一致が着地する）が、それは「読みにくい参照を機構に合わせて書く」ことになる。判断は設計選好。
7. **`CONTRIBUTING.md:13` の括弧内要約（`rustdoc / TSDoc の様式・粒度`）が、条項追加後に実装より狭くなる。** 折返し・語の自然さは様式でも粒度でもない。直すべきか、括弧内は例示なので放置でよいか、判断がつかない。`AGENTS.md:16` の括弧内（`rustdoc / TSDoc の様式・粒度・定型ラベル`）にも同じ問題がある。
8. **`.claude/agents/code-reviewer.md` に観点を足すと、そのファイルが G-stale-identifiers の母集団に入ることは実測したが、面積の課税は無いのか未確認。** `.claude/agents/` は `ALWAYS_LOADED_FILES` にも rules 面にも入らない（母集団の正規表現で確認）が、**サブエージェント起動時に必ず読まれる面**であることは harness の挙動であって `governance-check` の視界外である。
9. **`.claude/rules/` の `paths` frontmatter が harness で実際にどの glob 意味論で評価されるか未確認。** `.claude/settings.json` に `rules` の記述は無く（grep 0 件）、`docs/hooks.md` にも配送判定の実装は無い。ネイティブ機構ゆえ repo からは測れない。**12 件未カバーの結論は意味論に依存しないが、`build.rs` 2 件の帰属は意味論次第**。
10. **`docs/comment-guidelines.md` の 2 条項が正面衝突しているのを、この issue で解くべきか判断がつかない。** L21（`呼び出し元`・`到達可能性`を書かない）と L15（`過去の事故`は書く価値がある）が、`snotra-core/src/search.rs:331–337` という実在の 1 ブロックで衝突する。そのコメントは「呼び出し元を名指しする」ことを**事故の再発防止として自ら規則化している**。issue の要求は「ガイドラインがより良いものとして更新されること」なので射程内に見えるが、**#977 が足した条項の境界を引き直す作業は元の条項の意図を変えうる**ので、ユーザー確定事項（折返し・語の自然さ・検知器なし）の外に在る。
11. **折返し方針を「文中の物理改行を入れない」に統一すると、長い設計判断コメント（`SearchEngine` struct doc のような `# Why ...` 見出し + 表）が 1 行数百字になりうる。** 実測の最長コメント行は既に 197 字で fmt を通っているので機構上は可能だが、**diff の読みやすさ（1 語の修正が 1 行全体の変更に見える）が悪化するかは測っていない。**

---

## 見落とされやすいと考える点

1. **配送穴は 1 つではなく 2 つある。** issue コメントが名指しするのは `.claude/rules/` の穴だが、**`.claude/agents/code-reviewer.md` も comment-guidelines を 1 度も参照していない**（`.claude/` 全体で 0 件）。ルート `CLAUDE.md` は `/implement`「4b」が code-reviewer を自動起動すると書いており、**「ガイドラインに沿って書かれること」を事後に見る唯一の枠組みがコメント規約を知らない**。rules だけ直すと「書く前に届く」経路はできても「書いた後に検算される」経路はできない。
2. **rule 3 枚に足しても 12 件は届かないまま残る。** `snotra-egui-runtime/` は workspace member で `CLAUDE.md` も持つのに rule が 1 枚も無い。既存 4 枚を眺めて「crate ごとに在る」と読むと取りこぼす（`src-tauri` / `snotra-core` / `snotra-settings` の 3 crate ぶんしか無い）。しかもこの crate は **`窓` の出現密度が高い**（`input.rs` 窓 6 / ウィンドウ 1、`repaint.rs` / `runtime.rs` / `proof.rs` / `monitor.rs` が語の自然さの当たり多数）ので、**条項が最も要る場所が最も届かない場所と一致している**。
3. **`docs/comment-guidelines.md:7` の見出しが機械照合の外に在る。** これは grep では見えず、`governance-check.mjs` の関数を代表入力で回さないと分からない。この issue は「参照されるようにする」ことが主題なので、**参照を増やす作業の途中でこの穴に足を突っ込む可能性が高い**（`RETROSPECTIVE.md:33` が既に踏んでいる）。増やした参照が全部未検査だったという結末になりうる。
4. **適用範囲を書かないと、条項をコミットした瞬間に 2733 + 290 件の潜在違反が生まれる。** 「(a) ガイドラインに沿ってコメントが書かれること」という要求は、素直に読むと既存コメントの一括修正を含意する。repo には先例が 2 つ在る（L5 のスコープ宣言、L82 の「新規分から適用」）が、**先例が在ることと新条項がそれを継承することは別**で、L5 の宣言は「既存コメントの一括書き直し」に向いていて「新条項の射程」を明示的には覆っていない。
5. **`N つ` の一括潰しに走る危険。** #977 の条項は今 35 件の `N つ / 1 本` 系の記述に当たりうるが、**そのうち禁じられるのは 4 件程度で、残りは条項が推奨する一意性の不変条件（L14）か事故の予防（L15）である**。私自身、doc ブロック全文を読むまで `search.rs:331` を違反として数えていた——**1 行だけ見て条項に当てると誤判定する**（理由は隣接する行に在る）。「経路の数を書かない」を字面で適用すると、`保存先を導く経路はここ 1 つだけである`（`config.rs:723`）のような**壊れたら直ちに事故になる不変条件**が消える。`AGENTS.md`「重複した読み・冗長に見える状態を束ねる/消す」のトリガーが要求する「後で読まれることに依存していないか」の 1 行書き出しが、そのまま効く局面。
6. **`畳み込み` は規範文書とコードコメントの両方に在る。** `snotra-core/CLAUDE.md` 自身が `畳み込み比較を別実装で書き起こしてはならない` を規範として持ち、`indexer.rs:230` がそれを写している。**語の条項はコードコメントだけを直すと文書とコードで語が割れる**——`docs/comment-guidelines.md:21` が「この項の射程はコードコメントである」と射程を宣言した先例があるので、新条項も射程を宣言しないと同じ曖昧さを生む。
7. **どの検査も新条項の遵守を見ない。** 検知器を新設しないというユーザー確定事項の帰結として、折返し・語の自然さは **100% 規範であって機構ではない**。`CLAUDE.md`「フック」が言う「沈黙が『合格』なのは `selectChecks` に検査が割り当てられたファイルだけである」の裏で、`docs/comment-guidelines.md` の編集は PostToolUse hook が何も走らせない（`docs/hooks.md:57`）。**この issue の成果物の妥当性を測る唯一の経路は、次に `.rs` のコメントを書く作業が条項を実際に使うことである**（`.claude/rules/safety-nets.md`「規範そのものへフォールトインジェクションを当てる専用手順は置かない」がまさにこれを言う）。ゆえに配送（rules + code-reviewer）が条項本文より重い。
8. **`snotra-core-search.md` に同じポインタを足すと二重課税になる。** `paths` が `snotra-core.md` の真部分集合（15 ⊂ 33）なので、`search.rs` を触ると同じ行が 2 回届く。ルート `CLAUDE.md`「利用できるスキル」節が skill roster について同じ二重課税を明示的に避けているのと同型の判断。
