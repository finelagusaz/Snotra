# plan — issue #461 IndexCache 双子の Cow 統合

## 射程（ユーザー承認済み・spike 実証済み）

`IndexCache`（owned）+ `IndexCacheRef`（borrowed）を **1 つの `IndexCache<'a>`（`Cow<'a,[T]>`）へ統合**。postcard 位置依存の「フィールド順ズレ→index.bin 無言破損」footgun を型として消す。versioning（②）・CachedMasks（③）は irreducible として非接触。**バイト形式不変ゆえ version バンプ不要**（spike 済み）。

## 変更ファイル一覧

**コード変更は `snotra-core/src/indexer.rs` の 1 ファイルのみ**。doc として `snotra-core/CLAUDE.md`（version チェックリストから IndexCacheRef 項を削除）。

### `snotra-core/src/indexer.rs`

**1. import 追加**: `use std::borrow::Cow;`

**2. `IndexCache` を Cow 化・`IndexCacheRef` を削除**
```rust
/// index.bin の payload。save は Cow::Borrowed（clone 回避）、load は Cow::Owned で復元。
/// 単一 struct ゆえ owned/borrowed のフィールド順ズレによる index.bin 破損は型として起こり得ない。
#[derive(Serialize, Deserialize)]
struct IndexCache<'a> {
    built_at: u64,
    entries: Cow<'a, [AppEntry]>,
    config_hash: u64,
    char_masks: Cow<'a, [u64]>,
    file_name_char_masks: Cow<'a, [u64]>,
    lower_names: Cow<'a, [String]>,
    lower_file_names: Cow<'a, [Option<String>]>,
    normalized_keys: Cow<'a, [String]>,
}
```
- `IndexCacheRef<'a>`（L238-248）とその doc コメント（L230-237）を**削除**
- Cow フィールドに `#[serde(borrow)]` は**付けない**（Owned deserialize が正・spike 確認）

**3. `save_cache_sorted_in`（L423-437）**: `IndexCacheRef {...}` → `IndexCache { entries: Cow::Borrowed(entries), char_masks: Cow::Borrowed(&char_masks), file_name_char_masks: Cow::Borrowed(&file_name_char_masks), lower_names: Cow::Borrowed(&lower_names), lower_file_names: Cow::Borrowed(&lower_file_names), normalized_keys: Cow::Borrowed(&normalized_keys), .. }`。clone 無しは維持

**4. `load_cache_in` v4 分岐（L474-489）**:
```rust
if let Ok(cache) = try_deserialize_with_header::<IndexCache<'static>>(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION) {
    if cache.config_hash != config_hash { return None; }
    let masks = CachedMasks {
        char_masks: cache.char_masks.into_owned(),
        file_name_char_masks: cache.file_name_char_masks.into_owned(),
        lower_names: Some(cache.lower_names.into_owned()),
        lower_file_names: Some(cache.lower_file_names.into_owned()),
        normalized_keys: Some(cache.normalized_keys.into_owned()),
    };
    return Some(LoadCacheResult { entries: cache.entries.into_owned(), cached_masks: Some(masks) });
}
```
`.into_owned()` は deserialize が Owned ゆえ no-op move（clone 無し・性能非退行）。v3/v2 分岐は非接触

**5. テスト転用**: `index_cache_ref_serializes_identically_to_owned`（L1008-1067）を **`index_cache_on_disk_format_is_stable`** へ転用。双子のバイト一致（統合で構造的に自明化）に代え、**固定 fixture の serialize 出力が凍結 golden バイト列と一致**することを検証。IndexCache のフィールド順・型変更（= 既存 index.bin 破損）を version 非バンプでも検出する（save/load 自己整合ゆえ roundtrip では捕捉不能な reorder を捕捉）。golden は実装時に生成して凍結。
   - **補足（plan-review 訂正）**: 旧テストは *twin 乖離*（owned vs borrowed）のみを守っており、絶対バイト安定性は**どの自動テストも守っていなかった**（CLAUDE.md 人手 checklist のみ）。golden テストは不変条件の「継承」ではなく**未カバーの穴を塞ぐ**もの

**6. 既存 roundtrip テストの Cow 追従（plan-review 検出）**: `index_cache_binary_roundtrip`（L952-1004）は owned `IndexCache` を bare `Vec` で構築し `assert_eq!(restored.char_masks, vec![...])` で読む。Cow 化後は (a) bare `Vec` が `Cow<[T]>` に coerce しない (b) `Cow<[T]> == Vec<T>` の `PartialEq` 不在 でコンパイル不能。→ 構築を `Cow::Owned(vec![...])`（または `Cow::Borrowed(&…)`）に、assert を `restored.char_masks.as_ref() == [...]` へ改める。test-only・build gate で即検出・機械的だが Phase 1 の編集対象に含める。turbofish `try_deserialize_with_header::<IndexCache>`（v2/v3 テスト L1135/L1164）は lifetime elision で `'static` 推論され追従不要

### `snotra-core/CLAUDE.md`

- 「IndexCache バージョン変更チェックリスト」の **項 8（`IndexCacheRef<'a>` を IndexCache と同順で追加）を削除**（IndexCacheRef 消滅で不要＝checklist の構造吸収）。他項（version バンプ・V3/V2 fallback・CachedMasks）は維持
- CachedMasks 節の三兄弟記述を Cow 統合に合わせ更新（双子→1 struct）

