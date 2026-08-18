# 調査: #1124 `get_instant_commands` を呼び出し元へ寄せ、撤去済み IPC 境界の形を解く

## 1. issue の要約

`src-tauri/src/commands/instant.rs` の `get_instant_commands` は、#532 SU7 のフロント撤去で相手が消えた **IPC 境界の形をそのまま残している**。

- 無謬の `Result`（本体は無条件 `Ok(...)`・ファイルに `Err(` が 1 つも無い）
- IPC シリアライズのための DTO（`InstantCommandDto`）経由
- `AppHandle` を値で受け取る（呼び出し元は毎打鍵 `clone()`）

この形が、`commands/` に在りながら egui フレームの中で毎打鍵走るという異常を作り、#1076 はその説明に条項と ADR の散文をかなり費やした。**呼び出し元へ寄せれば異常の説明そのものが要らなくなる**、というのが本 issue の主眼である。

`&[InstantCommand] -> Vec<SearchResult>` の純関数を置き、呼び出し元が `read_config` でそれを呼ぶ形にする。現状の 2 段変換（DTO 化 → `SearchResult` 化）が 1 段になる。

**「速くなる」を目的に書かないこと**（issue の明示的な禁止）。費用の額は未測定であり、これは確保の形の整理である。

## 2. 関連ファイル・シンボル（LSP findReferences で実測・2026-08-18）

| シンボル | 定義 | 参照 |
|---|---|---|
| `get_instant_commands` | `src-tauri/src/commands/instant.rs:41` | **呼び出し元 1 件のみ** — `src-tauri/src/egui_shell/launcher_controller.rs:914`（LSP は定義含め 2 件を返す） |
| `InstantCommandDto` | `src-tauri/src/commands/launch.rs:228` | **11 件・2 ファイルのみ** — `instant.rs`（14 / 17 / 45）と `launch.rs`（定義・`From` impl・テスト 3 件）。**そもそも他 crate からリンクできない**（下記） |
| `matching_dtos`（private） | `src-tauri/src/commands/instant.rs:11` | 3 件・`instant.rs` 内のみ（定義 + `get_instant_commands` 内 2 呼び出し） |

grep（`--include=*.rs` リポジトリ全体）も同じ母集団を返した。**LSP と grep が一致している。**

**`src-tauri` は bin crate である**（`src-tauri/Cargo.toml` に `[lib]` 節が無い・実測）。`snotra-settings` は `snotra-core` にのみ依存し、`snotra-egui-runtime` は `snotra` に依存しない（両 `Cargo.toml` 実測）。ゆえに `InstantCommandDto` は **「他 crate で使われていない」のではなく「他 crate からリンクしようがない」**。`commands/mod.rs` の `pub use launch::*;` による再公開も crate 内に閉じる。（敵対的調査で得た、本節より一段強い保証。）

### 現状の 2 段変換（実測）

1. `InstantCommandDto::from(&InstantCommand)`（`launch.rs:234-251`）が候補ごとに `name` / `description` / `display` の 3 つの `String` を確保する
2. `launcher_controller.rs:914-931` の Instant 枝が DTO → `SearchResult` へ組み直し、**`description` が非空なら `display` を捨てる**

`display` の導出（`launch.rs:236-246`）:

- `InstantAction::Url { url }` → `url.clone()`
- `InstantAction::Exec { exe, args }` → `args` 空なら `exe.clone()`、非空なら `format!("{exe} {args}")`
- `InstantAction::Legacy { command }` → `command.clone()`

### 変換に関わるその他

- `filter_instant_commands`（`snotra-core/src/instant.rs:310-322`）: `&'a [InstantCommand] -> Vec<&'a InstantCommand>`。空入力は全件、非空は `name` の小文字前方一致
- `SearchResult`（`snotra-core/src/ui_types.rs:16-21`）: `{ name, path, is_folder, is_error }`。同 crate の `instant.rs` から素直に使える
- `egui_shell::read_config`（`src-tauri/src/egui_shell/mod.rs:427-437`）: `(&AppHandle, read, fallback) -> T`。`AppState` 不在の面倒だけを見て `AppState::read_config`（`state.rs:105`）へ委譲する
- `launch.rs:1` の import `use snotra_core::config::{InstantAction, InstantCommand, find_matching_tools};` — **`InstantAction` / `InstantCommand` は DTO の `From` impl とそのテストでしか使われていない**（実測: 234 / 235 / 237 / 238 / 245 と `mod tests`）。DTO 撤去で未使用になる

