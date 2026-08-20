# 独立導出 — #1154（観測結果の残し方 ＋ 折れた正準形の検知器）

作成: 2026-08-20 / ブランチ `chore/measurement-canon-and-fold-detector` / 他者の計画・`workspace/` 配下・`.superpowers/` は未読

---

## 0. 結論（先に数字）

- **今日のリポジトリに折れた正準形の見出し参照は 33 件在る。** 全件が対象を解決し（33/33）、**1 件は着地しない**（＝折れが本物の腐った参照を隠している）。
- 折れ方は**物理 2 行に跨る形しか無い**（33/33 が 2 行）。3 行以上に跨る形は 0 件。
- 内訳: `.rs` 24 / `.mjs` 4 / `.md` 2 / `.psm1` 2 / `.ps1` 1。**うち 2 件はガバナンス機構自身の中に在る**（`scripts/governance-check.mjs:122` と `scripts/governance/checks/G-near-heading-refs.mjs:39`）。
- ゆえに**検査を 1 本足すと初日から赤である**。33 件の畳み直しは検知器と同じ PR に入る（これが最大の漏れ候補）。
- 検査ファイル以外に**必須の変更は 2 つだけ**である: `scripts/governance/evidence.mjs`（照合件数の証跡）と **PR 本文の `## governance manifest delta`**。登録・母集団供給・0 件検知はいずれも既存の機構が自動で覆う（下に `file:line` で示す）。

---

## 1. 母集団と述語（自分で決めた数え方）

### 1.1 母集団

`G-heading-refs` と**同一**にした。理由は「死角そのものを塞ぐ」検知器だからで、母集団がずれれば塞いだつもりの穴が別の場所に開く。

- 走査元 = `allHeadingRefDocs(snapshot)`（`scripts/governance/lib.mjs:572-576`）＝ md の腕 + `.rs` の腕 + コメント記法を持つファイルの腕
- 走査する行 = `refScanLines(text, doc, findings)`（同 `lib.mjs:244-246`）＝ 散文はフェンスの外側の全行、スクリプトはコメント行だけ
- 実測: **走査文書 258 件 / 連続行ブロック 961 件 / 隣接行対 63,800 件**

### 1.2 「折れている」の述語

連続する走査行の**極大 run**（＝ブロック）を丸ごと 1 本の文字列へ繋ぎ、繋いだ後に `HEADING_REF`（`lib.mjs:168`）が当たる件数が、**行ごとに数えた件数より増えた**ものを折れとする。増えた分のうち、行境界を跨いでいる match だけを finding にする。

繋ぐときに継続行の**先頭からその言語のコメント標識を落とす**。これは実測上の必須条件である:

- 落とさずに繋ぐと **15 件**しか出ない（`/// ` が `` ` `` と `「` の間に挟まって `HEADING_REF` の `\s*` に当たらない）
- 落として繋ぐと **33 件**。**半分以上がこの一手で決まる**

判定の閾値は**「対象が解決すること」だけを課し、「着地すること」は課さない**。

- 着地必須にすると **32 件**になり、`snotra-core/tests/path_query_cost.rs:265` が落ちる。**そこが唯一の本物の腐り**であり、検知器がいちばん取りこぼしてはいけない 1 件である
- 誤検出の抑止は解決条件だけで足りている——**63,800 の隣接行対に対して偽陽性 0 件**（全 33 件を目視で確認した。すべて筆者が正準形として書いた参照が行長で折れたもの）
- `G-near-heading-refs` が「着地必須」を採ったのは、散文の引用と参照を分ける必要があったからである（同ファイル `G-near-heading-refs.mjs:25-36` の窓幅表）。**こちらは対象が `.md` パスか `/skill` の綴りであることが既に強い濾過になっている**ので、同じ理由が当たらない

### 1.3 窓幅の決定（ブロック連結 対 2 行窓）

| 数え方 | 件数 | 跨いだ行数の分布 |
|---|---|---|
| 2 行窓（隣接行対 63,800） | 33 | — |
| ブロック連結（極大 run 961） | 33 | すべて 2 行 |

**両者は今日一致する。** それでもブロック連結を推す——2 行窓は「ラベルが 3 行に跨る形」に対して原理的に沈黙する（どちらの境界でも完全な match が組み上がらない）のに対し、ブロック連結にはその死角が無く、費用は同じである（961 ブロックの連結は 63,800 の対よりむしろ安い）。2 行窓を採るなら、**3 行以上の折れを宣言する死角として検査のヘッダに書く**こと。

---

## 2. 折れの形（全形の列挙）

正準形は `` `<対象>` §<番号>? 「<ラベル>」 `` である。物理改行が入りうる位置は 3 か所しかなく、実測はそのうち 2 つに当たった。

| 形 | 折れる位置 | 今日の件数 | 例 |
|---|---|---|---|
| **A** | 対象のバッククォートの内側（`` `docs/ `` ⏎ ``hooks.md` ``） | **0** | 実例なし（`docs/comment-guidelines.md`「日本語の折返し」が「コードスパンを行またぎさせない」と既に禁じている） |
| **B** | 閉じバッククォートと `「` の間 | **20** | `snotra-core/tests/path_query_cost.rs:3` |
| **C** | ラベル `「…」` の内側 | **13** | `snotra-core/src/indexer.rs:60` |
| **D** | ラベルが 3 行以上に跨る | **0** | 1.3 の実測 |

**A も検知できる**（ブロック連結後に対象のバッククォート対が復元されるため）。今日 0 件なのは規範が先に効いているからであって、述語が見ないからではない。

---

## 3. 全 33 件（file:line 一覧）

`lands=false` の 1 件は、折れを直した瞬間に `G-heading-refs` が赤くする。

| # | file:line | 形 | 参照 | lands |
|---|---|---|---|---|
| 1 | `docs/design/2026-05-31-coherence-staleset.md:17` | B | `` `.claude/rules/governance-docs.md`「ガバナンス文書の参照と命名のルール」 `` | true |
| 2 | `PERFORMANCE.md:1530` | B | `` `snotra-core/CLAUDE.md`「incremental cache とパスクエリの非互換」 `` | true |
| 3 | `snotra-core/src/indexer.rs:60` | C | `` `PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」 `` | true |
| 4 | `snotra-core/src/indexer.rs:479` | C | 同上 | true |
| 5 | `snotra-core/src/indexer.rs:1030` | C | `` `PERFORMANCE.md`「採用: 保存が返した派生データを cache-miss がそのまま使う」 `` | true |
| 6 | `snotra-core/src/index_tree.rs:62` | B | `` `PERFORMANCE.md`「採用: 表示名を文字列アリーナで持つ」 `` | true |
| 7 | `snotra-core/src/index_tree.rs:148` | C | `` `PERFORMANCE.md`「採用: `kana_lower_names` も文字列アリーナで持つ」 `` | true |
| 8 | `snotra-core/src/index_tree.rs:181` | C | `` `PERFORMANCE.md`「採用: 表示名を文字列アリーナで持つ」 `` | true |
| 9 | `snotra-core/src/search/build.rs:77` | C | `` `PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」 `` | true |
| 10 | `snotra-core/src/search/build.rs:192` | B | `` `PERFORMANCE.md`「索引の常駐の内訳」 `` | true |
| 11 | `snotra-core/src/search/build.rs:462` | B | `` `PERFORMANCE.md`「採用: `kana_lower_names` も文字列アリーナで持つ」 `` | true |
| 12 | `snotra-core/src/search/footprint.rs:206` | C | 同上 | true |
| 13 | `snotra-core/src/search/path_store.rs:11` | B | `` `PERFORMANCE.md`「`target_path` のフォルダ木接頭辞共有」 `` | true |
| 14 | `snotra-core/src/str_arena.rs:5` | C | `` `PERFORMANCE.md`「採用: 表示名を文字列アリーナで持つ」 `` | true |
| 15 | `snotra-core/tests/path_query_cost.rs:3` | B | `` `snotra-core/CLAUDE.md`「モジュール構成」 `` | true |
| 16 | `snotra-core/tests/path_query_cost.rs:10` | B | `` `PERFORMANCE.md`「索引の常駐の内訳」 `` | true |
| **17** | **`snotra-core/tests/path_query_cost.rs:265`** | B | `` `docs/development-principles.md`「判定を持たない道具を層に数えてよい」 `` | **false** |
| 18 | `src-tauri/src/egui_shell/launcher_controller.rs:1828` | C | `` `docs/development-principles.md`「検証の層と、層と層の隙間」 `` | true |
| 19 | `src-tauri/src/egui_shell/layout.rs:403` | C | `` `src-tauri/CLAUDE.md`「可視性を変える操作はイベントループスレッドに閉じてある」 `` | true |
| 20 | `src-tauri/src/egui_shell/mod.rs:92` | B | `` `SPEC.md`「4.5 最大列挙数」 `` | true |
| 21 | `src-tauri/src/egui_shell/results_view.rs:76` | B | `` `docs/development-principles.md`「構造的設計原則と強制の階梯」 `` | true |
| 22 | `src-tauri/src/egui_shell/results_window.rs:111` | C | `` `src-tauri/CLAUDE.md`「可視性を変える操作はイベントループスレッドに閉じてある」 `` | true |
| 23 | `src-tauri/src/egui_shell/window_coordinator.rs:190` | B | `` `src-tauri/CLAUDE.md`「モジュール構成」 `` | true |
| 24 | `src-tauri/src/main.rs:614` | C | `` `src-tauri/CLAUDE.md`「ウィンドウ生成の制約」 `` | true |
| 25 | `src-tauri/src/state.rs:37` | C | `` `src-tauri/CLAUDE.md`「可視性を変える操作はイベントループスレッドに閉じてある」 `` | true |
| 26 | `src-tauri/src/working_set.rs:5` | B | `` `src-tauri/CLAUDE.md`「working set の能動回収」 `` | true |
| 27 | `.claude/hooks/lsp-config.mjs:8` | B | `` `docs/hooks.md`「Claude Code の RA インスタンスと hook の分担」 `` | true |
| **28** | **`scripts/governance/checks/G-near-heading-refs.mjs:39`** | B | `` `.claude/rules/governance-docs.md`「既に消滅した節の名前を正準形で書かない」 `` | true |
| **29** | **`scripts/governance-check.mjs:122`** | B | `` `.claude/rules/governance-docs.md`「序数で他を指してはならない」 `` | true |
| 30 | `scripts/lib/SnotraTraceInvariants.psm1:8` | B | `` `src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」 `` | true |
| 31 | `scripts/lib/SnotraWindowColors.psm1:17` | B | `` `docs/development-principles.md`「構造的設計原則と強制の階梯」 `` | true |
| 32 | `scripts/manual-smoke.ps1:63` | B | `` `docs/adr/ADR-folder-location-display-surface.md`「却下 6」 `` | true |
| 33 | `scripts/plan-review-ledger.test.mjs:2` | B | `` `.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」 `` | true |

（`file:line` は折れの**開始行**。#33 は `*.test.mjs` だがフィクスチャではなく本物のヘッダコメントであり、`linesOfComments`（`lib.mjs:201-237`）がコメント行として拾う。）

---

## 4. B 側（検知器）— 触るファイルとシンボル

### 4.1 必須（新規）

| ファイル | 置くもの |
|---|---|
| `scripts/governance/checks/G-folded-heading-refs.mjs` | `export const id`（ファイル名と一致必須）/ `export function run(snapshot, ctx)` → `ctx.record("foldedRefs", …)` / `export function scanFoldedHeadingRefs(snapshot, docs)`（`{findings, checked}` を返す純関数）/ 継続行の剥がし写像（言語ごと）/ 却下案 3 つの記録 |
| `scripts/governance/checks/G-folded-heading-refs.test.mjs` | red（折れた fixture）/ green（1 物理行の fixture）/ 判定対象外の不混入（`.md` の連続する箇条書き・見出し行を繋がないこと） |

**名前**: `G-<name>` 形（連番を持たない・`.claude/rules/governance-docs.md`「名前はテーマ・目的が決まった時点で、何を指すか分かる形で付ける」）。`G-folded-heading-refs` を推す。

**継続行の剥がしは lib へ出さない。** `lib.mjs` 冒頭（`lib.mjs:1-7`）の運用規則が「複数の検査が import しない限り検査ファイルへ置け」と定めており、消費者はこの 1 本だけである。

### 4.2 必須（既存の変更）

| ファイル | 触る場所 | 理由（`file:line`） |
|---|---|---|
| `scripts/governance/evidence.mjs` | `assembleEvidence` のテンプレートへ `${ev.foldedRefs}` を 1 語 | **消費点が母集団の SSOT である**（同ファイル `evidence.mjs:18-46` が「`REQUIRED_RECORDS` のような必須キー一覧を持たない」と宣言）。テンプレートへ書かないと `ctx.record` の呼び忘れが**誰にも捕まらない**（`ADR-facade-evidence-static-imports` が実測: `G-heading-refs` から `record` を外すと exit 0 で `npm test` も全緑だった）。#497 の「差分ゼロと照合していないを区別する証跡」もここに載る |
| **PR 本文** | `## governance manifest delta` へ `+G-folded-heading-refs` を逐語で | `manifest()` の `checks` は `buildChecks` 由来（`scripts/governance-manifest.mjs:23-33`）、`undeclared` が PR 本文の逐語一致だけを見る（同 `:59-65`）。CI の当該 step は `.github/workflows/ci.yml:111-126`。**リポジトリを grep しても出てこない要求である** |

