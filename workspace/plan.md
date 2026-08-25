# plan — issue #1178: `LoadOrScanStats` の残る `*_ms` も `Duration` で運ぶ

調査は `workspace/research.md`、敵対枠の全文は `workspace/adversarial-1178.txt`。

## 目的

`snotra-core` の生成時に行っている ms への丸めを 8 か所すべて撤去し、`Duration` のまま運ぶ。
丸めは表示境界（`main.rs` の `eprintln!` / `memory_footprint.rs` の `println!`）にだけ残す。
`startup.rs` の `//!`「丸めは表示境界でだけ行う」に対して内側の crate だけがその外に居る、
という #1027 由来の非対称を解消する。

## 受け入れ条件

1. `snotra-core/src/indexer.rs` に `.elapsed().as_millis()` が **0 か所**である
2. `LoadOrScanStats` の 7 フィールド（`total` を含む）がすべて `Duration` である
3. `[index-load]` の stderr 出力が**バイト単位で変わらない**
4. `cargo clippy --workspace --all-targets -- -D warnings` と `cargo test -p snotra-core` が緑
5. 旧識別子（`hash_ms` / `cache_load_ms` / `cache_read_ms` / `scan_ms` / `sort_ms` /
   `cache_save_ms` / `upgrade_save_ms` / `\bread_ms\b`）が、**生きた層**（`snotra-core/` ·
   `src-tauri/` · `PERFORMANCE.md` · `snotra-core/CLAUDE.md`）に 0 件である
   （`docs/superpowers/` は凍結ゆえ母集団外）

## 決定事項（実装者は再導出しない）

| # | 決定 | 理由 |
|---|---|---|
| D1 | 型変更と同時に `_ms` 接尾辞を落とす | 型が `Duration` なのに名前が `_ms` だと名前が嘘になる。#1181（`total_ms` → `total`）が先例 |
| D2 | `[index-load]` の format 文字列を 1 バイトも変えない | 挙動不変の宣言。**ハーネスは実は読んでいない**（3b 所見 A）ので規約でしか守られない |
| D3 | `memory_footprint.rs` の残余は**案 B**（生 `Duration` で引いて 1 回丸める） | `startup.rs` の `//!`「厳密に検算するのは生 ns だけである」。残余が丸め屑を吸わなくなる |
| D4 | 残余は**符号つき**で出す（`total` と和の差を i128 ns で取り ms へ） | 素の `-` は panic する（3b 所見 C）。飽和は**同じファイルの既存規範に反する**（下記） |
| D5 | 20,000 件の治具は据え置き、コメントを書き直す | 規模の根拠（`as_millis()` の量子化）が型変更で消える（3b 所見 B）。**黙って据え置くと理由だけが腐る** |
| D6 | テスト関数名もフィールド改名に追随する | 配線を名指ししているため |
| D7 | commit type は `refactor(core)!:` | `pub` フィールドの改名は破壊的変更。#1181 が先例 |

### D4 の裁定（`plan-review` Step 1 の概念 grep で出た）

`Duration` 同士の `-` は `checked_sub().expect("overflow when subtracting durations")` ゆえ
**負に振れると panic する**。今日の `u128::saturating_sub` は無音で 0 ms へ潰す。選択肢は 3 つ:

| 案 | 挙動 | 裁定 |
|---|---|---|
| 飽和（`Duration::saturating_sub`） | 今日と同一（無音の 0） | **却下** |
| panic | バグで即死 | **却下**（`#[ignore]` の手動計測に新しい失敗モードを持ち込む） |
| **符号つき** | 負を負として印字する | **採用** |

**印字の形は「符号つき f64 の ms」とする**（`delta_mib` と同じ）。**i128 の ns 差を整数除算で
ms へ落としてはならない**——ゼロ方向へ切り捨てるので `-0.4 ms` の残余が `0` と印字され、
**D4 が消そうとした無音がそのまま戻る**（`delta_mib` が f64 なのはこの理由である。
独立レビューの ⚠️ もここを指した）。

**却下の根拠は同じファイルの既存規範である**——`memory_footprint.rs:131` の `delta_mib` に
「**`saturating_sub` で丸めない**——飽和させると減少が `0.00 MiB` に化け、『何も起きなかった』と
読める嘘になる（**実測で踏んだ**）」と書いてある。しかも当の残余ブロック（`:312`）自身が
「残余が負へ振れて `saturating_sub` に黙って潰される……**符号が飽和側にしか出ないので
気づけない**」と自己申告している。**このファイルは同じ誤りを一度学習しており、残余の行だけが
その学習の外にある。**

