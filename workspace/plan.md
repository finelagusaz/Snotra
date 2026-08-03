# plan — #891 腐り検出器 G-stale-identifiers の射程拡大（#819 案 B）

## 目的

`G-stale-identifiers` の射程を、**#819 の腐り（識別子が現行語彙から外れる形）が機構で捕まる**ところまで広げる。今日の射程では `/plan-review` の独立導出が付随して見つけたにすぎず、`docs/development-principles.md`「構造的設計原則と強制の階梯」の言う「書き手の記憶」段に留まっている。

**#825 の腐りはこの拡大では捕まらない**（issue 本文の動機付けは「どちらも捕まらなかった」という現状の記述までが正しく、「広げれば両方捕まる」ではない）。`docs/adr/ADR-canonical-source-without-pointer-indirection.md` が既に「#891 は本件のクラスを閉じない——#825 が腐らせたのは**識別子ではなく命題**である」と明記しており、**この計画はその記述を変えない**。射程が閉じるのは #819 のクラスだけである。

## 受け入れ条件

1. `docs/**.md`（`superpowers/` と `adr/` を除く）・ルート `CLAUDE.md` / `AGENTS.md` / `snotra-settings/SETTINGS-DESIGN.md` が検査対象に入る
2. 述語が SCREAMING_SNAKE を見る（`G12_NO_LAUNCHER_READ` 型の腐りが捕まる）
3. 語彙源に `.yml` が入る（`.json` は入れない）
4. **新母集団が空になったら鳴る**（今日は鳴らずに緑で沈黙する・実測）
5. **配線を戻したら鳴る**（実リポジトリの finding は変わらないので dogfood も証跡も気づけない）
6. `npm run governance:check` が緑
7. `node --test scripts/governance-check.test.mjs` が緑
8. フォールトインジェクションで、述語・母集団・fail-closed をそれぞれ**切り分けて**実測した記録が ADR に残る
9. 測って見つけた腐りが、**母集団に入れないと決めた面のものも含めて**未修正で残らない（B14b）

## 変更ファイルと対象シンボル

| ファイル | 対象 |
|---|---|
| `scripts/governance-check.mjs` | `VOCAB_SOURCE_EXT` / `STALE_EXTRA_DOCS` / `STALE_IDENT` / 新設 `STALE_SNAKE_IDENT` / 新設 `staleIdentifierGuideDocs` / `staleIdentifierTargets` / `currentVocabulary` / `scanStaleIdentifiers` / `buildChecks` / `runAll` / G-stale-identifiers 節の doc コメント |
| `scripts/governance-check.test.mjs` | `describe("G-stale-identifiers checkStaleIdentifiers…")` / `describe("G-stale-identifiers の配線…")` / 新設 describe |
| `docs/development-principles.md` | `:39` `:78` `:81` `:83` `:84` `:128` |
| `docs/hooks.md` | `:67` |
| `snotra-core/CLAUDE.md` | `:21`（母集団外だが実在する腐り・B14b） |
| `docs/adr/ADR-stale-identifier-detector-scope.md` | 末尾へ追記節（**原文は 1 文字も書き換えない**） |

## 実装順序

### Phase 1 — 検出器の改修

- [ ] **B1** 新母集団を**2 経路に分けて**足す。**どちらも `staleIdentifierDocs` へは入れない**——`runAll` の `staleDocs.length === 0` が `.claude/**` の消滅を見ており、混ぜると長さが常に 1 以上になりその検知が永久に沈黙する（`:1432-1434` のコメントが SSOT）
  - **(a) 固定パス 3 本は `STALE_EXTRA_DOCS` の静的リテラルへ足す** — `["SPEC.md", "CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"]`。**静的リテラルは fail-closed を無償で持つ**（読めなければ `scanStaleIdentifiers` が「母集団の欠落」を出す）ので、この 3 本に新しい機構は要らない
  - **(b) グロブ由来の `docs/**` だけを新関数 `staleIdentifierGuideDocs(snapshot)` にする** — `/^docs\/.*\.md$/` かつ `docs/superpowers/` と `docs/adr/` を除く。`staleIdentifierTargets` はこの 3 者を連結する