### 4.3 「要らない」ことをコードで示す

| 触らない場所 | 根拠 |
|---|---|
| `scripts/governance/registry.mjs` | `checkModulesFrom` が `checks/` を `readdirSync` して自動登録する（`registry.mjs:19-37`）。忘れうる登録行が存在しない。id とファイル名の不一致は同 `:31` が throw する |
| `scripts/governance-check.mjs` の `buildChecks` | 走査元 `allRefDocs` は既に `ctx` へ載っている（`governance-check.mjs:133, 148`）。新検査は同じ母集団なので**供給の追加が要らない** |
| `scripts/governance-check.mjs` の `runAll` 0 件検知 | md / `.rs` / スクリプトの 3 腕ぶんが既に在る（`governance-check.mjs:157-161`）。新検査は同じ 3 腕を食うので**4 本目は要らない**（足すと同じ母集団に 2 本の検知が乗る） |
| `scripts/governance-check.mjs` の再輸出ブロック | 「名前を足す前に、その名前を読む消費者が実在するか確かめること」（`governance-check.mjs:86-88`）。facade からこの検査を読む消費者は無い。足すと #1094 が外した遮蔽が戻る |
| `.claude/hooks/post-edit.mjs` | `selectChecks` は `scripts/**` に検査を 1 本も割り当てない（`post-edit.mjs:132-178`。`.rs` / `Cargo.toml` / `.claude/hooks/` / `.githooks/` / `.claude/lsp/` / `rust-analyzer.toml` のみ）。**hook 側の変更は不要**——ただし裏返しとして、**検査ファイルを書いている間の沈黙は「何も走らなかった」である**。`npm test` と `npm run governance:check` は手で叩く |
| `.github/workflows/ci.yml` | governance-check job は `node scripts/governance-check.mjs` を叩くだけ（`ci.yml:73-74`）。検査の増減に無関心 |
| `scripts/governance-check.test.mjs` のカナリア | 凍結しているのは「`checks/` の名前が facade へ戻ってこないこと」（同 `:143-147`）であり、検査の本数ではない |
| `vitest.config.ts` | `scripts/**/*.test.mjs` を glob で拾う（`vitest.config.ts` の `include`）。列挙ではないので新しいテストは自動で入る |

