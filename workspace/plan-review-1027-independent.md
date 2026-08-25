# 独立導出レビュー: #1027

- **対象 issue**: #1027（`LoadOrScanStats.total_ms: u128` → `total: Duration`）
- **導出者**: 独立導出レビュア。`workspace/plan.md` / `research.md` / `adversarial-1027.txt` は未読、
  検索はすべて `git grep -- … ':!workspace'`（`workspace/` は untracked ゆえ `git grep` の母集団に元々入らない）
- **状態**: 測定済み・追記中

## 0. 数え上げの母集団（何を含め、何を除いたか）

**含めた**: `git grep -n "total_ms\|LoadOrScanStats\|set_index_load_stats_ms\|index_load_unattributed"`
をリポジトリ全追跡ファイルへ当て、`workspace` を除外。トークン単位の完全一致ではなく部分文字列で
引いたので `pub use` 経由の re-export 行も母集団に入る（実測 0 件——`LoadOrScanStats` の
re-export は無く、`snotra_core::indexer::LoadOrScanStats` を直接使う）。

**意味で除外した 1 件**: `snotra-egui-runtime/src/renderer.rs:183` の `total_ms={:.2}` は
**paint trace（tessellate/raster）の別概念**であり、`LoadOrScanStats` とは無関係。
→ この除外は所見 A-3（幽霊識別子）の根拠と同じ事実の裏表である。

**凍結された歴史として編集対象から外した**（AGENTS.md「ドキュメント参照」= ADR は凍結、
`docs/superpowers/` は #589 で非規範化）:
`docs/superpowers/plans/2026-08-09-rescan-in-situ-instrument.md`（130/195/362/974/979 行）、
`docs/superpowers/plans/2026-08-09-scan-all-seen-conditional.md:70`、
`docs/superpowers/plans/2026-08-10-explicit-scan-only.md`（591/641/645/670/672 行）、
`docs/superpowers/specs/2026-08-09-rescan-in-situ-instrument-design.md`（45/63/97/101 行）、
`docs/superpowers/specs/2026-08-10-explicit-scan-only-design.md:22`。
**実装者が grep して「直して」しまう事故を防ぐため、明示的に列挙して除外を宣言する。**

## 1. 導出した変更ファイル一覧

