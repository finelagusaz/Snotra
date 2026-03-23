# パス検索の廃止と通常検索のパスマッチング追加

## 背景

ユーザーが `C:\tools\ed` のようにパス形式で入力すると、フロントエンドが `parsePathQuery()` で検出し `list_folder` IPC に振り分ける。この「パス検索」ルートでは `folder.rs` のスコアリングが使われ、起動履歴もマッチスコアも反映されない。

ユーザーは通常検索とパス検索を意識していないため、入力の形式によってスコアリングの有無が変わるのは直感に反する。

参考実装の fenrir では `tool/editor` や `tool/editor/editor.exe` のようなパス部分一致検索に対応しており、パス検索を単に削除するだけでは退化になる。

**方針**: パス検索ルートを廃止し、通常検索（`search.rs`）にフルパスマッチングを追加することで、全入力を統一スコアリングで処理する。

### 意図的な機能廃止

パス検索ルートの廃止により、インデックス外ディレクトリの `read_dir` 探索（`C:\random\path\` と入力してファイルシステムを直接走査する機能）は廃止される。インデックス外ディレクトリの探索はフォルダ展開（ArrowRight）でカバーする。

## ゴール

- 全てのテキスト入力を通常検索（`search` IPC）に統一する
- 通常検索がエントリのフルパス（`target_path`）にもマッチできるようにする
- `tool/editor` のようなパス部分一致で候補がヒットし、履歴スコアリングも効く
- フォルダ展開は既存のまま維持する

## 実行順序

**Phase 2（バックエンド）→ Phase 1（フロントエンド）→ Phase 3（ドキュメント）**

Phase 2 を先に実装することで、中間状態での機能退化をゼロにする:
- Phase 2 完了時点: パスマッチングが追加されるが、フロントエンドはまだ `list_folder` に振り分けるため、ユーザーから見た挙動は変わらない
- Phase 1 完了時点: パス入力が通常検索に流れ、Phase 2 で追加済みのパスマッチングが即座に機能する

1 PR にまとめ、コミットは Phase ごとに分ける。

---

## Phase 2: 通常検索にフルパスマッチングを追加（バックエンド）

### 受け入れ条件

1. `tool/editor` と入力すると、`target_path` に `tool\editor`（または `tool/editor`）を含むエントリがヒットする
2. パスマッチのスコアはエントリ名マッチより低い（名前マッチ優先）
3. 履歴スコアリング（`global_count`、`query_count`）がパスマッチにも適用される
4. 既存の名前マッチ・かなマッチの挙動に変化がない
5. パフォーマンス劣化が許容範囲（30ms 以内の応答時間を維持）

### 変更するファイル

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/src/search.rs` | スコアリングループにパスマッチ追加、incremental cache ガード追加 |

### 触らないファイル

| ファイル | 理由 |
|---------|------|
| `snotra-core/src/indexer.rs` | `normalized_keys` を再利用するため IndexCache 変更不要 |
| `snotra-core/src/engine.rs` | 変更なし |
| `snotra-core/src/history.rs` | 変更なし |

### 前提知識

- `normalize_query()`（`query.rs`）は `\` `/` をそのまま保持する（空白正規化・小文字化・アクセント折り畳みのみ）
- `normalize_entry_key()`（`indexer.rs`）は小文字化 + `/` → `\` 正規化のみ（アクセント折り畳みなし）
- 両者のアクセント折り畳みに非対称性があるが、Windows ファイルパスにアクセント付き文字が含まれるケースは極めて稀なため許容する

### 設計

#### 2.1 新フィールド追加なし — `normalized_keys` を再利用

`normalized_keys` は既に `normalize_entry_key(&target_path)` で構築されており、「小文字化 + `/` → `\` 正規化」済み。`EntryView` には既に `normalized_key: &'a str` があるため、`v.normalized_key.find(pq)` で済む。

→ **5箇所同時更新ルールの適用外**。SearchEngine の構造体・`new()`・`new_with_cached_masks()`・IndexCache のいずれも変更しない。

#### 2.2 変数宣言（376行目付近、`has_dot` の近く）

```rust
let has_dot = norm_query.contains('.');                          // 既存（376行目）

// --- 以下を追加 ---
let has_path_sep = norm_query_str.contains('\\') || norm_query_str.contains('/');
let path_query: Option<String> = if has_path_sep {
    Some(norm_query_str.replace('/', "\\"))
} else {
    None
};
```

注: `path_query` は `Option<String>` で、rayon の `fold` クロージャ内から `path_query.as_deref()` で `&Option<&str>` として借用キャプチャされる。`norm_query_str` は `&str` なので `has_path_sep` の宣言は `norm_query_str` の束縛（399行目）の後に配置する。

