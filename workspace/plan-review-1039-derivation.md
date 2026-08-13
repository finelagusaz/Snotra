# #1039 独立導出（コードのみから）

対象 issue: **#1039**「行の差し替えと in-flight の失効を同じ型の内側へ入れる」

本文書は `workspace/plan.md` / `workspace/research.md` / `workspace/adversarial-1039.txt` を**一度も読まずに**、`gh issue view 1039 --comments`（本文＋コメント 2 件）とコードだけから導出したものである（`.claude/skills/plan-review/SKILL.md` Step 2b）。

## 0. 導出の方法（列挙の手段を明記する）

**LSP ツールはこのエージェントのツールセットに存在しない**（`findReferences` / `outgoingCalls` は未提供）。`AGENTS.md`「条件別チェック」表の「LSP の無い環境でのみ grep へ落とす」に従い ripgrep を使った。使ったパターンと母集団の切り方は以下。

| 目的 | コマンド | 備考 |
|---|---|---|
| production の `set_results` 呼び出し点 | `rg -n "state\.set_results" src-tauri/src/` | `self.state.` 越しの呼びだけを拾う。`search_state.rs` の `#[cfg(test)]` 内は `s.set_results(...)` ゆえこのパターンから自然に落ちる（テスト除外の根拠） |
| 全 `set_results`（テスト込み） | `rg -n "set_results" --type rust` | 差分がテスト呼び出し |
| 概念ラベルの散文 | `rg -n "is_unsettled\|set_results\|SearchDispatch\|rows_generation\|search_dispatch\|SearchState" --glob "**/*.md"` | `workspace/` と `docs/superpowers/` と `RETROSPECTIVE.md` を除外して評価 |
| スクリプト | `rg -n "is_unsettled\|set_results\|in.flight\|未反映\|SearchDispatch\|rows_generation\|search_dispatch\|invalidate" scripts/` | 4 件ヒット（すべて `SnotraTraceInvariants.psm1` のコメント） |

**grep の残余**: 同名の別物（`Vec::set_results` 等）は本リポジトリに無く、re-export 経由の呼びも `mod.rs:89` の 1 本を目視で確認済みだが、**LSP の照合を経ていない点は残余として明記する**。

## 1. 導出した設計の形

`SearchState` が `SearchDispatch` を**フィールドとして所有**し、行を差し替える唯一の関数 `apply_rows` がその内側で照合・失効を行う。

```rust
pub enum RowOrigin { Sync, Worker(u64) }          // Worker(seq) は照合対象

impl SearchState {
    /// 検索要求へ seq を振る（controller は `Instant` を渡すだけで、時計は読ませない）
    pub fn issue_search(&mut self, key_at: Instant, now: Instant) -> u64;

    /// 行を差し替える唯一の関数。Worker(seq) は in-flight と照合し、不一致なら**行を触らずに** None。
    #[must_use]
    pub fn apply_rows(&mut self, origin: RowOrigin, rows: Vec<SearchResult>, now: Instant)
        -> Option<Settled>;

    pub fn is_settled(&self, armed: bool) -> bool;  // = !is_unsettled(armed, pending_seq)
    pub fn pending_seq(&self) -> u64;               // trace の材料（後述 4）
}
```

`enter_tool` / `on_escape`（tool 段・folder 段）/ `reset` も `apply_rows(RowOrigin::Sync, …)` を通す。これで「行が差し替わったなら飛んでいる結果は必ず古い」が関数の内側の事実になる。

**`Instant` は引数で受けるだけで、核の中で `Instant::now()` を読まない**——純粋核の性格（テスト可能性）は保たれる。ただし「計時を核へ持ち込むか」は issue 本文が「設計を詰める前に決めること 1」として未決に置いた論点であり、ここでは**選択肢として提示するに留める**（→ 未検証 U-1）。

**2 つのカウンタは統合しない**（issue の制約）。`rows_generation`（行が差し替わったか）と `SearchDispatch::next_seq`（どの要求か）は別フィールドのまま同居する。**folder は触らない**——`folder_gen` / `accept_folder_result` は無改造（`search_state.rs:245-309`）。

## 2. 変更が要るファイルとシンボル