| ファイル | シンボル・行 | 変更の内容 | 根拠 |
|---|---|---|---|
| `snotra-core/src/indexer.rs` | `LoadOrScanStats.total_ms: u128`（455） | `pub total: Duration` へ | 型宣言。`#[derive(Debug, Clone, Copy)]`（422）ゆえ `Duration: Copy` で derive は不変 |
| 〃 | 778（cache-hit 枝） | `total_ms: …as_millis()` → `total: total_started.elapsed()` | 生成 1/3 |
| 〃 | 819（cache-miss 枝） | 同上 | 生成 2/3 |
| 〃 | 872（`config_dir` 不在枝・`load_or_scan_with_stats` 本体） | 同上 | 生成 3/3 |
| 〃 | struct doc 417–421・`cache_save_ms` doc 442 | 散文中の `` `total_ms` `` を `` `total` `` へ | doc コメント。改名で腐る（機構は届かない・所見 A-3） |
| 〃 | 3590 のテスト doc | 引用文「`cache_load_ms` と `total_ms` の間に…」の追随 | 上の struct doc の逐語引用 |
| `snotra-core/tests/memory_footprint.rs` | 319（`s.total_ms` を println へ） | `s.total.as_millis()` | **issue の「消費 2 か所」に入っていない** |
| 〃 | 331（`s.total_ms.saturating_sub(…)`） | `s.total.as_millis().saturating_sub(…)` | `Duration` に `u128` を渡す `saturating_sub` は無く**確実に compile-fail** |
| 〃 | 298・309・431・433・488 の散文 | `LoadOrScanStats` の名は不変ゆえ変更不要（確認のみ） | — |
| `src-tauri/src/main.rs` | 185（debug_assertions の eprintln の `s.total_ms`） | `s.total.as_millis()`（表示境界） | 消費 1/2 |
| 〃 | 195 `startup::set_index_load_stats_ms(result.stats.total_ms as u64)` | `set_index_load_stats(result.stats.total)`。**`as u64` キャストが消える** | 消費 2/2 |
| `src-tauri/src/startup.rs` | `Timeline.index_load_stats_ms: Option<u64>`（277） | `Option<Duration>` へ（名は ⚠ 所見 B-1） | 保持する型 |
| 〃 | `Timeline::set_index_load_stats_ms(&mut self, total_ms: u64)`（314–316） | 引数を `Duration` へ | issue 要件 3 |
| 〃 | 自由関数 `set_index_load_stats_ms(total_ms: u64)`（494–496） | 同上 | 〃 |
| 〃 | `to_json` の `index_load_unattributed_ms` ブロック（405–417） | `(Some(measured), Some(inner)) => json!(to_ms(measured) as i64 - to_ms(inner) as i64)` ＝ **ms への切り落としが `to_ms` 1 本に揃う** | issue 要件 3（「`to_json` の中で 1 度だけ ms へ落とす」） |
| 〃 | doc コメント 394–404 | **前提 2（両者とも切り捨て）が構造で消えたことを反映**。前提 1（包含）と `bench-startup.ps1` の `>= 0` の根拠は残す | issue 要件 4 |
| 〃 | テスト 832–838 `index_load_unattributed_is_the_gap_against_load_stats` | `t.set_index_load_stats_ms(42)` → `…(Duration::from_millis(42))`。期待値 8 は不変 | 算術を pin する既存検出器 |
| `scripts/lib/SnotraStartupContract.psm1` | 63–65 の doc（「内側の `LoadOrScanStats.total_ms` の差」）・162–164 の失敗メッセージ | 前提 2 が消えた記述へ追随＋フィールド名 | issue 要件 4。**`>= 0` の判定ロジック（161）は残す**（前提 1 が残るため） |
| `snotra-core/CLAUDE.md` | 219 行「`cache_load_ms` と `total_ms` の間に処理を足すときは…」 | `` `total` `` へ | モジュール索引の散文。**G-stale-identifiers の母集団外**（所見 A-3） |
| `PERFORMANCE.md` | 1883（表の見出し「ロード（壁時計・`total_ms`）」）・2177–2178 | **据え置きを推奨**（改名しない）。根拠は所見 A-7 の `digest_ms` 先例 | `PERFORMANCE.md:36`「既存の記述へ遡及して補完しない」＋ 先例（A-7） |
| `PERFORMANCE.md` | 2704（`SNOTRA_EGUI_PAINT_TRACE` の `total_ms`） | **変更しない**——paint trace の別概念（§0 の意味による除外と同一） | 意味で別物 |

**「1 度だけ ms へ落とす」の射程に注意**: 起動計器の経路では `to_json` の `to_ms` が唯一の丸めに
なる。ただし `main.rs:185` の debug eprintln と `memory_footprint.rs:319/331` は
**それぞれ自分の表示境界**であり、そこで `as_millis()` を呼ぶのは原則に適合する。
「リポジトリ全体で `as_millis()` の呼び出しが 1 回になる」と読ませないこと。

## 2. 導出した検証コマンド

