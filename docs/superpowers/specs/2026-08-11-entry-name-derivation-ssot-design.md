# エントリ名導出規則を単一関数へ集約する設計（#997）

`indexer` が `AppEntry.name` を作る規則を crate 内の単一関数へ集約し、通常スキャン、PATH スキャン、`index_tree` のテスト fixture が同じ実装を通るようにする。

この変更は既存規則に合わせる挙動不変のリファクタである。`SPEC.md` に規則そのものの記載はなく、ファイル名・フォルダ名・非 UTF-8・空名の扱いも変えない。

## 1. 解消する問題

現行の production は次の場所でエントリ名をインライン導出している。

- 通常スキャンの folder: `Path::file_name()`
- 通常スキャンの file: `Path::file_stem()`
- PATH スキャンの file: `Path::file_stem()`

PR #995 で `index_tree.rs` の `tree_with` が同じ条件分岐を fixture 内へ書き写し、4 枚目になった。導入時には `.env` のような先頭ドット名を自作の文字列分解で扱い、`Path::file_stem()` と食い違う木を建てた実績がある。fixture 内の assertion でずれを検出する形は、規則そのものの写しを残すため根本解決にならない。

## 2. 単一関数の契約

`snotra-core/src/indexer.rs` の `AppEntry` 付近に次の関数を置く。

```rust
pub(crate) fn entry_name_from_path(path: &Path, is_folder: bool) -> Option<&str>
```

関数の契約は次のとおりである。

- `is_folder == true` なら `path.file_name()` の UTF-8 表現を返す
- `is_folder == false` なら `path.file_stem()` の UTF-8 表現を返す
- 対象成分がない、または UTF-8 として表せない場合は `None` を返す
- 空文字を拒否しない
- `String` を確保しない
- fallback や panic を持たない

具体的な導出規則の実装上の正本は、この関数とその doc comment とする。`pub(crate)` に留め、crate 外の公開 API は増やさない。

## 3. 呼び出し側の責務

各呼び出し側は名前の導出だけを共通関数へ委ね、`None`・空文字・所有権の扱いは現在の責務を保つ。

| 呼び出し側 | 引数 | `None` / 空文字 | 所有化 |
|---|---|---|---|
| 通常スキャンの folder | `true` | その folder 自身を追加しない。再帰走査は続ける | `String` へ変換 |
| 通常スキャンの file | `false` | エントリを追加しない | `String` へ変換 |
| PATH スキャン | `false` | 候補を追加しない | `String` へ変換 |
| `index_tree.rs::tree_with` | fixture の `is_folder` | `None` は panic、空文字は assertion failure | `String` へ変換 |

production は現在の `.unwrap_or("")` と空名ガードを維持する。fixture は不正な入力を黙って捨てず、テストの組み立て失敗として可視化する。共通関数が返すのは借用 `&str` なので、共通化そのものによる追加確保はない。

## 4. 検知器

共通関数を production と fixture が共有すると、両者が同じ誤りへ同時に動く可能性がある。そのため、規則そのものは共通関数の単体テストで独立に固定する。

| 入力 | `is_folder` | 期待値 | 固定する境界 |
|---|---:|---|---|
| `tool.exe` | `true` | `Some("tool.exe")` | folder はドットを拡張子として剥がさない |
| `tool.exe` | `false` | `Some("tool")` | file は拡張子を剥がす |
| `archive.tar.gz` | `false` | `Some("archive.tar")` | 最後の拡張子だけを剥がす |
| `.env` | `false` | `Some(".env")` | 先頭ドット名を自作分解しない |
| 空パス | folder / file | `None` | 対象成分がない場合 |

テストは関数を実装する前に追加し、対象関数が存在しない Red を確認する。Green 後に一時的に「常に `file_name()`」「常に `file_stem()`」の 2 変異を入れ、それぞれ file 側・folder 側の期待値で落ちることを確認してから元へ戻す。

呼び出し点の結線は既存テストで確認する。

- 通常スキャンの folder / file 名
- PATH スキャンの file 名
- `IndexTree` のパス再構築と `file_key_into` の root / non-root 両腕

`tree_with` にある「写しがずれていないこと」を確かめる専用 assertion は、共通関数の単体テストへ責務を移して削除する。fixture 固有の `None` と空文字の検査は残す。

## 5. 文書の同期

`entry_name_from_path` の doc comment を具体的な導出規則の正本とし、既存の散文は正本への参照へ変える。

| 場所 | 変更 |
|---|---|
| `snotra-core/CLAUDE.md`「エントリ名の導出ルール」 | 規則を再掲せず、`indexer::entry_name_from_path` が単一定義であることと、スキャン・fixture が迂回しないことを書く。`folder.rs` の別規則は意図的な差として残す |
| `snotra-core/src/index_tree.rs` の module doc / `resolve_one` / fixture doc | `file_name()` / `file_stem()` の再掲と「SSOT の関数がない」という記述を、共通関数への参照へ置き換える。木表現に必要な帰結は残す |
| `snotra-core/src/query.rs` の `measure_derived_sharing` doc | 規則の再掲を共通関数への参照へ置き換え、`is_folder` から推論してはならない理由は残す |

`SPEC.md` は挙動を変更しないため触らない。`folder.rs` はフォルダ内列挙で拡張子付きの名前を使う別概念であり、共通関数へ集約しない。

## 6. 完了条件と検証

`rg` で Rust ソースと関連文書の `file_name()` / `file_stem()`、および「名前導出規則」の記述を再列挙する。production の対象 3 箇所と `tree_with` が `entry_name_from_path` を通り、旧インライン導出と規則の散文コピーが残っていないことを確認する。

本設計書を除く実装差分は `snotra-core/src/indexer.rs`、`snotra-core/src/index_tree.rs`、`snotra-core/src/query.rs`、`snotra-core/CLAUDE.md` に限定する。変更後は `docs/build-commands.md` のカテゴリ A と、ガバナンス文書変更時の検査を実行する。

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
```

## 7. 却下した案

### `index_tree.rs` に置く

木の表現は規則に依存するが、PATH 候補の名前も同じ規則を使う。木を作る前の名前導出まで `index_tree` の責務へ含めると、モジュール境界が実際の所有者より広くなる。

### 共通化せず fixture の検査だけ強化する

抽象化は増えないが、実際にずれた 4 枚目の写しが残る。fixture の assertion はずれの一部を検出できても、導出規則を共有しない構造を解消しない。

### `query.rs` に置く

`query::lower_file_name` は小文字化と accent-folding を含む検索用の派生であり、原文の `AppEntry.name` を返せない。原文名の導出を同じモジュールへ置くと、検索正規化と indexer のエントリ生成という別責務を混ぜる。

## 8. 触らないもの

- エントリ名の既存規則
- 非 UTF-8・空名の除外方針
- `AppEntry` の型と公開フィールド
- `IndexTree` のオンディスク形式とパス再構築規則
- `query::lower_file_name`
- `folder.rs` のフォルダ内列挙
- `SPEC.md`