- [ ] **B2** **`staleIdentifierGuideDocs` 専用の 0 件検知を `runAll` へ足す**
  - **実測の根拠**: 守られている `.claude/**` は照合 **1 件**、守られていない `docs/**` は照合 **35 件**を寄付する。既存の 3 つの 0 件検知（`ctx.docs` / `ctx.refDocs` / `ctx.staleDocs`）はどれも他の母集団で埋まったまま非空なので代替にならない
  - `buildChecks` の `sink` へ `staleGuides` を足し、`runAll` へ既存 `staleDocs.length === 0` と**対称な 1 行**を置く。**兄弟母集団が非空でも成立してはならない**（B10 で両方向に検算する）
- [ ] **B3** `STALE_IDENT` の隣へ `STALE_SNAKE_IDENT = /^([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)(\(\))?$/` を足し、`scanStaleIdentifiers` が両方を試す。**`checked` の加算は 1 回だけ**——2 述語は先頭文字で相互排他なので `raw.match(STALE_IDENT) ?? raw.match(STALE_SNAKE_IDENT)` の形にして二重計上を構造的に不可能にする
  - **2 述語を `|` で 1 本の正規表現へ畳んではならない**（独立導出の指摘）。`scanStaleIdentifiers:1479` が `im[1]` を読むため、キャプチャ群が 1 つずれると **`inVocab(undefined)` になって沈黙する**——赤が出ないので気づけない
- [ ] **B4** `VOCAB_SOURCE_EXT` へ `yml` を足し、`currentVocabulary` の `#` コメント除去分岐（現在 `/\.(ps1|toml)$/`）へ `yml` を回す。**`.yaml` は不要**（リポジトリに 1 本も無いことを `git ls-files` で実測済み）
- [ ] **B5** **自称スコープの doc コメントを改訂する**。「見るのは `.claude/**` の散文と `SPEC.md` の中のバッククォート内 camelCase 識別子だけである」は B1・B3 の瞬間に**偽**になる。新母集団が何を含むか（`docs/` は設計原則・ビルド手順・フック契約・アーキ説明という性質の違う 4 種）を書き、**`docs/adr/` を外した理由**と**モジュール `CLAUDE.md` を採らなかった理由**も同節へ置く
- [ ] **B6** finding のメッセージと `runAll` の証跡文字列を拡大後と整合させる。**証跡は 1 件 / 25 文書 → 77 件 / 35 文書へ動く**
- [ ] **B7** 既存テストを改訂する。**衝突するのは 1 件だけ**——`:944`「検査対象は規範の散文 + SPEC.md…」の `staleIdentifierTargets` 期待値。`:930`「母集団は skills / rules / agents の md に限る」は `staleIdentifierDocs` の意味を変えないので**真のまま残す**（残すこと自体が B1 の分離を固定する）
- [ ] **B8** **新母集団の配線テストと 0 件検知テストを新設する**。**B9〜B11 は実装時 1 回きりの測定であって、後日の退行を捕まえる面ではない**
  - **(a) 配線**: `:959` の `describe("G-stale-identifiers の配線…")` と同じ形で、`buildChecks` が `docs/**` と新静的 3 本を検査対象として渡すことを赤フィクスチャで固定する。同ファイルの論証（配線を戻しても実リポジトリの finding は変わらないので dogfood も証跡も気づけない）が新母集団にそのまま当たる
  - **(b) 0 件検知**: **既存の `:709-715`「runAll（空母集団の明示 fail）」は代替にならない**（独立導出の指摘・実測で確認）——`snap({})` に対して `findings.length > 0` しか見ないため、**どのガードが鳴ったかを区別しない。B2 のガードを丸ごと消しても緑のまま通る**。**兄弟母集団（`.claude/**` と静的リテラル）を非空に保ったまま `docs/**` だけを空にして、その 1 件が鳴ること**を固定するテストを別に置く

### Phase 2 — フォールトインジェクション（`.claude/rules/safety-nets.md` 必須）

**稼働中のガードは弱めない**——scratchpad の複製へ変異を当てる。

- [ ] **B9** **述語だけを切り分けて測る。** 種は**既存母集団**（`.claude/rules/*.md`）へ蒔く——`docs/**` へ蒔くと述語と母集団を同時に変異させることになり、失敗時に切り分けられない
  - 赤: `G12_NO_LAUNCHER_READ`（#825 の PR が消した実在の語）。緑の対: `NO_LAUNCHER_READ`
  - **逆向きを必ず測る**——語彙に在る SCREAMING_SNAKE（`CLEAR_COLOR` / `NO_LAUNCHER_READ` / `AREA_BUDGET`）が鳴らないこと
  - **`\b` と `_` の両方向**: `NO_LAUNCHER_READ` は語彙にヒットし、`G12_NO_LAUNCHER_READ` はヒットしない（scratchpad で実測済み・plan 段階で再現）