| コマンド | なぜ要るか |
|---|---|
| `cargo fmt --all -- --check` | カテゴリ A 必須。`.rs` を触る |
| `cargo check --workspace` | カテゴリ A 必須。crate 境界をまたぐ公開型の変更ゆえ `snotra` の compile-fail が移行漏れ検出器になる。**ただし `tests/` 配下の統合テストはコンパイルしない**——`memory_footprint.rs` はここでは捕まらない |
| `cargo clippy --workspace --all-targets -- -D warnings` | カテゴリ A 必須。**`--all-targets` が `snotra-core/tests/memory_footprint.rs` を初めてコンパイルする**。上表の 2 件（319/331）を捕まえる唯一のカテゴリ A 行 |
| `cargo test -p snotra-core` | カテゴリ A 必須（`snotra-core` を変更）。`#[ignore]` の `memory_footprint` も**コンパイルはされる** |
| `cargo test -p snotra` | カテゴリ A 必須（`src-tauri` を変更）。`startup.rs` のユニットテスト群（`to_ms_truncates_toward_zero` / `index_load_unattributed_is_*`）を通す |
| `cargo doc --workspace --no-deps --document-private-items` | カテゴリ A 必須。**doc コメントを触るので intra-doc link 切れ検査が要る**（CI 発火・hook 非発火。`.claude/rules/comments.md` のトリガー）。`LoadOrScanStats::total_ms` を intra-doc link で名指している箇所があれば改名でリンク切れになる |
| `npm run governance:check` | カテゴリ F 必須。`snotra-core/CLAUDE.md`・`PERFORMANCE.md` などガバナンス文書を触る（#587） |
| `npm run test:powershell` | `SnotraStartupContract.psm1` を編集するため（Pester）。**ただし所見 A-4 の通り、散文/メッセージの改名を `Tests.ps1` は観測しない** |
| `npm run smoke:startup` | **`index_load_unattributed_ms` の生成元そのものを書き換える**。実起動で payload を作り、キー網羅と `>= 0`（残る前提 1 の外部 pin）を端から端まで通す唯一の経路 |
| `npm run bench:startup`（任意） | 出力値が変わらないことを実データで見たい場合。`scripts/bench-startup.ps1:166` が `unattributed` を列に出す |

**要らない行**: カテゴリ B（TS 変更なし）、カテゴリ C の `smoke:egui`（ウィンドウ生成・hotkey・
表示経路に触れない）、カテゴリ D（UI 視覚に触れない）、カテゴリ E（`.githooks/` に触れない）。

## 3. 壊しうる不変条件と検知手段

### 3-1. `index_load_unattributed_ms` の出力値は変わらない（根拠つき）

- 現行: `inner = total_started.elapsed().as_millis()`（`Duration::as_millis` = ns/10^6 の**床**）を
  生成点で確定し、`as u64` で運ぶ。`to_json` は `to_ms(measured) as i64 - inner as i64`。
- 変更後: 同一の `total_started.elapsed()` を `Duration` のまま運び、`to_json` で
  `to_ms(inner)`。`to_ms` は `(d.as_nanos() / 1_000_000) as u64`（`startup.rs:456-458`）＝**同じ床**。
- **同一の `Duration` に同一の床関数を当てるので、出力はビット単位で一致する。** 変わるのは
  「いつ床を取るか」だけ。
- 検出器: 既存の `index_load_unattributed_is_the_gap_against_load_stats`（`startup.rs:832`。
  `set_index_load_stats_ms(42)` → 期待 8）が移行後も同じ算術を pin する。
  `to_ms_truncates_toward_zero`（703）が床であることを pin する。
- **前提 2 の消え方の精密な形**: 両辺が**同一の丸め関数を同一の式の中で**通るようになるので、
  残るのは「その関数が単調であること」だけになる（床でも四捨五入でも単調ゆえ、事実上消える）。
  「切り捨てであること」を要求する必要がなくなる、というのが構造的な差である。

### 3-2. 移行漏れを捕まえる検出器と、その死角

| 検出器 | 捕まえるもの | 見ない経路 |
|---|---|---|
| `cargo check --workspace`（下流の compile-fail） | `src-tauri/src/main.rs` の 2 か所 | **`tests/` 配下の統合テスト**（`memory_footprint.rs`）・doc コメント・散文 |
| `cargo clippy --workspace --all-targets` / `cargo test -p snotra-core` | `memory_footprint.rs:319/331` | doc コメント・散文・`.ps1` / `.psm1` |
| `cargo doc --workspace --no-deps` | intra-doc link で書かれた `[…::total_ms]` の切れ | **バッククォートだけの名指し**（`` `total_ms` ``）は解決対象でなく沈黙 |
| `npm run governance:check`（G-stale-identifiers） | — | **今回の全出現が母集団外**（次項） |
| `npm run smoke:startup` の contract 検査 | payload のキー網羅・`>= 0` | 値そのものの正しさ（下限が無い・psm1 の doc が明記） |

### 3-3. 改名で腐る散文の識別子 — 機構は届かない（`scripts/governance/checks/` を実読して判定）

`G-stale-identifiers.mjs` を読んだ結果、**`total_ms` の全出現がこの検出器の射程外**である。理由は二層:

**一次: 文書母集団の外**（`scripts/governance/lib.mjs:661`・`G-stale-identifiers.mjs:26-44`）
- 母集団は (a) `.claude/**` の規範散文 (b) `docs/**` から `docs/adr/` と `docs/superpowers/` を
  除いたもの (c) `STALE_EXTRA_DOCS = ["SPEC.md", "CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"]`。
- ゆえに **`PERFORMANCE.md` は入っていない**（(c) に無く、`docs/` 配下でもない）。
- **`snotra-core/CLAUDE.md` も入っていない**——(c) の `CLAUDE.md` は**ルートのみ**であり、
  検出器のヘッダーが「**モジュール `CLAUDE.md` は入れない**」と明示している（外部語彙の密度が理由）。
- **`.rs` の doc コメントは母集団外**（#975 で拡大を測って却下）——`indexer.rs:417/418/442` は見られない。
- **`.psm1` / `.ps1` は `.md` ではないので文書母集団外**——`SnotraStartupContract.psm1:63/163` も見られない。

**二次: 仮に母集団内でも、幽霊識別子が免罪する**
- 「現行語彙」は production ソースの**非コメント本文**（`currentVocabulary`。`stripRustComments` は
  コメントだけを落とし、**文字列リテラルは残す**）。
- `snotra-egui-runtime/src/renderer.rs:183` の `"SNOTRA_EGUI_PAINT win={} … total_ms={:.2} …"` は
  production の `.rs` の文字列リテラルであり、`\btotal_ms\b` に一致する。
- ゆえに **`LoadOrScanStats.total_ms` を消しても `total_ms` は現行語彙に残り続ける**——
  paint trace の**別概念**が同じ綴りを供給するため。AGENTS.md の撤去トリガー行が言う
  「実体が別名で在る旧名（幽霊識別子）」の実例である。
- さらに三重に: `` `LoadOrScanStats.total_ms` `` のようなドット修飾形は `staleTarget` が
  `seg.includes(".")` で `null` を返すため、そもそも判定対象にならない。

**他の G-check を読んだのか**: 深読みしたのは `G-stale-identifiers` だけである。残りの 22 検査は
パスの実在（`G-references`）・見出しの着地（`G-heading-refs` / `G-near-heading-refs` /
`G-folded-heading-refs`）・表の照合（`G-skill-table` / `G-build-commands` / `G-ci-table` /
`G-architecture-table` / `G-edit-findings-table`）・モジュール索引と `mod` 宣言
（`G-module-index` / `G-module-linkage`）・SPEC 番号・rules glob・clippy/lints 設定・hook 写像であり、
**散文の識別子を見るのは `G-stale-identifiers` ただ 1 つ**である。根拠は同ファイルのヘッダーの
自認——「**G-references が見るのはパスの実在までで、識別子の実在は誰も見ていなかった**」。

**結論**: 散文の腐りを捕まえる機構は**この変更に対して 1 つも存在しない**。危ういのは
`PERFORMANCE.md:1883/2177`・`snotra-core/CLAUDE.md:219`・`indexer.rs` の doc 417/418/442/3590・
`SnotraStartupContract.psm1:63/163` の 5 ファイル 9 箇所で、**目視の数え上げが唯一の手段**である。
（`workspace/` を除外した `git grep -n "total_ms"` の出力を、上の表の除外宣言と 1 行ずつ突き合わせること。）

### 3-4. `.psm1` の散文を直しても Pester は観測しない

`SnotraStartupContract.Tests.ps1:170-176` の「非負性」Context は
`(Test-Payload -Data $data) -join ' ' | Should -Match 'index_load_unattributed_ms が負'` で、
**キー名の部分文字列しか見ない**。`:163` のメッセージ中の `LoadOrScanStats.total_ms` を
`LoadOrScanStats.total` へ直しても、壊しても、**このテストは緑のままである**。
→ `npm run test:powershell` は「壊していないこと」の確認にはなるが、「直したこと」の検出器ではない。

## 4. `AGENTS.md`「条件別チェック」の該当行