## 実装順序

1. **Phase 1（統合本体）**: import → IndexCache Cow 化 + IndexCacheRef 削除 → save/load 更新 → **`index_cache_binary_roundtrip` の Cow 追従**（上記 6・コンパイル維持）→ `cargo test -p snotra-core` green
2. **Phase 2（テスト転用）**: golden-bytes テストへ転用・golden 凍結 → test green
3. **Phase 3（doc）**: CLAUDE.md チェックリスト項 8 削除・CachedMasks 節更新

各 Phase 後に `cargo test -p snotra-core` green を確認してコミット。

## 不変条件（壊れたら即アウト）

1. **on-disk バイト形式の不変**: 既存 v4 `index.bin` がそのままロード可能。version バンプ不要（spike 実証・golden テストで継続保証）
2. **backward-compat 維持**: v3/v2 fallback は非接触。既存 `load_cache_v3_fallback` / `load_cache_v2_migrates` / `save...roundtrip` テストが green
3. **save の clone 回避を維持**: `Cow::Borrowed` で entries/派生 Vec を借用（`entries.to_vec()` 相当の全件 clone を導入しない）
4. **load の性能非退行**: `.into_owned()` は Owned に対し no-op move（clone を増やさない）
5. **CachedMasks の内容不変**: v4 は 5 カラム全て Some、v3 は lower 系 None。統合は CachedMasks 生成の中身を変えない

## テスト方針

- **一次ガード = 既存 indexer テスト全 green**（roundtrip・v3/v2 fallback・extend が backward-compat とマスク整合を被覆）
- **転用**: `index_cache_on_disk_format_is_stable`（golden bytes）——形式安定＝既存 index.bin 破損の検出器。旧「双子バイト一致」テストが守っていた「形式安定」不変条件を、単一 struct 版として引き継ぐ（AGENTS.md「転用で失われる不変条件を別テストで補う」）
- 検証: `cargo test -p snotra-core` + clippy（`.rs` 編集で PostToolUse フック自動）

## SPEC.md 更新要否

- **不要**。挙動・IPC 契約・状態遷移・on-disk 形式すべて不変（純内部リファクタ）

## セルフレビュー（Step 5b）

1. **対称コードパス**: save（Serialize）/ load（Deserialize）の対。統合で両者が同一 struct を共有 → 対称性はむしろ強化（順序ズレ不能）
2. **影響範囲の網羅性**: `IndexCacheRef` の全参照（定義 L239・save L425・テスト L1041）+ owned `IndexCache` 構築/読み取り（struct L219・load L474・roundtrip テスト L952-1004・v2/v3 テスト turbofish L1135/L1164）を grep 済み。`index_cache_binary_roundtrip` の Cow 追従を編集リストに追加（plan-review 検出）。`CachedMasks` 消費側（`new_with_cached_masks` / `engine.rs`）は非接触（CachedMasks の型・中身不変）。`load_cache` 呼び出し元は `load_or_scan_with_stats`（L309）1 箇所のみで `result.entries`/`cached_masks` を読む＝型不変ゆえ無影響
3. **境界条件**: v4 ヒット（統合 struct）/ v3 ヒット（fallback・非接触）/ v2 ヒット（非接触）/ config_hash mismatch（早期 None・Cow 触らず）/ 空 entries（Cow::Borrowed(&[]) も往復可）
4. **リソース管理**: 新規リソースなし。Cow は所有権のみ・Drop 特殊なし
5. **既存パターン整合**: `Cow` は config.rs（`normalize_query`）で既に使用の慣用。新パターンではない
6. **YAGNI**: field-list マクロ（③）・versioning 集約（②）を意図的に除外。Cow 統合は「双子を畳む」最小
7. **シンプル化の挑戦**: 統合で struct 1 個・テスト前提 1 個・CLAUDE.md checklist 項 1 個が**減る**（純減）。Cow は借用/所有の両経路を 1 型で表す最小手段
8. **破壊不変条件の明示**: 「on-disk 形式の不変」が最重要（既存 index.bin 破損＝全ユーザーの再スキャン）。検知手段 = golden-bytes テスト（形式変更を version 非バンプでも検出）+ 既存 v3/v2/roundtrip テスト。spike で形式不変を事前実証済み

### plan-review / check スキル結果（Step 5a）

**`/plan-review`（Explore・6 観点）— 要対処 1 件（反映済み）**

- **問題なし（5 観点）**: (1) `IndexCacheRef` 全参照カバー (3) バイト形式不変（field 属性なし・順序/集合保存・`#[serde(borrow)]` 不付与が正） (4) load v4 分岐（config_hash Copy・`.into_owned()` は Owned に no-op move・v3/v2 非接触） (5) golden テスト必要性の論理正当（強化: 絶対バイト安定性は従来どの自動テストも未カバー） (6) 下流（`load_cache` 呼び出し元 1・`CachedMasks`・`new_with_cached_masks`）非影響
- **要対処→反映済み**: `index_cache_binary_roundtrip`（L952-1004）の Cow 追従を編集リスト（項 6）と Phase 1 に追加
- **`/cache-check` 非該当**: 本件は on-disk 永続化であり incremental 再利用ロジックではない
- **spike 実証**: feasibility は本計画着手前に実 serde/postcard で確認済み（研究 §feasibility spike）
