# plan — issue #436 Phase 1: search.rs スコアリングの分割と階層宣言化

## 射程（ユーザー承認済み）

- **今 PR = direction 2（fn 分割）+ direction 3（スコア階層宣言化）**。search.rs 内で完結、SoA/cache 形式に非接触
- **direction 1（cache 三兄弟集約）= 別 issue**。PERFORMANCE 実測後に「マクロに値するか」を判断
- **direction 4（SearchMode）= 畳まない**。doc コメントで層境界を明文化

## 変更ファイル一覧

**コード変更は `snotra-core/src/search.rs` の 1 ファイルのみ。** Phase C で doc（`.claude/rules/snotra-core-search.md`）も更新する（コードではない・下記 Phase C）。

### `snotra-core/src/search.rs`（唯一のコード変更対象）

**A. スコア階層の宣言化（direction 3）**

モジュールトップ（`GLOBAL_WEIGHT` 群の近傍, L17-23）に `mod score` を追加:

```rust
/// スコア階層（全順序の不変条件）: Prefix > Substring > Kana > Path > Fuzzy(nucleo)。
/// 散在していた基準スコアをこの単一定義に吸収する。値を変更する場合も全順序を崩さない
/// （テスト kana_search_direct_match_ranks_above_kana_match / score_tiers_are_strictly_ordered が保証）。
/// Fuzzy は nucleo-matcher が独自スコアを返すため基準 const を持たない。
mod score {
    /// Prefix: `PREFIX_BASE - lower_name.len()`（短い名前を優先）。
    pub const PREFIX_BASE: i64 = 10_000;
    /// Substring: `SUBSTRING_BASE - byte_idx`。
    pub const SUBSTRING_BASE: i64 = 5_000;
    /// Kana（migemo）: `KANA_BASE - byte_pos`。
    pub const KANA_BASE: i64 = 4_500;
    /// Path: `PATH_BASE - min(byte_pos, PATH_POS_CAP)`。
    pub const PATH_BASE: i64 = 3_000;
    /// Path マッチの位置ペナルティ上限。
    pub const PATH_POS_CAP: i64 = 500;
}
```

インライン数値を const 参照に置換（**値は完全保存**、意味変更なし）:
- L814 `10_000 - lower_name.len()` → `score::PREFIX_BASE - lower_name.len()`
- L819 `5_000 - idx` → `score::SUBSTRING_BASE - idx`
- L797 `4500i64 - pos` → `score::KANA_BASE - pos`
- L607 `3000i64 - (pos).min(500)` → `score::PATH_BASE - (pos).min(score::PATH_POS_CAP)`

**`mod score` に含めないもの（Codex 指摘）**:
- **`9000`（L555 `name_score.is_none_or(|s| s <= 9000)`）はスコア階層 *ではない***——「Prefix 高信頼 name マッチなら file_name scoring を短絡スキップ」の閾値。`PREFIX_BASE` と混同禁止。`mod score` に入れず、インラインのまま残す（別 const 化は最小主義で見送り、混同防止のコメントのみ付す）
- history weight（`GLOBAL_WEIGHT`/`QUERY_WEIGHT`/`FOLDER_EXPANSION_WEIGHT`, L17-23）は base score と別単位ゆえ `mod score` に混ぜない（現状維持）
- **`score_tiers_are_strictly_ordered` の射程**: base const の大小（`PREFIX_BASE > SUBSTRING_BASE > KANA_BASE > PATH_BASE`）のみを守る安価ガード。実行時の全順序（`- len` / `.max(1)` 補正込み）は既存の挙動テスト（`kana_search_direct_match_ranks_above_kana_match` 等）が保証する。`mod score` の doc コメントも「base 定数の順序」と明記して過信を防ぐ

**B. `search_with_options` の分割（direction 2）**

現状 330 行を、既に明瞭な 4 フェーズ境界で分ける。**挙動・順序・incremental 候補集合を完全保存**する純リファクタ。

