# Plan — Issue #214: ローマ字入力で日本語名ファイルを検索（kana マッチ）

## 概要

クエリが ASCII のみかつ一定文字数以上のとき、`wana_kana::to_hiragana()` でひらがな変換した
kana_query を生成し、エントリ名のひらがな正規化済み Vec（`kana_lower_names`）に対して
Substring マッチする。

設定で **オン/オフ** と **最小文字数** を制御（デフォルト: off / 2文字）。
"a" → "あ" のような単一文字の意図しないマッチを防ぐ。

---

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/Cargo.toml` | `wana_kana` 依存追加 |
| `snotra-core/src/config.rs` | `SearchConfig` に `migemo_enabled`, `migemo_min_chars` 追加 |
| `snotra-core/src/query.rs` | `to_kana(s: &str) -> String` 追加 |
| `snotra-core/src/search.rs` | `kana_lower_names` Vec 追加、kana マッチロジック追加 |
| `snotra-settings/src/tabs/search.rs` | migemo 設定 UI 追加（checkbox + DragValue） |
| `snotra-settings/src/i18n.rs` | 翻訳キー追加（日英） |
| `SPEC.md` | §3.1/3.2 にローマ字検索の挙動を追記 |

---

## フェーズ構成

### Phase 1: wana_kana 依存追加

**ファイル**: `snotra-core/Cargo.toml`

```toml
wana_kana = "0.4"   # 実装時に crates.io で最新版を確認
```

---

### Phase 2: config.rs に設定フィールド追加

**ファイル**: `snotra-core/src/config.rs`、`SearchConfig` に追加:

```rust
/// ローマ字入力でかな名ファイルを検索する（migemo 風検索）。
/// デフォルト off: "a" → "あ" のような意図しないマッチを防ぐため、
/// ユーザーが明示的に有効化する設計。
#[serde(default)]
pub migemo_enabled: bool,           // default: false

/// migemo 検索を有効にするクエリの最小文字数。
/// 1文字（"a"→"あ"）の意図しないマッチを防ぐため、デフォルト 2。
#[serde(default = "default_migemo_min_chars")]
pub migemo_min_chars: usize,        // default: 2
```

```rust
fn default_migemo_min_chars() -> usize { 2 }
```

`Config::validate()` に追加:
- `migemo_min_chars == 0` → エラー（最小 1 以上）

---

### Phase 3: query.rs に to_kana() を追加

**ファイル**: `snotra-core/src/query.rs`

```rust
/// ASCII ローマ字をひらがなに変換する（カタカナもひらがなに正規化）。
/// 非 ASCII 文字（漢字など）はそのまま通過する。
pub fn to_kana(s: &str) -> String {
    wana_kana::to_hiragana(s)
}
```

---

### Phase 4: search.rs — kana_lower_names Vec と kana マッチ追加

**SearchEngine struct に追加**:

```rust
/// エントリ名をひらがな正規化した Vec（katakana→hiragana、ASCII はそのまま）。
/// migemo 検索（ローマ字→かな変換マッチ）で使用。
kana_lower_names: Vec<String>,
```

**`new()` の Wave 1 に追加** (lower_names と rayon::join で並列):

```rust
let ((lower_names, lower_file_names), (normalized_keys, kana_lower_names)) = rayon::join(
    || rayon::join(
        || entries.iter().map(|e| to_lower_folded(&e.name)).collect(),
        || /* lower_file_names (既存) */,
    ),
    || rayon::join(
        || /* normalized_keys (既存) */,
        || entries.iter().map(|e| to_kana(&to_lower_folded(&e.name))).collect(),
    ),
);
```

**`new_with_cached_masks()` も同様に追加**（v3 フォールバックパスと v4 キャッシュパスの両方）。
`kana_lower_names` はインデックスキャッシュに保存しない（毎起動再計算・意図的）。

**`debug_assert!` に追加**:

```rust
debug_assert!(
    lower_names.len() == entries.len()
        && lower_file_names.len() == entries.len()
        && normalized_keys.len() == entries.len()
        && char_masks.len() == entries.len()
        && file_name_char_masks.len() == entries.len()
        && kana_lower_names.len() == entries.len(),  // ← 追加
    "SearchEngine: all parallel Vecs must have the same length as entries"
);
```

**`Self {}` に `kana_lower_names` を追加**。

**`search_with_history_boost()` のシグネチャ変更**: `migemo_enabled: bool` と `migemo_min_chars: usize` を受け取るか、`HistoryBoostConfig` に含めるか。
→ `HistoryBoostConfig` に追加する（既存の渡し方に合わせる）:

```rust
pub struct HistoryBoostConfig {
    pub normalization: ...,
    pub fuzzy_history_cap_ratio: f64,
    pub migemo_enabled: bool,       // 追加
    pub migemo_min_chars: usize,    // 追加
}
```

`impl From<&SearchConfig> for HistoryBoostConfig` にも追加。

**kana_query 生成**:

```rust
let kana_query: Option<String> = if history_boost_config.migemo_enabled
    && norm_query.is_ascii()
    && norm_query.chars().count() >= history_boost_config.migemo_min_chars
{
    let k = to_kana(norm_query.as_ref());
    // 変換されなかった（英単語など）か、ASCII アルファベットが残っている場合はスキップ
    if k != norm_query.as_ref() && !k.bytes().any(|b| b.is_ascii_alphabetic()) {
        Some(k)
    } else {
        None
    }
} else {
    None
};
```

**scoring fold 内で kana マッチを追加**:

```rust
let kana_score = if name_score.is_none() {
    kana_query.as_deref().and_then(|kq| {
        kana_substring_score(&self.kana_lower_names[i], kq)
    })
} else {
    None
};

