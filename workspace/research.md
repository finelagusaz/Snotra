# research — issue #1027: `LoadOrScanStats` の total を `Duration` で運ぶ

対象 issue: #1027 「LoadOrScanStats の total を Duration で運び、丸めを表示境界へ寄せる」
（labels: rust / size:S / type:refactor、関連 #1009）
ブランチ: `chore/1027-load-or-scan-total-duration`

## 1. issue の要約

`index_load_unattributed_ms`（起動計器の JSON 出力）は
`to_ms(外側の IndexLoad 区間) - 内側の LoadOrScanStats.total_ms` である。
この値の**非負性が 2 つの前提に乗っている**:

1. 外側の区間が内側の `total_ms` を包む（包含関係）
2. 両者とも**切り捨て**である（`startup.rs` の `to_ms` の除算と、`indexer.rs` の `Duration::as_millis`）

`a ≥ b ⇒ floor(a) ≥ floor(b)` ゆえ差は非負。#1009 は doc に前提を書き、ハーネス
（`SnotraStartupContract.psm1`）の `>= 0` 検査で外から捕まえる形で倒した。

**この issue は前提 2 を構造で消す。** `total_ms: u128` を `total: Duration` にし、
ミリ秒への丸めを `startup.rs` の `to_ms` 1 か所へ寄せる。前提 1（包含関係）は構造では
消せないので、ハーネスの `>= 0` 検査は**残す**。

### 範囲の確定（ユーザー判断・2026-08-25）

issue が残していた分岐（「他の `*_ms` をどうするかは別途判断」「やらない判断もありうる」）に対し、
ユーザーは **「`total` だけ `Duration` へ」** を選んだ。他の `*_ms` フィールドは本 issue では触らない。

採らなかった 2 案と理由:

- **`*_ms` を全部 `Duration` へ**: 筋は通るが、private な `LoadCacheResult`
  （`read_ms` の生成が 10 か所超・`upgrade_save_ms: Option<u128>` は `Some(0)` の意味論を
  #1054 / #1063 の doc が背負う）と `Scanned`・`indexer.rs` のテスト群まで波及し、size:S を超える。
- **やらない（wontfix）**: 前提 1 が残りハーネス検査も減らないため得は「破れる経路が 2→1」だけ、
  という issue 自身の指摘。ユーザーはそれを承知のうえで採択しなかった。

## 2. 事実の再計測（issue が命じた数え直し・2026-08-25）

issue の事実表は「消費は `src-tauri/src/main.rs` の **2 か所だけ**」「`/simplify` の報告は 4 か所
だったが実測は 2」と書く。**再計測の結果は 4 か所であり、issue の表のほうが誤っている。**

| 分類 | 箇所 | 内容 |
|---|---|---|
| 型の宣言 | `snotra-core/src/indexer.rs:455` | `pub total_ms: u128,` |
| 生成 | `indexer.rs:778` | cache-hit 枝（`load_or_scan_with_stats_in`） |
| 生成 | `indexer.rs:819` | cache-miss 枝（`load_or_scan_with_stats_in`） |
| 生成 | `indexer.rs:872` | `config_dir` が引けない枝（`load_or_scan_with_stats`） |
| 消費 | `src-tauri/src/main.rs:185` | `#[cfg(debug_assertions)]` の `eprintln!` |
| 消費 | `src-tauri/src/main.rs:195` | `startup::set_index_load_stats_ms(result.stats.total_ms as u64)` |
| 消費 | `snotra-core/tests/memory_footprint.rs:319` | フェーズ内訳の `println!` |
| 消費 | `snotra-core/tests/memory_footprint.rs:331` | 残余計算 `s.total_ms.saturating_sub(...)` |

**差の理由は数え損ないではなく母集団の取り方である**——issue は `src-tauri`（製品）だけを数え、
`snotra-core` 内のテスト（`tests/memory_footprint.rs`）を外している。`/simplify` の「4」は
テストを含めた数であり、**そちらが当たっていた**。本計画は 4 か所すべてを変更対象に載せる
（テスト側は crate 内なので下流 compile-fail ではなく `cargo test -p snotra-core --no-run` が検出器）。

### 列挙の方法と、その死角

- **LSP `findReferences` は使えなかった。** `rust-analyzer.exe` は起動している（プロセス実測）が、
  `findReferences` / `hover` / `workspaceSymbol` のいずれも本セッション中は
  "not fully indexed" を返し続けた（`indexer.rs:455` と `startup.rs:494` で実測）。
