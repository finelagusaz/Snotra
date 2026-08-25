# research — issue #1178: `LoadOrScanStats` の残る `*_ms` も `Duration` で運ぶ

## issue の要約

#1027（実装は PR #1181）で `LoadOrScanStats.total_ms: u128` → `total: Duration` にしたが、範囲を
`total` の 1 フィールドへ限った（**却下ではなく繰り延べ**）。残る 6 フィールド

`hash_ms` / `cache_load_ms` / `cache_read_ms` / `scan_ms` / `sort_ms` / `cache_save_ms`

は今も `snotra-core` の**生成時**に `.as_millis()` で丸めている。`startup.rs` の `//!`
「丸めは表示境界でだけ行う」が掲げる原則に対し、内側の crate だけがその外に居る。

波及先は `LoadCacheResult`（private・`read_ms` / `upgrade_save_ms`）、`Scanned`（private・
`scan_ms` / `sort_ms`）、`indexer.rs` のテスト群、`tests/memory_footprint.rs`、`src-tauri/src/main.rs`。

## 関連ファイル・モジュール・シンボル（すべて grep で実在確認済み）

### 変更する（コード）

| ファイル | 対象 | 件数 |
|---|---|---:|
| `snotra-core/src/indexer.rs` | `LoadOrScanStats`（`pub` struct・6 フィールド）/ `LoadCacheResult`（private・`read_ms` / `upgrade_save_ms`）/ `Scanned`（private・2 フィールド）/ `scan_and_sort_timed` / `load_or_scan_with_stats_in` / `load_or_scan_with_stats` / `upgrade_legacy_cache_in` / `finish_legacy_read` / `load_cache_in` / テスト群 | 111 |
| `snotra-core/tests/memory_footprint.rs` | フェーズ内訳の `println!` と残余計算、`report_scan_all_cost` の doc | 20 |
| `src-tauri/src/main.rs` | `#[cfg(debug_assertions)]` 下の `[index-load]` `eprintln!` | 5 |

（件数は `git grep -c "hash_ms\|cache_load_ms\|cache_read_ms\|scan_ms\|sort_ms\|cache_save_ms\|upgrade_save_ms\|\bread_ms\b"`。コメント中の出現を含む）

### 変更する（散文）

| ファイル | 対象 | 件数 |
|---|---|---:|
| `PERFORMANCE.md` | `scan_ms`（1837）/ `cache_read_ms`・`cache_load_ms`（2112）/ `cache_load_ms`（2177） | 3 |
| `snotra-core/CLAUDE.md` | 「indexer.rs の索引更新の契機」冒頭の `cache_load_ms` | 1 |

### 触らない（射程の明示）

- **`total`** — #1027 で済んでおり issue が明示的に除外する。その doc が持つ残余（生成側が丸めた
  値を渡す形に検査が届かない）は、**このフィールド群を `Duration` にしても減らない**。他の
  `*_ms` は引き算に使われていないため、同じ形が 6 つ増えるわけでもない
- **`startup.rs` / `to_ms` / `scripts/lib/SnotraStartupContract.psm1`** — `set_index_load_stats_total(Duration)`
  以降その経路は端から端まで `Duration` で、今回の差分は届かない
- **`index_load_unattributed_ms`** — 起動計器の JSON 出力キー（`SnotraStartupContract.psm1:89` が
  網羅列挙する）。`_ms` は外向きの出力名であって本 issue の母集団ではない
- **`docs/superpowers/plans/` · `specs/`（23 件）** — 凍結された歴史。#1181 も触っていない（実測）
- **`snotra-egui-runtime/src/renderer.rs` の `total_ms` / `tess_ms` / `raster_ms`** — paint trace の
  別計器。同名だが無関係
- **`SPEC.md`** — 当該識別子も `LoadOrScanStats` も **0 件**（実測）。ゆえに SPEC 同期は不要

## 再利用できる既存パターン

**先例は PR #1181（コミット `1a405180`）そのものである。** 次を踏襲する。

1. **型と同時に名前から `_ms` を落とす**（`total_ms` → `total`）。型が `Duration` なのに名前が
   `_ms` だと名前が嘘になる
2. **丸めは表示境界の `.as_millis()` へ寄せ、出力文字列は 1 バイトも変えない**
3. **`PERFORMANCE.md` の散文の識別子も追随させる**（#1181 は v6→v7 表の
   「ロード（壁時計・`total_ms`）」→「`total`」を実際に書き換えた）
4. **commit type は `refactor(core)!:`** — `pub` フィールドの改名は破壊的変更

### 改名表（案）

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

`hash` は「ハッシュ値」とも読めるが、struct doc が「各フェーズ所要時間」と宣言しており型も
`Duration` である。#1181 の `total` が先例。

## 技術的制約

1. **移行漏れの検出器に `cargo check` を使ってはならない**（issue が明記・memory の
   `cargo-check-skips-integration-tests` と同型）。`tests/` 配下は既定ターゲットの外。
   使うのは `cargo clippy --workspace --all-targets` と `cargo test -p snotra-core`
