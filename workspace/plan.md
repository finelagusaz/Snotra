# plan — issue #338: `Config::load()` の TOML parse エラー黙殺を解消

## ゴール（受け入れ条件・テスト可能形）

不正な `config.toml`（TOML 構文エラー）を `Config::load()` が読んだとき:

1. **ログ可視化**: parse エラーが stderr に `[config] ...` で出力される（黙殺しない）。
2. **上書き回避**: フォールバック default を**元の `config.toml` に save() しない**（恒久喪失を防ぐ）。
3. **`.bak` 退避**: 不正な `config.toml` を `config.toml.bak` へ移動（rename）し、ユーザーが手動復旧できる状態にする。

不変（壊してはならない既存挙動）:

4. **正常 TOML**: parse 成功時は従来どおり `apply_migrations() → (changed なら) save()` を実行し、設定が反映される。
5. **first-run（ファイル不在）**: 従来どおり `default + save()` で `config.toml` を生成する。
6. **migration 正常系**: `apply_migrations()` の legacy 移行（`paths.additional → scan` 等）が壊れない。

スコープ外: issue item 4（ユーザー向け通知）。理由は research.md「チームの既決方針」参照（`snotra-core/CLAUDE.md` の「UI 文字列を持たない」原則と衝突、`size:S`、RETROSPECTIVE 方針に不在）。

通知については **follow-up issue #343（トレイバルーン通知）に分離**（ユーザー合意済み）。#338 では `load() -> Config` の戻り値を変更しない（退避フラグを surface する `load_reporting()` の seam は #343 で追加。本 issue では消費者が居ないため YAGNI）。#338 の parse 失敗 arm が行う `.bak` 退避ロジックを #343 がそのまま土台に使う。

## 設計

### `.bak` は「rename（移動）」を採用

issue の語「退避」＝ファイルを安全な場所へ**移動**する意。`fs::rename(config.toml, config.toml.bak)` を採用。

- 利点: 元ファイルは `.bak` に保全（item 2「default で上書きしない」を満たす）＋ 次回起動で config.toml 不在 → first-run → fresh default 生成で**自己回復**する。
- copy 案（元を残す）との比較: copy では毎起動 parse 失敗を繰り返し自己回復しない。rename は「退避」語義・自己回復の両面で優れる。
- rename 失敗時（`.bak` ロック等）: config.toml は**その場に残る**（= default で上書きされない）。エラーをログして default で続行。item 2 は rename 失敗時も保たれる。

### `load()` の再構成（`config.rs:871-890`）

read 失敗と parse 失敗を**明示的に分離**。parse 失敗 arm から `save()` を**構造的に排除**する（`unwrap_or_default()` を撤廃）:

```rust
pub fn load() -> Self {
    let Some(path) = Self::config_path() else {
        return Self::default();
    };

    match fs::read_to_string(&path) {
        Ok(content) => match toml::from_str::<Self>(&content) {
            Ok(mut config) => {
                // 正常系: 従来どおり migration → 変化があれば save
                if config.apply_migrations() {
                    let _ = config.save();
                }
                config
            }
            Err(e) => {
                // TOML parse 失敗（ユーザーの構文ミス・破損等）。
                // 黙ってデフォルトで上書きしない（snotra-core/CLAUDE.md:
                // deserialize_failed → save() はデータ喪失を招く）。
                // エラーを可視化し、不正ファイルを .bak へ退避してから
                // in-memory default で続行する（save() しない）。
                eprintln!("[config] failed to parse {}: {e}", path.display());
                Self::backup_invalid(&path);
                Self::default()
            }
        },
        Err(_) => {
            // first-run / ファイル不在: 従来どおり default を生成・保存
            let config = Self::default();
            let _ = config.save();
            config
        }
    }
}
```

`Err(e)` arm に `save()` 呼び出しが存在しないことが item 2 の構造的保証。`Self::default()`（migration 未適用）を返すのは first-run arm（同じく migration 未適用の default を返す）と一貫。

### 新規ヘルパー `backup_invalid`（テスト可能 seam）