- 代わりに `grep -rn` で列挙し、**権威は Rust のコンパイラに置く**——
  `total` はフィールドであり、`u128` → `Duration` の型変更は
  `cargo check --workspace` / `cargo test -p snotra-core --no-run` /
  `cargo clippy --workspace --all-targets` の compile-fail が読む側を全数で落とす
  （src-tauri の package 名は `snotra`・`src-tauri/Cargo.toml` 実測）。
- ⚠ **「compile-fail が全数検出器である」は無条件では偽である**（敵対的調査 A2・実測で採用）。
  `main.rs:185` の消費は `#[cfg(debug_assertions)]` の下にあり、ルート `Cargo.toml` の
  `[profile.release]`（37-42 行）は `debug-assertions` を上書きしていない——**release 既定の
  `debug-assertions = false` ではこのブロックがコンパイルされない**。ゆえに
  `cargo build --release -p snotra` **単独**では移行漏れを捕まえられない
  （`scripts/bench-startup.ps1:28` が既定で `target/release/snotra.exe` を要求するので、
  release だけを触る経路は実在する）。**成立する条件を付けて言うと**:
  「dev プロファイル（`debug-assertions = true`）で走る `cargo check --workspace` /
  `cargo test` が、`total` を読むすべての行を落とす」。カテゴリ A の必須コマンドは
  すべて dev プロファイルなので、**規定の検証フローには穴は開かない**。
- **compile-fail が見ないのは散文の名前だけである**（§4）。
- `startup.rs:314, 494` の**引数名**が偶然 `total_ms` だが、これは
  `LoadOrScanStats.total_ms` フィールドの消費ではない（構造体を経由しない）。
  §2 の「4 か所」はフィールド読み出しの数であり、この 2 つは §3 の改修対象表に別途載る。

## 3. 関連ファイル・シンボル

| ファイル | シンボル / 行 | 役割 |
|---|---|---|
| `snotra-core/src/indexer.rs` | `LoadOrScanStats`（423-456） | `#[derive(Debug, Clone, Copy)]`。`Duration` は `Copy` ゆえ derive はそのまま通る |
| | struct doc 417-421 | 「`cache_load_ms` と `total_ms` の間に処理を足すときは項目を作れ」の**規範の正本** |
| | `load_or_scan_with_stats_in`（748-） | 生成 2 か所（cache-hit / cache-miss） |
| | `load_or_scan_with_stats`（838-） | 生成 1 か所（`config_dir` 不在枝） |
| | 22 行目 `use std::time::Instant;` | **`Duration` の import 追加が要る**（実測: 現状 `Instant` のみ） |
| `src-tauri/src/main.rs` | 178-195 | 消費 2 か所。195 の `as u64` キャストが消える |
| `src-tauri/src/startup.rs` | `Timeline.index_load_stats_ms: Option<u64>`（277, 287） | `Option<Duration>` へ |
| | `Timeline::set_index_load_stats_ms`（314） / 自由関数（494） | 引数を `Duration` へ |
| | `to_json` の `index_load_unattributed_ms` ブロック（392-417） | **前提 1/2 を書いた doc コメントの正本**。`inner as i64` → `to_ms(inner) as i64` |
| | `to_ms`（456-458） | `(d.as_nanos() / 1_000_000) as u64`。**丸めが起きる唯一の場所**（doc 452「丸めはこの 1 か所だけで起きる」） |
| | tests 823-838 | `index_load_unattributed_is_null_without_stats` / `..._is_the_gap_against_load_stats`（`set(42)` → `Duration::from_millis(42)`、期待値 8 は不変） |
| `snotra-core/tests/memory_footprint.rs` | 318-338 | 消費 2 か所。`s.total_ms` → `s.total.as_millis()` |
| `scripts/lib/SnotraStartupContract.psm1` | 63、162-164 | `>= 0` 検査と、`LoadOrScanStats.total_ms` を名指す散文 2 か所 |
| `snotra-core/CLAUDE.md` | 219 | 上記 struct doc の規範の写し（`total_ms` を逐語で持つ） |
| `snotra-core/src/indexer.rs` | 3590 | 同規範を doc から引用する箇所（`total_ms` を逐語で持つ） |

### 触らないもの

- `LoadCacheResult`（`read_ms` / `upgrade_save_ms`）・`Scanned`（`scan_ms` / `sort_ms`）・
  `LoadOrScanStats` の他の `*_ms` フィールド — 範囲外（§1）
- JSON ペイロードのキー `index_load_unattributed_ms` — **表示境界の名前であり ms のままが正しい**。
  変えるとハーネス（`SnotraStartupContract.psm1` の必須キー列挙 89 行）と
  `scripts/bench-startup.ps1:166` が壊れる。契約の形は `ADR-startup-instrument-contract-shape`。