1. **`struct QueryPlan`**（phase (a) の出力を束ねる）:
   ```rust
   struct QueryPlan<'a> {
       norm_query: std::borrow::Cow<'a, str>,
       has_dot: bool,
       has_path_sep: bool,
       query_mask: u64,
       kana_query: Option<String>,
       needle_u32: Utf32String,       // norm_query から生成後は独立（借用しない）
       path_query: Option<String>,
       path_history_key: Option<String>,
   }
   ```

2. **`fn prepare_query_plan(query, mode, options) -> Option<QueryPlan>`**（自由 fn）:
   phase (a) L382-443 を移設。`norm_query.is_empty()` の早期 return は `None` で表現。`self` 不要。

3. **`fn decide_incremental(&self, plan: &QueryPlan, mode) -> bool`**（メソッド）:
   phase (b) L445-478 の `kana_monotonic` + `use_incremental` 述語を**逐語移設**（変更なし）。`self.prev_*` の read はここに集約。

4. **`#[inline] fn score_one_entry(&self, i, plan: &QueryPlan, mode, kana_available, history, options) -> Option<ScoredEntry>`**（メソッド・**inline 必須**）:
   phase (c) の per-entry ボディ L513-648 を移設。bitmask pre-filter → name/file_name/kana/path スコア → 履歴ブースト → `ScoredEntry` 構築まで。`Some` ⟺ マッチ成立（`base_score.is_some()`。現行 `local_matches.push(i)` の条件と 1:1）。`MATCHER` thread_local はボディ内で借用（引数化しない）。全引数を参照/Copy で受け、rayon fold 内 codegen を不変に保つ。

   **逐語保存が必須の内部順序・分岐（Codex 指摘）**:
   - **bitmask pre-filter は関数の *先頭***（L517-525）。`entry_view(i)` 生成・`Utf32String::from`（L527/533）より前に置く。pre-filter で落ちる候補に `entry_view`/UTF-32 変換コストを掛けると退行する
   - **`has_dot` の file_name 短絡** `name_score.is_none_or(|s| s <= 9000)`（L555）を逐語保存。「name/file_name の max を常に計算」に単純化すると `MATCHER` 呼び出し回数が変わる（挙動は同値だが性能退行）
   - **履歴キーは `path_history_key`（`normalize_history_query_key(query)` 由来, L618）を使い、`path_query`（パスマッチ用・アクセント/スペース保持）と混同しない**。`QueryPlan` で両者を別フィールドに保つ（下記 struct 参照）

5. **orchestrator `search_with_options`**（~40 行に縮む）:
   ```
   if max_results == 0 { return Vec::new(); }
   let Some(plan) = prepare_query_plan(query, mode, &options) else { return Vec::new(); };
   let use_incremental = self.decide_incremental(&plan, mode);
   let candidate_indices = if use_incremental { take(prev_candidates) } else { (0..len).collect() };
   let kana_available = !self.kana_lower_names.is_empty();
   let (top_k, all_match_indices) = candidate_indices.into_par_iter()...fold(|(heap,matches), i| {
       if let Some(scored) = self.score_one_entry(i, &plan, mode, kana_available, history, options) {
           matches.push(i);
           // heap push/replace（L650-657 逐語）
       }
   }).reduce(...);           // reduce の heap マージ（L663-678）は逐語
   // phase (d): prev_* 更新（plan を destructure して move）→ into_sorted_vec → SearchResult
   self.prev_query = plan.norm_query.into_owned();
   self.prev_candidates = all_match_indices;
   self.prev_mode = Some(mode);
   self.prev_kana_query = plan.kana_query;
   heap_into_results(top_k)   // top_k → Vec<SearchResult> の純変換のみ抽出
   ```
   heap push/replace と reduce マージは orchestrator に残す（top-k 縮約の機構であり per-entry スコアリングではないため）。`prev_*` の write は orchestrator に残し、`decide_incremental` の read と**同一関数の視界**に置く（incremental 契約の可読性）。
   - **`prev_*` 更新は fold/reduce 後・sort 前**（現行 L680 の位置）を厳守。`heap_into_results` の後には移さない（`all_match_indices` は sort 前に確定済みゆえ位置の自由度はあるが、現行位置を保って diff を最小化）
   - **`heap_into_results` は `top_k.into_sorted_vec()` を使う**（`ScoredEntry::Ord` 逆順で昇順=best-first, L686-688）。通常の降順 re-sort に置き換えると同点 tie-break の意味が変わる。純変換（heap→昇順 Vec→`SearchResult` map）のみ抽出し、比較ロジックは触らない