| ファイル | 対象シンボル | 変更の内容 |
|---|---|---|
| `src-tauri/src/egui_shell/search_state.rs` | `SearchState`（フィールド追加: `dispatch`）・`set_results`（→ `apply_rows` へ改組・**public を消すか Sync 専用の薄いラッパーにするかは実装判断**）・`enter_tool:315`・`on_escape:360`（tool 段 361-368・folder 段 369-376）・`reset:389`・`rows_generation` の doc `:153-164`・`should_flush_on_enter` の doc `:414`・新規 `issue_search` / `is_settled` / `pending_seq` | 主戦場 |
| `src-tauri/src/egui_shell/search_dispatch.rs` | `is_unsettled:79` とその doc `:65-78`（`SearchState` へ移設）・`invalidate` の doc `:54`・`//!` `:1-5`・テスト 4 本 | `SearchDispatch` 型自体は残す（`accept` / `issue` / `invalidate` / `pending_seq` は `SearchState` の内側から呼ばれる privateized な部品になる） |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `start_launch:262`(272-274)・`clear_search:431`(433-434)・`run_search_with:759`(769-774, 786-787, 805-806, 837-838, 855-859)・`drain_search:868`(876-897 と doc :867)・`consume_reset_pending:974`(982-983)・`on_enter:1314`(1319-1341)・フィールド `dispatch:134` | 11 の呼び出し点が `apply_rows` へ寄る。`dispatch` フィールドは `SearchState` へ移るので削除 |
| `src-tauri/src/egui_shell/mod.rs` | `:88-89` の `pub(crate) use search_dispatch::{SearchDispatch, is_unsettled};` とその上のコメント | `is_unsettled` の re-export が消える。`SearchDispatch` も controller が持たなくなるなら不要 |
| `src-tauri/src/egui_shell/results_view.rs` | `RowsSnapshot::input_idle` の doc `:34-36` | `is_unsettled` への intra-doc link と、そこから導いた式の 1 行（→ 要対処 A-3） |
| `src-tauri/src/egui_shell/layout.rs` | `:387` のコメント内の `set_results(Vec::new())` という平文の名指し | 改名で沈黙して腐る（→ 軽微 B-2） |
| `docs/architecture.md` | `:158`（mermaid の participant `Disp`）・`:172-213`（invalidate と set_results の矢印 7 組）・`:230`（bullet 全体） | → 要対処 A-2 |
| `src-tauri/CLAUDE.md` | `:37`（`search_dispatch.rs` の索引行）・`:41`（`search_state.rs` の責務行） | **ファイルを消すなら索引行も消す**（`npm run governance:check` が捕捉）。消さないなら索引は不変で、`:41` の括弧内の列挙に新シンボルを足すかは任意 |

**触らないと導出したもの**: `SPEC.md`（→ 5）・`snotra-core/`・`scripts/`（→ 6）・`search_worker.rs`（`SearchRequest` / `SearchMsg` の形は不変）・`view.rs`（`drain_search` / `poll_search_debounce` / `on_enter` の**呼ぶ順序**は不変。`view.rs:1076-1080` のコメントが記す順序制約は `apply_rows` 化で変わらない）。

---

## 要対処

### A-1. **母集団が issue の記述より 1 件多い（11 箇所であって 10 箇所ではない）**

issue 本文は「production 呼び出しは **10 箇所**（…）。うち 9 箇所が直前で `dispatch.invalidate()` を呼び、`drain_search` の 1 件だけが対象外」と書き、母集団を `main = 23cba3c`（2026-08-11）で数えている。**現在の main（8ca9950）では 11 箇所である。**

```
$ rg -n "state\.set_results" src-tauri/src/ | wc -l
11
```

増えた 1 件は `launcher_controller.rs:805-806`（検索 worker への `send` が `Err` を返す枝）。

```
$ git log -1 --format='%h %ad %s' --date=short aba1356
aba1356 2026-08-12 fix(egui): 検索 worker の死を送信 Err で検知し、古い行の誤起動を止める (#1053)
$ git log -1 --format='%h %ad %s' --date=short 23cba3c
23cba3c 2026-08-11 chore(egui): #1032 の調査足場を撤去する (#1037)
```