let score = name_score.or(kana_score);
```

**kana_substring_score 関数**:

```rust
/// ひらがな正規化済みエントリ名に対して substring マッチを行い、
/// マッチした場合は `4500 - byte_position` を返す（Substring の 5000 より低いスコア）。
/// byte_position: UTF-8 バイト位置。ひらがな1文字は3バイトのため、
/// Substring の char ベーススコアとは単純比較できないが実用上は問題ない。
fn kana_substring_score(kana_lower_name: &str, kana_query: &str) -> Option<i64> {
    kana_lower_name.find(kana_query).map(|pos| (4500i64 - pos as i64).max(1))
}
```

---

### Phase 5: ユニットテスト追加

**snotra-core/src/query.rs**:

```rust
fn to_kana_converts_romaji_to_hiragana()   // "dokyu" → "どきゅ"
fn to_kana_converts_katakana_to_hiragana() // "ドキュメント" → "どきゅめんと"
fn to_kana_passes_through_ascii()          // "documents" → "documents"
fn to_kana_passes_through_kanji()          // "書類" → "書類" を含む
```

**snotra-core/src/search.rs**:

```rust
fn kana_search_disabled_by_default()              // migemo_enabled=false → マッチしない
fn kana_search_matches_katakana_entry()            // "dokyu" で "ドキュメント" がヒット
fn kana_search_no_false_positive_for_ascii_entry() // "dokyu" で "documents" がヒットしない
fn kana_search_direct_match_ranks_above_kana_match()
fn kana_search_min_chars_blocks_short_query()      // 1文字クエリが migemo_min_chars=2 でマッチしない
fn kana_search_partial_romaji_not_used()           // "dok"（変換後に英字残存）でマッチしない
```

---

### Phase 6: 設定 UI（snotra-settings）

**ファイル**: `snotra-settings/src/tabs/search.rs`

検索設定タブ末尾（履歴スコアセクションの後）に追加:

```rust
// -- Migemo 検索 --
ui.heading(tr.heading_migemo());
ui.add_space(4.0);

ui.checkbox(&mut config.search.migemo_enabled, tr.cb_migemo_enabled());
ui.label(RichText::new(tr.hint_migemo()).small().color(TEXT_SECONDARY));

ui.add_space(4.0);

