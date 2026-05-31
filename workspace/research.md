# research.md — issue #337 メモリ削減: kana_lower_names を migemo 有効時のみ常駐させる

## issue の要約

`SearchEngine` の派生並列 Vec `kana_lower_names: Vec<Box<str>>`（エントリ名のひらがな正規化、migemo＝ローマ字→かな変換マッチ専用）は、現状 **migemo の有効/無効によらず常に全エントリ分構築・常駐**している。migemo はデフォルト無効のため、無効ユーザーでは完全な死蔵メモリ（50k 件で ~2.0〜2.7MB）。

→ `migemo_enabled` が true のときのみ `kana_lower_names` を構築・常駐させる。**遅延（lazy）ではなく eager**（インデックス構築時に判定）に行う。

## 関連コード（影響を受けるファイル・関数）

### snotra-core/src/search.rs（中核）
- `SearchEngine` struct: `kana_lower_names: Vec<Box<str>>` フィールド（L116）
- `compute_wave1(entries) -> Wave1Strings`（L150-191）: rayon::join で 4 つの派生文字列 Vec を並列構築。kana はここで `to_kana(&to_lower_folded(&e.name))` を全エントリに map している（最内 join の片側）
- `assemble(...)`（L225-256）: 全並列 Vec 長 == entries.len() を `debug_assert!`（L234-242）。kana も含まれる
- `new(entries)`（L258-272）: `compute_wave1` → `compute_wave2` → `assemble`
- `new_with_cached_masks(...)`（L280-320）: v4 キャッシュヒット時、Wave1 はスキップするが **kana は毎起動 `par_iter` で再計算**（L295-298）。v3 フォールバック時は `compute_wave1` 経由
- 検索ループ内 kana スコア（L564-571）: `kana_score = if primary_score.is_none() { kana_query.and_then(|kq| kana_substring_score(&self.kana_lower_names[i], kq)) }`。**ここが唯一の kana_lower_names 読み取り**。`self.kana_lower_names[i]` が空 Vec だと index out of bounds で panic する
- `kana_query` は `options.migemo_enabled == true` のときのみ `Some`（L375-387）。migemo 無効時は常に `None` → kana_score は評価されず `kana_lower_names[i]` に到達しない（＝無効時は既に死蔵）
- incremental cache の `kana_monotonic` / `prev_kana_query`（L443-447, 124-128）は `kana_query` 文字列のみを使い、`kana_lower_names` には触れない → 本変更の影響を受けない

### snotra-core/src/engine.rs
- `PrebuiltIndex::new(entries)`（L31-33）→ `SearchEngine::new(entries)`。**config を受け取らない**
- `Engine::new(entries, history, config)`（L43-49）→ `SearchEngine::new(entries)`。config は持っている
- `Engine::new_from_cache(entries, cached_masks, history, config)`（L54-72）→ `SearchEngine::new_with_cached_masks(...)`。config は持っている
- `Engine::update_config(config)`（L143-145）: **config を差し替えるのみ。SearchEngine を再構築しない**（必須条件 #1/#4 の根拠）
- `Engine::apply_prebuilt_index(index)`（L155-157）: `self.search_engine = index.0`（O(1) ムーブ・スワップ）
- `replace_entries`（L149-151, `#[cfg(test)]`）→ `SearchEngine::new`

### src-tauri/src/indexing.rs
- `start_index_build`: 背景スレッドで config から `scan / show_hidden_system / show_icons / include_path_env` をロック内キャプチャ（L32-41）→ ロック外で `PrebuiltIndex::new(entries)`（L72）→ ロック内 `apply_prebuilt_index`（L75）
- ビルド後 needs_rebuild（L79-87）: ビルド中に上記 4 設定が変わったら再ビルド予約。**migemo_enabled は含まれない**

### src-tauri/src/main.rs
- 起動時 L415-419: `cached_masks` の有無で `Engine::new_from_cache` / `Engine::new` を選択。直前まで `config` が手元にある

### src-tauri/src/config_watcher.rs
- `apply_config_change`: config.toml 変更検知 → `update_config` → `needs_reindex` が true かつ非ビルド中なら `start_index_build`
- `needs_reindex(old, new)`（L208-213）: `scan / show_hidden_system / show_icons / include_path_env` の差分のみ。**migemo_enabled は含まれない**