---

## 5. A 側（規範）— 触るファイルと**既存の節**

裁定の 4 項目を面へ割り付けると、**3 項目は既存節への追記**であり、新設が要るのは A-1 の受け皿だけである。

### A-1 出所欄の必須化 → `PERFORMANCE.md` 冒頭

- **既存の節は無い。** `PERFORMANCE.md` は `# パフォーマンス最適化プレイブック` の直後が見出しの無い散文（WebView2 期の注記＋着手順序の番号付きリスト）で、最初の `##` は `## ビルドプロファイル最適化の知見`（実測: `grep -n "^## " PERFORMANCE.md` の 1 本目）。
- ゆえに**新しい `##` 節を H1 直後へ挿す**ことになる。**そのとき既存の散文が新節の中へ流れ込む**ので、既存散文にも見出しを与える必要がある（例: `## 着手の順序`）。これは #1154 が指示していない構造変更なので、**やる／やらないを実装者が独断で決めない**こと。
  - 逃げ道: 新節を H1 直後ではなく `## ビルドプロファイル最適化の知見` の**直前**へ置けば既存散文は無見出しのまま残る。ただし「冒頭」の裁定からはわずかにずれる。
- **副作用は無い**: `PERFORMANCE.md` は `governanceDocs`（`lib.mjs:461-470`）にも `STALE_EXTRA_DOCS`（同 `:588`）にも入らず、`sectionOf` を当てる検査も無い。`G-heading-refs` はこの文書を**アンカー源**として読むだけなので、見出しが増えることは既存参照を壊さない（着地の候補が増える方向）。