実際の配置場所は **399行目**（`let norm_query_str: &str = &norm_query;`）**の直後**。

#### 2.3 ビットマスク pre-filter の対応（458行目）

パス区切りを含むクエリでは、name/file_name のビットマスクで「パスだけでマッチするエントリ」が落ちる問題がある。パス区切りを含む入力は稀であり、かつパスの文字種が広いため pre-filter の除外効率が元々低い。`has_path_sep` 時は pre-filter をスキップする:

既存:
```rust
if mode == SearchMode::Fuzzy {
```

変更後:
```rust
if mode == SearchMode::Fuzzy && !has_path_sep {
```

#### 2.4 パスマッチの挿入（538行目の直後）

既存の 538行目 `let score = primary_score.or(kana_score);` の直後、540行目 `if let Some(base_score) = score {` の直前に挿入:

```rust
                    let score = primary_score.or(kana_score);    // 既存（538行目）

                    // --- 以下を挿入 ---
                    // パスマッチ: name/file_name/kana 全て不成立時のフォールバック。
                    // normalized_key は normalize_entry_key() で小文字化 + パス区切り正規化済み。
                    // スコア 3000 は Kana(4500) より低く、名前マッチを常に優先する。
                    let score = if score.is_none() {
                        path_query.as_deref().and_then(|pq| {
                            let pos = v.normalized_key.find(pq)?;
                            Some((3000i64 - (pos as i64).min(500)).max(1))
                        }).or(score)
                    } else {
                        score
                    };

                    if let Some(base_score) = score {             // 既存（540行目）
```

スコア体系:
- ベース `3000`（名前 Prefix 10000 / Substring 5000 / Kana 4500 より低い）
- byte_position 減衰を `min(500)` でクランプ（パスは 50-100+ bytes と長いため、無制限だと深いパスのエントリが極端に不利になる）
- 常に Substring（部分一致）固定。Prefix はパス先頭が `c:\` 固定で無意味、Fuzzy はパス全体に対してノイズが多すぎる

#### 2.5 incremental search cache（422-427行目）

`has_path_sep` が `true` → `false` に遷移するケース（クエリから `\` `/` を削除した場合）では incremental が使えない。`has_dot` ガードと同じパターンで対応:

既存:
```rust
let use_incremental = self.prev_mode == Some(mode)
    && !self.prev_candidates.is_empty()
    && !self.prev_query.is_empty()
    && norm_query.starts_with(self.prev_query.as_str())
    && (!has_dot || self.prev_query.contains('.'))
    && kana_monotonic;
```

変更後（1行追加）:
```rust
let use_incremental = self.prev_mode == Some(mode)
    && !self.prev_candidates.is_empty()
    && !self.prev_query.is_empty()
    && norm_query.starts_with(self.prev_query.as_str())
    && (!has_dot || self.prev_query.contains('.'))
    && (!has_path_sep || self.prev_query.contains('\\') || self.prev_query.contains('/'))
    && kana_monotonic;
```

単調性の根拠:
- Substring の `find()` は prefix 拡張で結果集合 ⊆ 前回
- prefix 拡張で `\` `/` は消えない → `has_path_sep` は `true` のまま
- `None` → `Some` 遷移（`\` が新たに追加された）は full scan になる（正しい — パスマッチ候補が新たに出現するため）
- ユーザーが `tool/ed` と入力 → `norm_query_str` = `tool/ed`, `prev_query` = `tool/ed`（`/` を保持） → 次に `tool/editor` → `prev_query.contains('/')` = true → incremental OK

### 新規テスト

テストでは `AppEntry` を直接構築する（既存の `make_entries()` は `target_path` を自由に設定できないため）:

```rust
fn make_entry(name: &str, path: &str) -> AppEntry {
    AppEntry {
        name: name.to_string(),
        target_path: path.to_string(),
        is_folder: false,
    }
}

fn make_folder_entry(name: &str, path: &str) -> AppEntry {
    AppEntry {
        name: name.to_string(),
        target_path: path.to_string(),
        is_folder: true,
    }
}
```

テストケース:

```
test path_match_substring_finds_entry_by_path_segment
  entries: [make_entry("app", "C:\\tool\\editor\\app.exe")]
  query: "tool\\editor"
  → 1件ヒット、name="app"

test path_match_score_below_name_match
  entries: [
    make_entry("editor", "C:\\tool\\editor\\editor.exe"),  // name="editor" で name_score あり
    make_entry("app", "C:\\tool\\editor\\app.exe"),        // name="app" で name_score なし → path_score
  ]
  query: "editor"
  → "editor" が "app" より上位（name_score > path_score はない。"editor" は name マッチ、"app" はパスに "editor" を含むが name は "app" なのでパスマッチ。ただし "editor" を含まない name "app" はパスマッチもしない）
  → 修正: query を "tool\\editor" にする。"editor" は name_score(Substring)、"app" は path_score のみ。name_score(5000) > path_score(3000) で "editor" が上位