## 3. 再利用できる既存パターン

**`read_visual` / `visual::visual_snapshot`（`egui_shell/mod.rs:445-` と `visual.rs`）がまさに同型の先例である。**

- 読みと `AppState` 不在の面倒は `read_config` 側が見る
- 導出は**純関数**が持ち、読みの中で呼ばれ、owned な値を返す
- 「読みの中で行うのは hex parse と算術と `&str` 比較まで。I/O や重い確保を足さない」というコメントが読みの契約を現地で宣言している

本 issue が作る形はこれと同じ骨格になる: 純関数を `snotra-core` に置き、`launcher_controller` が `read_config` の read クロージャからそれを呼ぶ。

## 4. 技術的制約

- **所有化は読みの中で終える必要がある**（`matching_dtos` の doc が正本）。`filter_instant_commands` は config を借りた参照を返すので、read guard の外へは出せない。**新関数が owned な `Vec<SearchResult>` を返す形は、この不変条件を構造的に再確立する**
- **read クロージャの中で錠も I/O も取らない**（`AppState::read_config` の doc が正本・crate 全体で read guard を取る唯一の地点）。新関数は `String` 確保までしか行わない
- **`-D warnings` 下では新 API の導入と呼び出し点の移行と旧 API の削除を 1 タスクに束ねる**（AGENTS.md 当該行）。未使用の新 API は `dead_code` で落ちる
- **`commands/mod.rs` は `pub use launch::*;`** ゆえ `InstantCommandDto` は crate 公開面に載っている。bin crate なので外部消費者は無い

## 5. `AppState` 不在時の fallback（唯一の実質的な挙動判断）

現状（`instant.rs:47-52`）は `Config::default().instant_commands` を建てて絞り込む。同関数の doc 自身がこれを異常として扱っている:

> **この fallback だけが `Config` 全体を建てる** …… `Config::default()` は走査パスの `exists()` と OS ロケールの読みを伴う。**到達しない経路ゆえ承知で払っている** …… **他の fallback へこの形を写さないこと**

- 到達しない根拠: `try_state::<AppState>()` は実運用で `None` を返さない。**これは既存の判断であり、本 issue が新たに主張するものではない**——正本は `AppState::read_config` / `egui_shell::read_config` の doc と `ADR-config-default-fallback-references`「後日の決定（#824 の 1 と 2）」（同じ根拠で 2 件の fallback を裁定済み）
  - **既存 doc が使う「`.manage` は `.setup` より前に走る」という言い回しは、逐語的には粗い**（敵対的調査の指摘・機序を tauri のソースで検算した結果）。両者は逐次実行文ではなく Builder への設定蓄積とコールバック登録であり、実際の機序は「`Builder::build()` が state を `AppManager` へ焼き込むのが、プラグイン初期化と `setup` 呼び出しより構造的に先」——`AppHandle` を持てるのは `build()` 完了後の世界だけなので結論は保たれる。**ただし敵対枠が読んだのは tauri 2.11.4 で、`Cargo.lock` の実バージョンは 2.11.5 である**（自分で実測）。パッチ差分は未検算ゆえ、**本 issue の成果物にこの機序を書き写さない**——判断は既存 doc へ委譲する
- 先例の向き: 同 ADR は `Language::Ja` 固定を「到達したときに誤る分岐」として直しており、**到達しない fallback でも意味の正しい側へ倒す**という判断を採っている
- #1123 の教訓（`egui_shell::read_config` の doc）: 「`&AppState` を既に持っているなら直接呼ぶ——通せば**到達しない `fallback` を書かされる**」