### A-2 「支えている値」と「歴史」を分けない → **ファイル変更 0**

裁定が「分けない」である以上、実装は**現状維持**である。書くとすれば A-1 の新節に 1 文（「本書は時系列の採否ログであり、現行値と歴史を面で分けない」）を添える程度で足りる。**A-2 のために別ファイルを作らない・`ADR-adr-frozen-history` 相当の凍結扱いを持ち込まない。**

### A-3 コメントに何を残すか → `docs/comment-guidelines.md`「歴史メモの様式」への**追記**

- 既存節「歴史メモの様式」（`docs/comment-guidelines.md:89-98`）は既に **「実測値には条件を添える」（日付・エントリ数・試行回数など）。条件のない数値は再検証できない**（同 `:96`）を持つ。**これは A-1 のコメント側の双子である。** #1128 が出荷した形（害の説明 ＋ 正準形の指し 1 物理行 ＋ 桁が設計理由そのものであるときだけ規模感）は、この箇条の隣へ足すのが正しい。**新設ではない。**
- 「1 物理行」の側は**既存節「日本語の折返し」**（同 `:56-65`）が既に持つ: 「**文途中で物理改行を入れない**（1 段落 1 行）」「**とくにコードスパン（バッククォートで囲んだ識別子・コマンド）を行またぎさせない**——rustdoc の描画は soft line break を跨げるので壊れず、壊れるのは検索だけである（だから気づかれない）」。**新設の検知器はこの既存規範の部分集合を機械化するものである**——検査のヘッダはこの節を正準形で指し、規範の側は「折れは `governance:check` が赤にする」を 1 文で受けるのが筋。
- 参考: 「第一原則」の書く価値があるもの（同 `:17`）に既に「**過去の事故**（実測値・再現条件・issue 番号つきの経緯）」が在る。

### A-4 引用されうる数値は測った時点でログへ着地させる → `AGENTS.md` の**既存行**