**C. direction 4 の文書化（削除しない）**

`search::SearchMode`（L32）の doc コメントに追記: 「`config::SearchModeConfig`（serde/wire 形式）と分離した純ドメイン enum。`From` 変換が config↔engine の層境界。統合すると engine が serde に依存するため意図的に 2 定義とする」。

## 実装順序（フェーズ）

各フェーズ完了後に `cargo test -p snotra-core` green を確認してからコミット（中断耐性）。

1. **Phase A（低リスク・独立）**: `mod score` 追加 + 4 箇所の const 置換 + `score_tiers_are_strictly_ordered` テスト追加 → test green → commit
2. **Phase B（分割本体）**: `QueryPlan` + `prepare_query_plan` + `decide_incremental` + `score_one_entry` + `heap_into_results` 抽出、orchestrator 縮約 → test green → commit
3. **Phase C（文書化）**: (i) `SearchMode`（L32）doc コメントに層境界の理由を追記 (ii) `.claude/rules/snotra-core-search.md` L10 のスコア階層行を「`mod score` に集約」へ更新 (iii) **SPEC 節参照の是正**——`kana_substring_score` の doc（L791）が「SPEC.md §3.2 に準拠」と書くが §3.2 は*エントリ識別子*の節。スコア記述は §4.2（かな `max(4500-byte_pos,1)`, SPEC:122）/§4.3（パス 3000, SPEC:146）。正しい節番号へ訂正（既存 doc ドリフトの as-built 是正・実装時に SPEC.md で節番号を再確認） (iv)（オプション）`path_match_incremental_cache_monotonic` の L2309 コメント「incremental cache 有効」を「path は `!has_path_sep` で incremental 無効・fresh 一致を検証」へ訂正 → commit
   - **doc 更新の要否確認**: `snotra-core/CLAUDE.md` の path score 記述（`3000 - min(byte_pos, 500)`）は**値保存ゆえ正確なまま**——更新不要（Codex 指摘に対する結論。実装時に値一致を確認）

## 不変条件（壊れたら即アウト）

1. **スコア値の完全一致**: const 置換は値保存。prefix/substring/kana/path の各順位テストが green
2. **top-k 順序の一致**: heap ロジック（`ScoredEntry::Ord` 逆順・push/replace・reduce マージ）は逐語で不動
3. **incremental 候補集合の一致**: `decide_incremental` の述語は逐語移設。`local_matches.push(i)` は `score_one_entry` が `Some` を返す条件（= 現行 `base_score.is_some()`）と 1:1
4. **`prev_*` 更新タイミングの不変**: fold 後・sort 前（L680 の位置）。`decide_incremental` の read と orchestrator の write が同一視界
5. **性能非退行**: `score_one_entry` は `#[inline]`、全引数 参照/Copy、`MATCHER` thread_local 不変 → fold 内 codegen 不変。SoA/cache 形式に非接触。**内部順序 3 点を逐語保存**（Codex 指摘）: (a) bitmask pre-filter を関数先頭に置き `entry_view`/`Utf32String::from` より前（無駄な UTF-32 変換の回避）、(b) `has_dot` の file_name 短絡 `<= 9000` を保存（`MATCHER` 呼び出し回数）、(c) `with_min_len(MIN_PAR_CANDIDATES)` を orchestrator の同 iterator 上に保持。自動ベンチ不在ゆえ**性能退行はテストで捕捉できない**——上記の構造保存が唯一のガード（レビューで pre-filter 位置・短絡・inline を目視確認する）

## テスト方針

- **一次ガード = 既存 search.rs テストスイート全 green**（スコア・順序・incremental・path・kana・migemo を網羅済み）。純リファクタゆえ「テスト green ＝退行なし」が成立
- **追加**: `score_tiers_are_strictly_ordered` — `score::PREFIX_BASE > SUBSTRING_BASE > KANA_BASE > PATH_BASE` を assert（将来の const 編集が全順序を反転させないコンパイル近接ガード）
- **`/cache-check`**: incremental 述語を `decide_incremental` へ移設した後の単調性非退行を検証
- 検証コマンド: `docs/build-commands.md` のカテゴリに従い `cargo test -p snotra-core` + clippy（PostToolUse フックが .rs 編集で自動実行）

