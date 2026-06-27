# plan.md — issue #333: IconCache 件数上限 + 挿入順 LRU 退避

## 判定: 仕様変更（挙動の上限導入）

`SPEC.md §3.4` に記載のアイコンキャッシュ挙動に「件数上限・退避」を追加 → **SPEC 同期が必要**（要求条件3）。
`SPEC §3.4` → コード → ドキュメントの順で整合させる。

## 受け入れ条件（テスト可能）

1. `IconCache` が件数上限 `cap` を持ち、`insert` で `len > cap` のとき挿入順で最古から退避し `len == cap` に収まる。
2. `get` は read-only（`&self`）のまま。アクセスで順序・件数を変えない。
3. cap 超過の既存 `icons.bin` をロードしたとき、ロード時点で `cap` 件に切り詰められる（常駐頭打ちの即時化）。
4. `retain_paths` 後も cap 不変条件は壊れない（retain は件数を減らすのみ）。
5. 新規 `[cache]` セクション欠落の `config.toml` を読むと `icon_cache_cap` がデフォルト値で補完される（後方互換）。
6. `validate()` は `icon_cache_cap < max_results` のとき `IconCacheCapTooSmall` を返す。
7. 旧 v5 `icons.bin`（HashMap 書き込み）が `IndexMap` 化後も読める（wire 互換）。`ICON_VERSION` は 5 のまま。

## 変更ファイル一覧

### Phase 1 — config（snotra-core）

1. **`snotra-core/src/config.rs`**
   - `default_icon_cache_cap() -> usize { 1000 }` を追加。
   - `CacheConfig { #[serde(default = "default_icon_cache_cap")] pub icon_cache_cap: usize }` + `Default` impl を追加。
   - `Config` に `#[serde(default)] pub cache: CacheConfig` を追加。
   - `Config::default()` の明示初期化に `cache: CacheConfig::default()` を追加。
   - `validate()` に Cache 検証ブロックを追加：
     `if self.cache.icon_cache_cap < self.appearance.max_results { errors.push(ConfigError::IconCacheCapTooSmall { cap, max_results }) }`
2. **`snotra-core/src/error.rs`**
   - `ConfigError` に `IconCacheCapTooSmall { cap: usize, max_results: usize }` を追加。
3. **`snotra-settings/src/app.rs`**
   - `config_error_message()` の網羅 match に `IconCacheCapTooSmall { cap, max_results } => format!(...)` を追加。
4. **`snotra-settings/src/i18n.rs`**
   - `err_icon_cache_cap_too_small()`（Ja/En）を追加。
   - メッセージ案: Ja「アイコンキャッシュ上限は表示件数（max_results）以上である必要があります」/ En「Icon cache cap must be at least max_results」。

### Phase 2 — IconCache 退避ロジック（src-tauri）

5. **`src-tauri/Cargo.toml`**
   - `indexmap = { version = "2", features = ["serde"] }` を `[dependencies]` に追加（既に lock に 2.14 存在）。
6. **`src-tauri/src/icon.rs`**
   - `use indexmap::IndexMap;`、`use std::collections::HashMap;` は他用途なければ削除。
   - `IconCacheData.png` を `IndexMap<String, Vec<u8>>` に変更（`#[derive(Default)]` 維持。serde map 互換）。
   - `IconCache` に `cap: usize` フィールドを追加（**永続化しない**＝runtime config）。
   - `IconCache::load(cap: usize) -> Self`：ロード後に `enforce_cap()` を呼び、切り詰めたら `dirty = true`。
   - `fn enforce_cap(&mut self) -> usize`：`let excess = self.data.png.len().saturating_sub(self.cap);` で算出（`saturating_sub` で cap >= len 時 0、アンダーフロー防止）。`excess > 0` なら先頭（最古）から `self.data.png.drain(0..excess)` で一括退避し `dirty = true`、退避件数を返す。`excess == 0` なら no-op で 0 を返す（dirty 据え置き＝無駄な save 回避）。
   - `insert(&mut self, path, png)`：`png.insert(path, png)` → `enforce_cap()` → `dirty = true`。
   - `get` / `save_if_dirty` / `retain_paths` はシグネチャ不変（`retain` は IndexMap でも順序保持）。
7. **`src-tauri/src/commands/icon.rs`**
   - `ensure_icon_cache_loaded_if_enabled()`：engine ロックを**1回だけ取得**し、その単一ロックブロック内で
     `show_icons` / `cap = config.cache.icon_cache_cap` / `max_results = config.appearance.max_results` の3値を読んでロック解放（lock を跨いで I/O しない、既存の `show_icons` 読み取りパターンを踏襲）。
     ロック外で **`effective_cap = cap.max(max_results)`** を算出 → `IconCache::load(effective_cap)`。
   - floor の理由（validate をすり抜けた極小 cap でも表示中アイコンを evict しない防御）をコード行にコメントする。

### Phase 3 — SPEC + テスト