**正しい現在形は「同期 10 箇所（すべて直前で `invalidate` を呼ぶ）+ `drain_search` の 1 箇所（`accept` が pending を take するので対象外）= 11」である。** 新しい 1 件も規律を守っているので**漏れは増えていない**が、**issue 本文の数を計画へ写すとその瞬間に腐る**。計画は数ではなく「`rg -n "state\.set_results"` が返す全件」を指すべきである。

### A-2. `docs/architecture.md:230` の bullet が、この変更で**偽になる**

当該 bullet は #1039 が閉じようとしている穴を、閉じていない前提で記述している。

> **射程は `set_results` の呼び出し点である**——`SearchState` の `enter_tool` と `on_escape` は `results` を直に置き換えるので、この規律の外にある（`reset` は呼び出し側の `consume_reset_pending` が `invalidate` を撃つ）。**その 2 つには in-flight が残りうる**: … 飛んでいた結果が次フレームで tool の行を置き換える並びは構造上ありうる（**未再現の観察である**。…）

`apply_rows` が入ると「規律の外にある」「in-flight が残りうる」「構造上ありうる」の 3 つがすべて偽になる。**同じ変更で書き換えること**（`AGENTS.md` 3層分担の第3層）。

同ファイルの mermaid（`:158` の participant `Disp as search_dispatch.rs（seq）` と `:172/176/180/186/212` の `View->>Disp: invalidate()` 計 5 本 + `:203` の `View->>Disp: accept(seq) → Settled`）も、矢印の宛先が `State` の内側へ移るため図として偽になる。`:204` の `set_results（世代はここで進む・#699）` と `:210` の `should_flush_on_enter ∘ is_unsettled` も名前が変わる。

### A-3. `results_view.rs:36` の「式からの導出」1 行が沈黙で腐る

```rust
/// **これは perf ヒューリスティックであって正しさの述語ではない。**
/// [`crate::egui_shell::search_dispatch::is_unsettled`]（最終クエリの結果が未反映か）
/// とは別概念であり、否定の関係にも無い——食い違うのは `armed == false ∧ pending != 0` のときである。
```

- **intra-doc link は `cargo doc` が守る**——`is_unsettled` を消せば link 切れで落ちる（`docs/build-commands.md` カテゴリ・`.claude/rules/comments.md` の「トリガー → 検査」）。
- **式（`armed == false ∧ pending != 0`）は誰も検査しない。** `is_settled(&self, armed)` が `!(armed || pending != 0)` と同値であれば式は真のまま残るが、**極性が反転する**ため「`is_unsettled` から導いた」という前置きが読めなくなる。

これは #1074 の申し送り 2 が「この issue の再検査対象に入れてほしい」と名指した箇所そのものである。**リンク先を `SearchState::is_settled` へ張り替えつつ、式の側も新しい関数の形で書き直すこと。**

### A-4. `apply_rows` の返り値が trace の 2 本を殺さない形であること（設計制約）

現在 `drain_search` は `accept` の返り値で 2 つの trace を撃ち分けている（`launcher_controller.rs:876-897`）。

- 一致 → `egui_search:settled`（`dispatch_seq` / `pending_seq` / `index_entries` / `since_key_us` / `since_dispatch_us`）
- 不一致 → `egui_search:dropped`（`dispatch_seq` / `pending_seq`）

**照合を `apply_rows` の内側へ隠した瞬間、controller は「採ったのか捨てたのか」も「経過はいくらか」も分からなくなる。** ゆえに `apply_rows` は `Option<Settled>`（または `enum RowApplied { Settled(..), Dropped }`）を返し、`pending_seq()` の読み口も残さねばならない。落とすと:

- `scripts/lib/SnotraTraceInvariants.psm1` の **H7**（`:18`「`egui_search:settled` が `dispatch_seq < pending_seq` で現れたら異常」）が読む `data.dispatch_seq` / `data.pending_seq` が消える
- `scripts/smoke-egui.ps1:380` は区間の切り出しに `egui_input:changed → egui_search:settled` を使っており、イベント自体が消えると**区間が取れない**
- **欠落は沈黙する。** psm1 は `Get-SnotraTraceProperty` 経由で読む（`:404-406` のコメント）ため、フィールドが無い行は `$null` になり StrictMode 例外にはならない——**「値が無い」が「異常が無い」に化ける**