**案 B（D3）がこの決定を不可避にする**——生 `Duration` で引く形にする以上、減算の意味論を
選ばずには書けない。ゆえに範囲外へ逃がさず、この差分で決める。実装は `delta_mib` と同じ
「符号つきの差分」の作りに揃える（新しい語彙を持ち込まない）。

**残余（この PR では塞がない）**: 負の残余を**赤にする**検知器は置かない。`#[ignore]` の
手動計測ゆえ CI は走らせず、印字を読む人間が唯一の読み手である。

### 改名表

| 旧 | 新 | 型 |
|---|---|---|
| `LoadOrScanStats::hash_ms` | `hash` | `Duration` |
| `LoadOrScanStats::cache_load_ms` | `cache_load` | `Duration` |
| `LoadOrScanStats::cache_read_ms` | `cache_read` | `Duration` |
| `LoadOrScanStats::scan_ms` | `scan` | `Duration` |
| `LoadOrScanStats::sort_ms` | `sort` | `Duration` |
| `LoadOrScanStats::cache_save_ms` | `cache_save` | `Duration` |
| `LoadCacheResult::read_ms` | `read` | `Duration` |
| `LoadCacheResult::upgrade_save_ms` | `upgrade_save` | `Option<Duration>` |
| `Scanned::scan_ms` | `scan` | `Duration` |
| `Scanned::sort_ms` | `sort` | `Duration` |
| `load_cache_reports_upgrade_save_ms_only_when_it_upgrades_a_legacy_format` | `..._reports_upgrade_save_only_when_...` | — |
| `load_or_scan_with_stats_reports_upgrade_save_ms_in_cache_save_ms` | `..._reports_upgrade_save_in_cache_save` | — |

## 変更ファイルと対象シンボル

| ファイル | 対象 |
|---|---|
| `snotra-core/src/indexer.rs` | `LoadOrScanStats` / `LoadCacheResult` / `Scanned` / `scan_and_sort_timed` / `load_or_scan_with_stats_in` / `load_or_scan_with_stats` / `upgrade_legacy_cache_in` / `finish_legacy_read` / `load_cache_in` / 上記 2 テスト + `load_or_scan_with_stats_does_not_scan_on_cache_hit` 系 |
| `src-tauri/src/main.rs` | `#[cfg(debug_assertions)]` 下の `eprintln!`（`s.hash.as_millis()` 等へ） |
| `snotra-core/tests/memory_footprint.rs` | フェーズ内訳 `println!` と残余算式、`report_scan_all_cost` の doc |
| `PERFORMANCE.md` | 1837（`scan_ms`）/ 2112（`cache_read_ms`・`cache_load_ms`）/ 2177（`cache_load_ms`）+ 2180 の残余の意味論注記 |
| `snotra-core/CLAUDE.md` | 「indexer.rs の索引更新の契機」冒頭の `cache_load_ms` |
| `src-tauri/src/startup.rs` | **416-417 行のコメントのみ**（下記） |

**`startup.rs` を対象へ入れた理由**（独立レビューの要対処 1・一次証拠で再照合済み）:
416-417 行が逐語で「**主張はこの式に限る**: `LoadOrScanStats` の他の `*_ms` は今も
`snotra-core` の中で生成時に丸めている」と書いており、**本 issue はまさにその丸めを消す**。
**受け入れ条件 5 の終端 grep では捕まらない**——この一文は `hash_ms` 等の具体的識別子を持たず
`*_ms` というグロブで書かれているため。「触らない」節の除外根拠（`set_index_load_stats_total`
以降は端から端まで `Duration`）は `total` についての話であって、この一文には当たらない。

**触らない**: `total`（#1027 済み）/ `startup.rs` · `to_ms` · `scripts/` の PS ハーネス /
`index_load_unattributed_ms`（起動 JSON の出力キー）/ `docs/superpowers/`（凍結）/
`renderer.rs` の `total_ms` 等（paint trace の別計器）/ `SPEC.md`（識別子 0 件・実測）。

## 不変条件と異常系

1. **`upgrade_save` の variant が「昇格 save を通ったか」の判定である**——時間の値は判定に
   使わない。`Some` を作るのは `upgrade_legacy_cache_in` の 1 行だけ（この構造は不変）
2. **`cache_read` は常に、`cache_save` は cache-hit 枝で `cache_load` の内数**——フェーズの和に
   足さない。二重計上すると残余が負へ振れる