- [ ] **B10** **母集団を切り分けて測る。** B9 の種を移して、(a) `docs/**` で捕まる、(b) `docs/adr/` で**捕まらない**、(c) `docs/superpowers/` で**捕まらない**、(d) 新静的 3 本で捕まる、を測る（`.claude/rules/safety-nets.md`「検査の入力集合を、具体対象で検算する」の両方向）
  - **B2 の fail-closed も同時に測る**: `docs/**` を丸ごと空にして鳴ること。**かつ、兄弟母集団（`.claude/**`・`SPEC.md`）が非空でも鳴ること**
  - **B16 の ADR 追記と B9 のフィクスチャが自分自身を赤にしないことを確認する**——ADR は母集団外、テストファイルは `VOCAB_TEST_FILE` が語彙から外す。**構造的に安全だが、仮定せず測る**
- [ ] **B11** `.yml` が寄付する語彙を**列挙して**受容する残余として記録する。実測 9 語: `GITHUB_ENV` `GITHUB_OUTPUT` `GITHUB_TOKEN` `TAG_NAME` `TAURI_SIGNING_PRIVATE_KEY` `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` と、**日付書式の断片 `ddTHH` `ssZ` `yyyyMMddHHmm`**（`'` で分断された残骸が camelCase 述語に当たる。同名の識別子を誤免罪しうる・今日 0 件）

### Phase 3 — 拡大が指す件数の是正（**同じ PR に束ねる**）

未修正なら `governance:check` が赤のままなので分離できない。**finding は実測 9 件**（issue の「10 件 / 真の腐り 8」は #825 マージ前の値だった）。

- [ ] **B12** `docs/development-principles.md` の**真の腐り 7 件 / 相異なる識別子 5 個**を是正する。**調査で処方が変わった**——2 軸導出は Rust へ生き残っており、名前だけが TS 期のまま取り残されていた。**4 個は現行の等価物へ差し替える**:

  | 位置 | 散文の語 | 差し替え先（`src-tauri/src/egui_shell/search_state.rs`） |
  |---|---|---|
  | `:78` `:83` | `viewKind()` | `view_kind()` / `ViewKind`（Results / Folder / Tool・`:10`） |
  | `:78` `:84` | `interpKind()` `interpKind` | `interpret()` / `QueryIntent`（Plain / Command / Instant・`:18` `:35`） |
  | `:84` | `isInstantPrefix` | `is_instant_prefix()`（doc に「instant 検出の SSOT」と明記・`:30`） |
  | `:81` | `assertNever` | Rust の網羅 `match`（**コンパイラが直接検出するので補助関数が要らない**——技法そのものが変わった） |

  - `:39` `shouldShowResults` だけは**現行の等価物が無い**（`layout.rs:536` が「`show_results` へ潰していた」のを分解済み）。バッククォートを外して散文にする（`.claude/rules/governance-docs.md`「歴史を書くならバッククォートを外して散文にする」）
  - **差し替え先は snake_case / PascalCase ゆえ述語の外に出る**——これは既存の受容する残余（「snake_case・PascalCase も `STALE_IDENT` の外」）であって新しい穴ではない。B16(e) に書く
- [ ] **B13** `:128` `backgroundThrottlingPolicy` を**外部語彙として**処理する。現行の等価物が存在せず、**存在してはならない**（「Windows 非対応でビルドエラーになる」と在ってはならないことを述べるために名指している＝検出器が要求する向きが逆）。**バッククォートを外して散文にし、`tauri.conf.json` はバッククォートのまま残す**——検索性は素のテキストで保たれる
- [ ] **B14** `docs/hooks.md:67` `CLAUDE_PROJECT_DIR` を処理する。`EXTERNAL_CMD_LINE` は `gh|npm|cargo|git|node|pwsh|npx` にしか当たらずコマンド行化は使えない（実測）。**`${CLAUDE_PROJECT_DIR}` の形へ直す**——先頭が `$` なので述語が構造的に外し、**同じ行の `.claude/settings.json` の実際の記法（`${CLAUDE_PROJECT_DIR:-.}`）と一致する＝記述の正確化になる**
- [ ] **B14b** `snotra-core/CLAUDE.md:21` の `iconCacheSize` を `Config::icon_cache_cap()`（`snotra-core/src/config.rs:626` に実在）へ畳む。**撤去済みフロントの語を現在形で `Config::icon_cache_cap()` と並記している**真の腐りである（独立導出が発見）
  - **母集団には入れない**（モジュール `CLAUDE.md` は却下したまま）が、**測って見つけた腐りを未修正のまま残さない**——却下が既知の欠陥を隠す形にしないため。B16(c) にその旨を書く
  - **この変更は検査で守られない**（母集団外ゆえ再発しても鳴らない）ことを ADR の受容残余に書く