さらに psm1 `:402-405` が明記している射程の限界を、計画は前提に置くべきである。

> **とくに `dispatch.invalidate()` の呼び忘れは検知しない。** …古い行が生えるのに `pending_seq` は 0 で PASS になる。spec §4.5 の規則を守る機構は Task 9 の実装（同期出所すべてへの invalidate）自身であって、この検知器ではない。

**つまり #1039 が置き換えようとしている「機構」は、ハーネスから見えない。** 検知力はユニットテスト（→ A-6）が単独で担う。

### A-5. `armed` の disjunct をどこに置くか — 3 案と副作用

前提となる一次証拠を先に置く。

- `search_debounce` は controller のフィールドである（`launcher_controller.rs:108`）。`SearchState` からは見えない。
- `consume_reset_pending` は **`Debouncer` を丸ごと作り直す**（`launcher_controller.rs:987`: `self.search_debounce = Debouncer::new(Duration::from_millis(50), true);`）。
- `Debouncer::new` は `armed: false` で作る（`layout.rs:430-436`）。
- 対で動く `last_input_at`（`launcher_controller.rs:109`）は `consume_reset_pending` が**触らない**（`:977-1007` の本文に代入が無い）。

| 案 | 形 | 副作用（`consume_reset_pending` との相互作用） |
|---|---|---|
| **1（推奨）** | `is_settled(&self, armed: bool) -> bool` — 引数で受ける | **`Debouncer` と `last_input_at` の対がそのまま controller に残る**ため、`:987` の「丸ごと作り直し」という救いも `last_input_at` を触らなくてよい理由も無傷。変更量は最小 |
| 2 | `Debouncer` ごと `SearchState` の内側へ移す | **`last_input_at` を置き去りにできない**——`poll(self.last_input_at.elapsed())`（`:1297`）が対で使う。両方移すと `elapsed()` ＝時計読みが純粋核へ入る。さらに `:987` の作り直しは `SearchState::reset()` の内側へ移す必要があり、`reset()` の**2 番目の呼び出し点**（`launcher_controller.rs:354`・→ B-1）でも debounce が作り直されることになる。**これは現状の挙動変更である**（今は `clear_search:435` の `cancel()` だけが armed を下ろす。結果は同じ `armed=false` だが `interval` / `leading` まで作り直す点が違う） |
| 3 | 合成は controller の 1 メソッドに残し、`SearchState` は in-flight だけ持つ | 現状の `is_unsettled(armed, pending_seq)` を controller のメソッドへ改名するだけ。**#1038 の申し送り「述語を型の内側へ移す」を実質果たさない** |

**案 1 を推奨する。** ただし**正直な残余を書いておく**: 案 1 でも合成（`armed || in_flight`）は型が構造的に所有せず、呼び出し側が `armed` を正しく渡す規約に依存し続ける。**#1039 が消すのは「(B) の失効」の規約であって「(A′) `armed` の合成」の規約ではない。** 計画がこの 2 つを混ぜて「規約が機構になった」と書くと、実装より強い主張になる（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

**受け入れ条件（issue コメント 1 の指定を逐語で満たす）**: `armed` の disjunct を落とす変異（`fn is_settled(&self, _armed: bool) -> bool { self.dispatch.pending_seq() == 0 }`）でテストが落ちること。新品の `SearchState` に対する `assert!(!s.is_settled(true))` がこれを殺す。

### A-6. 必要なテストと、その検知力（変異 → 落ちるべきテスト）

**移設で消えるものを先に補償する。** `search_dispatch.rs` のテスト 5 本のうち 2 本は移設しないと検知器ごと消える。