→ 呼び出し元へ寄せる際、fallback を `Vec::new` にすれば `Config::default()` の I/O とその弁明の doc が丸ごと消える。**これは到達しない経路の挙動変更であり、plan に挙動差分として明記する。**

意味の裏取り: `AppState` 不在 = config 未ロードの時点であり、そこで**既定の instant コマンド（`g` / `gh`）を返すのは「たまたま既定と一致するユーザーにだけ正しい」**。空を返すのは「まだ設定を知らない」の素直な表現で、`SPEC.md` §19.5 の「マッチするコマンドが 0 件の場合は結果を空にする」とも矛盾しない。

## 6. 散文の同期範囲（grep 実測）

| 参照先 | 扱い |
|---|---|
| `SPEC.md:904`（§19.2）「バックエンドが **DTO 生成時に**常に算出する派生値」 | **同期が要る。** DTO が消え、かつ `description` 非空なら `display` を算出しなくなるので「常に算出」も偽になる。§19.5 の表示規則そのもの（description 優先・display の中身）は不変 |
| `docs/adr/ADR-config-read-exception-discriminator.md:7,20` / `ADR-config-read-without-exception.md:19` | **触らない**（歴史記録・当時の判断として正しい） |
| `docs/superpowers/plans/*` / `specs/*`（7 ファイル） | **触らない**（過去サイクルの計画・設計の記録） |
| `src-tauri/CLAUDE.md:33`（`commands/` のファイル索引） | `instant.rs` は残るので**索引の変更なし**。同行の `launch.rs` の公開面の説明は `InstantCommandDto` を名指ししていない（実測）ため変更不要 |
| `snotra-core/CLAUDE.md:62`（`instant.rs` の索引行） | 責務散文は `//!` を正本とする方針。新関数を足すなら `//!` の公開関数列挙へ追記（索引行はファイル名なので変更不要） |

`SPEC.md` を触るので `npm run governance:check`（カテゴリ F）が要る。

## 7. 実運用点の確認（`C:/Users/Eoh/AppData/Roaming/Snotra/config.toml`）

```toml
[[instant_commands]]
name = "g"
description = "Google検索"
url = "https://www.google.com/search?q={query}"

[[instant_commands]]
name = "gh"
description = ""
url = "https://github.com/search?q={query}"
```

**description 有 / 無の両形が実在する**（`display` を捨てる枝と使う枝の両方が毎打鍵走っている）。既存テスト 3 件（`launch.rs:315-352`）は Url / Exec+args / Exec-args 無し の display 導出を覆うが、**description 優先の分岐は 1 件もテストされていない**（現状その判定は `launcher_controller` 側に在り、テストが無い）。

実 config は **`url` 種別 2 件のみ**で、`exec` 種別と `Legacy` 種別は 0 件である。**この 2 分岐はテストでしか走っていない。**

### `InstantAction::Legacy` の到達性（自分で裁定・実測）

`Legacy` を `Url` へ変換するのは `migrate_instant_legacy_commands`（`config.rs:908-918`）で、`apply_migrations()` の一部である。実運用で `Legacy` が生き残る経路は**見つからなかった**:

- `Config::load()` の正常系（`load_from_dir_reporting`・`config.rs:975`）は必ず `apply_migrations()` を通す
- parse 失敗 / first-run / read 失敗はいずれも `Config::default()` へ落ち、既定は `Url` 種別 2 件（`config.rs:625,632`）
- 設定 GUI の保存は `EditKind::Url` / `Exec` しか組まない（`snotra-settings/src/tabs/instant.rs:325`）
- backup import も `apply_migrations()` を呼ぶ（`snotra-settings/src/tabs/backup.rs:273`）

**「構造的に存在しえない」という全称の形では書かない**——上の 4 経路を数え上げた下限主張として扱う。`commands/instant.rs:78` のコメント「load 後は移行済みで到達しないが、防御的に Url 扱い」が実装側の同じ判断である。ゆえに新関数の `Legacy` 枝は **`match` の網羅性のために要る防御的分岐**であり、テストは書くが「実運用では到達しない」と明記する。