| 該当行 | この変更で実際に必要なもの |
|---|---|
| **関数・型を新規定義／改名／導入** | 呼び出し元の列挙。LSP の `findReferences` が既定だが、無い環境なので `git grep` へ落とし**母集団を §0 で宣言**した。**旧 API の削除は下流の compile-fail を移行漏れ検出器に**（`cargo check --workspace` ＋ **`--all-targets` の clippy**）。`total_ms` と `total` を**同時に持たせない**（1 タスクに束ねる。残すと `dead_code` と導出 2 箇所が生じる） |
| **`.rs` のコメントの見出し参照（正準形）とその参照先を変更／ガバナンス文書を変更** | `npm run governance:check`（`snotra-core/CLAUDE.md`・`PERFORMANCE.md` を触るため）。**編集時の reminder が鳴らないことを「緑」と読まない** |
| **件数 N・上限パラメータ・導出の入力を変更** | `total` は `index_load_unattributed_ms` の**導出の入力**である。下流全段（`to_json` → trace payload → `SnotraStartupContract.psm1` → `bench-startup.ps1:166` の列）を辿った（→ §1・§3-1）。**永続化する消費者は無い**（trace は追記ログで、この値を on-disk 形式へ書く経路は無い） |
| **文書に事実の写しを増やす変更** | 前提 2 の記述は `startup.rs:394-404`（正本）と `SnotraStartupContract.psm1:63-65`（写し）の 2 か所に在る。**片方だけ直すと写しが腐る**。数え上げの母集団に **PR 本文**を含めること（squash で main の commit message になる） |
| **レビュー指摘へ修正（fix-forward）を当てた** | 該当したら、指摘を出した枠組みを**修正差分にも**再実行してから閉じる |
| **`Option`/フラグ/enum variant など分岐を決める値の出所を変更** | **非該当**。`total` は分岐に使われない（分岐に使われるのは `cache_hit` で、これは触らない） |
| **永続形式・識別子/キー形式を変更** | **非該当**（→ 所見 A-5 の根拠） |

## 5. 入れ忘れやすいもの（issue のとおり実装するだけでは漏れるもの）

1. **`snotra-core/tests/memory_footprint.rs` の 2 か所**（最重要）。issue が「消費は `main.rs` の
   2 か所だけ」「`/simplify` の報告は 4 か所だったが実測は 2 か所である」と**明示的に否定した**
   ものが実在する。`cargo check --workspace` は届かないので、**カテゴリ A を省略すると
   `cargo clippy --all-targets` を打つまで気づかない**。
2. **`indexer.rs` の struct doc（417–421）と `cache_save_ms` の doc（442）**。前者は
   「`cache_load_ms` と `total_ms` の間に処理を足すときは必ずここに並ぶ項目を作ること」という
   **生きた規範**であり、`snotra-core/CLAUDE.md:219` にその写しがある。**両方直す。**
3. **`SnotraStartupContract.psm1` の doc（63–65）**。issue は「ハーネスの `>= 0` 検査は残す」としか
   言っていないが、**その隣の散文が前提 2 を説明している**。判定は残し散文は直す、という非対称。
4. **`startup.rs` の `index_load_stats_ms` フィールドと 2 つの関数名**（⚠ 所見 B-1）。
5. **`main.rs` の `as u64` キャストの消滅**（機械的だが列挙に載る）。
6. **凍結文書を直さない判断を明示する**（§0）。grep 結果に出てくるので、放置の理由を残さないと
   次のレビューで「漏れ」として再提起される。
7. **`cargo doc` を手で打つ**。doc コメントを触るのに **PostToolUse hook は沈黙する**
   （`.claude/rules/comments.md` のトリガー）。

## 所見

### 要対処

- **A-1. issue の「消費 2 か所」は偽である。** `snotra-core/tests/memory_footprint.rs:319` と `:331`
  が `s.total_ms` を消費している。`:331` は `saturating_sub` で他の `*_ms`（`u128`）と混ぜて
  算術しており、型変更で**必ず compile-fail する**。`/simplify` の「4 か所」が正しかった。
  issue 自身が「この issue に着手する者は数え直すこと」と書いているので、その指示に従った結果。
- **A-2. `cargo check --workspace` は `memory_footprint.rs` に届かない。** `tests/` 配下の統合
  テストは `cargo check` の既定ターゲットに入らない。検出器として書くなら
  `cargo clippy --workspace --all-targets -- -D warnings` か `cargo test -p snotra-core`。
  「compile-fail を移行漏れ検出器に使う」という issue の方針は正しいが、**どのコマンドが
  どこまで見るか**を計画に書かないと死角が残る。