| # | テスト | 変異（改悪） | 現状 |
|---|---|---|---|
| T1 | `unsettled_covers_in_flight_after_trailing_fired` を `search_state.rs` へ移設（**`should_flush_on_enter` との合成 assert `:151-156` を含めて逐語で**） | `armed` の disjunct を落とす | `search_dispatch.rs:137-157`。**#1038 の合成を固定している唯一の検知器**（同 doc `:77` が明言）。関数と一緒に消える |
| T2 | `unsettled_is_grounded_on_real_dispatch` を移設（sentinel をリテラルで書かず `SearchState` 自身から取る） | `pending_seq` の sentinel 解釈を反転 | `search_dispatch.rs:159-181`。**接地テストの不動点化を避ける形**（`grounding-test-becomes-fixed-point` の教訓）が既に書かれている |
| T3 | `stale_result_is_dropped_after_synchronous_replacement` を `apply_rows` の形で移設 | `RowOrigin::Sync` の枝で `invalidate` を落とす | `search_dispatch.rs:183-197` |
| T4 | `accepts_only_the_latest_seq` / `accept_is_once_per_issue` / `invalidate_drops_in_flight` | `accept` の照合を外す | `search_dispatch.rs:87-134`。型が残るなら**そのまま残す** |

**新規（Red が先に立つもの — 今日のコードで落ちること）**。これが #1039 の本体である。

| # | テスト | 変異 |
|---|---|---|
| T5 | `enter_tool_invalidates_in_flight`: `issue_search` → `enter_tool` → 旧 seq の `apply_rows(Worker(seq))` が `None`（＝ツール行が上書きされない） | `enter_tool` を `apply_rows` から外す |
| T6 | `escape_from_tool_invalidates_in_flight`: 同上を `on_escape` の tool 段で | `on_escape` の tool 段を `apply_rows` から外す |
| T7 | `escape_from_folder_invalidates_in_flight`: 同上を `on_escape` の folder 段で | `on_escape` の folder 段を外す |
| T8 | `reset_invalidates_in_flight` | `reset` を外す |
| T9 | **`rows_generation` の両方向が保たれる**（既存 `search_state.rs:591-679` の 8 本）——**`apply_rows` 化で 1 本も落ちてはならない**。とくに `rows_generation_is_stable_on_enter_folder:638` と `..._on_escape_to_hide:648` と `..._on_selection_change:672`（進めすぎの側） | `enter_folder` を誤って `apply_rows` へ通す（＝進めすぎ） |

**T9 は「進めすぎ」を殺す枠である**——`enter_folder` は `results` を frame へ退避するだけで差し替えない（`search_state.rs:330-331` は `std::mem::take` だが、これは `enter_tool` の側）。`apply_rows` を「行に触る全経路」へ機械的に当てると `enter_folder` まで通しかねず、そのとき #699 の照合が正当なクリックを全部捨てる。

**検知力の空白（正直に書く）**: T5〜T8 が測るのは `SearchState` の内側だけである。`drain_search` が `apply_rows` を呼ぶ配線（`launcher_controller.rs:886`）が消えても、これらは緑のままである。同種の限界は既存コードが 2 か所で自認している（`search_state.rs:792-798` の #743 ブロック冒頭・`:896-898` の #838 ブロック冒頭）ので、**計画も同じ様式で受容残余として書くこと**。

---

## 軽微

### B-1. `SearchState::reset()` の呼び出し点は 2 つあり、issue の表は 1 つしか挙げていない

```
$ rg -n "state\.reset\(\)" src-tauri/src/
src-tauri/src/egui_shell/launcher_controller.rs:354:                    self.state.reset();
src-tauri/src/egui_shell/launcher_controller.rs:982:            self.state.reset();
```

issue 本文の表は `reset` の行に「呼び出し側 `consume_reset_pending` が撃つ」とだけ書く（＝ `:982` のみ）。**`:354` は `finish_launch` の `LaunchTag::Tool` 成功枝**であり、直前の `clear_search()`（`:353`）が `:433` で `invalidate` を撃つため**結果として (B) は守られている**。実害は無いが、**「呼び出し側が撃つ」という規約の成立根拠が 2 か所に分かれている**ことは計画が知っておくべきである（`apply_rows` 化後は両方とも内側で閉じる）。

### B-2. `layout.rs:387` の平文の名指しは、改名で沈黙して腐る

```rust
/// `start_launch` が `set_results(Vec::new())` を撃つため、行クリック起動フレームでは②が
```

`///` の中だが**リンク形ではない平文**なので、`cargo doc` も `governance:check` も検知しない。#1074 の申し送り 3 が名指した類型そのもの（「`.rs` の doc コメントに書いた平文の名指しは、コンパイラも `cargo doc` も `governance:check` も検知しない」）。**intra-doc link 形へ移すと次の改名が機構で守られる。**

