# 調査 — issue #1214: G-module-index の索引行が実質無防備である

## issue の要約

`<crate>/CLAUDE.md`「モジュール構成」の**索引行**を消しても `governance:check` が緑のまま通る。`G-module-index` の逆方向の照合が「索引行に載っているか」ではなく「文書のどこかにバッククォート付きで現れるか」を見ているため。#1210 で `autostart.rs` を足したとき、索引行のほかに開発ルールの散文と `win_registry.rs` の索引行でも同名に触れており、索引行だけを削る変異が exit 0 になった。

issue は 3 案を挙げ、**案 A（`text` → `section` へ絞る）は実測で効かないと自ら却下済み**。判断を求めているのは案 B（索引行の所有関係をパースする）と案 C（死角として宣言し止める）のどちらを採るか。

**判定は 2 段で決まった（2026-09-02・いずれも AskUserQuestion で明示的に選択）。**

1. **まず案 C**（死角として宣言し止める）。案 A は issue が実測で却下済み、案 B は費用が大きい、という前提だった。
2. **敵対的調査（3b）が前提を崩し、案 A＋宣言へ変わった。** issue の案 A 却下は `snotra-core/autostart.rs` **1 例だけ**の実測だった。索引行を持つ全 86 本へ 1 本ずつ削る変異を当て直すと、案 A は現行より **12 件多く捕まえ、現行ツリーへの掃除の費用は 0 件**である。この再測をユーザーへ提示して選び直してもらった。

**後から読む者へ**: 「なぜ issue が却下した案 A を採ったのか」の答えは下の「敵対的調査（3b）の所見と採否」の ⚠️ 節にある。

## 自分で測った実測（複製への変異注入・稼働中のガードは 1 バイトも触っていない）

`scripts/governance/checks/G-module-index.mjs` の `checkModuleIndex` を実物のまま import し、`snotra-core/CLAUDE.md` の**メモリ上の複製**へ変異を当てた。ファイルは読むだけで、作業ツリーは変更していない。

| 測ったこと | 結果 |
|---|---|
| ベースライン `npm run governance:check` | **exit 0**（全検査 passed・検査 23 件） |
| `autostart.rs` の索引行だけを削る変異 → 現行判定（`text` 包含） | ベースライン 0 件 → 変異後も **0 件**（★ 沈黙・死角を再現） |
| 同じ変異 → 案 A（`section` 包含へ絞った版） | ベースライン 0 件 → 変異後も **0 件**（★ 案 A も沈黙） |
| 案 A の「掃除の費用」 | 現行ツリーで新たに赤になるファイル **0 件** |
| 変異後も本文に残る `` `autostart.rs` `` の言及 | **2 件**（散文 1・`win_registry.rs` の索引行 1。issue の経緯 1/2 と一致） |

**issue の 3 つの主張はすべて自分の測定で再現した。** 逐語追認ではない。

## 死角の実寸 — issue が書いていない 2 つの事実

### (1) 索引行が所有しないファイルは 45 件ある（母集団 131 件）

母集団は**実物の `moduleIndexSources`** で取り、節の切り出しも**実物の `sectionOf`（`ending: "heading"`）**を使って数え直した（初版は自前の再帰走査と自前の節切り出しで数えており、見出しの件数が表と食い違っていた——敵対枠 [争点3・独立] が指摘。訂正済み）。

| crate | 実ファイル | 行頭が所有 | 集約行に載る | それ以外 |
|---|---|---|---|---|
| snotra-core | 59 | 31 | 0 | **28** |
| snotra-egui-runtime | 12 | 12 | 0 | 0 |
| src-tauri | 45 | 37 | 7（`commands/` `platform/`） | 1 |
| snotra-settings | 15 | 6 | 9（`tabs/`） | 0 |
| **合計** | **131** | **86** | **16** | **29** |

「それ以外」29 件の内訳（snotra-core 28 + src-tauri 1）はすべて**散文か別ファイルの索引行にしか現れない**:

