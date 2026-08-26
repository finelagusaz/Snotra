# 調査: 撤去済みの `digest_ms` が散文に残っている（issue #1180）

## issue の要約

`digest_ms` は `LoadOrScanStats` から撤去済みのフィールドだが、散文に残っている。issue は 2 箇所
（`src-tauri/src/startup.rs` の `unmarked_tail_ns` 直前のコメント / `PERFORMANCE.md`「計測の隙間が
この複製を隠していた」）を挙げ、次の 3 つを求める。

1. 2 箇所を「撤去を描写している（＝直さない）」「在る前提で書いている（＝直す）」へ振り分ける
2. 直す側があればバッククォートを外して散文にする（`.claude/rules/governance-docs.md` の同型）
3. 同じクラスの残存が他に無いか、#1023 周辺で撤去された語彙全体を `git grep` で走査する

「やらない判断もありうる」——放置を選ぶなら受容する残余として明記する。検知器の新設・
`G-stale-identifiers` の母集団拡張は issue が明示的に範囲外としている。

## 事実（一次証拠つき）

### F1. #1001 と #1023 は同じ撤去である（issue と PERFORMANCE.md の食い違いは無い）

issue は「撤去は #1023（`ae3335df`）」と書き、`PERFORMANCE.md` の既存注記は「#1001 で消えた」と書く。
実測: `gh issue view 1001` は **issue**（`perf: 背景再スキャンが毎起動 C:\ を全走査している`・CLOSED）、
`#1023` はそれを閉じた **PR**（`ae3335df`・2026-08-10）。`git log -S digest_ms -- snotra-core/ src-tauri/`
の先頭が `ae3335df` であり、`digest_ms` を消したコミットは 1 つだけである。**同一事象を issue 番号で
呼ぶか PR 番号で呼ぶかの違いにすぎない。**

### F2. `digest_ms` の現存は 3 箇所（issue の挙げた 2 箇所＋ 1 箇所）

`git grep -n digest_ms` を歴史記録（`docs/adr/` / `docs/superpowers/`）を除いて走らせた結果:

| # | 箇所 | 文面 | 語が書かれた時点 | 段落が最後に触られた時点 |
|---|---|---|---|---|
| A | `src-tauri/src/startup.rs:380` | 「（反復 6 が `digest_ms` を足してフェーズ間の隙間を塞いだのと同じ形）」 | `1133fa6e`（2026-08-09・撤去**前**） | 同左（以後 touch 無し） |
| B | `PERFORMANCE.md:2178` | 「`LoadOrScanStats` に `digest_ms` を足し、…出すようにした」 | `9acd129f`（2026-08-07・撤去**前**） | **`46386da6`（2026-08-25・撤去後）** |
| C | `PERFORMANCE.md:1954` | 「（`digest_ms` は 8〜17 ms なので digest 区間は約 2 倍になる）」 | `4c96fef7`（2026-08-08・撤去**前**） | 同左（以後 touch 無し） |

**C は issue が挙げていない。**

**B だけは「撤去前に書かれ、以後誰も見ていない記録」ではない**（初稿はそう総括していた。敵対枠が
争点 3 で破り、主エージェントも独立に同じ証拠へ到達した）。`git log -L 2170,2185:PERFORMANCE.md`
実測: `1a405180`（#1181）と `46386da6`（#1182）が **2026-08-25 に同じ段落を 2 度編集**している。
`git show 46386da6 -- PERFORMANCE.md` の差分は決定的である——**`digest_ms` の 1 行手前で
`cache_load_ms` → `cache_load` を書き換え、`digest_ms` の直後に新しい注記文を追記しながら、
`digest_ms` だけを素通りした**（同コミットは `scan_ms` → `scan`・`cache_read_ms` → `cache_read` も
遡及的に直している）。

**つまり issue #1180 の本題（改名と撤去は別クラスか）には、同じコミットの隣り合う行に一次証拠が
在る。** 改名には遷移先の名前が在るので機械的に直せたが、撤去には無いので手が止まった。

### F3. `PERFORMANCE.md` は「撤去された概念」に対する既定の作法を**既に持っている**