### B-3. `search_state.rs:156-158` の呼び出し元列挙が偽になる

```rust
/// 全称の射程はこの型の内側に限る——外から `results` を差し替える経路は無い
/// （フィールドは private で、`set_results` / `enter_tool` / `on_escape` / `reset` だけが触る）。
```

`apply_rows` 化後は「`apply_rows` だけが触る」になる。**この doc は #1039 の主張そのもの（(A) が型の内側に住んでいる理由）を書いているので、書き換えは義務であって任意ではない。**

### B-4. `drain_search` の doc（`launcher_controller.rs:867`）

> **seq が現 pending と一致するときだけ行を差し替える**——追い越された結果は捨てる。世代は `set_results` が進める（#699 は無傷）。

照合の主語が `SearchState` へ移り、関数名も変わる。

### B-5. `mod.rs:88-89` の re-export とコメント

```rust
// is_unsettled は同 `on_enter` の flush 判定が消費する（#1038: `armed` だけでは worker の in-flight を覆えない）。
pub(crate) use search_dispatch::{SearchDispatch, is_unsettled};
```

`is_unsettled` が消えれば export も消える。`SearchDispatch` も controller が直接持たなくなるなら不要（**`dead_code` / 未使用 import は `-D warnings` 下で compile-fail になるので、これは機構が守る**）。

### B-6. インラインコメント「同期で差し替える＝in-flight は古い（spec §4.5）」×9

`launcher_controller.rs:272, 433, 769, 773, 785, 805(近傍), 837, 855, 858`。`apply_rows` の内側へ事実が移るので、**呼び出し点から消えるのが正しい**（残すと「規約がまだ在る」と読める）。なお `spec §4.5` の指す先は `SPEC.md` ではなく `docs/superpowers/specs/2026-08-10-search-worker-design.md:91`（「### 4.5 in-flight の失効 — 同期で行を差し替える経路はすべて `pending_seq` を進める」）である（実測）。

### B-7. `search_dispatch.rs` の `//!`（`:1-5`）と `invalidate` の doc（`:54`）

- `//!:3`「#699 の世代は `set_results` が持ったままにする」→ 関数名が変わる
- `:54`「**同期で `set_results` を呼ぶ出所は必ずここを通す**（spec §4.5）」→ **規約の宣言そのもの**。機構化されたら書き換える

### B-8. `src-tauri/CLAUDE.md` の索引行

`search_dispatch.rs` を**消す**なら `:35` のファイル名列挙と `:37` の索引行を消す（`npm run governance:check` / CI の `governance-check` job が捕捉する。`.claude/rules/pr-governance-check-before-pr` 相当の運用）。**残す設計（推奨）なら索引は不変。**

---

## 未検証

### U-1. `Instant` ベースの計時が純粋核に馴染むか（issue 本文が「決めること 1」として未決に置いた論点）

`SearchDispatch` を丸ごと `SearchState` へ入れると `Pending { key_at, dispatched_at }` と `Settled { since_key, since_dispatch }` も入る（`search_dispatch.rs:9-20`）。`search_state.rs` は現在 `std::time` を import していない（`:4-5` は `snotra_core` のみ）。

- 引数で `Instant` を受けるだけなら `Instant::now()` は controller に残り、テスト可能性は保たれる（`search_dispatch.rs:89-109` の既存テストが `base + Duration` を渡す形で実証済み）
- ただし「純粋核」の語義をどこまで厳密に取るかは**設計判断であり、コードからは導出できない**。seq 照合だけを移して計時を controller に残す案（issue 本文の示唆）も成立する——その場合 `SearchDispatch` が 2 つに割れ、**seq が 2 か所に住む**というこの issue が消そうとしている形が別の軸で再発しうる

**裁定は計画側に委ねる。ここでは選択肢の列挙に留める。**

### U-2. `PERFORMANCE.md` の 2 か所は書き換えるべきか

- `:538` `| `drain_search`（`set_results` 込み） | ≤ 92 | drain |`
- `:2576-2577` 「`SearchDispatch::issue` と `SearchState::set_results` しか挟まない区間で 12 ms」