```rust
/// Best-effort: 解析不能な config ファイルを `<path>.bak` へ退避（移動）し、
/// ユーザーが手動復旧できるようにする。結果をログする。panic しない。
/// 退避に失敗した場合は元ファイルをその場に残し（default で上書きしない）、
/// ログして default 続行する。
fn backup_invalid(path: &Path) {
    let bak = path.with_extension("toml.bak");
    match fs::rename(path, &bak) {
        Ok(()) => eprintln!(
            "[config] backed up unparseable config to {} (running on defaults; original NOT overwritten)",
            bak.display()
        ),
        Err(e) => eprintln!(
            "[config] failed to back up unparseable config at {}: {e} (running on defaults; original left in place)",
            path.display()
        ),
    }
}
```

`with_extension("toml.bak")`: `config.toml` → `config.toml.bak`（`save()` の `with_extension("toml.tmp")` と同形式）。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `snotra-core/src/config.rs` | `load()` を read/parse 失敗分離に再構成（`unwrap_or_default()` 撤廃）。`backup_invalid(&Path)` を追加。テスト追加。既存テスト `partial_toml_falls_back_to_default_via_unwrap_or_default` の stale コメント修正 |

他ファイルの変更なし（呼び出し元・`save()`・`apply_migrations()`・docs・SPEC.md・E2E は不変）。

### SPEC.md 更新要否

**不要**。SPEC.md は config の「読み込みに失敗したら default」までは規定するが（挙動の方向は不変）、本変更は「失敗時にログ＋退避し、上書きしない」という**内部の堅牢化**であり、IPC 契約・状態遷移・文書化フローを変えない。念のため実装時に SPEC.md を grep し、「parse 失敗で黙って default」と明記された箇所があれば（あれば）「ログ＋.bak 退避」に同期する。なければ更新不要。

## 実装順序（TDD）

1. **Red**: `config.rs` の `#[cfg(test)]` に temp-dir ヘルパー（`config.rs` 内ローカル `temp_dir(tag)`、`indexer.rs:855` 同形式）と新テストを追加し、まだ存在しない `backup_invalid` を呼ぶ → コンパイルエラー/失敗を確認。
2. **Green**: `backup_invalid` を実装、`load()` を再構成 → テストを通す。
3. 既存テストの stale コメント（`config.rs:2758-2761`）を「`load()` は match で parse 失敗を捕捉し `.bak` 退避後 default にフォールバックする」に修正（テスト本体＝serde 直叩きは valid なので維持）。
4. **検証**: 下記「変更後の検証」を実行。

## テスト方針

`snotra-core/src/config.rs` の `#[cfg(test)]` に追加（temp-dir ヘルパーは `config.rs` ローカルに `snotra_config_test_<tag>` 名で定義）:

- `backup_invalid_renames_to_bak_preserving_content`
  - 不正 TOML を temp の `config.toml` に書く → `Config::backup_invalid(&path)` → アサート: (a) 元 `config.toml` が存在しない（退避済み＝default で上書き不能）、(b) `config.toml.bak` が**元の不正内容を保全**している。
- `backup_invalid_overwrites_existing_bak`
  - 既存 `.bak`（古い内容）を置く → 新しい不正 `config.toml` を書く → `backup_invalid` → アサート: `.bak` が**新しい内容**で上書きされる（単一 `.bak`・KISS の文書化）。
- （任意・余裕があれば）`backup_invalid_missing_source_is_noop_no_panic`
  - 存在しない path に対し `backup_invalid` を呼んでも panic せず、`.bak` も作られない（rename Err 経路の安全性）。

既存テストとの関係:

- `invalid_toml_falls_back_to_default`（2729）/ `partial_toml_falls_back_to_default_via_unwrap_or_default`（2758）: `toml::from_str(...).unwrap_or_default()` を直叩きする **serde レベルの fallback テスト**。parse 失敗 → in-memory default 値という不変は維持されるため**テスト本体は維持**。後者のコメントのみ `load()` 実装の現状（match + `.bak` 退避）に合わせて修正。

### 検証の限界（明示）

`config_path()` が env 差し替え不可のため、`Config::load()` 全体（read→parse 失敗→`.bak`→no-save の統合経路）は自動単体テストできない。担保は次の3点:

- **(b) 上書き回避**: `load()` の `Err(e)` arm に `save()` 呼び出しが存在しない**構造的保証**（コードレビューで確認） + `backup_invalid` テストで「元ファイルが退避され存在しない」ことを保証。
- **(a) ログ**: `eprintln!` 出力の自動アサートは行わない（既存 855-864 の eprintln 同様）。手動 smoke で確認。
- **手動 smoke**: 実装後、`%APPDATA%\Snotra\config.toml` に不正 TOML（例: クォート閉じ忘れ）を書いて `snotra.exe` を起動 → stderr に `[config] failed to parse ...` が出る／`config.toml.bak` が生成され元の不正内容を保つ／`config.toml` が default 内容で上書きされていない（rename 済みなので不在 → 次回起動で fresh 生成）ことを目視確認。手順を実装報告に記載。

将来 `config_path()` に test 用 override を入れれば統合経路も自動化可能だが、`save()` 改修を伴いスコープ拡大するため本 issue（size:S）では見送り。

## 変更後の検証（`docs/build-commands.md` カテゴリ参照）

- カテゴリ A（Rust ロジック変更）: `cargo test -p snotra-core`（新規・既存テスト）、`cargo clippy -p snotra-core`、`cargo fmt`。
- カテゴリ（起動 smoke）: `scripts/smoke-startup.ps1`（正常 config で `*:error` 不在を確認、本変更が正常起動を壊さないことを担保）。加えて上記「手動 smoke」を実施。
- 実装時に SSOT（`docs/build-commands.md`）の該当コマンド文字列を確認して実行する。

## 不変条件

- **データ保全**: parse 失敗時にユーザーの設定内容が（`.bak` として）必ず保全される。`rename` 失敗時も元ファイルはその場に残り、default で上書きされない。「壊れたら即アウト」＝「ユーザーの config が default で恒久上書きされる」を構造的に排除する（`Err` arm に `save()` なし）。
- **正常系不変**: parse 成功時の `apply_migrations() → save()` 経路、first-run の `default + save` 経路は一切変更しない。
- **層の責務**: 変更は `snotra-core` 内に閉じ、`fs` のみ使用。UI 文字列・IPC・Win32 に触れない（CLAUDE.md 原則順守）。
- **命名衝突なし**: `config.toml.bak` は既存コード・E2E のいずれにも参照されない新名称。

## セルフレビュー

`/plan-review`・`/symmetric-check` は `disable-model-invocation` 設定でモデルからは自動起動不可（ユーザーが手動で `/plan-review` 等を打つ前提）。代替として、これら check が担う検証のうち最重要の「同一パターン全コードパス検索」をモデルが直接実施した。完全な並列サブエージェント検証が必要ならユーザーが `/plan-review` を実行可能。

1. **対称コードパス（load/save ペア）**: 変更は `load()` のみ。`save()`（write 経路）は不変で、退避は `rename`（move）のみ。`load()` 内の3分岐（read 失敗＝first-run / parse 失敗 / parse 成功）すべてに挙動を割り当て済み。`reset_to_default()`（CLAUDE.md:145）は `load()` を経由しないため非影響。対をなす変更は不要。
2. **影響範囲網羅（同一パターン横断検索の結果）**: `unwrap_or_default()` / load 系関数を全 grep。
   - **config が唯一の急性ケース**: `load() → unwrap_or_default() → apply_migrations() → 即 save()`。「load 直後の save」が config 固有の危険。
   - `history.rs::load`（37-50）: `load_with_fallback`（CLAUDE.md:66 推奨の正しいパターン）使用。**load では save しない**（dirty 時のみ save）→ 同一バグでない・緩和済み。
   - `window_data.rs`（33,43）: `unwrap_or_default()` は **save 経路の read-modify-write**。`load_state_v5` の save(64) は V4→V5 移行成功時のみで parse 失敗は `?` で抜ける → 同一バグでない。
   - → **フォローアップ issue 不要**。本 issue のスコープ（config のみ）が正しい。
