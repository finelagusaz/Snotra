# research — issue #461 IndexCache 双子の Cow 統合

## issue の要約

#436 direction 1（SearchEngine 派生フィールド追加コストの削減）の分離 issue。build 側は #337/#437 で集約済みゆえ、残る痛みは **cache スキーマの三兄弟**（`IndexCache` / `IndexCacheRef` / `CachedMasks`）+ versioning に集中。「実測・調査してから *やるか否か* を決める」設計判断が核。

## 一段抽象化 — cache 側の 3 concern

派生カラム追加時の cache 側の触り箇所（約 8）は性質の違う 3 concern に分かれる:

| concern | 実体 | 判断 |
|---|---|---|
| **① owned/borrowed 双子** | `IndexCache`(`Vec<T>`・読) + `IndexCacheRef`(`&[T]`・書, clone 回避) | **本 issue で畳む**（Cow 統合） |
| **② versioning** | `IndexCacheV3`/`V2` + `INDEX_CACHE_VERSION` + `load_cache` 版分岐 | **不可約**（後方互換は版ごと処理が本質）→ 触らない |
| **③ カラム横断重複** | 同 5 カラムが `IndexCache`/`CachedMasks`/`save`/`extend` に列挙 | field-list マクロは**過剰設計**（カラム非一様・追加は年1回程度）→ やらない |

**結論（ユーザー承認済み）**: ① 双子の Cow 統合のみ実施。最も危険な footgun（postcard 位置依存で `IndexCacheRef` と `IndexCache` のフィールド順がズレると `index.bin` を**無言破損**、テスト1本+コメントで辛うじて防御）を、型として起こり得なくする。②③ は irreducible / over-engineering として維持。

## feasibility spike（実証済み）

使い捨てテスト `spike_cow_unified_byte_identical_and_roundtrips`（実施後撤去）で実 serde/postcard により検証:

1. ✅ `Cow::Borrowed`（save 経路）は owned `IndexCache` と**バイト一致**（clone 無しシリアライズが成立）
2. ✅ deserialize は `Cow::Owned` を返す（load 経路成立・`IndexCache<'static>` へ）
3. ✅ **バイト形式不変ゆえ `INDEX_CACHE_VERSION` バンプ不要**（既存 v4 `index.bin` がそのままロード可能）

serde の `Cow<'a,[T]>` は `#[serde(borrow)]` 無しで Owned deserialize（AppEntry/u64/String/Option<String> 全カラムで確認）。

## 関連コード

### 変更対象（`snotra-core/src/indexer.rs`）

- `IndexCache`（L219-228, owned・`Serialize+Deserialize`）→ `IndexCache<'a>` に Cow 化
- `IndexCacheRef<'a>`（L238-248, borrowed・`Serialize`）→ **削除**（統合）
- `save_cache_sorted_in`（L404-441）: `IndexCacheRef {...}` → `IndexCache { entries: Cow::Borrowed(entries), char_masks: Cow::Borrowed(&char_masks), ... }`
- `load_cache_in` v4 分岐（L474-489）: `try_deserialize::<IndexCache<'static>>` + Cow フィールドを `.into_owned()` で `CachedMasks`/`entries` へ
- テスト `index_cache_ref_serializes_identically_to_owned`（L1008-1067）: 双子が 1 struct になり**前提消滅** → golden-bytes 形式安定テストへ**転用**（下記）

### 触らない

- `IndexCacheV3` / `IndexCacheV2`（versioning・後方互換）
- `CachedMasks`（in-memory ハンドオフ・Option 段は v3 由来ゆえ維持）
- `extend_cached_masks` / `INDEX_CACHE_VERSION`
- `search.rs::new_with_cached_masks`（`CachedMasks` の消費側・非接触）

## 既存パターン

- `Cow<'a, [T]>` の serde Owned deserialize は spike で実証。`try_deserialize_with_header<T: DeserializeOwned>` ゆえ load は `IndexCache<'static>` へ
- backward-compat の既存ガード: `save_cache_sorted_in_then_load_cache_in_roundtrip` / `load_cache_v3_fallback_yields_masks_without_lower_names` / `load_cache_v2_migrates_to_no_masks`（v2/v3 の deserialize を担保・**非接触で維持**）

## 技術的制約・不変条件

- **on-disk バイト形式は不変**（spike 済み）。version バンプ不要＝既存ユーザーの `index.bin` を壊さない
- **read 側 SoA レイアウトは維持**（AoS 化は #110 で 35–120% 劣化確認済み・本 issue は touch しない）
- **load の `.into_owned()` は no-op move**（deserialize が Owned を返すため clone 無し・性能非退行）
- **孤立する不変条件の補填**: 旧テストは「双子のバイト一致」を守っていた。統合で双子が消えると「IndexCache フィールド順を変えると save+load 自己整合ゆえ既存テストが素通りし、既存 `index.bin` を無言破損」という新たな盲点が生じる（save/load が同一 struct になるため roundtrip テストが reorder を検出できない）。→ golden-bytes 形式安定テストで補填（AGENTS.md「転用で失われる不変条件は別テストで補う」）

## 未解決の疑問

- なし（feasibility は spike 実証済み・射程はユーザー承認済み）