### snotra-core/src/config.rs
- `SearchConfig.migemo_enabled: bool`（L332、既定 `false`）、`migemo_min_chars: usize`（既定 2）

## 既存パターン（再利用できるもの）

- **eager 構築 @ インデックス構築時**: kana は既に `compute_wave1` / `new_with_cached_masks` の構築時に作られている。「常に作る」を「migemo 有効時のみ作る」に変えるだけで eager のまま。lazy 化は不要（必須条件 #1 が明示的に否定）
- **config 由来パラメータのスレッド渡し**: `show_hidden_system` 等が `Engine::new` → 内部で config から導出、`indexing.rs` でロック内キャプチャ → 背景処理へ、というパターンが既にある。migemo_enabled も同じ経路で渡せる
- **needs_reindex による設定変更→再構築**: show_icons / show_hidden_system トグルは既に full reindex をトリガーする確立されたパターン。migemo トグルもこれに乗せられる
- **ビルド中設定変更の差分検出**（indexing.rs L79-87 / 「開始時キャプチャ vs 完了後の現在値」AGENTS.md パターン）: migemo を同集合に加えれば、ビルド中の migemo 変更も拾える
- **`#[ignore]` + `std::time::Instant` ベンチ**（search.rs L1358-1461, `bench_new` 等）: criterion 不使用。migemo on/off の構築コスト比較ベンチをこの形式で追加できる

## 技術的制約

- **`update_config` は engine を再構築しない**（必須条件 #1/#4）。したがって kana 構築判定は `search_with_options` 内（毎検索・ロック内 par_iter スパイク）でも `update_config` 内でもなく、**インデックス構築時（`PrebuiltIndex::new` / `SearchEngine::new*`）**に置く
- **空 Vec アクセスの panic**（必須条件 #2）: migemo OFF で構築（kana 空）→ その後 migemo ON にして検索すると `kana_query=Some` になり `self.kana_lower_names[i]` に到達 → panic。空ガードが必須
- **並列 Vec 長の不変条件**: `assemble` の `debug_assert!` が「全 Vec == entries.len()」を要求。kana だけ 0 を許す例外に変更が必要（必須条件 #2）
- **Win32 非依存**: 変更は snotra-core（純ロジック）と src-tauri の config 配線。kana 構築・検索・状態遷移は snotra-core 内でユニットテスト可能
- **テスト構造**: search.rs の ~70 箇所が `SearchEngine::new(entries)` を直接呼ぶ。うち migemo を検索時に有効化するテスト（`migemo_config()` / inline `migemo_enabled: true` 使用）は ~13 箇所。これらは kana が構築済みであることに依存する

## 未解決の疑問（→ plan.md で意思決定）

1. **構築 API の形**: `SearchEngine::new(entries)` のシグネチャに `migemo_enabled: bool` を足す（~70 テスト改変）か、`new` は kana 常時構築のまま据え置き本番経路に別コンストラクタ（`new_with_migemo`）を足す（テスト改変ゼロ）か。
   - → **後者を採用予定**: `new(entries)` = `new_with_migemo(entries, true)` の薄いラッパー。本番（`Engine::new` / `new_from_cache` / `PrebuiltIndex::new`）のみ config 由来の migemo を渡す。テスト無改変・本番は最適化が効く。`new` がテスト/convenience 用であることをドキュメント化
2. **migemo ランタイムトグルの UX**: 現状（kana 常時構築）は migemo トグルが即時反映される。本変更後、`needs_reindex` に migemo を入れないと「ON にしても次の無関係な reindex / 再起動まで効かない（panic はしないがサイレントに無反応）」という後退が生じる。
   - → **`needs_reindex` + indexing.rs の in-flight needs_rebuild に migemo を追加予定**（既存 machinery 再利用＝KISS、対称に閉じる）。disk 再スキャンを伴うが migemo トグルは稀で、show_icons 等の既存トグルと同コスト水準。plan.md セルフレビューで veto 可能な決定として明示する
