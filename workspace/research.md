# research: issue #522 — アイコンキャッシュ無効化の TOCTOU

## issue の要約

`invalidate_icon_cache_with`（`src-tauri/src/icon.rs:156-163`）が「メモリ `None` 化 → **unlock** → `icons.bin` 削除」の順で動くため、unlock と削除の間に `ensure_icon_cache_loaded_if_enabled`（`commands/icon.rs:8-26`）が None を検知して**削除直前の旧ファイル**をメモリへ戻せる。確率的プローブで **17/2000 回再現済み**（issue 本文）。残存した旧アイコンは IPC で返され続け、`dirty` が立てば終了時 `save_if_dirty`（`main.rs`）で再永続化され無効化が恒久的に巻き戻る。

## 関連コード

| 箇所 | 内容 |
|---|---|
| `icon.rs:151-153` | `invalidate_icon_cache`（公開ラッパー） |
| `icon.rs:156-163` | `invalidate_icon_cache_with` — **修正対象**。lock → `*g = None` → unlock → `bf.remove()` |
| `commands/icon.rs:8-26` | `ensure_icon_cache_loaded_if_enabled` — icons lock 内で None 検知 → `IconCache::load()`（競合の相手方。**変更不要** — 削除が同一 lock 内に入れば割り込めない） |
| `main.rs:737-751` | 唯一の呼び出し元（背景再スキャンスレッド）。**他のロックを保持せず**呼ぶ（`try_state` → 直呼び） |
| `icon.rs:369-383` | 既存テスト `invalidate_icon_cache_clears_in_memory_state`（bin_file=None 経路） |

## 既存パターン

- `IconCacheState` lock 内のファイル I/O は既存（`commands/icon.rs:24` の `IconCache::load`）。削除（`remove_file` 1 回）を lock 内に入れるのは既存パターンの範囲内
- テスト dir 注入は `BinFile::new_in`（#429 の確立済み経路）。実証プローブ（issue 記載）がそのまま回帰テストの雛形になる

## 技術的制約・ロック順序

- `invalidate_icon_cache_with` が icons lock 保持中に取得する他のロックは無い（`bf.remove()` = `fs::remove_file` のみ）→ デッドロック循環なし。Codex 分析（本セッション）でも「Engine と IconCacheState を同時保持する経路なし」を確認済み
- lock 内の `remove_file` は死んだ UNC 問題（#524）とは無関係 — `icons.bin` は `%APPDATA%\Snotra`（ローカル固定パス）

## 修正方針（issue 記載案を採用）

lock 保持中に「ファイル削除 → メモリ None 化」を行う:

```rust
fn invalidate_icon_cache_with(icons: &IconCacheState, bin_file: Option<BinFile>) {
    let mut g = icons.lock().unwrap();
    if let Some(bf) = bin_file {
        bf.remove();
    }
    *g = None;
}
```

`ensure_icon_cache_loaded_if_enabled` の「None 検知 → ロード」も同一 lock 内なので、削除が lock 内に入れば「旧ファイルを読める瞬間に None が観測される」状態が消える（load が観測する None は常に「ファイル削除完了後」）。

## 未解決の疑問

なし。