同文書は撤去済みの語彙を消さず、**追記した注記で無効化を宣言する**形を反復して使っている。

| 行 | 形 |
|---|---|
| `PERFORMANCE.md:70-72` | 節冒頭の引用ブロック「**この節は WebView2 期の記録である**（#532 SU7 でフロント撤去済み）」 |
| `PERFORMANCE.md:176` | 「**この根拠は #1001 で消えた**（再スキャンごと撤去した）」 |
| `PERFORMANCE.md:1943` | 「**この消費者は #1001 で消えた**（再スキャンも digest も撤去した）」 |
| `PERFORMANCE.md:1972` | 「**この見送りは対象ごと消えた**（#1001）」 |
| `PERFORMANCE.md:2219` | 「**解消済み**（反復 6・上の…）」 |
| `PERFORMANCE.md:2291-2294` | 「> **後日（#996）**: この 3 者のうち…」 |

**C（`PERFORMANCE.md:1954`）は、その 11 行上（1943）に既に「この消費者は #1001 で消えた」注記を持つ。**
B（2178）が属する節「採用: 背景再スキャンの比較を digest へ（…反復 6）」は、下位の節
`PERFORMANCE.md:2219` に「解消済み」注記を持つが、**B の段落自身には撤去の注記が無い。**

### F4. 「改名は反映する」は明文の裁定ではなく観測された挙動である。撤去へ類推してはならない

**issue #1180 は「#1027 では改名について『反映する』と裁定した」と書くが、#1027 自体に文書運用の
明文は無い**（`gh issue view 1027` 実測——`LoadOrScanStats.total` を `Duration` へ変える実装 issue で、
受け入れ条件はコード変更と doc 反映のみ。散文への遡及反映を宣言した箇所は無い）。敵対枠が争点 5 で
これを突いた。**「裁定」の実体は `1a405180` / `46386da6` が実際に `PERFORMANCE.md` の歴史記録を
書き換えたという観測された挙動であり、明文の規範ではない。**

観測された挙動（`git show` 実測）: `total_ms` → `total`（`:1883`）・`scan_ms` → `scan`・
`cache_load_ms` → `cache_load`・`cache_read_ms` → `cache_read` を歴史記録の中で遡及的に書き換えた。

**そして issue #1180 自身が、この類推を先回りして禁じている**（本文「判断を要する点（これが本題）」
の逐語）——「**#1027 では改名（概念が生きたまま名前が変わる）について「反映する」と裁定したが、
撤去（概念そのものが消えた）は別クラスである**——`digest_ms` に「現在の正しい名前」は存在しない」。

**したがって「改名を反映したのだから撤去も反映すべき」は成立しない。** 改名の遡及反映が正当なのは
**指示対象が保たれる**（`total_ms` と `total` は同じものを指す）からであって、撤去にはその保証が無い。
撤去語を「今の名前」へ書き換えることは原理的にできず、選べるのは (i) 注記を追記する
(ii) 現在形をやめて散文化する (iii) 何もしない の 3 つだけである。**F3 の 6 例が示す同文書の作法は
(i) であり、`.claude/rules/governance-docs.md:20` が示す規範は (ii) である。**

`PERFORMANCE.md:30-40` の規約は次を言う。

- L36「**適用は、これ以降に新しく書く記録に限る。既存の記述へ遡及して補完しない。**」——ただし
  この文の主語は直前の L32-34（**測定値の出所・機体名**）であり、識別子の名前ではない。
  遡及を禁じる理由も「過去の測定がどちらの機体で取られたかを知る手段は無い」であって、
  識別子には当てはまらない（撤去したのが誰かは `git log -S` で確定できる）。
- L40「**この文書を『今も支えている値』と『歴史』に分けない。**」——節の分割を禁じるのであって、
  節の中に無効化の注記を置くことは禁じていない（F3 の 6 例が同文書内の反例）。

### F5. 撤去語彙の走査（初稿・**方法論に欠陥があった**）