- `tests.rs` × 13（`autostart/` `config/` `config/io/` `config/location/` `config/migrate/` `config/paths/` `config/schema/` `config/validate/` `indexer/` `indexer/cache/` `indexer/columns/` `indexer/path_env/` `indexer/scan/`）
- `search/tests/*.rs` × 9（`basic` `build` `common` `incremental` `migemo` `mod` `path` `performance` `ranking`）
- `indexer/test_support.rs`
- `search/{build,footprint,path_store,query_plan,scoring}.rs` × 5
- `src-tauri/src/egui_shell/launcher_controller/activation/tests.rs`

### (2) basename 衝突は crate 単位で 4 種 20 件（初版の「14 種 44 件」は判定単位と合っていなかった）

**`checkModuleIndex` は crate ごとに別々の `<crate>/CLAUDE.md` を読む。** ゆえにリポジトリ全体で数えた衝突（14 種 44 件）は判定の単位ではない——crate を跨ぐ同名は、それぞれの crate で別々に言及が要る。**crate 内**で数え直すと:

| crate | crate 内で同名が複数ある basename | 覆うファイル |
|---|---|---|
| snotra-core | 2 種（`tests.rs`×13 / `build.rs`×2） | 15 |
| src-tauri | 2 種（`mod.rs`×3 / `icon.rs`×2） | 5 |
| snotra-egui-runtime / snotra-settings | 0 | 0 |

**`snotra-core/CLAUDE.md` に `` `tests.rs` `` が 1 回あれば、同 crate の 13 枚が緑になる。** これは節の内外という切り口とは独立の天井であり、案 B をどれだけ深くしても basename 方式のままでは閉じない。

**この 2 つは、案 B の費用が issue の見立てより大きいことを示す**——素直に採ると 29 件へ索引行を新設する話になり、うち 22 件はチームが意図して索引していないテストモジュールである。**案 B が最後まで採られなかった根拠はこれである**（案 A への転換とは独立に成立する）。

## 敵対的調査（3b）の所見と採否

`workspace/adversarial-1214.txt`（general-purpose / sonnet 1 体）。**所見はすべて自分で再測して裁定した**（`m6.mjs`・実物の `makeSnapshot` / `moduleIndexSources` / `sectionOf` / `checkModuleIndex`）。

### 壊せた項目（3 件・すべて採用）

| # | 所見 | 自分の再測 | 採否 |
|---|---|---|---|
| 争点1 | 「索引行の削除に沈黙する」は `autostart.rs` から一般化できない。索引行を持つ全 86 本へ 1 本ずつ削る変異を当てると **赤 38 / 緑 48** | **完全一致**（赤 38 / 緑 48。crate 別: core 15/19・egui 8/4・tauri 12/22・settings 3/3） | **採用。** 初版は 4 crate の 1 本ずつしか測っておらず、死角の広さを過大評価していた |
| 争点3・独立 | research.md の「32 件」が表の合計（45）とも後段（29）とも一致しない | **一致**。正しくは 45 件（86 + 16 + 29 = 131） | **採用**（上で訂正済み） |
| 争点3 | 「`tests.rs` の 1 回の言及が 14 枚を緑にする」は不正確。判定は crate ごと | **一致**。crate 内衝突は 4 種 20 件 | **採用**（上で訂正済み） |

### 壊せなかった項目（4 件）

- **争点2**（分類方法）: 4 つの節を全文読み、番号付きリスト・表セル・太字ラップの索引行は 0 件。分類の regex は壊れていない
- **争点3**（母集団）: 131 件・`MODULE_INDEX_CRATES` の 4 crate が `Cargo.toml` members と一致
- **争点4**（reminder）: `scopedFindings` は `governance:check` と**文字どおり同一関数**を呼ぶ。「さらに狭い」でも「別経路で鳴る」でもない
- **争点5**（残余）: `crateSourceFiles` と `moduleIndexSources` は今日 131 = 131 で一致。ただし構造的保証ではない（`lib.mjs` の doc が自ら警告）

### ⚠️（確信の持てない所見）— **これが最も重い**