- `docs/superpowers/plans/` / `specs/` の `total_ms` — #589 で非規範化された凍結文書（§4）

## 4. 名前の腐り（compile-fail が見ない面）と、その検出器の射程

改名後 `total_ms` は production の非コメント本文から消える。散文に残った `total_ms` は
**幽霊識別子**になる。機構がどこまで見るかを実測した。

`G-stale-identifiers`（`scripts/governance/checks/G-stale-identifiers.mjs`）の母集団は
`.claude/**` の規範散文 + `docs/**`（歴史記録 2 種を除く）+ 固定パス
`STALE_EXTRA_DOCS = ["SPEC.md", "CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"]`
（`scripts/governance/lib.mjs:661` 実測）。ゆえに:

| 散文の在り処 | 検出器 | 扱い |
|---|---|---|
| `snotra-core/CLAUDE.md:219` | **無い**（モジュール `CLAUDE.md` は母集団外・当該 mjs の doc が明示） | 手で直す |
| `snotra-core/src/indexer.rs` の doc コメント（417-421, 442, 3590） | **無い**（`.rs` は母集団外。#975 で拡大を測って却下） | 手で直す |
| `src-tauri/src/startup.rs:396` の前提コメント | 同上 | 手で直す（前提 2 の記述を消す） |
| `scripts/lib/SnotraStartupContract.psm1:63, 163` | **無い**（`.psm1` は母集団外） | 手で直す |
| `PERFORMANCE.md:1883, 2177` | **無い**（`STALE_EXTRA_DOCS` にも `docs/**` にも入らない） | **直す**（下記・当初の判断を覆した） |
| `PERFORMANCE.md:2704` | — | **触らない**。ここの `total_ms` は `SNOTRA_EGUI_PAINT_TRACE` の別フィールド（正本は `snotra-egui-runtime/src/renderer.rs`）で、`LoadOrScanStats` とは無関係 |
| `docs/superpowers/plans/` / `specs/` | 母集団から明示除外（凍結された歴史） | 直さない |

**当初は「`digest_ms` が `PERFORMANCE.md` に残っている」を先例に直さない判断を書いた。
敵対的調査（A1）がその先例を崩し、一次証拠で確認して結論ごと覆した。**

`git log --all -S"digest_ms" -- snotra-core/src/indexer.rs` と `git show ae3335df` の実測:
`digest_ms` は #970 で導入され、**#1023（`ae3335df` 「背景再スキャンを撤去し、索引の更新を
明示操作だけにする」）でフィールド宣言 `pub digest_ms: u128,` と生成 3 行ごと削除された**。
つまりあれは**概念そのものの撤去**であって改名ではない。#1027 は「ロード全体の所要時間」という
概念が生き続けたまま名前と型だけが変わる**真の改名**であり、**先例として効かない**。

`PERFORMANCE.md` の規約「既存の記述へ遡及して補完しない」（36 行）も根拠にならない——
同行が自分で射程を書いている（「過去の測定がどちらの機体で取られたかを知る手段は無く、
埋めれば測定記録の顔をした推測が残る」）。**出所（機体名）の遡及補完を禁じているのであって、
既に分かっている改名の反映を禁じてはいない。**

**ゆえに `PERFORMANCE.md:1883, 2177` の `total_ms` は `total` へ直す。数値・散文・出所は
1 文字も変えない**（変えるのは識別子の綴りだけ）。同文書 40 行の「今も設計を支えている値は
コードの doc を正本にする形で表す」は、**存在しないフィールド名を指したままでは満たせない**。

## 5. 再利用できる既存パターン

- **`Duration` を持ち回してミリ秒は表示境界で作る**は `startup.rs` が既に全面採用している
  （`Timeline.durations: [Option<Duration>; COUNT]`、`to_ms` は 1 か所、生 ns は `*_ns` キーで出す）。
  本変更は**その原則を `snotra-core` 側の 1 フィールドへ広げるだけ**であり、新しい設計判断は無い。
  `startup.rs` の `//!` 13-16 行「丸めは表示境界でだけ行う」が原則の正本。
- 型変更の移行漏れ検出に**下流の compile-fail を使う**は `AGENTS.md`
  「関数・型を新規定義／改名／導入」トリガーの既定手段。

## 6. 技術的制約