3. **境界条件**: (a) rename 失敗（`.bak` ロック等）→ 元ファイルその場に残り default 上書きされない・ログのみ。(b) `.bak` 既存 → `fs::rename` が上書き（Windows MOVEFILE_REPLACE_EXISTING）= 単一 `.bak`。(c) ソース不在で `backup_invalid` 呼び出し → rename Err・panic なし・`.bak` 不作成。(d) first-run（read 失敗）は不変。各々テスト or 構造で担保。
4. **リソース管理**: `.bak` は `rename`（移動）でファイルハンドルを保持しない → リーク無し。退避後 config.toml は不在 → 次回起動で first-run が default を再生成（自己回復）。生成/破棄ペアの非対称リソースは導入しない。
5. **既存パターン整合**: ログ＝`eprintln!("[config] ...")`（855-864 前例）、退避＝`with_extension` + `fs::rename`（save 901-903 と同形式）、test＝ローカル `temp_dir(tag)`（indexer 855 同形式）。新規パターン導入なし。
6. **YAGNI**: フロントエンド通知（item 4）除外。`.bak` はタイムスタンプ無しの単一ファイル。`config_path()` の test override（save 改修を伴う）も導入しない。要求範囲を超える追加なし。
7. **シンプル化の挑戦**: 新たな `AtomicBool`・Mutex・子プロセス・汎用 IF を一切導入しない。parse 失敗 arm は `Self::default()` を返すのみで `save()` 呼び出しを**構造的に持たない**（「この操作が失敗したら」＝rename 失敗時も元ファイル保全・default 続行と明記済み）。copy 案を検討し rename を採用（「退避」語義＋自己回復）。
8. **破壊不変条件の明示**: 「壊れたら即アウト」＝**ユーザーの config が default 内容で恒久上書きされる**。検知手段: (i) `load()` の parse 失敗 arm に `save()` が無いことのコードレビュー（構造的保証）、(ii) `backup_invalid` テストで「元ファイルが退避され存在しない」検証、(iii) 手動 smoke（不正 config 投入 → ログ出力／`.bak` 生成・内容保全／config.toml が default で上書きされていない、を目視）。3 点を実装報告に含める。

### plan-review 結果の反映（Explore サブエージェント × 3: 影響範囲 / 不変条件 / スコープ）

3 観点すべてで **要対処ゼロ**。計画の completeness 高・実装着手可。以下は検証で得た補強・実装時の注意:

- **config_watcher は無限ループしない（確証）**: `src-tauri/src/config_watcher.rs` のウォッチャは CREATE/MODIFY イベントに反応し DELETE は無視する。`backup_invalid` の `rename`（config.toml を `.bak` へ移動＝config.toml は DELETE 扱い）はウォッチャを誤発火させず、再 `load()` ループを誘発しない。research.md の「起動時 load は watcher セットアップ前」に加え、たとえ起動後に発生してもループしないことが裏付けられた。
- **parse 失敗時 default は None sentinel のまま（first-run と一貫）**: 返す `Self::default()` は `search.top_n_history` / `max_history_display` が `None`。実行時は `effective_*()`（config.rs:359,364）の `unwrap_or_else` が処理するため正常。**将来この値を `.unwrap()` で直叩きすると panic** する点に注意（first-run arm も同じ None 状態なので新規リスクではない）。
- **item 4 除外の根拠を特定**: `.claude/rules/snotra-core.md:16`「UI 表示文字列を持たない: エラーは `is_error: true` フラグで伝え、表示は UI 層の責務」。フロントエンド通知の除外はこの明文ルールと整合。
- **既存テストコメント修正は必須（plan Step 3 に既出）**: `config.rs:2758-2761` の「`Config::load()` uses unwrap_or_default()」コメントは実装後に実態（match + `.bak` 退避）と齟齬する。テスト本体（serde 直叩き）は維持し、コメントのみ修正。
- **手動 smoke を実装報告の必須フィールドに**: `config_path()` が env 差し替え不可で `load()` 統合経路を自動テストできない以上、手動 smoke の「実施結果」を報告に明記する（スキップ不可）。
- **SPEC.md 同期確認**: 実装時に SPEC.md（リポジトリ root）を grep。存在し「parse 失敗で黙って default」と明記があれば同期、なければ更新不要（plan「SPEC.md 更新要否」に既述）。
