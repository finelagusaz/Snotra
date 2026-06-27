# research.md — issue #333: IconCache 件数上限 + 挿入順 LRU 退避

## issue の要約

`src-tauri/src/icon.rs` の `IconCache`（PNG バイト列を `HashMap<String, Vec<u8>>` に保持）は
件数上限・退避がなく **単調増加**する（常駐メモリと `icons.bin` の両方）。
挿入順 LRU（FIFO 退避）を導入し、上限超過で最古をエントリから退避して常駐・永続の両方を頭打ちにする。
主目的は「無制限増加の停止」。中央値 1MB 未満〜数MB のメモリ削減見込み。

## 関連コード

- `src-tauri/src/icon.rs`
  - `IconCacheData { png: HashMap<String, Vec<u8>> }`（serde 永続化対象、`ICON_VERSION = 5`）
  - `IconCache { data, dirty }`：`load()` / `get()`（read-only）/ `insert()` / `save_if_dirty()` / `retain_paths()`
  - `IconCacheState = Mutex<Option<IconCache>>`
  - テスト `invalidate_icon_cache_clears_in_memory_state` が `IconCache { data, dirty }` を直接構築（フィールド追加時に要更新）
- `src-tauri/src/commands/icon.rs`
  - `ensure_icon_cache_loaded_if_enabled()`：engine ロックで `show_icons` を読み、`IconCache::load()` で遅延初期化
  - `get_icons_batch()`：Step1 = read lock でヒット確認（**ここは read-only 維持が必須**）、Step2 = ロック外抽出、Step3 = write lock で `insert` + バイナリ応答構築
- `snotra-core/src/config.rs`
  - `Config`（`#[serde(default)]` セクション群：general / visual / search / openers / instant_commands）
  - `AppearanceConfig.max_results`（既定 8、**上限クランプなし**）
  - `validate() -> Vec<ConfigError>`（report-only、clamp しない）
  - `Config::default()` の明示 struct 初期化
- `snotra-core/src/error.rs`：`ConfigError` enum
- `snotra-settings/src/app.rs`：`config_error_message()` が `ConfigError` を**網羅 match**（variant 追加で要更新）
- `snotra-settings/src/i18n.rs`：`Tr` の `err_*()` 翻訳メソッド（Ja/En）
- `snotra-core/src/binfmt.rs`：postcard シリアライズ。ヘッダ（magic+version）+ postcard 本体
- `SPEC.md §3.4 アイコン`：挙動文書（上限導入で同期が必要）

## 既存パターン（再利用）

- **`#[serde(default)]` セクション + `default_*()` 関数 + `Default` impl**：`SearchConfig` / `VisualConfig` が踏襲。
  新規 `[cache]` セクションも同型で追加すれば、キー欠落時のデフォルトフォールバックが serde で自動的に成立する（要求条件4）。
- **`validate()` への ConfigError 追加**：`MaxResultsZero` / `MigemoMinCharsZero` と同型。
  追加時は `error.rs`（enum）+ `app.rs`（網羅 match）+ `i18n.rs`（Ja/En 翻訳）の3点を揃える（既存変更の前例どおり）。
- **後方互換テスト**：`from_toml_str_fills_defaults`（必須セクションのみの TOML で optional セクションがデフォルト補完されることを検証）が雛形。

## 技術的制約

- **挿入順 LRU の厳守（要求条件1）**：`get` でアクセス順を更新すると Step1 の read lock が write lock に変質し性能退行する。
  → 退避は **`insert` / `load` の write-lock 経路のみ**。`get` は read-only を維持する。実質「FIFO 退避（最古挿入を pop）」。
- **`icons.bin` フォーマット不変（要求条件5）**：`ICON_VERSION` バンプ不要。
  - postcard は struct を**フィールド位置順**でエンコードし、`HashMap` / `IndexMap` はどちらも serde の `serialize_map`（`[len varint][k][v]...`）。
    → `IconCacheData.png` の型を `HashMap<String, Vec<u8>>` → `IndexMap<String, Vec<u8>>` に替えても **wire 形式は byte 互換**。
    旧 v5 `icons.bin`（HashMap 書き込み）も `IndexMap` に問題なくデシリアライズできる（順序はファイル出現順＝旧 HashMap 反復順、以後の insert で正しい FIFO に収束）。
  - `IndexMap` は **挿入順を反復・シリアライズで保持**するため、今後書く `icons.bin` は FIFO 順序が round-trip で永続化される（追加のサイドカー不要）。
- **`indexmap 2.14.0` は既に依存ツリーに存在**（transitive）。`src-tauri/Cargo.toml` に `indexmap = { version = "2", features = ["serde"] }` を直接依存として宣言する（serde feature で Serialize/Deserialize を有効化）。
- **cap >= max_results（要求条件2）**：`config.rs` の `max_results` は上限クランプがない。
  - `validate()` に `IconCacheCapTooSmall { cap, max_results }` を追加（report-only、設定 UI でユーザーへフィードバック）。
  - ただし validate() は保存時（設定 UI）にしか走らず、手編集 `config.toml` のロードでは走らない。
    → **退避の実効 cap を `cap.max(max_results)` で floor**（`commands/icon.rs` で両値を読める）し、
      バリデーションをすり抜けた極小 cap でも「表示中アイコンが evict されフォールバック絵文字に落ちない」不変条件を担保する（防御的多重化）。
- **config 変更の即時反映はスコープ外（YAGNI）**：cap を実行中に変えても、in-memory `IconCache` は次回ロード（invalidate / 再起動）まで旧 cap。
  cap 変更は稀でデータ損失リスクもないため、`config_watcher` への配線は行わない（issue にも要求なし）。

## 未解決の疑問

- なし。要求条件1〜5が受け入れ条件として一意に与えられており、実装判断のみ。
- cap デフォルト値は issue 提示の「200〜1000」のうち **1000**（キャッシュヒット率を最大化しつつ常駐 ≤ ~1–2MB に頭打ち）を採用予定。plan.md で確定。