- **A-3. 散文の識別子を捕まえる機構は 1 つも無い**（→ §3-3）。母集団外が一次要因、
  `renderer.rs:183` の同名別概念（幽霊識別子）が二次要因。**目視の数え上げしかない。**
- **A-4. `.psm1` の散文修正は Pester に観測されない**（→ §3-4）。「テストが緑」を根拠にしない。
- **A-7. 同型の腐りが既にリポジトリに残っている（先例・実測）。** `digest_ms` は
  `LoadOrScanStats` から**削除済み**のフィールドである（`docs/superpowers/plans/2026-08-10-explicit-scan-only.md`
  Task 5 で撤去）。`git grep -n "digest_ms" -- '*.rs'` の出力は
  **`src-tauri/src/startup.rs:380`（`//` コメント）1 件のみ**——production の識別子としては
  1 つも残っていない。にもかかわらず散文には残っている:
  - `src-tauri/src/startup.rs:380`（`.rs` のコメント＝ G-stale-identifiers の母集団外）
  - `PERFORMANCE.md:2178`（母集団外）
  - `snotra-core/CLAUDE.md` にも `docs/` にも無い（＝**母集団内には最初から出現しない**）

  **これは A-3 の「機構が届かない」の実証である**——推測ではなく、同じクラスの腐りが
  **既に検出されないまま残存している**。`total_ms` を改名すれば、その 2 か所と同じ場所に
  同じ形の腐りが生まれる。
  **同時に、`PERFORMANCE.md` を遡及改名しない先例でもある**（⚠ B-2 の裁定材料）。
- **A-8. `PERFORMANCE.md` と `.psm1` の編集に PostToolUse hook は 1 つも検査を割り当てない**（実読）。
  `.claude/hooks/post-edit.mjs` の `selectChecks`（132–177 行）が checks を積むのは
  `.rs` / `Cargo.toml` / `tauri.conf.json`・`config.toml` / `.claude/hooks/` /
  `.githooks/` / `.claude/lsp/` / `rust-analyzer.toml` **だけ**である。
  `.md` と `.psm1` は 1 つも当たらない → **沈黙は「何も走らなかった」**（`AGENTS.md` の規範どおり）。
  `PERFORMANCE.md` / `snotra-core/CLAUDE.md` は `npm run governance:check` が事後に見るが、
  §3-3 のとおり `total_ms` はその判定の射程外である。`.psm1` は `npm run test:powershell` だけで、
  §3-4 のとおり散文を観測しない。

### 軽微

- **A-5. `/persistence-check` は非該当**（根拠を明示すること）。`LoadOrScanStats` は
  `#[derive(Debug, Clone, Copy)]` のみで serde を持たず（`indexer.rs:422`）、`index.bin` にも
  `config.toml` にも載らない。trace payload の JSON キー名（`index_load_unattributed_ms` 等）も
  変わらないので、ハーネス側の互換も壊れない。
- **A-6. `to_ms` は `startup.rs` の private fn である**。`snotra-core` からは使えないので、
  `memory_footprint.rs` と `main.rs:185` の表示は `as_millis()` を自前で呼ぶことになる。
  これは重複ではなく「別の表示境界」である（→ §1 末尾）。

### ⚠（確信の持てない所見・要裁定）

- **⚠ B-1. `_ms` 接尾辞が嘘になる問題。** `Timeline.index_load_stats_ms: Option<Duration>`、
  `set_index_load_stats_ms(d: Duration)` は名前が中身と食い違う。issue は改名を要求していない。
  - 改名する場合の追随先: `Timeline` のフィールド、`Timeline::set_index_load_stats_ms`、
    自由関数 `set_index_load_stats_ms`、`main.rs:195` の呼び出し、
    `startup.rs:835` のテスト。**JSON の出力キー `index_load_unattributed_ms` は変えない**
    （ハーネスと `bench-startup.ps1:166` が読む契約）。
  - **確かめていないこと**: 計画側が改名を意図しているかどうか。裁定は計画側に委ねる。
    ただし「`_ms` の名で `Duration` を運ぶ」形を残すなら、それは #1027 が消そうとした
    「丸めの所在が名前から読めない」問題の再生産に近い。