## SPEC.md 更新要否

- **不要**。挙動・IPC 契約・状態遷移に変更なし（純内部リファクタ）。スコア階層は SPEC.md §3.2 の記述と一致を保つ（値不変）

## セルフレビュー（Step 5b）

1. **対称コードパス**: `search` / `search_with_options` の関係は「便宜 API → 本体」。`search`（L354）は `search_with_options` へ委譲するのみで分割の影響を受けない（引数転送）。show/hide 型の対称ペアなし
2. **影響範囲の網羅性**: `search_with_options` の呼び出し元 = `search`（同ファイル L361 委譲）+ `engine.rs:118`（`Engine::search`）。シグネチャ不変ゆえ全呼び出し元（IPC `src-tauri/commands/search.rs:20`・フロント `ui/src/stores/search.ts` 含む）無変更。スコア階層をエンコードする本番リテラルは **4 箇所（L607/797/814/819）で確定**。**注意**: `10_000`/`5_000`/`4500`/`3000` は他にも出現するが（ベンチ件数 L1480 群・説明コメント L603/L789-793）スコア階層コードではないため置換対象外。`#[cfg(test)]` にこれら数値を assert するテストは無い（順位のみ検証）ため、const 化でテスト更新は発生しない
3. **境界条件**: `max_results == 0`（早期 return 維持）/ `norm_query 空`（`prepare_query_plan` → None）/ `candidate_indices` 空 / `kana_available == false`（空 Vec ガード）/ `has_path_sep`（bitmask skip）— いずれも既存テストが被覆、移設で分岐位置不変
4. **リソース管理**: 新規リソース（listen/Mutex/子プロセス/AtomicBool）なし。`MATCHER` thread_local は既存、生成/破棄ペア不変
5. **既存パターン整合**: 抽出先は既存の自由 fn（`adjusted_history_boost` 等）と同型。新パターン導入なし。`QueryPlan` は phase (a) の局所変数を束ねるだけの純データ struct
6. **YAGNI**: マクロ・trait・汎用 builder を導入しない（direction 1 を意図的に除外）。`QueryPlan` は今の分割に必要な最小
7. **シンプル化の挑戦**: `heap_into_results` は 15 行程度——抽出すべきか？ → orchestrator の視覚ノイズ（`.map(SearchResult{..})`）を除くため抽出。ただし `prev_*` 更新は `&mut self` 依存かつ incremental 契約の一部ゆえ orchestrator に残す（過度に細分化しない）
8. **破壊不変条件の明示**: 上記「不変条件」5 項が該当。検知手段 = 既存テストスイート（スコア/順序/incremental）+ `score_tiers_are_strictly_ordered` + `/cache-check`。Win32/ホットキー/IPC など「戻ってこない」系のリスクなし（純ロジック lib crate 内）

### plan-review / check スキル結果（Step 5a）

**`/plan-review`（Explore 2 体・忠実性 + スコープ）— 要対処ゼロ**

- **抽出の忠実性（5 観点すべて問題なし）**:
  - `score_one_entry -> Option<ScoredEntry>` は現行 `local_matches.push(i)`（唯一 L612・`if let Some(base_score)` 直下）と 1:1。`Some` ⟺ マッチ成立。**実装ウォッチ**: `matches.push(i)` は heap 採否（L650-657）に**先行し独立**——heap 落ちエントリも記録する。orchestrator で heap 枝に畳み込むと incremental cache 退行（不変条件#3 で明記済み）
  - fold body の `&mut self` 変異は皆無。`self` アクセスは全 read（char_masks/file_name_char_masks/entry_view/kana_lower_names）。thread_local は MATCHER のみ
  - `needle_u32` は `Utf32String::from` で所有型・`norm_query` 非借用（`&plan` 共有・fold 後の部分ムーブ健全。QueryPlan は Send+Sync・Drop 未実装）
  - `decide_incremental` の述語（L461-478）は `prev_*` を READ のみ。WRITE（L481 take・L681-684）は述語より後で orchestrator 残留。`&mut self` 不要