- 該当行は **`AGENTS.md:63`**: 「調査・測定のための一時的な足場（script・workflow・env フック）を新設 | 撤去条件（どの issue が閉じたら消すか）と撤去対象の列挙を、**その成果物自身の doc へ書く**…」
- 裁定どおり引き金が「計装の撤去」なので、**この行のトリガー欄を「新設」から「新設・撤去」へ広げ**、参照先欄へ「撤去する前に、その計装が生んだ**引用されうる数値**が `PERFORMANCE.md` へ出所つきで着地しているか確かめる（`40〜95 ms` は着地しなかったため、計装の撤去と同時に出所が消えた・#1154）」を足す。
- **`docs/development-principles.md`「観測と計装」（同ファイル `:223`）にも同型の受け皿が在る**が、裁定は `AGENTS.md` の表と名指ししているので、そちらへは書かない（写しを増やす行為そのものが `AGENTS.md`「文書に事実の写しを増やす変更」に当たる）。

### A 側で追加が要る 1 か所（裁定の外だが機構の帰結）

- **`.claude/rules/governance-docs.md:18`** は「G-heading-refs が見るのは正準形だけである。散文形…は検知されない（助詞が挟まった近傍形は G-near-heading-refs が拾う）」と、**死角の一覧を名乗っている**。折れが機構で拾われるようになったのにここが黙っていると、規範を読んだ人が「折れても誰も見ていない」という**逆向きに古い**理解を持つ。1 節（または既存箇条への括弧書き）で新検査を名乗ること。

---

## 6. 所見（3 分類）

### 要対処

1. **【最大】33 件の畳み直しが検知器と同じ PR に入る。** 検査を足した瞬間 exit 1 になる（§3 の全件一覧）。順序は「畳み直し → 検査」でなければならない——**逆順にすると（検査を先に入れると）その間 main が赤い窓を持つ**。PR を分けること自体は問題ない。24 件が `.rs` なので PostToolUse の fmt / clippy / crate test が走る（内訳は `snotra-core` 15 件 → `core-test`、`src-tauri` 9 件 → `tauri-test` の **2 crate**。`selectChecks` の `rel.startsWith` 分岐・`post-edit.mjs:143-146`）。
   - 行長の心配は要らない: リポジトリに `rustfmt.toml` が無く（`ls` 実測）、rustfmt は既定でコメントを折り返さない（`docs/comment-guidelines.md:63` が同じことを実測で書いている）。**畳み直した行を fmt が再び折ることは無い。**
2. **`snotra-core/tests/path_query_cost.rs:265` は畳み直した瞬間 `G-heading-refs` が赤にする。** 参照先 `docs/development-principles.md:176` は `**判定を持たない道具を層に数えてよい。**` で始まる行だが、**行頭に箇条書き記号が無い**ため `ANCHOR_SPECS` の太字リード（`lib.mjs:391` の `` /^\s*(?:[-*]|\d+[.)])\s+\*\*(.+?)\*\*/ ``）に当たらず、アンカーにならない。**折れがこの腐りを 1 件隠していた**（issue が主張する「二重の死角」の実物）。選択肢は 3 つで、**どれを採るかは実装者が独断で決めず提示すること**:
   - (a) ラベルを当該行を含む `###` 見出しへ指し替える
   - (b) `docs/development-principles.md:176` を箇条書き項目にしてアンカー化する（他の参照への影響を要確認）
   - (c) バッククォートを外して散文化する（`.claude/rules/governance-docs.md`「既に消滅した節の名前を正準形で書かない」の逆——こちらは消滅ではないので (c) は最後の手段）
3. **ガバナンス機構自身が 2 件折れている**（`scripts/governance-check.mjs:122` / `scripts/governance/checks/G-near-heading-refs.mjs:39`）。**新設する検査ファイルのヘッダコメントで同じ折れをやらないこと**——自分自身を赤にする。
4. **継続行のコメント標識を剥がさない実装にすると、検出は 33 → 15 件へ落ちる**（§1.2 実測）。この差は「取りこぼしても赤くならない」向きなので、実装後に**剥がしを外す変異で件数が落ちることまで測る**（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）。
5. **`PERFORMANCE.md` の A-1 節を H1 直後へ置くと、既存の無見出し散文が新節へ流れ込む**（§5 A-1）。構造変更の可否は裁定に無いので、実装前に確認する。
6. **`.claude/rules/governance-docs.md:18` の死角一覧を更新する**（§5 末尾）。ここを直さないと、機構を足した当の PR が「折れは検知されない」という偽の記述を残す。
7. **PR 本文へ `+G-folded-heading-refs` の宣言が要る**（§4.2）。CI の `governance manifest delta` step でしか現れず、ローカルの `npm run governance:check` では鳴らない。