3. **cache-hit 枝は `scan` / `sort` が `Duration::ZERO`**（リテラルであって計測値ではない）
4. **`index.bin` のオンディスク形式に影響しない**——`LoadOrScanStats` はディスクへ出ない。
   ゆえに `INDEX_CACHE_VERSION` のバンプは不要（`/persistence-check` の発火条件に当たらない）
5. **異常系**: 残余が負に振れる状況は構造上ロジックバグのときだけ（丸めでは負にならない
   ——floor は劣加法的なので `Σfloor(phase) ≤ floor(total)`）。D4 により**負は負として印字される**

## 実装順序（フェーズ）

### Phase 1 — `indexer.rs` の型と改名

- [x] `LoadOrScanStats` の 6 フィールドを改名表どおり `Duration` へ変える
- [x] `LoadCacheResult` の `read` / `upgrade_save` を追随させる
- [x] `Scanned` の 2 フィールドを追随させる。**分解時に shorthand を使わないこと**——
      呼び出し 3 箇所（`scan_and_sort_timed` 自身・`load_or_scan_with_stats_in` の cache-miss 枝・
      `load_or_scan_with_stats` の `None` 枝）はいずれも同スコープに `scan: &[ScanPath]` を持ち、
      `let Scanned { entries, scan, sort } = ..` と書くと**新しい `scan: Duration` が
      `scan: &[ScanPath]` を同名シャドウする**（コンパイルは通るが将来の事故の形。
      独立レビューが実測）。`let Scanned { entries, scan: scan_took, sort: sort_took } = ..`
      のように明示的に別名で束ねる
- [x] 生成 8 か所（`indexer.rs` の 751 / 755 / 779 / 785 / 809 / 828 / 1218 / 1293）から
      `.as_millis()` を落とす。`unwrap_or(0)` は `unwrap_or(Duration::ZERO)` へ
- [x] `cargo clippy --workspace --all-targets` を**移行漏れの列挙器として**回し、
      compile-fail が指す箇所を**すべて書き出す**（`cargo check` は使わない——`tests/` を見ない）。
      **この時点では緑にならない**——消費側の追随は Phase 2 であり、
      「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」に従って Phase 1+2 を
      **1 コミットに束ねる**（`-D warnings` 下の中間状態を残さない）

### Phase 2 — 消費側の追随

- [x] `main.rs` の `eprintln!` を `.as_millis()` 付きへ。**format 文字列は変えない**（D2）
- [x] `memory_footprint.rs` のフェーズ内訳を `.as_millis()` 付きへ
- [x] 残余算式を案 B へ（D3）: 生 `Duration` の和を作り、**符号つき**で差を取ってから
      ms へ落とす（D4。`delta_mib` と同じ作りに揃える）
- [ ] `cargo test -p snotra-core` が緑であること

### Phase 3 — 意味論が変わったコメント・doc の書き直し

- [x] `LoadOrScanStats` の各フィールド doc から ms 前提の言い回しを外す
      （「走ったが 1 ms を切った」等）。**原則「variant が判定・値は計器」は残す**
- [x] `LoadCacheResult::upgrade_save` の doc を同様に書き直す
- [x] 20,000 件治具のコメント（`indexer.rs:3724-3728`）を D5 のとおり書き直す
      ——「`as_millis()` の量子化を跨がせるため」という根拠が消えたことを明示する
- [x] `memory_footprint.rs` の残余ブロックのコメントを案 B・D4 の意味論へ書き直す
      （`saturating_sub` に触れた記述が現存するので、符号つきへ変わったことを明記する）
- [x] `src-tauri/src/startup.rs:416-417` の「他の `*_ms` は今も生成時に丸めている」を書き直す。
      **消すのではなく、残る主張へ弱める**——`LoadOrScanStats` は全フィールドが `Duration` に
      なるが、「生成側が丸めた `Duration` を渡す形が残り、そこに検査が届かない」という
      `total` の doc の残余は依然として真である（issue が明示するとおり、この変更でも減らない）
- [ ] `cargo doc --workspace --no-deps --document-private-items` を**手で**走らせる
      （intra-doc link 切れは hook が沈黙する・`.claude/rules/comments.md`）

### Phase 4 — 散文の追随

- [x] `PERFORMANCE.md` の 3 か所を新識別子へ
- [x] `PERFORMANCE.md:2180` の「現在の残余は 0〜1 ms である」の近傍へ、その値が**案 A の
      意味論で測られたもの**であることを明記する
- [x] `snotra-core/CLAUDE.md` の 1 か所を新識別子へ
- [ ] `npm run governance:check` が緑であること