- `LoadOrScanStats` は `#[derive(Debug, Clone, Copy)]`。`Duration: Copy` ゆえ derive は変更不要。
  `Debug` の出力形は変わる（`total_ms: 80` → `total: 80ms`）が、`Debug` 出力を読む検査は
  見つからない（`{:?}` で `LoadOrScanStats` を出す箇所は grep で 0 件）。
- `LoadOrScanStats` は `pub`。**crate 境界をまたぐ破壊的変更**だが、下流は同一ワークスペースの
  `snotra`（src-tauri）だけであり、外部公開はしていない。
- `main.rs:195` の `as u64` は消える。`startup::set_index_load_stats_ms` が `Duration` を取るため。
- `memory_footprint.rs:331` の残余計算は `u128` 同士の `saturating_sub`。
  `s.total.as_millis()` にすれば型は `u128` のままで、**式の意味も丸めの向きも変わらない**
  （`as_millis()` は切り捨て、変更前と同一）。
- `startup.rs` の `to_json` は `i64` で引く。**前提 1 が破れれば依然として負値が出る**——
  `i64` のままにする（`u64` にすると wrap して、ハーネスの `>= 0` が見る対象が消える）。

## 7. 挙動の不変性

**JSON 出力の値は変わらないはずである。**

- 変更前: `to_ms(measured) as i64 - (total_started.elapsed().as_millis() as u64) as i64`
- 変更後: `to_ms(measured) as i64 - to_ms(total_started.elapsed()) as i64`

`to_ms(d) = d.as_nanos() / 1_000_000` と `d.as_millis()` は**同じ切り捨て**なので、同じ
`Duration` に対して同じ値を返す（返り値型の `u64` と `u128` の差は、ms 表現で `u64` を
溢れさせる起動が無い限り現れない）。**丸めの回数は前後とも 1 回で、向きも同じ。**

⚠ 「値が変わらない」は代理ではなく対象で測る（§検証・Phase 3 の単体テスト）。

## 7.5 計器が動く条件（測定環境の確認・敵対的調査 C2 を自分で実測）

**`index_load_unattributed_ms` の JSON は、既定の起動では 1 行も出ない。**
`startup::begin()` は `crate::trace::trace_enabled()` が偽なら即 return し（`startup.rs:471-473`）、
`trace_enabled()` は `env_flag("SNOTRA_TRACE")` である（`src-tauri/src/trace.rs:35-38` 実測）。
実際にこの計器を回すのは `scripts/bench-startup.ps1` が自分で `$env:SNOTRA_TRACE = "1"` を
立てる経路（同 116 行）だけである。

**この事実は本変更の効き目を減じない**——理由は 2 つで、どちらも実測に基づく:

1. `total` の**生成**（`total_started.elapsed()`）と `main.rs:195` の
   `set_index_load_stats_ms` の**呼び出し**は `SNOTRA_TRACE` に依らず常に走る。
   ゲートされるのは `Timeline` の初期化と JSON の出力だけである。
2. 前提 2 の消滅を固定する検知器は `startup.rs` の `#[cfg(test)]` テストに置く。
   テストは `Timeline` を直接構築するので `begin()` のゲートを通らない。

⚠ 実機に `%APPDATA%/Snotra/config.toml` / `index.bin` が在るかは確かめていない
（敵対的枠も確認できなかった・C1）。**本変更は cache-hit / cache-miss のどちらの生成箇所も
同じ 1 行の書き換えなので、実運用点がどちらの枝を通るかは裁定に影響しない。**

## 8. 未解決の疑問（plan.md の未確定欄へ引き継ぐ）

1. ~~`PERFORMANCE.md` を直さない判断~~ — **解消（§4）**。敵対的調査 A1 が先例を崩し、
   一次証拠で裁定して「直す」へ覆した。plan.md の作業項目に載せる。
2. 前提 2 が構造で消えたことを doc へどう書くか。**全称で書くと即座に偽になる**——
   本 issue は `total` だけを `Duration` 化するので、`hash_ms` / `cache_load_ms` /
   `cache_read_ms` / `scan_ms` / `sort_ms` / `cache_save_ms` は**依然 `snotra-core` 内で
   生成時に丸めている**。「丸めは表示境界でだけ起きるようになった」は書いた日に偽になる
   （#977 / #1091 の再生パターン）。主張は
   **「`index_load_unattributed_ms` の計算に限り、丸めは `to_ms` の 1 回だけになった」**へ絞る。
3. 前提 2 の消滅を守る**検知器**を置くか。候補: `startup.rs` のテストで
   「外側 50.9 ms / 内側 50.1 ms（同一ミリ秒に落ちる）で差が 0 になる」を固定する。
   これは `to_json` が内側だけ別の丸めを通すようになったら落ちる。置くかどうかは未確定欄で決める。