- [ ] **B15** `npm run governance:check` が緑になるまで反復する。**`governance-check.test.mjs:1306` の dogfood テスト（実リポジトリで全検査が緑）も同時に緑になる必要がある**

### Phase 4 — ADR 追記と検証

- [ ] **B16** `docs/adr/ADR-stale-identifier-detector-scope.md` へ追記節を足す（既存の「その後（#735 完了後・射程を広げた）」と**同じ形**。**原文は 1 文字も書き換えない**）:
  - (a) 測定表（**実測値で**。「偽陽性 0」ではなく「外部語彙 2 件」と正しく分類する）
  - (b) `docs/adr/` を母集団から外した理由 + **`governanceDocs` は `docs/adr/` を含むという非対称**とその理由。**書かないと次の人が統一しにいく**
  - (c) **モジュール `CLAUDE.md` を却下した理由**（否定の知識）。「外部語彙は `docs/**` には出ない」ではない——**実測で 2 件出ている**。本当の理由は「ラップ対象の外部 API（Win32 / tao / TTC）を語る場所ゆえ外部語彙の**密度**が高い」（実測 真 1 : 外部語彙 3。外部語彙は `WM_SETCURSOR` / `numFonts` / `MARKER_DONT_FOCUS` で、**語彙源をどう広げても免罪できない**）。**却下したが、そのとき見つかった唯一の真の腐り（`iconCacheSize`）は手で直した**（B14b）——却下が既知の欠陥を隠す形にしないため。**ただしそれは検査で守られない**（受容する残余）
  - (d) **`.json` を語彙源へ入れない判断**（否定の知識。測定上は等価でありながら 3 つの契約——生成物/依存メタデータ、テストコード非寄付、決定性——を破る）。「手書きの `.json` 3 本だけ」に絞る案も**リストゆえ**却下したこと。**独立導出は別の答え（`.json` を足し、lock/生成物/CI 不在ファイルを除く）へ達したことも併記する**——その方式なら finding が 1 件（`CLAUDE_PROJECT_DIR`）減るが、**除外リストは冒頭契約「免除注記の機構を設けない」に正面から当たる**ため採らない
  - (e) 「述語は camelCase しか見ない」という既存の受容残余の**更新**（SCREAMING_SNAKE を得た / snake_case・PascalCase は依然外・B12 の差し替え先がそこへ落ちる）
  - (f) **`docs/design/` を入れた理由**（U1 の決定）
  - (g) `.yml` が寄付する GitHub 提供語彙と日付書式断片を受容する残余として記録
  - (h) B9〜B11 のフォールトインジェクション結果