test path_match_slash_normalized
  entries: [make_entry("app", "C:\\tool\\editor\\app.exe")]
  query: "tool/editor"（`/` で入力）
  → 1件ヒット（`/` が `\` に正規化されてマッチ）

test path_match_receives_history_boost
  entries: [
    make_entry("app1", "C:\\tool\\editor\\app1.exe"),  // global_count=5
    make_entry("app2", "C:\\tool\\editor\\app2.exe"),  // global_count=0
  ]
  history: app1 を 5回 record_launch
  query: "tool\\editor"
  → app1 が app2 より上位（path_score は同等だが history boost で差がつく）

test path_match_incremental_cache_monotonic
  entries: [make_entry("app", "C:\\tool\\editor\\app.exe")]
  query1: "tool\\"、query2: "tool\\ed"
  → query1 でヒット → query2 で incremental cache が使われ、同じエントリがヒット

test path_match_no_match_returns_none
  entries: [make_entry("app", "C:\\tool\\editor\\app.exe")]
  query: "xyz\\abc"
  → 0件

test path_match_fuzzy_mode_skips_bitmask_prefilter
  entries: [make_entry("zzz", "C:\\tool\\editor\\zzz.exe")]
    — name="zzz" はクエリ "tool\\editor" の文字を含まない → ビットマスクで落ちるはず
  mode: Fuzzy
  query: "tool\\editor"
  → 1件ヒット（has_path_sep でビットマスクがスキップされ、path_score でマッチ）
```

### ベンチマーク

既存のベンチマーク（`bench_fuzzy_search_scaling` 等）にパス区切りを含むクエリ `"tool\\ed"` を追加し、pre-filter スキップ時のパフォーマンスを計測する。

### 検証

1. `cargo test -p snotra-core`
2. `cargo check -p snotra-core -p snotra -p snotra-settings`
3. `cargo clippy -p snotra-core`

---

## Phase 1: パス検索ルートの廃止（フロントエンド）

### 受け入れ条件

1. `\` `/` を含む入力がパス検索ルートに振り分けられず、通常検索に流れる
2. フォルダ展開（ArrowRight/ArrowLeft）は従来通り動作する
3. スラッシュコマンド・インスタントコマンドに影響がない

### 削除するファイル

| ファイル | 理由 |
|---------|------|
| `ui/src/lib/pathQuery.ts` | パスクエリ判定ロジック。不要になる |
| `ui/src/lib/pathQuery.test.ts` | 上記のテスト |

### 変更するファイル

| ファイル | 変更内容 |
|---------|---------|
| `ui/src/stores/search.ts` | `parsePathQuery` の import 削除、`refreshResults()` から `pathQuery` 分岐・trace ログを削除 |

### 設計

`search.ts` の `refreshResults()` から `pathQuery` 関連を全て削除:

1. **7行目**: `import { parsePathQuery } from "../lib/pathQuery";` を削除
2. **159行目**: `const pathQuery = fs ? null : parsePathQuery(q);` を削除
3. **160-164行目**: `source` 判定を簡略化
4. **177-185行目**: `else if (pathQuery) { ... }` 分岐を丸ごと削除（trace ログ含む）

変更後の `refreshResults()` 該当部分:

```typescript
  const source = trimmed === "/r"
    ? "history"
    : fs
    ? "folder"
    : "query";
  perfStartSearch(requestId, source);

  let items: SearchResult[];
  if (fs) {
    trace("search:api:call", {
      requestId,
      api: "list_folder",
      dir: fs.currentDir,
      filter: folderFilter(),
      mode: "folder_state",
    });
    items = await api.listFolder(fs.currentDir, folderFilter());
  } else if (trimmed === "") {
    trace("search:refresh:branch", { requestId, branch: "empty_query" });
    items = [];
  } else {
    trace("search:api:call", { requestId, api: "search", query: q });
    items = await api.search(q);
  }
```

### 挙動変化の注意点

- **`noResults` 表示**: 現在パス入力は `source === "folder"` で `noResults` が常に `false` だった。変更後は `source === "query"` になり、パスマッチで結果が0件の場合「該当なし」表示が出る。これは意図的な挙動変更（通常検索と同じフィードバック）
- **perf ラベル**: パス入力時のラベルが `"folder"` → `"query"` に変わる
- **`search.test.ts`**: `pathQuery` を直接テストしていないため影響なし

### 検証

1. `npm run typecheck`（`pathQuery.ts` への参照が残っていないこと）
2. `npm run build`
3. `npm test`

---

## Phase 3: ドキュメント更新

### 3.1 SPEC.md

#### §14.1 補足（563行目）

before:
```
- 先頭 `/` ではない入力で `/` または `\` を含む場合は、通常検索ではなくパス（フォルダ）検索として扱う
```