どちらも**過去に実測した値の記録**である。名前は stale になるが、**書き換えると「新しい名前で測り直した」と読める**——`fixing-instrument-invalidates-ab-comparison` と `own-measurement-refutes-adjacent-prose` の型。**「旧名 `set_results`（現 `apply_rows`）」のような併記に留めるのが安全**だと考えるが、この判断は測定記録の運用規約に依るため未検証とする。

### U-3. `docs/superpowers/specs/2026-08-10-search-worker-design.md` §4.5 を編集するか

`:91`「### 4.5 in-flight の失効 — 同期で行を差し替える経路はすべて `pending_seq` を進める」・`:119`「同期で `set_results` する以上、§4.5 の規則に従って `pending_seq` を bump する」。**設計 spec は意思決定の記録**であり、#1039 で規約が機構へ変わっても「そのとき何を決めたか」は真のままである。**編集不要と考えるが、`docs/adr/` と違い spec の改訂規約を確認していない。**

### U-4. `apply_rows` に `#[must_use]` を付けるか

`src-tauri/CLAUDE.md`「処置を返す純粋核の強制（#934）」の規約に当たる。`&mut self` で状態を進めてから `Option<Settled>` を返す形なので、**規約の文面では「`Option` は型に `#[must_use]` を持たないので必ずメソッド段」に該当する**。ただし `Settled` は計時の材料であって「落とすと副作用が消える処置」ではない（落ちるのは trace だけ）。**同 CLAUDE.md が挙げる対象外 2 種（トークンを返すもの／状態を進めずに導くもの）のどちらにも綺麗には当てはまらない**ため、規約の当否は実装時に読み直す必要がある。

### U-5. `drain_search` の `while let` ループが `apply_rows` 化で意味を変えないか

現状 `:869-897` は「`accept` が `None` を返しても `continue` して次のメッセージを読む」形である。`apply_rows` が不一致で `None` を返す設計なら等価だが、**`apply_rows` が「行を触らない」ことをテストで固定していない場合、不一致でも `rows` を代入する実装ミスが通る**（T5〜T8 は `enter_tool` 等の経路を測るので、この配線を直接は測らない）。**T3 の移設がこれを覆うと考えるが、実装形が確定していないため未検証。**

### U-6. `search_state.rs` が `SearchDispatch` を持つと `SearchState::new()` / `Default` が変わる

`:168-179` と `:401-405`。`SearchDispatch` は `#[derive(Default)]`（`search_dispatch.rs:22`）なので機械的には通るが、`SearchState::new()` の呼び出しは同ファイル内に **35 件**ある（`rg -c "SearchState::new\(\)" src-tauri/src/egui_shell/search_state.rs` → `35`）。定義側の `:168`（`pub fn new()`）と `Default` の `:403`（`Self::new()`）はこのパターンに一致しないため 35 件には**含まれない**。35 件はすべて `#[cfg(test)]` 内である（最初の一致が `:593` で、tests mod の開始 `:489` より後——`rg -n "SearchState::new\(\)" … | head -3` と `rg -n "mod tests" …` で実測）。すべて `dispatch` が空の状態から始まるので影響は無いはずだが、**全件を目で確かめてはいない。**

---

## 5. `SPEC.md` の更新要否 — **不要（バグ修正の側）**

`AGENTS.md`「開発ワークフロー」1 の判定は「`SPEC.md` に当該挙動の記述があるか、その記述に**合わせる**のか**変える**のか」である。

**記述はある。**

```
$ rg -n "18\.5|上書きされない" SPEC.md
816:### 18.5 状態モデル
820:- ツール選択中の入力は無効化（検索結果が上書きされない）
```

`docs/architecture.md:230` が記録している未再現の観察——「飛んでいた結果が次フレームで tool の行を置き換える並びは構造上ありうる」——は、**`SPEC.md:820` の記述に対する違反**である。#1039 はコードを記述へ**合わせる**。ゆえに**仕様変更ではなくバグ修正であり、`SPEC.md` は更新しない**。

同じ判定を他の 2 経路にも当てる。`on_escape`（folder 復帰）と `reset` が守る「展開前状態への復帰」「resetForShow」は `SPEC.md` の §6 / §19.7 が記述しており、そこにも「古い結果が生え直してよい」とは書かれていない。**3 経路とも記述に合わせる方向である。**