- **⚠ B-2.（裁定済み・据え置きを推奨）`PERFORMANCE.md` の `:1883` / `:2177-2178` は改名しない。**
  規約（`PERFORMANCE.md:30-40`）を読んだ結果:
  - 「**この文書を「今も支えている値」と「歴史」に分けない**」——凍結文書ではなく
    時系列の採否ログである（ゆえに「凍結だから触らない」という理由は使えない）。
  - しかし「**適用は、これ以降に新しく書く記録に限る。既存の記述へ遡及して補完しない**」の
    精神と、**A-7 の `digest_ms` 先例**（削除済みフィールドが `:2178` に残ったまま
    誰も直していない）が、遡及改名しない運用を実証している。
  - 加えて `:1883` は「そのとき `total_ms` という名で測った値」の記録であり、改名すると
    測定記録が測定当時の対象を指さなくなる。
  - **⚠ 残る不確かさ**: これは規約の明文ではなく、明文（provenance の遡及補完の禁止）と
    先例からの導出である。計画側が「散文の腐りは全部潰す」方針を採るなら覆りうる。
    **どちらに倒すにせよ、`digest_ms` の残存と揃った扱いにすること**（片方だけ直すと非対称になる）。
- **B-3.（取得済みの grep 出力で裁定済み・⚠ から格下げ）** 最初の `git grep` は部分文字列一致
  ゆえ `total_ms` の全出現を既に取っており、**角括弧の intra-doc link 形（`[…total_ms]`）は 0 件**
  ——すべてバッククォート形か素のコードである。struct 名 `LoadOrScanStats` は不変なので、
  リンクが切れうるのは**フィールドを名指す形だけ**であり、それが存在しない。
  `cargo doc --workspace --no-deps --document-private-items` は実装時の backstop として
  カテゴリ A 必須のまま残す（doc コメントを触る以上、他の理由でも要る）。
- **⚠ B-4. `snotra-egui-runtime/src/renderer.rs:183` の `total_ms` を今回改名すべきか。**
  幽霊識別子を消すという意味では筋が通るが、**#1027 の範囲外**であり、
  `SNOTRA_EGUI_PAINT_TRACE` の出力形式（`PERFORMANCE.md:2704` が記録）を壊す。
  **やらない方に倒すべき**だが、「幽霊識別子が残る」ことは受容する残余として記録すべきである。

### 未検証

- **`cargo doc` の実行**。ブランチはコードを 1 行も変えていないので、改名後の intra-doc link
  切れは実測できない（→ ⚠ B-3）。**`cargo doc` / `cargo clippy` / `cargo test` を
  1 つも実行していない**——コード変更禁止の制約下では、compile-fail の予測（A-1・A-2）は
  型の読みからの導出であって実測ではない。ただし `Duration` に `u128` を渡す
  `saturating_sub` は存在しないので、A-1 の compile-fail は型定義から確実に言える。
- **`git grep` の母集団の完全性**。LSP の `findReferences` を使っていない（環境に無い）。
  `git grep` は文字列一致ゆえ**同名の別物を拾い**（`renderer.rs` で実際に拾った・§0 で除外）、
  **re-export 経由を落としうる**。`LoadOrScanStats` の `pub use` は実測 0 件だが、
  `git grep` 自身でそれを確かめているので循環している。
- **`smoke:startup` / `bench:startup` を実行していない。** 出力値が変わらないという §3-1 の
  主張は算術からの証明であり、実起動での A/B 実測ではない。**実装後に 1 回測ること**を推奨する
  （変更前の payload を先に取っておかないと、後からは A 側を再現できない）。
- **`load_or_scan_with_stats_in` の 3 生成点が同じ `total_started` を使っているか**は
  778 / 819 は同じ関数（752 で `let total_started`）、872 は別関数（`load_or_scan_with_stats` の
  `None` 枝・850 で別に `let total_started`）と読んだが、**行を跨いだ束縛の追跡は目視である**
  （LSP が無いため）。実装時にコンパイラが裁定する。
