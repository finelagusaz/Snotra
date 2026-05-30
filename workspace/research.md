# research — issue #338: `Config::load()` の TOML parse エラー黙殺

## issue の要約

`snotra-core/src/config.rs:878` の `Config::load()` は

```rust
let mut config: Self = toml::from_str(&content).unwrap_or_default();
```

で TOML/serde の parse 失敗を黙殺し、無言で `Config::default()` にフォールバックする。

- **データ損失リスク**: ユーザーが `config.toml` を手編集してタイプミスをすると、全設定（スキャンパス・ホットキー・テーマ・オープナー・インスタントコマンド等）が無言で初期値に戻る。通知もログも無い。
- さらにフォールバック後 `apply_migrations()` が `true` を返すと `config.save()` が走り、不正な `config.toml` を default 内容で上書きして恒久喪失しうる。
- **デバッグ困難**: 何のエラーも出ないため原因特定が困難。

#332（E2E 全滅）の調査中に発見。E2E が生成した不正 TOML が parse 失敗 → この `unwrap_or_default()` が default にフォールバック → fixture 未インデックスで検索テスト 11 件全滅。E2E ハーネスは修正済みだが、根本の「黙殺」はアプリ側に残存。

## 該当コード（現状）

`snotra-core/src/config.rs:871-890`:

```rust
pub fn load() -> Self {
    let Some(path) = Self::config_path() else {
        return Self::default();
    };
    match fs::read_to_string(&path) {
        Ok(content) => {
            let mut config: Self = toml::from_str(&content).unwrap_or_default();  // ← 黙殺
            if config.apply_migrations() {
                let _ = config.save();   // ← 不正ファイルを default で上書きしうる
            }
            config
        }
        Err(_) => {                      // ファイル不在（first-run）
            let config = Self::default();
            let _ = config.save();
            config
        }
    }
}
```

現状の `load()` は **read 失敗（ファイル不在）と parse 失敗を区別していない**。read OK のときは parse 成否を問わず必ず `apply_migrations() → (changed なら) save()` 経路に入る。

## 関連コード・呼び出し経路

- `snotra-core/src/config.rs`
  - `load()` (871-890): 本体。**変更対象**
  - `save()` (892-907): atomic write（`.toml.tmp` → rename）。`config_path()` を内部で解決。**変更しない**
  - `apply_migrations()` (818-869): legacy 移行・正規化・システムショートカットフォールバック。`changed` を返す。default に対しては `false` を返す（後述）。**変更しない**
  - `config_path()` (779-781) / `config_dir()` (775-777): `dirs::config_dir().join("Snotra")` 固定。**env 差し替え不可**
  - 既存 eprintln 前例: 855-864（`[config] system shortcut detected ...`）
- `Config::load()` 呼び出し元（挙動を引き継ぐ側、変更不要）:
  - `src-tauri/src/main.rs:365`（起動時）
  - `src-tauri/src/config_watcher.rs:79, 247-289`（ファイル監視・ホットリロード）
  - `snotra-settings/src/main.rs:20`（設定 GUI 別プロセス）
  - `snotra-settings/src/tabs/backup.rs:328`（同じ migration を適用するコメントあり）

## 既存パターン（再利用できるもの）

- **ログ**: `snotra-core` は Win32 非依存・UI 文字列を持たない層。ログは `eprintln!("[config] ...")` が作法（855-864 に前例）。`src-tauri` の `trace_main`（`*:error` JSON を吐く）は別 crate の資産でここでは使えない。
- **smoke 判定との非干渉**: `scripts/smoke-startup.ps1` は stderr の `[trace] {json}` 行のうち `event` が `*:error` のものだけを失敗判定する（54-67行）。平文 `[config] ...` eprintln は JSON でないため smoke を壊さない。正常起動時は config が valid なのでそもそも発火しない。
- **atomic な file 操作**: `save()` は `path.with_extension("toml.tmp")` → `fs::rename` を使う。`.bak` 退避も同じ `path.with_extension("toml.bak")` + `fs::rename` で書ける（命名規約・API ともに既存と一致）。
- **temp-dir テスト**: `indexer.rs:855` / `history.rs:285` / `binfmt.rs:210` に `temp_dir(tag)` ヘルパー（`std::env::temp_dir().join(...)` → 既存削除 → 作成、末尾で `remove_dir_all`）。同形式で `config.rs` のテストを書ける。
- **`Path` import 済み**: `config.rs:3` に `use std::path::{Path, PathBuf};`。新規 import 不要。