**案 A（`text` → `section`）は死んでいない。** 同じ 86 本の変異へ当てると **赤 50 / 緑 36** で、現行より **12 件多く捕まえる**。しかも**現行ツリーへ当てたときの掃除の費用は 0 件**である（issue の実測と一致）。

自分の再測でも同じ（赤 50 / 緑 36 / 差 +12 / 費用 0 件）。crate 別の内訳:

| crate | 索引行 | 現行 赤/緑 | 案 A 赤/緑 |
|---|---|---|---|
| snotra-core | 34 | 15 / 19 | **23 / 11** |
| snotra-egui-runtime | 12 | 8 / 4 | **9 / 3** |
| src-tauri | 34 | 12 / 22 | 12 / 22 |
| snotra-settings | 6 | 3 / 3 | **6 / 0** |

**issue の「案 A は実測で消えました」は、#1210 が踏んだ形（同じ節の中の別の索引行が同名に言及している）についてだけ正しい。** その 1 例で却下したため、**費用 0 で 12 件ぶん強くなる面が見落とされていた**。

**機序は自分で裁定した**（採るのは所見であって説明ではない）: 差が出るのは `snotra-core` / `snotra-settings` / `snotra-egui-runtime` で、いずれも**節の外の散文**が basename に触れている crate である。`src-tauri` で差が 0 なのは、言及が節の中に在るためで、#1210 の形はこちらに当たる。

**ただし費用 0 は今日の値であり、将来の下界ではない**——節の外の散文だけが触れているファイルが現れれば、そのとき新たに赤くなる。

## 残る死角 — 編集時 reminder も同じ述語を使う（検算済み）

issue の「あれも同じ判定を使うので、同じ理由で鳴りません」は正しい。

`scripts/governance/edit-findings.mjs:134` が `checkModuleIndex(snapshot, [crate.name])` を実物のまま呼び、結果を編集ファイルへ帰属させている（同ファイル `//!` が「**判定を再実装しない**」と宣言）。ゆえに逆方向の述語は編集時経路と `governance:check` で共有され、片方だけが索引行の消失を見ることはない。

**受容する残余（案 A を入れた後も残る分）**: 索引行の消失・不記載は、**その basename が同じ「モジュール構成」節の他所に現れるときだけ**検知されない（案 A の前は「同じ `CLAUDE.md` の他所」だった。狭まるが消えない）（実測では索引行 86 本のうち 48 本がこの側で、38 本は今日でも赤くなる）。「索引行の消失は誰も検知しない」は**偽である**。ファイルそのものは `G-module-linkage`（`mod` 宣言の到達性）と順方向（索引に書かれた名前の実在）に守られたままで、失われるのは**索引の網羅性の一部**である。

## 関連ファイル・シンボル（すべて grep で実在確認済み）

| パス | 役割 |
|---|---|
| `scripts/governance/checks/G-module-index.mjs` | 機序の正本。`MODULE_INDEX_CRATES` / `moduleIndexSources` / `checkModuleIndex` |
| `scripts/governance/checks/G-module-index.test.mjs` | 46 行・6 ケース。集約行のベア名列挙が誤検出しないことを固定するケースが既に在る |
| `scripts/governance/edit-findings.mjs` | 編集時 reminder。`checkModuleIndex` をそのまま呼ぶ |
| `scripts/governance/checks/G-module-linkage.mjs` | `mod` 宣言の到達性。「G-module-index が塞がない足を塞ぐ」と自称 |
| `scripts/governance/checks/G-adr-file-names.mjs` | `docs/adr/` のファイル名が `ADR-<slug>.md` 形で、本文の見出しと stem が一致するか |
| `scripts/governance/checks/G-adr-citations.mjs` | `ADR-<slug>` の短縮引用が実在の ADR を指すか。母集団は governance 文書 + skills + 製品ソース + ADR 同士 |
| `docs/adr/ADR-governance-meta-demotion.md` | 段を増やす判断の正本 |
| `docs/adr/ADR-stale-identifier-detector-scope.md` | **形の先例**——「採った述語と理由はコード側、却下した案は ADR」 |
| `.claude/rules/safety-nets.md` | フォールトインジェクションの作法（`scripts/**` を触ると自動配送） |