after:
```
- `/` や `\` を含む入力も通常検索として扱う（パスマッチングによりエントリの `target_path` への部分一致が機能する）
```

#### §3.1 検索方式（107行目の後に追加）

ローマ字検索の後に以下を追加:

```
- パスマッチング: クエリにパス区切り文字（`/` `\`）が含まれる場合、エントリ名に加えて `target_path`
  に対しても中間部分一致で検索する。`tool/editor` と入力すると `C:\tool\editor\app.exe` にヒットする。
  クエリ内の `/` は `\` に正規化してからマッチングする。
```

#### §3.3 検索結果の優先順位（`base_score` の説明を拡張）

`base_score` の箇条書きを以下に変更:

before:
```
- `base_score`: 選択中検索方式のマッチスコア
```

after:
```
- `base_score`: 選択中検索方式のマッチスコア。名前マッチ（Prefix/Substring/Fuzzy）・かなマッチ・パスマッチの順で試行し、最初にヒットしたスコアを使用する。パスマッチのベーススコアは `3000`（名前マッチ・かなマッチより低い）
```

#### §1 スコープ境界

変更なし（パスマッチングが通常検索に統合されるため退化なし）。

### 3.2 ui/CLAUDE.md

モジュール構成の `lib/` セクション（35行目付近）から以下を削除:

```
- `pathQuery.ts`: パスクエリ判定ロジック（`parsePathQuery`・`isPathQuery`）。入力がパス形式かを判定しフォルダ参照モードへの切り替えをトリガー
```

`search.ts` の説明（23行目付近）は `refreshResults()（ソースに応じた検索実行）` で、`pathQuery` に直接言及していないため変更不要。

### 3.3 snotra-core/CLAUDE.md

「モジュール構成」セクションの `search.rs` の説明末尾に追記:

```
パスマッチング: クエリにパス区切り文字（`\` `/`）を含む場合、`normalized_key`（= `normalize_entry_key(target_path)`）に対して Substring マッチを試みる。スコアは `3000 - min(byte_pos, 500)`。name/file_name/kana 全て不成立時のフォールバック。`has_path_sep` 時は Fuzzy ビットマスク pre-filter をスキップする
```

### 3.4 .claude/rules/snotra-core-search.md

スコア階層の記述を更新:

before:
```
- **スコア階層は不変**: Prefix(10000) > Substring(5000) > Kana(4500) > Fuzzy(nucleo)。テスト `kana_search_direct_match_ranks_above_kana_match` で検証
```

after:
```
- **スコア階層は不変**: Prefix(10000) > Substring(5000) > Kana(4500) > Path(3000) > Fuzzy(nucleo)。テスト `kana_search_direct_match_ranks_above_kana_match` で検証
```

incremental ガードのルールを追加:

```
- **`has_path_sep` 変更 → incremental ガードも見直す**: パスマッチ条件と `(!has_path_sep || self.prev_query.contains('\\') || self.prev_query.contains('/'))` は連動している
```

### 3.5 ルート CLAUDE.md

変更なし。

### 3.6 e2e/tauri.slash.e2e.ts（544行目）

before:
```
// C:\ を入力してフォルダ結果を表示（pathQuery モード、folderState は null）
```

after:
```
// C:\ を入力して通常検索のパスマッチングで結果を表示（folderState は null）
```

---

## 影響範囲まとめ

### 対称コードパス確認

- **パス検索**（`\` 含む入力）→ ~~`list_folder`~~ → 通常検索に統合（Phase 2 + 1）
- **フォルダ展開**（ArrowRight/ArrowLeft）→ `list_folder` IPC: **変更なし**
- **通常検索** → `search` IPC: パスマッチ追加（Phase 2）
- **スラッシュコマンド**（`/` 始まり）→ コマンドモード: **変更なし**
- **インスタントコマンド**（`@` 始まり）→ インスタントコマンドモード: **変更なし**
- **トレイ履歴メニュー** → `recent_history()`: **変更なし**

### リスク・注意点

- **パフォーマンス**: パス区切り含有クエリで Fuzzy ビットマスク pre-filter をスキップする。パス区切りを含む入力は稀であり、かつスキップしても `find()` 1回で O(n) なので影響は軽微。ベンチで確認する
- **インデックス外ディレクトリの探索廃止**: `C:\random\path\` 入力でのファイルシステム直接走査は廃止。フォルダ展開でカバーする
- **アクセント折り畳みの非対称性**: `normalize_entry_key` は折り畳みなし、`normalize_query` は折り畳みあり。パスにアクセント文字（`café` 等）を含むケースは極めて稀なため許容