### Phase 5 — 終端の検証（受け入れ条件の実測）

- [x] `git grep -n "\.elapsed()\.as_millis()" snotra-core/src/indexer.rs` が **0 件**
- [x] 旧識別子の終端 grep を**除外句なしで**走らせ、生きた層の残存が 0 件であることを実測する。
      **ヒットは 2 つの箱へ仕分ける**——`docs/superpowers/`（凍結）と `workspace/`（本サイクルの
      調査・計画・レビュー成果物。旧識別子を引用しているのは正しい）。どちらでもないヒットが
      1 件でもあれば未完である
- [ ] **`startup.rs:416-417` の書き直しは grep では検算できない**（識別子を含まないため）。
      当該 2 行を目で読み、`LoadOrScanStats` の `*_ms` を現在形で語る文が残っていないことを確かめる
- [ ] `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` /
      `cargo test -p snotra-core` / `cargo test -p snotra` / `cargo doc ...` / `npm test` /
      `npm run governance:check` をすべて緑にする
- [ ] `cargo run -p snotra` を dev で 1 回起動し、`[index-load]` の行が
      **変更前と同じ形**（`cache_hit=.. total=..ms hash=..ms ...`）で出ることを目視で確認する
- [ ] 実装差分を確定させる（コミット可能な状態にする）

## テスト方針と検証コマンド

**新規テストは足さない。** この変更は挙動不変であり、守るべき不変条件はすべて既存の検知器が
持っている（`load_cache_reports_upgrade_save_*` / `load_or_scan_with_stats_reports_upgrade_save_*` /
`load_or_scan_with_stats` の cache-hit 枝テスト）。**足すとすれば「丸めを生成側へ戻す退行」の
検知器だが、`total` の doc が「その形を守る検査は見つかっていない」と実測つきで書いており、
同じ死角がこの 6 フィールドにも当てはまる**——見つかっていない検査を新規に発明することは
本 issue の範囲ではない。**この死角は残余として PR 本文に書く。**

| カテゴリ | コマンド |
|---|---|
| A | `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra-core` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items` |
| F | `npm run governance:check`（`PERFORMANCE.md` · `snotra-core/CLAUDE.md` を触るため） |
| — | `npm test`（`.claude/hooks` 等のカナリア。`.md` を触るので走らせる） |

**カテゴリ C は該当しない**（ウィンドウ生成・表示順・ホットキー・スラッシュコマンドに触れない）。
ただし `src-tauri/` を触るので CI の `Smoke` workflow は paths により自動起動する。
**カテゴリ D も該当しない**——表示の見た目を 1 ピクセルも変えない（#1106 の過大申し送りの反省）。

## `SPEC.md`・関連文書の更新要否

- **`SPEC.md`: 不要**。当該識別子も `LoadOrScanStats` も 0 件（実測）。フェーズ計測を語る節も無い
  （3b が独立に確認）
- **`PERFORMANCE.md`: 要**（識別子 3 か所 + 残余の意味論注記）
- **`snotra-core/CLAUDE.md`: 要**（識別子 1 か所）
- **`docs/adr/`: 不要**。否定の知識（却下した設計）は生じるが、いずれも
  「本 issue の範囲外ゆえ却下」であって設計上の否定の知識ではない。PR 本文に残す

## 網羅性の母集団と、それを誰が知っているか

`AGENTS.md`「網羅性が要件（全箇所改名）」のトリガーに当たるため明示する。

- **コードの母集団はコンパイラが知っている。** 改名すれば下流は必ず compile-fail する。
  列挙器は `cargo clippy --workspace --all-targets` + `cargo test -p snotra-core`。
  **`cargo check` は使わない**——`tests/memory_footprint.rs` を見ない（issue が実測で明記）
- **散文の母集団は `git grep` しか知らない。** 機構は無い（#1027 で `PERFORMANCE.md` を旧語へ
  戻す変異を注入し `governance:check` が exit 0 で通ることを実測済み）
- **grep の罠を 1 つ実測した**: `read_ms` を語境界なしで grep すると `thread_msg_target`
  （`th` + `read_ms` + `g_target`）が 3 ファイルで偽陽性になる。`\bread_ms\b` で消える
- **終端 grep は除外句を付けずに走らせる**（`:!workspace/` も付けない）——除外は狙った以上を
  落とす

## 未確定（実装前に潰す）

（なし——Q1 / Q2 / Q3 は D3・D4 / D5 / D6 として決定済み）

## plan-review 結果

- リスク: **高**（`LoadOrScanStats` は `pub` ゆえ「公開 API を変更する」に当たる。
  当初「通常」と書いたのは誤りで、判定基準を読み直して訂正した）
- レビュー方式: 計画準拠レビュー 1 体（Step 2）
- エージェント数: **2**（Step 3b の敵対枠 + Step 2 の計画準拠レビュー）

### 要対処

- **`startup.rs:416-417` のコメントが PR 後に偽になる** — 計画修正（変更ファイル表へ追加 +
  Phase 3 に作業項目 + Phase 5 に目視の検算） — 再照合: 当該 2 行を実読し、逐語で
  「他の `*_ms` は今も生成時に丸めている」と書いてあることを確認。`*_ms` がグロブゆえ
  終端 grep で捕まらないことも確認
- **`Scanned` の分解が `scan: &[ScanPath]` をシャドウする** — 計画修正（Phase 1 に
  明示的別名の指示） — 再照合: 呼び出し 3 箇所とも同スコープに `scan: &[ScanPath]` が
  在ることを実読（`indexer.rs:748` / `814-819` / `875-879`）
- **D4 の印字形式が未確定だった**（レビューの ⚠️ と助言が独立に同じ点を指摘） — 計画修正
  （符号つき f64 ms に固定） — 再照合: i128 の整数除算はゼロ方向へ切り捨てるため
  `-0.4 ms` が `0` になり、D4 の目的を自ら壊す

### 軽微

- `LoadCacheResult::read` / `LoadOrScanStats::hash` に識別子衝突なし（実測）
- `#[derive(Debug, Clone, Copy)]` は `Duration` / `Option<Duration>` でも壊れない
- `memory_footprint.rs:317-330` の `cache_save` 二重表示（括弧内の内数 + 文末の独立項）は
  **D3 適用前からの既存の非対称であり、温存されるだけで悪化しない**