**ただし `type:refactor` ラベル（`gh issue view 1039 --json labels`）との食い違いは指摘しておく**——`AGENTS.md` は「判定はラベルの意味ではなく 2 つの参照で決まる」と明記しており、ラベルが refactor でも実際には振る舞いの穴を 3 つ塞ぐ。**コミット種別を `refactor` にするか `fix` にするかは、SPEC 同期の要否とは独立の判断である**（`AGENTS.md`「『fix』というコミット種別は SPEC 同期を免除しない」の裏返し）。

## 6. 既存の検査の前提が壊れないか

| 検査 | 前提 | 判定 |
|---|---|---|
| `scripts/smoke-egui.ps1:380` | 区間の切り出しに `egui_input:changed → egui_search:settled` を使う | **A-4 の設計制約を守れば壊れない。** 守らないと区間が取れず沈黙で SKIP 相当になる |
| `scripts/smoke-egui.ps1:396` | `Start-Sleep -Milliseconds 120` で debounce(50ms) の trailing を跨がせる | **不変**（`Debouncer` の interval を動かさない案 1 を採る限り） |
| `SnotraTraceInvariants.psm1` H7（`:18`, `:385-406`） | `egui_search:settled` の `data.dispatch_seq` / `data.pending_seq` を読む | **フィールド名を保てば不変。** `pending_seq()` の読み口を残すこと |
| 同 H1（hidden 区間に `egui_results:show`） | results 窓の可視性 | **無関係** |
| `cargo doc --workspace --no-deps --document-private-items` | intra-doc link の実在 | **`is_unsettled` を消すと `search_state.rs:414` と `results_view.rs:36` の 2 本が落ちる**——これは**望ましい**（機構が移設漏れを捕まえる）。ただし `.claude/rules/comments.md` のとおり **PostToolUse hook は沈黙する**ので手で走らせること |
| PostToolUse hook（fmt / clippy / test） | `.rs` 編集で自動 | `-D warnings` 下で未使用 import / `dead_code` が移設漏れを捕まえる（B-5） |
| `npm run governance:check` | モジュール索引の実在 | **`search_dispatch.rs` を消す場合のみ**関係する（B-8） |

**トレース名（`egui_search:settled` / `egui_search:dropped`）を変えないこと**が最大の前提である。`src-tauri/CLAUDE.md`「機能削除・trace イベント名／hotkey 登録・表示経路の変更」のトリガーに当たる。

---

## 付録: 一次証拠の索引

| 主張 | 出所 |
|---|---|
| `set_results` production 11 箇所 | `rg -n "state\.set_results" src-tauri/src/`（本文 A-1 に全 11 行） |
| 増分は #1053 | `git log -1 --format='%h %ad %s' --date=short aba1356` |
| `enter_tool` が `results` を直接書く | `src-tauri/src/egui_shell/search_state.rs:330-341` |
| `on_escape` の 2 分岐が直接書く | 同 `:361-376` |
| `reset` が直接 `clear` する | 同 `:389-398` |
| `rows_generation` は 4 メソッドが進める | 同 `:204, 341, 366, 375, 397` |
| `dispatch` は controller のフィールド | `src-tauri/src/egui_shell/launcher_controller.rs:134` |
| `search_debounce` も controller のフィールド | 同 `:108` |
| `consume_reset_pending` が `Debouncer` を作り直す | 同 `:987` |
| `last_input_at` は reset されない | 同 `:977-1007`（代入が無い） |
| `Debouncer::new` は `armed: false` | `src-tauri/src/egui_shell/layout.rs:430-436` |
| `is_unsettled` の実体 | `src-tauri/src/egui_shell/search_dispatch.rs:79-81` |
| 合成の唯一の検知器 | 同 `:137-157` と、その事実を明言する doc `:77` |
| H7 は invalidate 忘れを検知しない | `scripts/lib/SnotraTraceInvariants.psm1:402-405` |
| `SPEC.md` §18.5 の記述 | `SPEC.md:820` |
| `spec §4.5` の指す先 | `docs/superpowers/specs/2026-08-10-search-worker-design.md:91, 119` |