4. `set_index_load_stats_ms` の名前から `_ms` を落とすか（引数が ms でなくなるため）。
   落とすと `Timeline` のメソッドと自由関数の 2 か所 + 呼び出し 2 か所（main.rs / test）の改名。

## 9. 敵対的調査（3b）の結果

サブエージェント 1 体（general-purpose / sonnet）。全文は `workspace/adversarial-1027.txt`（235 行）。
**機序はすべて主エージェントが一次証拠で裁定してから採った**（所見と機序は独立に誤りうる）。

### 壊せた項目（2 件・どちらも採用）

| # | 所見 | 機序の裁定 | 採否と反映先 |
|---|---|---|---|
| A1 | 「`PERFORMANCE.md` を直さない」の根拠にした `digest_ms` の先例が、改名ではなく**撤去**だった | **確認**。`git show ae3335df`（#1023）で `pub digest_ms: u128,` と生成 3 行の削除を実測。相手の機序（撤去 ≠ 改名ゆえ先例が効かない）は正しい | **採用し、結論ごと覆した**。§4 を「直す」へ書き換え。相手は「結論自体は別の理由で支持されうる」と留保したが、**その別の理由（36 行の規約）は自分で射程を書いており使えない**ため、留保は採らない |
| A2 | 「compile-fail は全数検出器」が release プロファイルで偽 | **確認**。ルート `Cargo.toml` 37-42 行に `debug-assertions` の上書きが無いことを実測。`main.rs:185` は `#[cfg(debug_assertions)]` の下 | **採用**。§2 の主張へ「dev プロファイルで走る `cargo check --workspace` / `cargo test` が」という条件を付けた（全称を条件つきへ弱める・`AGENTS.md`「検証の作法」） |

### 壊せなかった項目（5 件）

| # | 命題 | 相手が試したこと | 結果 |
|---|---|---|---|
| B1 | 消費は 4 か所（issue の「2 か所」は誤り） | リポジトリ全体の grep で 5 件目を探索 | **堅持**。追加ヒットは `.superpowers/`（`.gitignore` で追跡外）と `startup.rs` の**引数名**の同名衝突のみ |
| B2 | `to_ms(d) == d.as_millis()` | repo 外の scratch で `rustc -O` を使い 9 ケース実測（`Duration::new(u64::MAX, 999_999_999)` を含む） | **堅持**。`u128` 比較は全ケース一致。食い違ったのは `u64` キャストが溢れる約 5,849 億年の起動のみで、これは research.md が明記済みの前提 |
| B3 | `G-stale-identifiers` の母集団 | `lib.mjs` / 当該 check の通読 + `governance:check` 実行 | **堅持**。`STALE_EXTRA_DOCS` は glob ではなくリテラル配列ゆえ `"CLAUDE.md"` はルートのみ |
| B3' | ⚠ §4 が留保していた「他 check が鳴る射程の穴」 | `G-references` / `G-heading-refs` / `G-near-heading-refs` / `G-folded-code-spans` を通読 | **穴は無かった**（4 検査とも `total_ms` の改名を検出する構造を持たない）。§4 の ⚠ を解消 |
| B4/B5 | `to_ms_truncates_toward_zero` の継続有効性・`Debug` 出力 0 件 | 該当箇所の読み取り | **堅持**（`Debug` の 0 件は主エージェントも独立に grep で再確認した） |

### ⚠ 確信の持てない所見（3 件）

- **C1**: 実機の `config.toml` / `index.bin` の有無を確認できなかった → §7.5 の ⚠ に記録。裁定には影響しない
- **C2**: `startup::begin()` が `SNOTRA_TRACE` でゲートされ、既定では計器が一切生成されない
  → **採用**。主エージェントが `trace.rs:35-38` で独立に実測し §7.5 を新設した
- **C3**: A1 の留保（結論は別の理由で支持されうる） → **採らない**（上表 A1 の裁定）

### D. research.md が問うていなかった論点

- **D3**（記録のみ・裁定を変えない）: `staleTarget()` は識別子が `.` を含むと照合自体を
  スキップするため、`` `LoadOrScanStats.total_ms` `` のようなドット修飾形は母集団の内側でも鳴らない
  （当該 check の doc「PascalCase・ドット区切り…も述語の外にある」と整合）。
  現状 population 内に `total_ms` の言及が 0 件なので効果が重複しており、§4 の表は変わらない
- D1 / D2 は A2 / C2 と同一。D4（`docs/superpowers/` の言及）は既定の除外方針でカバー済み