### 未検証

- **「丸めを生成側へ戻す退行」を捕まえる検査は存在しない。** `LoadOrScanStats::total` の doc が
  カテゴリ A〜F 全通し + `bench-startup.ps1` で緑のまま通ることを #1027 で実測しており、
  同じ死角がこの 6 フィールドにも当てはまる。**本 issue では塞がない**（残余として PR 本文へ）
- ⚠️ `docs/superpowers/` の凍結文書に `LoadOrScanStats` 参照が多数残る。計画が母集団外と
  明示しており、#1181 も同じ扱いをした（先例あり）

### 判断

- 実装着手: **可**（要対処 3 件はすべて計画へ反映済み。人間の承認待ち）
- **再レビューは行わない**——変わったのは対象ファイル 1 枚の追加と実装指示の具体化であり、
  要件・インターフェース・不変条件・テスト期待値はいずれも変わっていない

### 自己照合で自分が踏んだ誤り（記録）

**`startup.rs:416` は、自分が Step 1 で走らせた概念 grep（「丸め」）が既に出力していた**
（出力 71 行目）。29.6KB が退避されプレビュー 2KB だけを読んだため見落とし、独立レビューが
同じ事実を再発見した。AGENTS.md「主張は代理ではなく対象そのもので測ってから書く——
**切り詰めた出力**」がそのまま発火した形である。

### 自己照合（`/start-issue` Step 5a の 5 項目）

1. **issue の全要件に作業項目が対応する** — issue の「やること（案）」5 項目すべてに
   Phase 1〜4 が対応する
2. **境界条件を列挙し、各条件に検証がある** — cache-hit 枝 / cache-miss 枝 / 旧版昇格枝 /
   `config_dir` が引けない枝の 4 つ。前 3 つは既存テストが、4 つ目はコンパイラが覆う
3. **新しい状態・リソース・プロセス** — 追加しない（型の置換のみ）
4. **より単純な既存パターンで置き換えられないか** — #1181 の手順そのものであり、それが最も単純
5. **壊してはならない不変条件に検知手段がある** — 不変条件 1〜3 は既存テストが持つ。
   不変条件 4（ディスク形式に影響しない）は `LoadOrScanStats` がディスクへ出ないという構造。
   **丸めの位置そのものには検知手段が無い**（上の「未検証」）

## 人間レビュー

- [x] 承認済み — 2026-08-25 / 問い: "**この計画を承認いただけますか。** あるいは `workspace/plan.md` へ直接注釈を入れてくださっても構いません。" / 回答: "承認"

注釈は無し。**D4（残余の印字を符号つきへ変える）を名指しで確認したうえでの承認である**
——実装者はこの判断を再導出しない。