## 8. display 導出の 3 つ目の写し（`snotra-settings`）

`snotra-settings/src/tabs/instant.rs:110-131` に**同一の display 導出**がある（`suspect_legacy` 判定と 1 つの `match` に融合している）。

- 表示規則は**違う**: settings は `description` と `display` を**両方**出す（`style::hint` を 2 回）。ランチャは description 優先で片方だけ
- 導出そのもの（Url → url / Exec → `exe args` / Legacy → command）は**同一である**
- 寄せるには `snotra-core` 側の display 導出を `pub` にする必要がある。`docs/development-principles.md`「config の値は到達性の検出器を持たない」が言うとおり **lib crate の `pub` 項目に `dead_code` は出ない**ため、公開面を増やすことは到達性の検出器を失うことでもある

→ **本 issue の射程内で寄せるかは plan の未確定欄で裁定する。** issue が射程としているのは `get_instant_commands` の解体であり、別 crate の別の表示規則への波及は「やりすぎ」になりうる。

## 9. 敵対的調査（Step 3b）の所見と採否

全文は `workspace/adversarial-1124.txt`。5 命題を偽にしにいかせ、**壊せた項目は 0 件**だった。

| # | 命題 | 結果 | 採否 |
|---|---|---|---|
| 1 | `get_instant_commands` の呼び出し元は 1 件だけ | 壊れず（`#[tauri::command]` マクロ残存・`SNOTRA_EGUI_MAIN` フラグ・`commands/mod.rs` の再公開・`build.rs` 4 本をすべて当たった） | — |
| 2 | `InstantCommandDto` の消費者は 2 ファイルだけ | 壊れず + **一段強い保証**（`src-tauri` に `[lib]` が無く他 crate から依存もされていない） | **採用**（§2 へ反映・自分で `Cargo.toml` を実測して裏取り） |
| 3 | `AppState` 不在は実運用で到達しない | 結論は壊れず。ただし**根拠の言い回しが逐語的には粗い**と指摘 | **所見は採用・機序は書き写さない**（§5 へ反映。敵対枠が読んだのは tauri 2.11.4 で `Cargo.lock` は 2.11.5・自分で実測。差分未検算ゆえ判断は既存 doc へ委譲する） |
| 4 | §19.5 の表示出力は純関数化しても同一に保てる | 壊れず（description 空白のみ・args 空白のみ・0 件マッチを洗って抜け穴なし）+ **`Legacy` は load 後に残らない**という追加所見 | **所見は採用・全称の形では書かない**（§7 へ反映。`apply_migrations` を通す 4 経路を自分で列挙し、下限主張として扱う） |
| 5 | `SearchResult` / DTO を実際にシリアライズする点は無い | 壊れず（`serde_json::to_string` / `to_value` / `to_vec` が全 crate 0 件） | — |

⚠️ 所見（敵対枠が確信を持てないとしたもの）の扱い:

1. tauri 2.11.5 と 2.11.4 のパッチ差分未検算 → **採用**。成果物に機序を書かない理由にした
2. 命題 3 の根拠の言い回しが粗い → **採用**。ただし既存 doc の書き換えは本 issue の射程外（規範文書の変更はチームの合意事項でもある）
3. `research.md` の行番号に数行のずれ → **採用・修正済み**（`launcher_controller.rs:917-930` → 実測 `914-931`）

**機序の説明を逐語追認しなかった項目**（自分で一次証拠を取った）: `src-tauri` が bin crate であること（`Cargo.toml`）・`Cargo.lock` の tauri バージョン・`apply_migrations` の呼び出し経路・行番号。

## 10. 裁定済みの設計判断（plan 本文へ）

1. 純関数の置き場 → `snotra-core/src/instant.rs`
2. `display` 導出は private に畳む（`snotra-settings` への波及は射程外）
3. `Result` 撤去・DTO 撤去・fallback 変更・呼び出し点移行を 1 コミットに束ねる
4. 裏取りは §19.5 を直接接地させるテスト（旧経路の写しは書かない）

理由は `workspace/plan.md`「計画時点で確定した設計判断」が正本。