8. **`SPEC.md`**（plan-review 指摘で §13.2 + UI 非公開の明記を追加）
   - **§3.4 アイコン**: 「件数上限 `cache.icon_cache_cap`（既定 1000、最小 `max_results`）を超えると挿入順で最古から退避（FIFO）。常駐・永続の両方を頭打ちにする」を 1 行追記。
   - **§13.2 アプリケーションデータ**: `icons.bin` の行に「件数上限 `cache.icon_cache_cap` で頭打ち」を注記（容量制限の文書化）。
   - **設定 UI 非公開の明記**: `icon_cache_cap` は **設定画面（snotra-settings の7タブ）に公開しない config.toml 専用キー**。§7.2 のタブ構成は変更しない（新タブ・新項目を追加しない）。§3.4 追記文に「（`config.toml` の手編集で調整、設定 UI 非公開）」と添える。
9. **テスト追加**（下記「テスト方針」）。

### スコープ判断: 設定 UI を追加しない（plan-review 指摘への回答）

issue #333 は「無制限増加の停止（メモリ削減）」が目的で、要求条件4は「`[cache]` config のデフォルト値フォールバック（後方互換）」のみ。**設定画面 UI での編集は要求されていない**。`icon_cache_cap` は高度な内部チューニングノブであり、config.toml 手編集で十分（YAGNI）。`snotra-settings/tabs/*.rs` への UI 追加は行わない。`app.rs`・`i18n.rs` の変更は ConfigError 翻訳（手編集で不正値を入れたユーザーへのバリデーションフィードバック）のためであり、UI 入力ウィジェットの追加ではない。

## 実装順序

Phase 1（config / error / settings）→ Phase 2（Cargo.toml / icon.rs / commands）→ Phase 3（SPEC / テスト）。
Phase 間に依存：commands/icon.rs（Phase 2）は `config.cache.icon_cache_cap`（Phase 1）に依存。

## 不変条件

- **`get` は `&self`（read-only）**：退避は `insert` / `load`（`&mut self`、write-lock 経路）に限定。要求条件1。
  異常系：`get` 経由で書き込みロックに変質する経路を作らない（Step1 の read lock を保つ）。
- **cap >= 表示ワーキングセット**：実効 cap = `config_cap.max(max_results)`。floor により、validate をすり抜けた極小 cap でも表示中アイコンを evict しない。
  異常系：`config_cap = 0`（手編集）でも `effective_cap = max_results`（既定 8 ≥ 1）。`drain(0..excess)` は cap=0 でも panic しない（全件退避）。
- **`icons.bin` wire 互換**：`HashMap`→`IndexMap` は serde map として byte 同一。`ICON_VERSION` 据え置き（5）。
  異常系：旧ファイルが読めなくなったらアイコン再抽出になるだけ（機能影響ゼロ）だが、互換テストで回帰検知する。
- **FIFO 順序の永続化**：今後書く `icons.bin` は IndexMap 反復順（挿入順）で書かれ、round-trip で順序保持。
- **dirty フラグの真偽ペア**：`enforce_cap` が退避したら `dirty = true`（次回 save で永続側も頭打ち）。退避なしなら据え置き（無駄な save を避ける）。

## テスト方針

### snotra-core
- `config.rs`:
  - `cache_config_defaults_when_section_missing`：`[cache]` 無し TOML → `icon_cache_cap == 1000`（`from_toml_str_fills_defaults` 雛形）。
  - `validate_icon_cache_cap_below_max_results`：`cap < max_results` で `IconCacheCapTooSmall` を含む。
  - `validate_icon_cache_cap_at_or_above_max_results_ok`：`cap == max_results` でエラーなし（境界）。
- `error.rs`: 既存の Display/source テストには `ConfigError` は含まれない（`ConfigError` は Display 未実装、settings 側で翻訳）。追加不要。

### src-tauri（icon.rs ユニットテスト、Win32 非依存で実行可能）
- `insert_evicts_oldest_when_over_cap`：cap=2 で 3 件 insert → len==2、最古キーが消え最新2件が残る。
- `insert_within_cap_keeps_all`：cap=3 で 2 件 → len==2、退避なし。
- `load_trims_when_over_cap`：cap=2 の `IconCache` を `enforce_cap` 相当で 3→2 に切り詰め、dirty=true。
  （`IconCache::load` はファイル I/O を伴うため、`enforce_cap` を直接叩くか、テスト用に `IconCacheData` を構築して検証）
- `get_does_not_mutate`：`get` 後に件数・順序が不変（read-only 契約）。
- `retain_paths_preserves_cap_invariant`：retain 後 len <= cap。
- 既存 `invalidate_icon_cache_clears_in_memory_state`：`IconCache { data, dirty }` 直接構築に `cap: <値>` を追加（icon.rs:348 の1箇所のみ。grep 確認済み）。
- **`wire_compat_hashmap_format_loads`（受け入れ条件7の明示テスト）**：旧形式（`HashMap<String,Vec<u8>>` を持つヘルパー struct）を `try_serialize_with_header(ICON_MAGIC, ICON_VERSION, ..)` でバイト列化 → `try_deserialize_with_header::<IconCacheData>(..)`（新 `IndexMap` 版）でデシリアライズし、全エントリが読めることを検証。`HashMap`/`IndexMap` の postcard wire 互換を回帰テストで固定する。