2. **散文に残る旧識別子を捕まえる機構は無い**（`G-stale-identifiers` の母集団外。#1027 で
   `PERFORMANCE.md` を旧語へ戻す変異を注入し `governance:check` が exit 0 で通ることを実測済み）。
   `git grep` の手作業が唯一の手段
3. **`[index-load]` の出力文字列は凍結する。** 根拠は**挙動不変の宣言そのもの**であって、
   ハーネスがその形を読んでいることではない。**この点は 3b で訂正した**——
   `scripts/lib/SnotraSmoke.psm1` の `Read-SnotraTraceSnapshot` は `[index-load]` 行を
   「`[trace]` で始まらない行」として捨てるだけで、`cache_hit=` / `hash=Nms` を 1 つも
   parse しない（`SnotraTraceInvariants.Tests.ps1:480` の fixture が固定しているのも
   「非 `[trace]` プレフィックスであること」だけである・実測）。ゆえに**形を変えても
   ハーネスは赤くならない**——だからこそ規約でしか守られておらず、凍結すると決める。
   表示は `s.hash.as_millis()` などで作り、format 文字列は変えない
4. **`Some(Duration::ZERO)` の意味論を壊さないこと**（issue の要求）。`upgrade_save` は
   **variant が「昇格 save を通ったか」の判定そのもの**であり、時間の値は判定に使わない
   （`LoadCacheResult::upgrade_save_ms` の doc・#1054 / #1063）。`Option<Duration>` にしても
   `Some` を作るのは `upgrade_legacy_cache_in` の 1 行だけという構造は変わらない
5. **`cache_read` と `cache_save`（cache-hit 枝）は `cache_load` の内数**という非対称は型では
   表現されず、doc と `memory_footprint.rs` のコメントが正本のまま残る
6. **doc コメントを触るので `cargo doc` を手で走らせる**（`.claude/rules/comments.md`：intra-doc
   link 切れは CI でのみ発火し PostToolUse hook は沈黙する）

## 敵対的調査（3b）の所見と採否

`general-purpose` / `sonnet` を 1 体。全文は `workspace/adversarial-1178.txt`。

### 壊せなかった項目（5 争点すべて）

1. 母集団は 5 ファイルで尽きている（`snotra-settings/` · `.github/workflows/` · `scripts/` ·
   `.claude/` · `docs/hooks.md` · `RETROSPECTIVE.md` · `*.json` / `*.toml` すべて 0 件）
2. `[index-load]` の凍結でハーネスは無傷
3. `SPEC.md` にフェーズ計測を語る節は無く、同期不要
4. `Option<u128>` → `Option<Duration>` は `Some(ZERO)` の意味論を壊さない
5. 残余算式は案 B が健全（floor の劣加法性から）

**測定環境の疑い 3 点も前提を裏づける方向で決着した**（反証にはならず）: 実 `index.bin` は
**v7＝現行版**（ヘッダ実測）ゆえ開発機では旧版昇格枝が自然には測れない／ルート `Cargo.toml` に
`debug-assertions` の上書きは無い／`post-edit.mjs` の `selectChecks` に `cargo doc` は無い。

### ⚠️ 所見 3 件——すべて採用（機序は一次証拠で自分で裁定した）

| # | 所見 | 裁定 | 反映先 |
|---|---|---|---|
| A | 制約 3 の根拠づけが過大。fixture は「非 `[trace]` 行であること」しか固定しない | **正しい**（`SnotraSmoke.psm1` の `Read-SnotraTraceSnapshot` の doc と Pester の assert を実読して確認）。結論（凍結する）は変わらないが**理由が違う**ので上の制約 3 を書き換えた | 制約 3 |
| B | 20,000 件治具の存在理由が型変更で消滅する。Q2 は文書問題としてしか扱っていない | **正しい**。当該コメント（`indexer.rs:3724-3728`）は逐語で「**判定は `as_millis()` の整数値なので**、昇格 save が 1 ms を切ると落ちる——1 件の治具では 8 回中 3 回落ちた」と書いており、規模の根拠が `as_millis()` に**明示的に**掛かっている | Q2 を実装判断へ格上げ |
| C | 案 B を素の `-` で書くと `Duration::Sub` が panic する（`checked_sub().expect(..)`）。失敗モードが「無音の 0」から panic へ変わる | **正しい**。Q1 はこの変化に触れていなかった | Q1 に決定を追記 |

**B・C はどちらも「所見は正しいが、採るべき対処は自明でない」形である**——下の Q1 / Q2 で
決定として明示する。

## 未解決の疑問

### Q1（要決定）— `memory_footprint.rs` の残余算式をどちらの意味論にするか

現在の残余は `total.as_millis() - (hash_ms + cache_load_ms + scan_ms + sort_ms + [cache_save_ms])`
で、**各項が生成時に丸まっている**ため丸め屑（最大 ~1 ms × 5 項）を残余が吸っている。