## 再利用できる既存パターン

1. **`ADR-stale-identifier-detector-scope` の分担**: 採用した述語の理由は検査ファイルのヘッダコメント、**却下した案は ADR**。#1214 は「案 A を実測して却下」「案 B を費用で却下」という否定の知識を持つので、この分担がそのまま当たる。
2. **検査ヘッダの自己申告スコープ**: `G-module-index` のヘッダは既に「意図的な弱化」を宣言しており、逆方向の直前のコメントも 2026-08-17 の実測を持つ。**追記する場所は既に在る**——新しい様式を作らない。
3. **`G-module-linkage` の「受容する残余」節**: 残余を箇条書きで列挙し、向き（沈黙側 / 赤に倒れる側）を明記する書式。

## 技術的制約

- **検査ヘッダに数え上げを書かない。** ルート `AGENTS.md`「検証の作法」——「数え上げは偽になる時点が確定している。足すたびに腐る。数ではなく正本（分岐そのもの）を指す」。上の「44 件」「14 種」「29 件」は**ファイルを 1 枚足せば偽になる**。日付つきの実測として ADR（凍結された歴史）へ置き、検査ヘッダには**性質**（basename 照合ゆえ同名のファイル群は 1 回の言及で覆われる）だけを書く。
- **全称表現を検査ヘッダへ持ち込まない。** 「索引行の消失を見る層は無い」は現時点の主張であり、`G-module-linkage` や順方向が別の足を守っている事実と併記しないと過剰に強い。
- **ADR を新設するなら `ADR-<slug>.md` 形で、H1 見出しの stem と一致させる**（`G-adr-file-names`）。引用は `G-adr-citations` が実在照合する。
- **`docs/adr/` は凍結された歴史である**（`ADR-adr-frozen-history`）。後から本文を書き換えず、追記で読み替えを与える。
- **`.md` と `scripts/` を触るので `npm run governance:check` が要る**（`AGENTS.md` カテゴリ F。PR では CI の governance-check job が常時実行）。
- **`scripts/**` を触るので `.claude/rules/safety-nets.md` が自動配送される。** 案 A は**検知器の走査元を絞る**変更（`AGENTS.md`「ガバナンス機構自身の…母集団の切り出しの変更」に当たる）なので、**フォールトインジェクションは省けない**——強くなった側と残る死角の両方を測る（`plan.md` Phase 2）。
- **セーフティネットの変更はユーザーの合意が要る**（ルート `CLAUDE.md` 最重要ルール 2）。案 A＋宣言の選択は取得済み。**規範文書 3 枚（`AGENTS.md` / `docs/hooks.md` / `.claude/skills/implement/SKILL.md`）を同じ PR で直すかは未取得**（`plan.md` の未確定欄）。

## 調査時点の未解決（すべて `workspace/plan.md` で決着済み）

| 問い | 決着 |
|---|---|
| ADR を新設するか | **新設する**（`plan.md` 未確定欄）。案 B の費用却下と、**案 A を 1 例で却下してから覆した**経緯は再導出が困難 |
| 宣言の追記先 | ヘッダの「basename 包含方式」段落＋逆方向直前のコメント（`plan.md` Phase 1・5） |
| 規範文書へ波及するか | **する。** grep で 3 枚を特定した——`AGENTS.md:73` / `docs/hooks.md:107` / `.claude/skills/implement/SKILL.md:77` がいずれも「`.rs` を編集すれば索引漏れの reminder が鳴る」を**無条件で**主張しており、案 A を入れても偽のまま残る（`plan.md` Phase 6。**この PR に含めるかはユーザーの裁定待ち**） |
| テストへ死角を固定するか | **しない。** 固定するのは**強くなった側**だけにする。死角側を `toEqual([])` で固定すると仕様へ昇格し、将来閉じるときにテストを消す作業が要る（`plan.md` 未確定欄） |