#1023（`ae3335df`）の `.rs` 差分から削除された項目名を機械抽出し（`pub`/`fn`/`struct`/`enum`/
`trait`/`const`/`static`/`mod` の削除行）、主要語彙を `G-stale-identifiers` の**母集団外**の層
（`*.rs` / `PERFORMANCE.md` / `RETROSPECTIVE.md` / `README*` / `CONTRIBUTING.md` / `scripts/**` /
モジュール `CLAUDE.md`）へ当てた。走査語: `BackgroundRescanTask` `RescanOutcome` `RescanRun`
`RescanRecord` `DigestSource` `LoggedOutcome` `INDEX_GENERATION` `DIGEST_CHUNK`
`try_background_rescan` `entries_digest` `digest_over` `digest_ms` `rescan_task`
`try_with_index_write_lock` `current_index_generation` `snapshot_index_generation`
`load_with_index_generation` `lower_current_thread_priority` `setup_background_rescan`
`apply_rescanned_index` `rescan_log` `rescan-log` `unattributed_ms` `背景再スキャン` `再スキャン`。

**この走査は不完全だった。** 語を機械抽出しておきながら、そこから「主要語彙」を**手で選んで**
grep に掛けたため、抽出済みの `measure_raw_path_rebuild_cost_over_real_index` が落ちた
（敵対枠が争点 2/7 で発見し、主エージェントが `git grep` で追認）。**方法論の誤りは
「決め打ちのリスト」ではなく「機械抽出の後に手選びを挟んだこと」である。**

### F5'. 手選びを外した完全走査（コード識別子の軸。散文の軸は F5 が持つ）

`ae3335df` の削除行から (a) `.rs` の項目定義名、(b) `.rs` のフィールド名、(c) `.md`/`.ps1`/`.json`
の削除されたコードスパンを機械抽出し（165 語）、**1 語も手で落とさずに**現存判定
（`git grep -qw` を `*.rs` `*.ts` `*.mjs` `*.ps1` `*.psm1` `*.json` `*.toml` へ）へ掛けて
**撤去済み 83 語**を得た。その 83 語を生きた層（`*.md` `*.rs`・`docs/adr/`・`docs/superpowers/`・
`workspace/` を除く）へ当てた全件が下表である。

**この機械抽出が見るのはコード識別子だけである**——`AGENTS.md` の撤去の行が要求する「その層が
持ち込んだ**語彙**」には**散文の語**（`背景再スキャン` / `再スキャン`）も含まれ、それらは
定義行を持たないので抽出に乗らない。**散文の軸は F5 の初稿走査が別に当てており**（走査語の末尾
2 語）、生きた層の全出現が `#1001` / `#1023` の注記を伴うことを確認済みである。
**したがって数え上げの正本は「F5' の 83 語 ∪ F5 の散文 2 語」であって、F5' 単独ではない。**

| 撤去語 | 生きた層での出現 | 注記の有無 | 振り分け |
|---|---|---|---|
| `measure_raw_path_rebuild_cost_over_real_index` | `PERFORMANCE.md:1951`（+1952「同上」） | **無し・現在形の表の「計器」欄** | **直す（D）** |
| `digest_ms` | `PERFORMANCE.md:2178` | 無し（段落は撤去後に 2 度保守された） | **直す（B）** |
| `try_background_rescan` | `PERFORMANCE.md:2287` | **注記自体が「残る 2 者は今も…」と現在形で偽**（`PERFORMANCE.md:2291`） | **直す（E＝旧 F6）** |
| `digest_ms` | `PERFORMANCE.md:1954` | 11 行上（`:1943`）に「この消費者は #1001 で消えた」 | 直さない（C） |
| `digest_ms` | `src-tauri/src/startup.rs:380` | 過去形の引き合い（「反復 6 が…塞いだ」） | 直さない（A） |
| `BackgroundRescanTask` | `PERFORMANCE.md:2161,2213,2216` | `:2219`「**解消済み**（反復 6…）」 | 直さない |
| `entries_digest` | `PERFORMANCE.md:724,1939,1967,1972` | `:1943`/`:1972` に撤去注記。`:724` は日付つき歴史測定 | 直さない |
| `sorted_comparison_ignores_enumeration_order` | `PERFORMANCE.md:1970,2210` | `:1972`「対象ごと消えた」が `:1970` を覆う。`:2210` は当該反復の過去形の記録 | 直さない |
| `background_rescan_does_not_rewrite_when_format_is_current` / `background_rescan_upgrades_stale_format_when_entries_are_unchanged` | `PERFORMANCE.md:2123,2124` | 無し。ただし**当時の変異注入の実測結果**を過去形で述べた一次証拠 | 直さない |
| `cached_version` / `Unchanged` | `PERFORMANCE.md:2122,2203` | 削除されたコードの逐語引用（歴史） | 直さない |
| `rescan-log.jsonl` | `PERFORMANCE.md:2851,2854`・`snotra-core/CLAUDE.md:265` | 過去形・`#1001` の注記あり | 直さない |