## チームの既決方針（重要）

`RETROSPECTIVE.md:37` にこのチーム自身が改修方針を記録済み:

> #338（`Config::load()` の parse 失敗黙殺）の改修方針を判断（**ログ可視化 + 上書き回避 + `.bak` 退避**）

→ issue 提案の item 1（ログ）・2（上書き回避）・3（`.bak` 退避）を採用。**item 4（フロントエンド通知）は方針に含まれず**、かつ `snotra-core/CLAUDE.md` の「UI 表示文字列を持たない／エラーは `is_error` フラグで伝える」原則と衝突するため **対象外**。issue ラベル `size:S` とも整合。

`snotra-core/CLAUDE.md:66`:

> `deserialize_failed → save()` パターン（デコード失敗時に空データを即時上書き保存）は ... データ喪失を招く。

→ 本 issue はこの原則の Config 版違反そのもの。TOML の parse 失敗は「format バージョン不一致」ではなく「ユーザーの構文ミス」なので「旧形式フォールバックデシリアライザ」は不要。適用すべきは「default で即上書きしない」部分。

## `apply_migrations()` が default に対して `false` を返す確認

parse 失敗時に `Self::default()` を返したとき、誤って save() 経路に入らないことの裏付け（ただし本実装では parse 失敗 arm から save() を構造的に排除するため、この性質には依存しない）:

- `paths.additional` 空 → no-op
- `appearance.top_n_history` / `max_history_display`（legacy）→ default では `None` → `take()` が None → 変化なし
- `search.top_n_history` / `max_history_display` → `get_or_insert_with` で Some 化するが `changed` は立てない（843-844 は `let _ =`）
- `sanitize` / `normalize_scan_paths` / `normalize_openers` → default は正規化済み → no-op
- `is_system_shortcut("Alt","Q")` → default ホットキーは system shortcut でない → no-op

→ default に対し `apply_migrations()` は `false`。なお **first-run（read 失敗）arm は元々 `apply_migrations()` を呼ばず** `default + save` するだけなので、parse 失敗時に bare `Self::default()`（migration 未適用・None sentinel のまま）を返すのは first-run の挙動と一貫する。

## 技術的制約

- **`config_path()` は env 差し替え不可**: `Config::load()` 全体を実 `%APPDATA%` 非依存で単体テストできない。テスト可能な seam（`backup_invalid(path: &Path)`）を切り出して検証する。`load()` の parse 失敗 arm に save() を書かないことは「構造的保証」＋手動 smoke で担保する。
- **`std::fs::rename` の上書き挙動（Windows）**: Rust の `fs::rename` は Windows で `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` を使い、既存の宛先ファイルを上書きする。よって `.bak` は単一・最新の不正ファイルで上書きされる（KISS）。
- **`config_watcher` との相互作用**: 起動時 `load()`（main.rs:365）は config_watcher セットアップより前に走る。parse 失敗で config.toml を `.bak` へ rename した時点では watcher 未起動 → イベント発火なし。次回起動時に config.toml 不在 → first-run → default + save で自己回復。
- **Win32 非依存を維持**: 変更は `snotra-core` 内で完結、`fs` のみ使用。ユニットテスト可能。

## E2E への影響

- #332 修正後、`e2e/tauri.slash.e2e.ts` は `buildE2EConfigToml(fixtureDir)` で**正しい TOML を生成**しており、黙殺フォールバックに依存していない。`config.toml` ホットリロードテスト（661行）も valid TOML を書く。本変更は valid-TOML 経路を変えないため E2E 影響なし。
- 既存コードに `.toml.bak` 参照は皆無 → 新名称の衝突なし。

## 未解決の疑問

- なし（設計判断はコードと記録から一意に解決）。`.bak` 退避を「rename（移動）」とするか「copy（複製）」とするかは plan で決定（issue の語「退避」＝移動を採用予定）。
