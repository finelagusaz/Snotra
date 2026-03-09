# Research — Issue #214: 英語入力のまま日本語名ファイルを検索（ローマ字/かな検索）

## issue の要約

IME を切り替えずにローマ字入力のまま、カタカナ・ひらがな名のファイルやフォルダを検索できるようにする。
"migemo 検索に対応する" が公式の案として挙げられている。

---

## 関連コード

| ファイル | 役割 | 変更要否 |
|---------|------|--------|
| `snotra-core/Cargo.toml` | 依存クレート | ✅ wana_kana 追加 |
| `snotra-core/src/query.rs` | クエリ正規化 | ✅ to_kana() 追加 |
| `snotra-core/src/search.rs` | 検索スコアリング | ✅ kana_lower_names・kana マッチ追加 |
| `SPEC.md` | 仕様書 §3.1/3.2 | ✅ ローマ字検索の挙動を追記 |
| `snotra-settings/**` | 設定 UI | ❌ 設定なし（常時 ON） |

---

## 既存パターン

- `SearchEngine` は Parallel Vec (SoA) レイアウト。`kana_lower_names: Vec<String>` を追加するのが自然。
- `new()` と `new_with_cached_masks()` の両コンストラクタで Vec を構築する必要がある（`lower_names` と同じパターン）。
- `entry_view()` / `debug_assert!` / 末尾 `Self {}` は新 Vec 追加時に必ず更新するルールが CLAUDE.md に明記されている。
- Fuzzy モードのビットマスク pre-filter: 非 ASCII エントリは `u64::MAX` が設定されるため、カタカナ名エントリは必ず pre-filter を通過する → kana マッチの機会が確保される。

---

## アプローチ選定

### 採用: 常時 ON（設定フィールドなし）

クエリが ASCII のみのとき → `to_hiragana(query)` で kana_query を生成
エントリの kana_lower_name（katakana → hiragana 変換済み）に substring マッチ

**偽陽性分析**:

| エントリ名 | lower_name | kana_lower_name | kana_query例 "どき" | 結果 |
|-----------|-----------|-----------------|-------------------|------|
| "Documents" | "documents" | "documents" (変換なし) | 含まれない | 偽陽性なし ✓ |
| "書類" (漢字) | "書類" | "書類" (変換なし) | 含まれない | 偽陽性なし ✓ |
| "ドキュメント" | "ドキュメント" | "どきゅめんと" | 含まれる | 正しい一致 ✓ |
| "Desukutoppu" (ASCII 日本語風) | "desukutoppu" | "desukutoppu" | 含まれない | 偽陽性なし ✓ |

→ 常時 ON で安全。KISS/YAGNI の観点で設定フィールド追加不要。

### 不採用: opt-in フラグ（SearchConfig に romaji_enabled 追加）

偽陽性がないため、設定追加は YAGNI。

---

## 使用ライブラリ: wana_kana

- crates.io: `wana_kana`（wana_kana.js の Rust 実装、成熟・メンテ継続中）
- API:
  - `wana_kana::to_hiragana("dokyu")` → `"どきゅ"`
  - `wana_kana::to_hiragana("ドキュメント")` → `"どきゅめんと"`（katakana → hiragana も対応）
  - ASCII 文字列はそのまま通過（"documents" → "documents"）
- 漢字変換は **不対応**（辞書不要・軽量な点を優先。漢字名は今回対応外 = YAGNI）

---

## kana_lower_names の構築コスト

- 各エントリ名に `to_hiragana(to_lower_folded(name))` を適用（O(name_len) の文字列操作）
- `lower_names` 構築と同じ rayon::join の Wave 1 に追加可能
- キャッシュ不要（`lower_names` から導出できる、インデックスファイルに保存しない）

---

## kana マッチのスコアリング

- Primary match（既存）が `None` の場合のみ kana マッチを試みる（OR 関係）
- kana マッチは常に Substring 方式: `4500 - byte_position`
  - Prefix(10000) > Substring(5000) > kana-Substring(4500)
  - 直接一致のほうが kana 経由より常に高スコア ✓

---

## インクリメンタルキャッシュとの整合

- `prev_candidates`: primary OR kana にマッチしたインデックスを格納
- クエリが ASCII のまま伸長 → kana_query も単調収束 → prev_candidates から絞り込み可能 ✓
- クエリが ASCII → 非 ASCII に変化 → `norm_query.starts_with(prev_query)` が false → full scan ✓

---

## 技術的制約

- `wana_kana::to_hiragana()` は部分ローマ字（"dok" の "k" など）を残す可能性がある
  - "dok" → "どk"（"k" は kana_lower_name に存在しない → substring 失敗）
  - インクリメンタル検索なので次のキー入力で改善。実用上問題なし
- 漢字ファイル名（"書類フォルダ"）はローマ字検索不可 → issue には漢字は言及なし。YAGNI

---

## 未解決の疑問

- `wana_kana` の正確なバージョン（0.4.x を想定、実装時に crates.io で確認）
