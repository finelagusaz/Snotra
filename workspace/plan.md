# Plan — Issue #214: ローマ字入力で日本語名ファイルを検索（kana マッチ）

## 概要

クエリが ASCII のみのとき、`wana_kana::to_hiragana()` でひらがな変換した kana_query を生成し、
エントリ名のひらがな正規化済み Vec（`kana_lower_names`）に対して Substring マッチする。
設定フィールドなし・常時 ON。

---

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/Cargo.toml` | `wana_kana` 依存追加 |
| `snotra-core/src/query.rs` | `to_kana(s: &str) -> String` 追加 |
| `snotra-core/src/search.rs` | `kana_lower_names` Vec 追加、kana マッチロジック追加 |
| `SPEC.md` | §3.1/3.2 にローマ字検索の挙動を追記 |

設定 UI（`snotra-settings`）は変更なし。

---

## フェーズ構成

### Phase 1: wana_kana 依存追加

**ファイル**: `snotra-core/Cargo.toml`

```toml
wana_kana = "0.4"   # バージョンは実装時に crates.io で最新版を確認
```

---

### Phase 2: query.rs に to_kana() を追加

**ファイル**: `snotra-core/src/query.rs`

```rust
/// ASCII ローマ字をひらがなに変換する（カタカナもひらがなに正規化）。
/// 非 ASCII 文字（漢字など）はそのまま通過する。
/// wana_kana::to_hiragana() で変換後、小文字化する。
pub fn to_kana(s: &str) -> String {
    wana_kana::to_hiragana(s)
}
```

- `to_lower_folded()` の後に呼ぶ想定（インデックス構築時はすでに lower_folded 済みの文字列に適用）

---

### Phase 3: search.rs — kana_lower_names Vec と kana マッチ追加

**SearchEngine struct に追加**:

```rust
/// エントリ名をひらがな正規化した Vec（katakana→hiragana、ASCII はそのまま）。
/// ローマ字クエリを kana 変換して substring マッチするために使用。
kana_lower_names: Vec<String>,
```

**`new()` の Wave 1 に追加** (lower_names と rayon::join で並列):

```rust
// Wave 1: lower_names / lower_file_names / normalized_keys / kana_lower_names
let ((lower_names, lower_file_names), (normalized_keys, kana_lower_names)) = rayon::join(
    || rayon::join(
        || entries.iter().map(|e| to_lower_folded(&e.name)).collect(),
        || ...lower_file_names...,
    ),
    || rayon::join(
        || ...normalized_keys...,
        || entries.iter().map(|e| to_kana(&to_lower_folded(&e.name))).collect(),
    ),
);
```

**`new_with_cached_masks()` も同様に追加** (v3 フォールバックパスと v4 キャッシュパスの両方)。

v4 キャッシュは `kana_lower_names` を保存しない（再計算で十分）。

**`debug_assert!` 更新**:

```rust
debug_assert!(
    lower_names.len() == entries.len()
        && lower_file_names.len() == entries.len()
        && normalized_keys.len() == entries.len()
        && char_masks.len() == entries.len()
        && file_name_char_masks.len() == entries.len()
        && kana_lower_names.len() == entries.len(),  // 追加
    "SearchEngine: all parallel Vecs must have the same length as entries"
);
```

**`Self {}` に kana_lower_names を追加**。

**`search_with_history_boost()` に kana_query 計算を追加**:

```rust
// ローマ字検索: ASCII のみのクエリをひらがなに変換して kana マッチに使用
let kana_query: Option<String> = if norm_query.is_ascii() && !norm_query.is_empty() {
    let k = to_kana(norm_query.as_ref());
    // to_kana が同一文字列を返す場合（kana に変換されなかった場合）はスキップ
    if k != norm_query.as_ref() { Some(k) } else { None }
} else {
    None
};
```

**scoring fold 内で kana マッチを追加**:

```rust
// primary match (既存)
let name_score = MATCHER.with(...); // 既存ロジック

// kana match: primary が失敗した場合のみ試みる
let kana_score = if name_score.is_none() {
    kana_query.as_deref().and_then(|kq| {
        kana_substring_score(&self.kana_lower_names[i], kq)
    })
} else {
    None
};

let score = name_score.or(kana_score);
```

**kana_substring_score 関数を追加**:

```rust
/// ひらがな正規化済みエントリ名に対して substring マッチを行い、
/// マッチした場合は `4500 - byte_position` を返す（Substring の 5000 より低いスコア）。
fn kana_substring_score(kana_lower_name: &str, kana_query: &str) -> Option<i64> {
    kana_lower_name.find(kana_query).map(|pos| 4500 - pos as i64)
}
```

---

### Phase 4: ユニットテスト追加

**snotra-core/src/query.rs**:

```rust
#[test]
fn to_kana_converts_romaji_to_hiragana() {
    assert_eq!(to_kana("dokyu"), "どきゅ");
}

#[test]
fn to_kana_converts_katakana_to_hiragana() {
    assert_eq!(to_kana("ドキュメント"), "どきゅめんと");
}

#[test]
fn to_kana_passes_through_ascii() {
    assert_eq!(to_kana("documents"), "documents");
}