### 検証コマンド（`docs/build-commands.md` カテゴリ A/B 該当）
- `cargo test -p snotra-core -p snotra`
- `cargo clippy --all-targets -- -D warnings`
- UI 表示で evict 時のフォールバック挙動を目視（PR 作成前、要求条件「検証」）。
- `config.toml` 後方互換テスト（上記 `cache_config_defaults_when_section_missing`）。

## SPEC.md 更新要否

**要**（要求条件3）。§3.4 に上限・FIFO 退避の 1 行を追加。セクション番号の増減はないため子セクション/後続番号の影響なし。

---

## セルフレビュー（Step 5a/5b）

### 5a. check スキル適用結果

| スキル | 適用 | 結果 |
|---|---|---|
| `/plan-review` | 実行（3 Explore 並列） | config / icon.rs / SPEC・test の3レイヤー検証。**要対処0件**。下記の指摘を計画へ反映済み。 |
| `/symmetric-check` | plan-review 内で実施 | plan-review agent2 が対称ペア（insert↔enforce_cap/retain、load↔save）を「直交・一貫、問題なし」と判定。退避は `insert`/`load`（&mut）に限定し `get`(&self) は read-only 維持＝要求条件1の対称性が成立。独立再実行は重複のため省略。 |
| `/cache-check` | N/A | cache-check は incremental search / メモ化の**述語単調性（結果集合 ⊆ 前回）**を検証する。本件は前回クエリ結果の再利用ではなく key-value の永続ストアに FIFO 退避を足すだけで、再利用の妥当性を決める述語が存在しない（key ヒットは常に有効）。単調性の論点なし。 |
| `/state-check` | N/A | UI モード・状態遷移図・ガード条件を追加しない。`cap` は config 値、退避はデータ操作で UI 状態機械に直交。 |
| `/race-check` | N/A | 新規 async 関数なし。`get_icons_batch` は同期 `#[tauri::command]`、退避は `Mutex` ロック内。await 地点の状態競合なし。 |

### plan-review で反映した指摘

- **設定 UI 非公開を明記**（要対処→回答）: issue は UI 編集を要求しない。`icon_cache_cap` は config.toml 専用キーとし `tabs/*.rs` を変更しない旨を計画に明記。
- **SPEC §13.2 + UI 非公開の追記**: §3.4 だけでなく §13.2（`icons.bin` 容量制限）にも注記し、設定 UI 非公開を §3.4 追記文に添える。
- **wire 互換テストの明示**（受け入れ条件7）: `wire_compat_hashmap_format_loads` をテスト方針に追加。
- **実装コメント**: `effective_cap` floor の理由、engine 単一ロック読み取り、`enforce_cap` の `saturating_sub` を計画に反映。
- **config_watcher 反映スコープ外**: YAGNI 妥当と確認（issue に実行時反映の要求なし）。
- **docs/architecture.md**: フロントエンド `LruIconCache` のみ記載でバックエンド IconCache は未記載。実装詳細は `src-tauri/CLAUDE.md` に既出のため強制更新せず（軽微・見送り）。

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: insert↔enforce_cap/retain、load↔save を確認（plan-review 検証済み）。
2. **影響範囲の網羅性**: `ConfigError` 消費箇所（error.rs / app.rs 網羅 match / i18n.rs）、`IconCache` 直接構築箇所（icon.rs:348）、`IconCacheData.png` 型変更の影響を grep で確認済み。
3. **境界条件**: cap==max_results（境界 OK テスト）、cap=0（`saturating_sub` + `effective_cap` floor で安全）、空キャッシュ、re-insert（IndexMap は位置保持）。
4. **リソース管理**: `cap` は永続化しない runtime 値（生成=load 時注入、破棄不要）。`dirty` は退避時のみ true（真偽ペア）。新規プロセス・listen・AtomicBool なし。
5. **既存パターンとの整合**: `#[serde(default)]` セクション（SearchConfig/VisualConfig）、`ConfigError` 3点同期、`from_toml_str_fills_defaults` テスト雛形を踏襲。新規パターンなし。
6. **YAGNI 違反**: 設定 UI・config_watcher 反映・サイドカー順序ファイルを足さない。最小実装。
7. **シンプル化の挑戦**: `HashMap + VecDeque<key>` 手動順序管理より `IndexMap` 単一構造の方が membership と順序の SSOT が1つで済み単純（retain・順序永続化が自然）。新たな `AtomicBool`/`Mutex`/子プロセスを導入しない。「cap < len で drain」「config_cap < max_results で floor」の失敗時挙動を明記。
8. **破壊不変条件**: `icons.bin` wire 互換（破れるとアイコン再抽出に劣化するのみ＝機能影響ゼロだが回帰テストで検知）。`get` の read-only（破れると Step1 が write lock 化し性能退行＝要求条件1、`&self` シグネチャで型レベル担保 + テスト `get_does_not_mutate`）。Win32 フック・ホットキー・IPC 契約には触れない。

**計画の completeness: 高 / 実装着手可否: 可**