egui::Grid::new("migemo_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
    ui.label(tr.label_migemo_min_chars());
    ui.add_enabled_ui(config.search.migemo_enabled, |ui| {
        ui.add_sized(
            [60.0, ui.spacing().interact_size.y],
            egui::DragValue::new(&mut config.search.migemo_min_chars).range(1..=10),
        );
    });
    ui.end_row();
});
```

**ファイル**: `snotra-settings/src/i18n.rs`

```rust
pub fn heading_migemo(&self) -> &'static str {
    match self.0 {
        Language::Ja => "ローマ字検索（Migemo）",
        Language::En => "Romaji Search (Migemo)",
    }
}

pub fn cb_migemo_enabled(&self) -> &'static str {
    match self.0 {
        Language::Ja => "ローマ字入力で日本語名ファイルを検索する",
        Language::En => "Search Japanese filenames with romaji input",
    }
}

pub fn hint_migemo(&self) -> &'static str {
    match self.0 {
        Language::Ja => "例: \"dokyu\" と入力すると \"ドキュメント\" がヒットします。漢字名は対象外",
        Language::En => "e.g. type \"dokyu\" to find \"ドキュメント\". Kanji names are not supported",
    }
}

pub fn label_migemo_min_chars(&self) -> &'static str {
    match self.0 {
        Language::Ja => "最小文字数:",
        Language::En => "Min chars:",
    }
}
```

---

### Phase 7: SPEC.md 更新

§3.1「検索方式」の末尾に追加:

```
- ローマ字検索（設定で有効化、デフォルト無効）: クエリが ASCII のみかつ
  最小文字数以上の場合、ひらがな変換してエントリのかな正規化名に
  中間部分一致で検索する（カタカナ名対応、漢字名は対象外）。
```

§3.2「クエリ正規化」の末尾に追加:

```
- ローマ字検索時の追加処理: migemo_enabled が true かつクエリが ASCII のみ
  かつ migemo_min_chars 以上の場合、to_hiragana() で kana_query を生成し、
  エントリのかな正規化名に中間部分一致する（score: max(4500 - byte_pos, 1)）。
  直接一致（Prefix/Substring/Fuzzy）のスコアより低い。
  kana_query に ASCII アルファベットが残留する場合は使用しない。
```

---

## 不変条件

1. `kana_lower_names.len() == entries.len()` は常に成立（debug_assert で保証）
2. kana マッチは primary match の代替（OR）であり、加算ではない
3. `migemo_enabled = false`（デフォルト）のとき kana パスは完全に不活性
4. `kana_query` に ASCII アルファベットが残留する場合は `None` にして使用しない
5. kana_lower_names はインデックスキャッシュに保存しない（毎起動再計算）

---

## テスト方針

- `cargo test -p snotra-core`（全既存テスト + 新規 10 テスト）
- `cargo check -p snotra-core -p snotra -p snotra-settings`

---

## SPEC.md 更新要否

✅ §3.1/3.2 に追記必要

---

## セルフレビュー（更新済み）

| 観点 | 対応状況 |
|------|---------|
| 対称コードパス | `new()` / `new_with_cached_masks()` 両方に `kana_lower_names` 構築を追加 ✅ |
| debug_assert! 更新 | `kana_lower_names.len() == entries.len()` を追加 ✅ |
| 負スコア対策 | `.max(1)` で 0 以下を防止 ✅ |
| 不完全ローマ字ガード | `k.bytes().any(|b| b.is_ascii_alphabetic())` で ASCII 残留を検出 → None ✅ |
| "a" 単体問題 | `migemo_min_chars`（デフォルト 2）で排除 ✅ |
| migemo デフォルト off | 既存ユーザーの検索結果を変えない ✅ |
| `HistoryBoostConfig` 拡張 | `migemo_enabled` / `migemo_min_chars` を追加（既存の渡し方と統一）✅ |
| YAGNI | 漢字未対応・kana_char_masks なし・キャッシュなし ✅ |
| `entry_view()` 非追加 | `kana_lower_names[i]` は fold 内で直接参照（`char_masks` と同パターン）✅ |