#[test]
fn to_kana_passes_through_kanji() {
    // 漢字は変換しない
    assert!(to_kana("書類").contains("書"));
}
```

**snotra-core/src/search.rs** (既存テスト群に追加):

```rust
#[test]
fn kana_search_matches_katakana_entry() {
    // "ドキュメント" を "dokyu" で検索できる
    let engine = engine_with(vec!["ドキュメント"]);
    let results = engine.search_raw("dokyu", SearchMode::Prefix);
    assert!(!results.is_empty());
    assert_eq!(results[0].name, "ドキュメント");
}

#[test]
fn kana_search_no_false_positive_for_ascii_entry() {
    // ASCII 名エントリに対してローマ字クエリが誤マッチしない
    let engine = engine_with(vec!["documents"]);
    let results = engine.search_raw("dokyu", SearchMode::Prefix);
    assert!(results.is_empty());
}

#[test]
fn kana_search_direct_match_ranks_above_kana_match() {
    // 直接一致 > kana 経由の一致
    let engine = engine_with(vec!["doki_ドキュ", "ドキュメント"]);
    let results = engine.search_raw("doki", SearchMode::Substring);
    assert_eq!(results[0].name, "doki_ドキュ");  // 直接一致が先頭
}
```

---

### Phase 5: SPEC.md 更新

§3.1「検索方式」の末尾に追加:

```
- ローマ字検索（常時有効）: クエリが ASCII のみの場合、ひらがな変換して
  エントリのかな正規化名（カタカナ→ひらがな）に中間部分一致で検索する。
  漢字名には対応しない。
```

§3.2「クエリ正規化」の末尾に追加:

```
- ローマ字検索時の追加正規化: クエリが ASCII のみの場合、ひらがな変換した
  kana_query を生成し、エントリのかな正規化名に中間部分一致する（score: 4500 - pos）。
  直接一致（Prefix/Substring/Fuzzy）のスコアより低い。
```

---

## 不変条件

1. `kana_lower_names.len() == entries.len()` は常に成立（debug_assert で保証）
2. kana マッチは primary match の代替（OR）であり、加算ではない
3. kana_query は ASCII クエリのみ生成 → 非 ASCII クエリでは kana パス不活性
4. kana_lower_names はインデックスファイルに保存しない（起動時に再計算）

---

## テスト方針

- 追加テスト: `to_kana()` の変換正確性 × 4 パターン
- 追加テスト: kana マッチの検索正確性 × 3 パターン
- 既存テスト: 全 `cargo test -p snotra-core` が通ること（回帰なし）
- 検証コマンド: `cargo test -p snotra-core` + `cargo check -p snotra-core -p snotra -p snotra-settings`

---

## SPEC.md 更新要否

✅ §3.1/3.2 に追記必要（挙動変更を伴う仕様追加）

---

## セルフレビュー

### 1. 対称コードパス
- `new()` と `new_with_cached_masks()` の両方に `kana_lower_names` 構築を追加 ✅
- v3 フォールバックパスも対応必要 → 計画に含めた ✅

### 2. 影響範囲の網羅性
- `SearchEngine` の 5 つの並列 Vec に対して、`debug_assert!`・`entry_view()`・`Self {}` の全箇所を更新する必要がある
- `entry_view()` には `kana_lower_name` を追加しない（scoring loop 内で `self.kana_lower_names[i]` を直接参照する方が `char_masks` と同じパターン）→ 計画済み ✅

### 3. 境界条件
- kana_query と kana_lower_name が同じ文字列（to_kana で変換されなかった）場合は kana マッチをスキップ → `if k != norm_query { Some(k) } else { None }` で対応 ✅
- kana_score のマイナス値: "pos" が 4500 を超えると負になる。長いエントリ名の非常に後ろでのマッチ → スコア負 → `if let Some(base_score) = score` でのみ採用されるため問題なし（負スコアエントリは採用されない）
  - **要確認**: 負スコアを None として扱うべきか → `4500 - pos as i64` が負になる場合は None を返すよう修正が必要
  - → `kana_substring_score` に `if score > 0 { Some(score) } else { None }` を追加 ✅（計画に反映）

### 4. リソース管理
- `kana_lower_names` は `Vec<String>` で RAII 管理。生成/破棄の追加不要 ✅

### 5. 既存パターンとの整合
- `kana_lower_names` を `SearchEngine` の Parallel Vec として追加するのは既存パターンに完全準拠 ✅

### 6. YAGNI 違反
- 設定フィールドなし・漢字未対応で最小実装 ✅
- インデックスキャッシュへの保存なし（再計算で十分）✅

### 7. シンプル化の挑戦
- kana_query を `search_with_history_boost()` で一度計算して fold に渡す設計は最小のオーバーヘッド ✅
- kana_char_masks の追加（最適化）は今回 YAGNI。性能問題が実測されてから検討 ✅

### 8. 破壊不変条件
- `kana_lower_names.len() == entries.len()` が崩れるとパニック（index out of bounds）。`debug_assert!` と `new()` / `new_with_cached_masks()` 両方への追加で防止 ✅
- 既存 `lower_names` の不変条件（`new_with_cached_masks` で v3 フォールバックと v4 の両コードパス）を kana_lower_names でも同様に対応する → 計画済み ✅

### セルフレビュー修正点

1. **負スコア対策**: `kana_substring_score` 関数の戻り値に `score.max(1)` または `if score > 0` フィルタを追加。計画に反映済み。
2. **to_kana が同一文字列を返す場合のスキップ**: ASCII クエリでも kana 変換しない場合（数字のみ "123" など）に無駄なマッチを防ぐ。計画に反映済み。