- [ ] **B17** `npm run governance:check` / `node --test scripts/governance-check.test.mjs`
- [ ] **B18** PR 本文へ「CI での実測」をチェックリストとして送る（`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」）

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| `.claude/**` が空なら鳴る | `runAll` の `staleDocs.length === 0`。**B1 が新母集団を混ぜないことで保つ**（B7 が `:930` を残すことで固定） |
| **`docs/**` が空なら鳴る** | **B2 で新設**（現状は緑で沈黙する・実測）。B10 で兄弟母集団が非空でも鳴ることまで測る |
| 固定パス 3 本が消えたら鳴る | 静的リテラルゆえ `scanStaleIdentifiers` の「母集団の欠落」が無償で担う |
| 配線を戻すと鳴る | **B8 で新設**（実リポジトリの finding は変わらないので dogfood も証跡も気づけない） |
| `docs/adr/` `docs/superpowers/` の歴史記述は鳴らない | B10 の逆向き検算 |
| 免除注記の機構を設けない | 除外リストを追加しない。**`.json` を採らない判断もこの契約から出ている** |
| テストコードは語彙を寄付しない | `VOCAB_TEST_FILE` を維持。`.json` を採らないので `test-results/.last-run.json` の経路は開かない |
| 判定は決定的（手元と CI で同じ） | `.json` を採らないので gitignore 済みファイルが語彙へ入る経路は開かない |
| `checked` が二重計上されない | B3 の `??` 連鎖（2 述語は先頭文字で相互排他）+ テスト |
| `ADR-canonical-source-without-pointer-indirection.md` の受容残余が真であり続ける | **`docs/adr/` とモジュール `CLAUDE.md` を母集団へ入れない決定に依存している**。**入れると同 ADR が偽になり、それを検知する機構は無い**（B16(c) に明記する） |
| **（守れないもの）** B14b で直した `snotra-core/CLAUDE.md:21` の再発 | **無い**——母集団外ゆえ鳴らない。受容する残余として B16(c) へ書く |

**異常系**: 新母集団のファイルが読めない → 静的分は「母集団の欠落」finding、グロブ分は列挙に現れないので B2 の 0 件検知が担う。両者を B10 で測る。

## テスト方針と検証コマンド

- `node --test scripts/governance-check.test.mjs`（新設 describe を含む）
- `npm run governance:check`（dogfood）
- フォールトインジェクション B9〜B11 は scratchpad の使い捨てスクリプト（**リポジトリへコミットしない**）
- PostToolUse hook が `scripts/*.mjs` の編集で自動発火する検査は沈黙 = 合格

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要**。製品の挙動を変えない（ガバナンス検査の射程変更）
- `docs/adr/ADR-stale-identifier-detector-scope.md`: **要**（B16）
- `docs/development-principles.md` / `docs/hooks.md`: **要**（B12〜B14。射程拡大が指す件数の是正）
- `AGENTS.md` / ルート `CLAUDE.md`: **不要**。`governance:check` のトリガーは既に条件別チェック表に在り、検査 ID も件数も文書側に写していない（`buildChecks` が SSOT）

## 未確定（実装前に潰す）

- [x] **U1 `docs/design/` を母集団へ入れるか** — **入れる**（2026-08-03 決定）
  - 母集団から外す基準は「日付を持つ」ではなく「**もう成り立たないことを書く場所**」である。`docs/adr/` は却下案（存在しない案）、`docs/superpowers/` は #589 で非規範化された当時の設計
  - `docs/design/2026-05-31-coherence-staleset.md` は `status: Agreed` で、**`docs/architecture.md:99` が「詳細は」と現在形で指す先**である（`:210` にも索引がある）。読者はそこで現行の識別子に出会うことを期待する
  - 外すと、G-references が守るポインタの**指し先だけが黙って腐る**
  - コスト実測: 照合 2 件 / finding 0 件
- [x] **U2 採用セルの測り直し**（issue 本文が着手時に要求していた） — 2026-08-03 実測。**issue の表を 2 箇所訂正した**: 照合 43 → **77**（新静的 3 本の 35 件を数えていなかった）、真の腐り 8 → **7**（8 は #825 マージ前の値）。内訳・per-file・`.yml` の寄付語彙は `workspace/research.md`
- [x] **U3 `.yaml` の存在** — `git ls-files "*.yaml"` = **0 件**。`yml` だけで漏れない
- [x] **U4 新母集団の正規表現を実際の戻り値で検算** — 関数を印字して **7 本**を確認（`architecture` / `build-commands` / `check-skill-skeleton-design` / `comment-guidelines` / `design/2026-05-31-coherence-staleset` / `development-principles` / `hooks`）。`superpowers/` `adr/` は 0 本
- [x] **U5 `\b` が `_` を跨がないか** — 実測。`\bNO_LAUNCHER_READ\b` は語彙にヒットし、`\bG12_NO_LAUNCHER_READ\b` はヒットしない。部分一致で誤免罪される経路は無い
- [x] **U6 B12 の処方（散文化か差し替えか）** — **4 個は差し替え、1 個は散文化**。現行の等価物を grep で実在確認済み（`workspace/research.md`「調査で判明した、計画を変える事実」1.）
- [x] **U7 `/norm-review` の起動可否** — 2026-08-03 ユーザー裁定により**起動しない**。問い: "issue #891 が「起動そのものを省く判断は着手時にユーザーへ確認する」と定めているため伺います。/norm-review を起動しますか？（規範へ判定を足す変更なので起動条件には当たります）" / 回答: "起動しない（推奨）"

## plan-review 結果

- リスク: **高**（判定述語の変更 + ガバナンス検査の母集団拡大 + 網羅性が要件）
- レビュー方式: **独立導出1体**（Step 2b。計画と research を読ませず、issue の WHAT だけからコードで再導出させた）
- エージェント数: 1
- 成果物: `workspace/plan-review-stale-scope-derive.md`（311 行・3 分類あり＝成立）

### 要対処（すべて計画へ反映済み）

- **R1 目的文が偽だった** — 「#819 と #825 の腐りが機構で捕まる」と書いていた。**#825 が腐らせたのは識別子ではなく命題**であり、この検出器はどれだけ射程を広げても命題の真偽を見ない。根拠: `docs/adr/ADR-canonical-source-without-pointer-indirection.md`「#891 は本件のクラスを閉じない」（前サイクルで自分が書いた記述と矛盾していた）。→ 目的節を訂正
- **R2 `snotra-core/CLAUDE.md:21` の `iconCacheSize` が真の腐り** — 撤去済みフロントの語を `Config::icon_cache_cap()`（`snotra-core/src/config.rs:626` に実在）と現在形で並記している。母集団には入れないと決めた面だが、**測って見つけた以上、未修正のまま残すと却下が既知の欠陥を隠す**。→ B14b を新設
- **R3 既存の 0 件検知テストが新ガードの欠落を通す** — `governance-check.test.mjs:709-715` は `snap({})` に対し `findings.length > 0` しか見ず、**どのガードが鳴ったかを区別しない**。B2 のガードを丸ごと消しても緑のまま通る。→ B8 を (a) 配線 / (b) 0 件検知の 2 本立てにし、兄弟母集団を非空に保ったまま測る形を明記
- **R4 2 述語を `|` で 1 本へ畳むと沈黙する** — `scanStaleIdentifiers:1479` が `im[1]` を読むため、キャプチャ群が 1 つずれると `inVocab(undefined)` になり**赤が出ない**。→ B3 に `??` 連鎖を採る理由として明記
- **R5 `.json` について独立導出が別の答えへ達した** — 「`.json` を足し、lock / 生成物 / CI 不在ファイルを除く」方式。finding は 1 件減る（`CLAUDE_PROJECT_DIR`）が、**除外リストは冒頭契約「免除注記の機構を設けない」に正面から当たる**。→ 採らない。両論を B16(d) へ残す

### 軽微

- 独立導出の finding 件数は **8**、わたくしの実測は **9**。差は `.json` を語彙源へ入れたか否かの 1 件（`CLAUDE_PROJECT_DIR`）だけで、**残り 8 件は完全一致した**
- 境界例の採否は 8 面すべてで一致（`docs/adr/` 却下・`docs/superpowers/` 却下・モジュール `CLAUDE.md` 却下・`.github/**.md` 却下・`PERFORMANCE.md` 却下・`capabilities/README.md` 却下・`SETTINGS-DESIGN.md` 採用・ルート規範文書 採用）。**実測値も一致**（モジュール `CLAUDE.md` 真 1 : 外部 3 / `.github/**.md` 9 件全偽 / `PERFORMANCE.md` 8 件全て既存の免責注記内）
- **汚染の開示あり** — 独立導出は `ADR-canonical-source-without-pointer-indirection.md:35-36` を先に読んでおり、そこに既に `docs/adr/` とモジュール `CLAUDE.md` の結論が書かれていた。境界例の採否は独立に測り直されているので、実測値の一致は汚染されていない
- 軸 2（SCREAMING_SNAKE）と軸 3（`.yml`）は**単独では finding を 1 件も動かさない**——価値は軸 1（母集団）と組んだときだけ出る、という独立導出の観察は実測と一致する

### 未検証

- **CI での実測** — `ci.yml` は `pull_request` でのみ起動し、`gh pr create` は未チェック `- [ ]` で block されるため計画内で閉じられない。**B18 で PR 本文のチェックリストへ送る**（`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」）
- **`docs/superpowers/` を母集団へ入れた場合の実測**（独立導出は 221 照合 / 145 finding と測った）——却下が明白なため再照合していない

### 判断

- 実装着手: **可**（要対処 5 件はすべて計画へ反映済み。うち R1・R2 は計画の誤りの訂正）

## 人間レビュー

- [x] 承認済み — 2026-08-03 / 問い: "workspace/plan.md（#891 G-stale-identifiers の射程拡大）を承認して実装へ渡してよろしいですか？" / 回答: "承認する"