### 軽微

8. **`docs/build-commands.md:163` の括弧内の列挙**（「参照実在・モジュール索引・スキル表・SPEC 番号・rules glob・コマンド写像・見出し参照の着地」）へ折れ検知を**足さない**ことを推す。`.claude/rules/governance-docs.md`「機構の実装の詳細（述語の種類・件数・分岐の列挙）を散文へ写さない」に当たり、#984 が同じ形で並行マージにより偽になっている。既存の記述は網羅を主張していない。
9. **却下済み 3 案（行跨ぎ正規表現 / 照合件数の ratchet / 同じ数値が他所に現れたら赤）の記録先は新検査のヘッダコメントで足りる。** 先例は `G-near-heading-refs.mjs:25-36` の窓幅表。**新しい ADR を切らない**（`docs/adr/` は射程外であり、否定の知識は検査ファイルが持てる）。
10. **`G-near-heading-refs` 側の折れ（助詞が挟まった近傍形が行を跨ぐ）は今回の射程外**。宣言する死角として検査ヘッダへ 1 行書くのが安い（`detector-scope-only-as-tight-as-needed`）。
11. **隣接する腐り 1 件を見つけた（#1154 とは別件）**: `scripts/governance/lib.mjs:489` と `scripts/governance/checks/G-module-index.mjs:25` が `domains.test.mjs` を指しているが、**このファイルは #1152（`74ae45fc`「錨の層を撤去し…」）で削除済み**である（`ls scripts/governance/domains*` が not found）。`.mjs` は `isRefTargetSpelling`（`lib.mjs:171`）に当たらないため `G-heading-refs` の視界外で、**折れ検知器を足しても拾われない**。別 issue 候補。
12. **`docs/design/2026-05-31-coherence-staleset.md:17` の折れは blockquote（`> `）の中に在る。** 継続行の剥がしを md で `^[ \t]*(?:>[ \t]*)*` にしないと落ちる（実測でこの 1 件だけがその形）。

### 未検証

13. **偽陽性の将来率を測っていない。** 今日 0 件（63,800 隣接行対）だが、md で「`` `docs/foo.md` `` で終わる行」の直後に「`「…」` で始まる無関係な行」が来れば偽陽性になる。構造上ありうるのは連続する箇条書き項目・表のセル・引用の折返しで、**今日そのパターンが 0 件であることしか測っていない**。
14. **CRLF チェックアウトでの挙動を測っていない。** `refScanLines` → `linesOutsideFences` は `text.split("\n")` なので各行末に `\r` が残る（`lib.mjs:135`）。形 B（閉じバッククォートと `「` の間で折れる）は `\s*` が `\r` を吸うので当たるが、**形 A（対象の内側で折れる）は `` `docs/\rhooks.md` `` になって `resolveRefTarget` が解決に失敗しうる**。実装時に連結前後で `\r` を落とすことと、その fixture を 1 本置くことを推す。今日の露出は形 A が 0 件ゆえ 0。
15. **`npm test` / `npm run governance:check` の現状を走らせていない**（検査対象を変更しながら検査を走らせない・`AGENTS.md`。本レビューは読み取りのみで、実装は行っていない）。33 件という数は `scripts/governance/lib.mjs` の公開関数を import して測った独立スクリプトの結果であり、**`governance:check` 本体の出力ではない**。
16. **A-1 の「出所欄」の具体的な列（日付・機体・規模・版・標本数）が既存 40 件の日付つき記述とどう並ぶかを見ていない。** 裁定は「既存は遡及補完しない」なので実装は新規分のみだが、**新旧が同じ面に並ぶ見え方**は実物を書いてみないと判断できない。