- **案 A（丸めてから引く）**: 今日と同じ数字を再現。残余は屑を吸い続ける
- **案 B（生 `Duration` で引いてから 1 回丸める）**: 残余が「名前の付いていない処理」だけになる。
  これは `startup.rs` の `//!`「隣接区間を個別にミリ秒へ落とすと丸め境界で和が合わない」
  「厳密に検算するのは生 ns だけである」が掲げる原則そのもの。代償は、印字した各フェーズ ms の
  和が印字した total と厳密には一致しなくなること（ずれ < ~5 ms）

**決定: 案 B**（この refactor の目的そのものであるため）。ただし `PERFORMANCE.md:2180` の
「現在の残余は 0〜1 ms である」は案 A の意味論で測った値なので、意味論が変わったことを
その近傍に明記する。`cache_save_is_internal` による除外は直交（どの項が和に入るかの
話であって、どこで丸めるかの話ではない）ので、どちらの案でも生き残る。

**引き算の意味論を選ぶ必要がある**（3b 所見 C）。素の `-` は
`checked_sub().expect("overflow when subtracting durations")` ゆえ**負に振れると panic する**。

**panic を検知器に格上げする案は却下する**——`#[ignore]` の手動計測ハーネスに新しい失敗モードを
持ち込むと、残余の意味論変更と同じ差分に 2 つの判断が混ざる。

**当初ここには「`Duration::saturating_sub` を使い、失敗モードを今日と同一（無音の 0）に保つ」と
書いていた。`/plan-review` Step 1 の概念 grep で覆した**——同じファイルの `delta_mib`
（`memory_footprint.rs:131`）に「**`saturating_sub` で丸めない**——飽和させると減少が
`0.00 MiB` に化け、『何も起きなかった』と読める嘘になる（**実測で踏んだ**）」という規範が
既にあり、しかも当の残余ブロック自身が「符号が飽和側にしか出ないので気づけない」と自己申告して
いた。**このファイルは同じ誤りを一度学習しており、残余の行だけがその学習の外にあった。**

**決定は `plan.md` の D4 が正本である**（符号つき f64 の ms・`delta_mib` と同じ作り）。
ここに条件を書き写さない。

### Q2（要決定）— 20,000 件治具の存在理由が型変更で消滅する

`indexer.rs:3724-3728` のコメントは逐語で「**判定は `as_millis()` の整数値なので**、昇格 save が
1 ms を切ると『配線は生きているのに 0』で落ちる——実際に 1 件の治具では 8 回中 3 回落ちた。
**時計を跨がせるのは閾値ではなく仕事量である。**」と書く。`cache_save: Duration` にすると
`> Duration::ZERO` は ns 分解能で判定されるので、**この根拠は丸ごと消える**（3b 所見 B）。

**決定: 治具の規模は 20,000 件のまま据え置き、コメントを書き直して「今は必要条件ではない」と
明示する。** 縮める判断は本 issue の範囲外であり、`- [ ]` の作業項目にすると
「縮めた／据え置いた」のどちらでも他方が残る形になる。**根拠が消えたことを黙って据え置く
のが最悪である**——規模だけが残って理由が腐り、次の読者が「ms を跨がせるため」と読む。

原則「variant が判定・値は計器」（`LoadCacheResult::upgrade_save` の doc）は変わらない。
`LoadOrScanStats` 側が variant を持たないという当該コメントの但し書きも、型が
`Duration` になっても真のままである。

**観察（この PR では直さない）**: `Duration` 化によりこの検知器は**より緩い入力でも緑になる**
——これは検知力の低下ではなく、flaky の除去である（配線を落とす退行は `Duration::ZERO` に
なるので今までどおり赤くなる）。ただし **`> Duration::ZERO` は「1 件の治具でも通る」**ため、
将来この検知器の治具を縮める変更が来たときに止める機構は無い。

### Q3 — テスト関数名も改名するか

`load_cache_reports_upgrade_save_ms_only_when_it_upgrades_a_legacy_format` /
`load_or_scan_with_stats_reports_upgrade_save_ms_in_cache_save_ms` はフィールド名を名乗る。
**推奨は追随**（配線を名指ししているため）。

### 観察（この PR では直さない）

`PERFORMANCE.md:2177` 近傍の `digest_ms` は**既に実在しない識別子**である（#984 の
「explicit scan only」で撤去され、`snotra-core/src/` に 1 件も無い）。本 issue の母集団外なので
触らないが、散文に旧識別子を捕まえる機構が無いことの実例として記録する。

## 数え上げの母集団と、その所在

`AGENTS.md`「網羅性が要件」のトリガーに当たるため、母集団を誰が知っているかを明示する。

- **コードの母集団はコンパイラが知っている** — 改名すれば下流は compile-fail する。
  `cargo clippy --workspace --all-targets` + `cargo test -p snotra-core` が完全な列挙器。
  `cargo check` は `tests/` を見ないので使わない
- **散文の母集団は `git grep` しか知らない** — 機構は無い（制約 2）。除外句を付けずに走らせる
- **grep の罠を 1 つ実測した**: `read_ms` を語境界なしで grep すると `thread_msg_target`
  （`th` + `read_ms` + `g_target`）が 3 ファイルで偽陽性になる。`\bread_ms\b` で消える