**偽陽性 1 件**: `LoadCacheResult.version`（`snotra-core/CLAUDE.md:175`）はドット付き名を
`git grep -w` に掛けたための取りこぼしで、`version: u32` は `snotra-core/src/indexer.rs:1159` に
**現存する**（`load_cache` も `:1174` に現存）。撤去語ではない。

**残存はほぼ `PERFORMANCE.md` に集中する。** 生きた層の他の面（`src-tauri/**`・`snotra-core/**` の
`.rs` と `CLAUDE.md`）は #1023 が撤去と同時に注記を入れており、`startup.rs:380` の 1 件を除いて
腐りが無い。`scripts/**` の `index_load_unattributed_ms` は**現存する出力キー**であり撤去語彙では
ない（`src-tauri/src/startup.rs:426` で今も生成される）。

**#1155 型（9 群が残る）の再発は観測されなかった**——注記の無い残存は 3 件（B・D・E）である。

### F6. D（新発見）が最も害が大きい理由——同じ表に生きた計器と死んだ計器が並んでいる

`PERFORMANCE.md:1949-1952` の表は 3 行あり、「計器」欄に 2 つの関数名が並ぶ。

| 行 | 計器 | 実在 |
|---|---|---|
| 1950 | `path_store_raw_matches_target_path_over_real_index` | **現存**（`snotra-core/src/search/tests/path.rs:319`・実測） |
| 1951 | `measure_raw_path_rebuild_cost_over_real_index` | **撤去済み**（`git grep` 0 件・`ae3335df` が削除） |
| 1952 | 同上（＝ 1951 を指す） | 撤去済み |

**両者は名前が似ており、書式も同じコードスパンで、欄の見出しも同じ「計器」である。**
読者は 1 行目を検算できてしまうので、2〜3 行目も同じように在ると読む。`digest_ms` の 3 件が
「読んだ人が実在しないフィールドを探す」害だとすれば、**ここは「探した結果、隣の行は見つかるので
自分の探し方が悪いと思う」害**であり、一段深い。

### F7. 走査が拾った、腐りではない 1 件（射程外だが記録に値する）

**`snotra-core/src/indexer.rs:678` `scan_identity_hash`** — doc が「呼び出し元は #1001 で撤去済みで、
この関数は現在どこからも呼ばれない…消費者が戻る日のために残す」と**明示的に宣言している**。
注記は正しく、腐りではない。**散文ではなくコードの残置なので issue の射程外**。

## 関連ファイル・シンボル（実在確認済み）

| パス | 対象 |
|---|---|
| `src-tauri/src/startup.rs:380` | `Timeline::to_json` 内のコメント（`unmarked_tail_ns` の直前） |
| `PERFORMANCE.md:1954` | 「反復 10」節・払い戻しの段落 |
| `PERFORMANCE.md:2178` | 「#### 計測の隙間がこの複製を隠していた」 |
| `PERFORMANCE.md:2291` | 「> **後日（#996）**」注記（F6） |
| `PERFORMANCE.md:30-40` | 「## この文書へ記録するときの規約」 |
| `.claude/rules/governance-docs.md:20` | 「歴史を書くならバッククォートを外して散文にする」 |
| `scripts/governance/lib.mjs:694,700,711,718` | `STALE_EXTRA_DOCS` / `staleIdentifierDocs` / `staleIdentifierGuideDocs` / `staleIdentifierTargets` |
| `scripts/governance/checks/G-stale-identifiers.mjs` | 検査本体 |