- **スコープ過不足なし**: 本番リテラル 4 箇所確定。呼び出し元（engine.rs:118 / IPC / フロント）シグネチャ不変で無変更。SearchMode 二重定義は search→config の単方向依存・serde 染み出し回避の**意図的層分離**（統合不可・文書化が正当）
- **軽微指摘 2 件を計画へ反映済み**: (a) セルフレビュー#2 の grep 主張を訂正 (b) SPEC §3.2 誤参照を Phase C(iii) で是正

**`/cache-check` — 単調性保証あり・再利用は安全**

計画は `use_incremental` 述語の**逐語移設のみ**（ロジック不変）。全 7 述語は既に単調:
1. `prev_mode == Some(mode)` — モード切替で full scan
2. `!prev_candidates.is_empty()` / 3. `!prev_query.is_empty()` — 前回状態の存在
4. `norm_query.starts_with(prev_query)` — prefix 拡張（狭まる方向）。backspace で不成立→full scan
5. `!has_dot || prev_query.contains('.')` — no-dot→dot 遷移は file_name スコア未適用ゆえ full scan
6. `!has_path_sep` — path クエリは正規化が異なり単調性を保証できないため無条件無効
7. `kana_monotonic` — (None,_)→OK / (Some curr, Some prev) は `curr.starts_with(prev)` / (Some,None)→full scan（非単調 "kan"→"かん"/"kana"→"かな" を捕捉）

**移設の唯一のリスク＝順序**: `decide_incremental`（`prev_*` READ）→ `take(&mut prev_candidates)`（L481）→ fold → `prev_*` WRITE（fold 後）の順を厳守。述語は**旧** `prev_*` を読み、書き込みは fold 後（不変条件#4）。Explore が read/write 分離を確認済み。

**退行ガード = 既存 incremental テスト 14 本**（`incremental_search_*` 7 本 + `incremental_kana_*` 5 本 + `path_match_incremental_*` 2 本）が extension/backspace/mode-change/dot 遷移/kana 単調性/path を網羅。移設が順序 or 述語を壊せば必ず落ちる。

**path テストの根拠は「再利用単調性」ではなく「incremental 無効化の正しさ」（Codex 指摘・実証済み）**: `path_match_incremental_cache_monotonic`（L2296）の L2309 コメントは「incremental cache 有効」と書くが、クエリ `tool\ed` は `\` を含み `!has_path_sep` ガード（L477）で incremental は**無効**。テストは path クエリが fresh 一致の結果を返すことを検証する有効な回帰だが、path が incremental *再利用* する経路は存在しない。→ Phase C(iv) でこの誤解コメントを訂正（オプション・doc ドリフト是正）。

**結論: 全述語で単調性が保証されており、キャッシュ再利用は安全**（path は再利用せず無効化で安全側に倒す）。

**Codex 独立レビュー（計画段階・実証フィルタ後）**

Codex に plan を独立レビューさせ、severity をコードで実証して取捨選択（[[feedback_codex_review_unreliable]]）。**既にカバー済み**（`Some`=base_score・`matches.push` 先行・借用/部分ムーブ・read-before-take）は完全性の再確認として記録。**実質回収 4 件を計画へ反映済み**:
1. `9000`（L555）は score 階層でなく file_name 短絡閾値 → `mod score` から除外を明記（Part A）
2. スコープ記述の矛盾（「search.rs のみ」↔ Phase C の `.claude/rules`）→ 「コード変更のみ search.rs」へ是正（変更ファイル一覧）
3. 性能不変条件の欠落 → bitmask pre-filter を `score_one_entry` 先頭に固定（不変条件#5・Part B item 4）
4. `path_match_incremental_cache_monotonic` の誤解コメント → cache-check の根拠を「無効化の正しさ」へ訂正 + Phase C(iv) で修正
その他（`.max(1)` 条件付き不変・`snotra-core/CLAUDE.md` path score 値保存）も記述に取り込み済み。