## 技術的制約

- **`G-stale-identifiers` の母集団は `.claude/{skills,rules,agents}/*.md` ＋ `docs/**`（`docs/adr/`・
  `docs/superpowers/` を除く）＋ `STALE_EXTRA_DOCS` の 4 本**（`scripts/governance/lib.mjs:694-720` 実測）。
  `*.rs` も `PERFORMANCE.md` も**モジュール `CLAUDE.md`**（`snotra-core/CLAUDE.md` 等）も母集団外である。
  issue の記述は正しい。
- `.claude/rules/governance-docs.md:20` のバッククォート除去則は、原文では **見出しの正準形**
  （`G-heading-refs` の対象）についての規則である。識別子への適用は「同型」であって直接の規則ではない。
- `PERFORMANCE.md` を編集すると `AGENTS.md`「条件別チェック」のガバナンス文書行が発火する
  → `npm run governance:check`。`*.rs` のコメント編集は PostToolUse hook が検査を割り当てる。
- **`PERFORMANCE.md` は `G-stale-identifiers` の母集団外なので、直しても直さなくても検査は沈黙する。**
  つまりこの issue の成果は機構では保護されない——issue 自身がそれを受容する前提で書かれている。

## 振り分けの判定基準（調査から導いた規則。個々の列挙ではなくこれを正本とする）

F5' の全件を振り分けるために、次の 1 つの問いへ還元した。

> **その文を読んだ人が、その識別子を「今このリポジトリに在るもの」として探しに行くか。**

行くなら「在る前提で書いている」＝直す。行かないなら「撤去を描写している」＝直さない。
判定の材料は 3 つで、いずれも機械的に確かめられる。

| 材料 | 「在る前提」の側 | 「描写」の側 |
|---|---|---|
| 時制 | 現在形（「今も〜する」「〜になる」） | 過去形（「〜した」「〜だった」） |
| 文中の役割 | **指示的**（表の「計器」欄・「検知器は X」など、読者に引かせる形） | 出来事の引き合い |
| 直近の注記 | 段落に撤去注記が無い | 同じ段落か直前に「#NNNN で消えた」が在る |

**この基準は issue が挙げた 2 箇所を含む全 11 群へ当てて検算済みである**（F5' の表）。
結果は「直す 3 件・直さない 8 群」で、**issue が名指しした 2 箇所のうち直すのは 1 つ（B）だけ**になる。

## 未解決の疑問（`plan.md` の未確定欄で潰す）

1. **D・B・E の 3 件をどの形で直すか**——(i) 注記を追記する（F3 の同文書の作法・6 例）か
   (ii) バッククォートを外して散文にする（`.claude/rules/governance-docs.md:20` の同型・issue 案）か。
   **両者は排他ではない**（D は「計器」欄という指示的な役割ゆえ (ii) が効き、B・E は散文なので (i) が合う）。
2. **A（`startup.rs:380`）を「直さない」で確定してよいか。** 上の基準では過去形かつ引き合いなので
   直さない側だが、**issue が名指しした 2 箇所の片方を「直さない」で閉じることになる**ので、
   受容する残余として明記する必要がある。
3. **E（`PERFORMANCE.md:2287-2294`）を範囲に入れるか。** `digest_ms` ではないが、
   issue のチェック項目 3（同じクラスの残存の数え上げ）の直接の産物である。⚠️ 敵対枠も
   「同じクラスか判断が分かれる」と留保した（注記自体の腐り vs 裸の識別子）。
4. **再発防止の機構を足すか。** **足さないことを推奨する**——`.claude/rules/governance-docs.md`
   の「書く約束」(3)「**古い情報を残さない——触った節の隣にある主張が今も真か見る**」が既に
   この形を名指しており、#1182 はそれを守り損ねた。規範を 2 枚目に書くのは `AGENTS.md`
   「文書に事実の写しを増やす変更」に当たる。`docs/adr/ADR-measurement-record-provenance.md` は
   既に「`PERFORMANCE.md` を `governanceDocs` へ足さない。穴は**開いたまま残る**」と
   **機構の不在を明示的に受容済み**である。
