# 計画: #1124 `get_instant_commands` を呼び出し元へ寄せ、撤去済み IPC 境界の形を解く

調査は `workspace/research.md`。

## 目的

`get_instant_commands` が残している **撤去済み IPC 境界の形**（無謬の `Result`・DTO 経由・`AppHandle` を値で受け取る）を解き、`&[InstantCommand] -> Vec<SearchResult>` の純関数へ寄せる。これにより `commands/` に在りながら egui フレームの中で毎打鍵走るという **異常そのものが消え、その説明に費やしていた doc も消える**。

**「速くなる」を目的にしない**（issue の明示的な禁止）。確保の形の整理として書く——コミットメッセージ・PR タイトルにも性能の主張を書かない（PR タイトルはリリースノートへ逐語で載る）。

## 受け入れ条件

1. `snotra-core::instant` に `&[InstantCommand] -> Vec<SearchResult>` の純関数が在り、`SPEC.md` §19.5 の表示規則（`description` 優先・`display` の導出 3 種）をそこが持つ
2. `launcher_controller.rs` の Instant 枝が `read_config` からその純関数を呼び、DTO も `Result` も `AppHandle` の `clone()` も経ない
3. `commands/instant.rs` から `get_instant_commands` と `matching_dtos` が消え、`launch.rs` から `InstantCommandDto` と `From` impl が消える
4. §19.5 の表示規則にテストが在る（**`description` 優先の分岐は現状ノーテスト**・研究 §7）
5. `SPEC.md` §19.2 の `display` の記述が実装と整合する
6. カテゴリ A・F の検証がすべて green

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `snotra-core/src/instant.rs` | **追加**: `pub fn matching_results(&[InstantCommand], &str) -> Vec<SearchResult>`、private `fn display_text(&InstantAction) -> String`。**変更**: `filter_instant_commands` を `pub` → private（下記の決定 4）。`//!` の公開関数列挙を追随。テスト追加 |
| `src-tauri/src/egui_shell/launcher_controller.rs` | Instant 枝（`run_search_with` の `QueryIntent::Instant` 腕・実測 914-931）を `read_config` + `matching_results` へ差し替え |
| `src-tauri/src/commands/instant.rs` | `get_instant_commands` / `matching_dtos` を削除。未使用になる `use` を整理（`filter_instant_commands` / `InstantCommand`） |
| `src-tauri/src/commands/launch.rs` | `InstantCommandDto` と `From<&InstantCommand>` impl を削除。1 行目の `use` から `InstantAction, InstantCommand` を落とす（`find_matching_tools` は残す）。テスト 3 件を `snotra-core` 側へ移植 |
| `SPEC.md` | §19.2 の `display` の記述を同期（「DTO 生成時に常に算出する」→ `action` から算出する派生値） |

**新規ファイルは作らない**——`G-module-linkage`（索引 + `mod` 宣言）を踏まない。

## 実装の形

```rust
// snotra-core/src/instant.rs

/// instant 候補の絞り込みと結果行の組み立て（`SPEC.md`「19.5 マッチングと結果表示」）。
///
/// **owned な `SearchResult` を返すことが契約である**——config を借りた参照
/// （`filter_instant_commands` の戻り値）を読みの外へ出さないため、所有への移しを
/// この関数の中で終える。呼び出し元は `AppState` の read guard の中でこれを呼ぶ。
/// **行うのは文字列の確保までで、I/O も錠も無い**（`AppState::read_config` の契約）。
pub fn matching_results(commands: &[InstantCommand], prefix_input: &str) -> Vec<SearchResult> {
    filter_instant_commands(commands, prefix_input)
        .into_iter()
        .map(|c| SearchResult {
            name: c.name.clone(),
            // §19.5: description があれば優先、無ければ display（URL / exe args）
            path: if c.description.is_empty() {
                display_text(&c.action)
            } else {
                c.description.clone()
            },
            is_folder: false,
            is_error: false,
        })
        .collect()
}
```

```rust
// src-tauri/src/egui_shell/launcher_controller.rs（Instant 枝）
let rows = crate::egui_shell::read_config(
    &self.app_handle,
    |c| snotra_core::instant::matching_results(&c.instant_commands, &filter_name),
    Vec::new,
);
```

## 計画時点で確定した設計判断（理由つき）

1. **置き場は `snotra-core/src/instant.rs`。** `filter_instant_commands` の隣、`SearchResult` は同 crate の `ui_types`。tauri 非依存でテストでき、新ファイル無しゆえ `G-module-linkage` を踏まない
2. **`display_text` は private に畳む。** `snotra-settings/src/tabs/instant.rs:110-131` に同じ導出の 3 つ目の写しが在るが（研究 §8）、**本 issue の射程外とする**——(a) 設定画面の表示規則は違う（description と display を**両方**出す・ランチャは優先で片方）、(b) 寄せるには `pub` が要り、lib crate の `pub fn` に `dead_code` は出ないので**到達性の検出器を 1 つ失う**（`docs/development-principles.md`「config の値は到達性の検出器を持たない」の括弧書き）、(c) settings 側は `suspect_legacy` 判定と 1 つの `match` に融合しており、分離すると `match` が 2 回になる。**写しの存在は research.md に記録済み**
3. **`Result` 撤去・DTO 撤去・fallback 変更・呼び出し点移行を 1 コミットに束ねる。** `-D warnings` 下では未使用の新 API が `dead_code` で落ち、旧 API を残せば導出が 2 箇所になる（AGENTS.md「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）。`SPEC.md` 同期も同じコミットに入れる（AGENTS.md「3層分担」——挙動を変える変更では両者を同じ変更で整合させる）
4. **`filter_instant_commands` を private へ落とす。** 非テスト呼び出し元は `matching_dtos` ただ 1 つであり（実測・研究 §2）、それが `matching_results` に置き換わると `pub` である理由が消える。**公開面の純増を避ける**——`matching_results` を `pub` にする一方でこちらを private にすれば、到達性の穴の数は変わらない。既存テスト 5 件は同 crate 内なのでそのまま生きる
5. **`AppState` 不在時の fallback を `Vec::new` にする**（唯一の挙動差分・下記）

## 挙動差分（到達しない経路 1 件のみ）

| 経路 | 現状 | 変更後 |
|---|---|---|
| `try_state::<AppState>()` が `None`（**実運用では到達しない**） | `Config::default()` を建てて既定 instant コマンド（`g` / `gh`）を絞り込んで返す | 空を返す |
| 上記以外すべて | — | **不変** |

**到達しないことの根拠は既存 doc へ委譲する**——`AppState::read_config` / `egui_shell::read_config` の doc と `ADR-config-default-fallback-references`「後日の決定（#824 の 1 と 2）」が正本であり、本 issue はその判断を引き継ぐだけである。**tauri 内部の機序を新たに主張しない**（既存 doc が使う「`.manage` は `.setup` より前に走る」の言い回しは逐語的には粗いと敵対枠が指摘したが、検算されたのは tauri 2.11.4 で `Cargo.lock` は 2.11.5・研究 §5）。既存 doc の書き換えは本 issue の射程外。

**変更の理由**: `AppState` 不在 = config 未ロードの時点であり、そこで既定コマンドを返すのは「たまたま既定と一致するユーザーにだけ正しい」。空は「まだ設定を知らない」の素直な表現で、`SPEC.md` §19.5 の「マッチするコマンドが 0 件の場合は結果を空にする」と矛盾しない。同時に `Config::default()` が伴う `exists()` と OS ロケール読み（`get_instant_commands` の doc 自身が「この fallback だけが `Config` 全体を建てる」「他へ写すな」と異常視していたもの）が消える。先例は上記 ADR——到達しない fallback でも意味の正しい側へ倒している。

**ADR は書かない**——却下した案が既存 ADR の系列の中にあり（同 ADR が同じ判断を 2 件記録済み）、新しい否定の知識が生じていない。判断の根拠は `matching_results` の呼び出し点のコメントに残す。

## 消える行の不変条件と再確立地点

| 消える doc / コード | 不変条件 | 再確立地点 |
|---|---|---|
| `matching_dtos` の doc「DTO 化まで config の読みの中で終える必要がある」 | 所有化を読みの中で終える（借りた参照を read guard の外へ出さない） | **構造で再確立**——`matching_results` が owned な `Vec<SearchResult>` を返す。同趣旨を新関数の doc に書く |
| `get_instant_commands` の doc「読みの中で行うのは文字列の確保まで・I/O も錠も無い」 | `read_config` の read クロージャの契約 | 新関数の doc に書き写す（正本は `AppState::read_config`） |
| `get_instant_commands` の doc「AppState 不在時に panic しなくなった（#1076）」 | 不在経路で panic しない | `read_config` の `fallback` が引き続き受ける（`Vec::new`） |
| `get_instant_commands` の doc「`commands/` に在るが egui フレームの中で毎打鍵走る」の説明群 | — | **消滅が目的**（issue の主眼）。読みがスレッドの自明な場所へ移る |
| `launch.rs` の DTO テスト 3 件（Url / Exec+args / Exec-args 無し） | `display` 導出の 3 分岐 | `snotra-core` 側の新テストへ移植（**カバレッジを落とさない**） |

## テスト方針

`snotra-core/src/instant.rs` の `mod tests` へ追加する。§19.5 を直接接地させる形にする（旧経路の写しを書かない——`grounding-test-becomes-fixed-point` の罠）。

- `description` 非空 → `path == description`（`display` は使わない）
- `description` 空 × `InstantAction::Url` → `path == url`
- `description` 空 × `Exec`（args 有）→ `path == "exe args"`
- `description` 空 × `Exec`（args 無）→ `path == "exe"`（**末尾スペース無し**）
- `description` 空 × `Legacy` → `path == command`（**実運用では到達しない防御的分岐**・下記）
- 前方一致 0 件 → `Vec` が空（§19.5「該当なしの行を出さない」）
- 空入力 → 全件（`filter_instant_commands` との合成が保たれること）
- 全ケースで `is_folder == false` / `is_error == false`

**実運用点の裏取り**（研究 §7）: 実 `config.toml` は `url` 種別 2 件のみで、`description` 有 / 無の両形を持つ。**つまり実運用で毎打鍵走るのは上の最初の 2 ケースだけである。** `Exec` と `Legacy` はテストでしか走らない。

**`Legacy` の位置づけ**: `apply_migrations` を通る 4 経路（`load` 正常系・default 落ち・設定 GUI 保存・backup import）を列挙した結果、実運用で `Legacy` が生き残る経路は見つからなかった（研究 §7・「構造的に存在しえない」という全称の形では書かない）。ゆえに新関数の `Legacy` 枝は **`match` の網羅性のために要る防御的分岐**である。テストで固定はするが、doc とテスト名に「到達しない防御」であることを残す——`commands/instant.rs:78` の既存コメント（「load 後は移行済みで到達しないが、防御的に Url 扱い」）と同じ判断である。

## 検証コマンド

```
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
```

- `cargo doc` は **hook が発火しない**（`partial-automation-habituates`）。`get_instant_commands` / `InstantCommandDto` / `matching_dtos` への intra-doc link が残ると壊れるため**手で必ず走らせる**
- `npm run governance:check` は `SPEC.md` を触るため（カテゴリ F）
- `cargo test -p snotra-settings` は変更しないので任意（CI が担保）
- カテゴリ C / D は非該当（ウィンドウ生成・ホットキー・スラッシュコマンド・視覚スタイルのいずれにも触れない）

## `SPEC.md` の更新

§19.2「共通フィールド」の `display` 行（904 行目）:

- 現行: 「…ユーザーが設定する config フィールドではなく、バックエンドが **DTO 生成時に**常に算出する派生値。」
- 変更後: 「…ユーザーが設定する config フィールドではなく、`action` から算出する派生値。」

**§19.5 の表示規則そのものは変更しない**（description 優先・display の中身は不変）。ADR 2 本と `docs/superpowers/plans/*` `specs/*` は歴史記録ゆえ触らない（研究 §6）。

## その他文書

- `snotra-core/CLAUDE.md:62` の `instant.rs` 索引行はファイル名の索引ゆえ**変更不要**。責務散文は `//!` が正本なので、`//!` の公開関数列挙に `matching_results` を足し `filter_instant_commands` を外す
- `src-tauri/CLAUDE.md:33` は `InstantCommandDto` を名指ししていない（実測）ゆえ**変更不要**

## フェーズと作業項目

### Phase 1: 純関数の新設・移行・旧 API 撤去（1 コミット）

- [x] `snotra-core/src/instant.rs` に `matching_results` と private `display_text` を追加（doc に read クロージャの契約と所有化の理由を書く）。**doc に書く `SPEC.md` の見出し参照は正準形で逐語一致させる**——`G-heading-refs` は `.rs` を走査対象に含む（`docs/build-commands.md` カテゴリ F・#925）ので、`SPEC.md`「19.5 マッチングと結果表示」の字面をそのまま使う
- [x] `filter_instant_commands` を private へ落とし、`//!` の公開関数列挙を追随させる
- [x] `launcher_controller.rs` の Instant 枝を `read_config` + `matching_results` + `Vec::new` fallback へ差し替える（`app_handle.clone()` を `&self.app_handle` へ）
- [x] `commands/instant.rs` から `get_instant_commands` / `matching_dtos` と未使用 `use` を削除する
- [x] `launch.rs` から `InstantCommandDto` / `From` impl / DTO テスト 3 件と未使用 import を削除する
- [x] テストを `snotra-core/src/instant.rs` の `mod tests` へ追加する（上の 8 ケース）
- [x] `SPEC.md` §19.2 の `display` 行を同期する

### Phase 2: 検証

- [x] カテゴリ A の全コマンドを実行する（`cargo doc` を含む・hook 非発火）
- [x] `npm run governance:check` を実行する
- [x] `grep -rn "InstantCommandDto\|get_instant_commands\|matching_dtos" --include=*.rs .` が **0 件**（歴史記録の `.md` を除く）であることを確認する
- [x] `/race-check` を実行する（本文が「計画段階では起動しない」と定めているため実装後・フレーム内 live-read の位置が変わるトリガーに対応）

### Phase 3: レビュー対応（実装中に判明・計画外）

`code-reviewer` round 1 は Critical / High **0 件**、Medium 1・Low 5・⚠ Low 1。**7 件すべてを採用**し、機序はいずれも自分で一次証拠を取り直して裁定した。

- [x] **Medium 1**: `matching_results_rows_are_neither_folder_nor_error` の doc がアイコン取得へ**誤って帰属**していた。実測（`results_view.rs` の `request_icons_for_results` は行の種別を見ない / `needs_extraction` は path と試行回数だけ）で確認し、実際に守っている 2 点（→ 展開ガード・`execute_instant_selected` の `is_error` チェック）へ書き直した
- [x] **Low 1**: 根拠のポインタが誤り（`AppState::read_config` の doc に `.manage` / `.setup` は**含まれない**・実測 0 件）→ `egui_shell::read_config` のコメントへ差し替え。§19.5「マッチ 0 件」との類比（別条件）を削り、代わりに**レビュアが見つけた非対称**（既定コマンドの行を出しても実行側の `|| None` が起動できない）を書いた
- [x] **Low 2**: 「唯一の消費者」→「テストを除く消費者は」（同ファイルのテスト 5 箇所が呼ぶので字義どおりには偽だった・`AGENTS.md`「全称表現は前提条件とセットで書く」）
- [x] **Low 3**: doc 第 3 段落が直下の分岐の写しだったので削り、理由（非空なら文字列をその場で捨てる）だけ残した
- [x] **Low 4**: `launch.rs` のテスト移送の記録コメントを削除（`docs/comment-guidelines.md`「変更履歴の再現」）。失われる情報の再確立地点は `SPEC.md` §19.2 の同期と移植先テスト `matching_results_prefers_description_over_display`
- [x] **Low 5**: `SPEC.md:1031` に `display` **フィールド**の語彙が残っていた——**§19.2 を直した当のコミットが取りこぼしていた**（`AGENTS.md`「写しを直す当のコミットが典型」の実例）。「§19.2 の `display`」へ改め、「表示されず、算出もされない」を明記
- [x] **⚠ Low 6**: `docs/architecture.md` の純ロジック行に結果行の組み立てを追記（表示規則が UI 層から移ったため）
- [x] fix-forward 差分にカテゴリ A + F を再実行（`AGENTS.md`「レビュー指摘へ修正を当てた」行）
- [x] 指摘を出した枠組み（`code-reviewer`）を **`SendMessage` で継続**して修正差分を検算させる（新規起動しない＝Step 4b「1 体だけ」を守る）

### Phase 4: round 2 の指摘への対応（fix-forward が新しい誤りを生んでいた）

round 2 は Critical / High / Medium **0 件**、Low 5（新規の誤り 2・不正確 1・過剰 1・⚠ 部分修正 1）。
**7 件のうち 6 件は正しく着地したが、Medium 1 への修正そのものが 2 件の新しい誤りを持ち込んでいた**——
`AGENTS.md`「修正は指摘箇所へ注意が集中し、周辺に新しい誤りを生む」の実例である。

- [x] **R2-1（新規の誤り）**: Medium 1 の修正がコードスパンを 2 本**行またぎ**させ、`sel.is_folder && !sel.is_error` と `if sel.is_error { return }` が **grep で辿れなくなっていた**（両方 0 件を実測）。`cargo doc` はリンクではないので緑のまま通る
- [x] **R2-2（新規の誤り）**: 同じ修正が「ゲートは `show_icons` と `needs_extraction` **だけである**」という、**`snotra-core` が維持できない全称**（`src-tauri` の実装に依存）を書き込み、`SPEC.md:1036`（instant モード中はアイコン取得をスキップ）と**正面から矛盾**していた。**pre-existing だったのは挙動であって、矛盾の顕在化はこの差分が持ち込んだもの**である。→ アイコンの節を丸ごと落とし、肯定的記述（→ 展開ガード・実行可能条件）だけで自足させた。R2-1 も同じ書き直しで閉じた
- [x] **R2-3（不正確）**: 「取得側と実行側が同じ答えになる」は成立しない。取得側が空を返すと `results()` が空になり、`execute_instant_selected` は入口の `results().get(index)` で**先に return する**ので実行側の fallback には到達しない（実測）——2 つの答えが同時に求まる場面が無い。→「空へ倒せば、起動できない行がそもそも出ない」へ
- [x] **R2-4（過剰）**: `SPEC.md` の「算出もされない」を撤回。利用者から観測できず、**非算出を測るテストは書けない**（どのテストも支えない主張が第 1 層に増えていた）。記録は Low 3 の修正が書いた doc 文（過去形ゆえ腐らない）が既に持つ
- [x] **⚠ R2-5（部分修正）**: 「借用のまま受け取る呼び出し元は**もう居ない**」の全称否定を撤回し、private 化の理由（#1124 で crate 外の消費者が消えたこと・過去の事実）だけを残した
- [x] レビュアが指定した検出器（`git diff main -- '*.rs' | grep '^+' | awk -F'\`' 'NF%2==0'`）を**修正差分にも**当てて 0 件を確認（`.md` 側にも当てた）。**round 1 で 0 件・round 2 で 2 件を出したのはこの検出器だけ**であり、`cargo doc` も `clippy` も `governance:check` もこの型を見ていない
- [x] カテゴリ A + F を再実行（すべて green）

**別 issue へ送る残余**: `SPEC.md` §19.5「インスタントコマンドモード中はアイコン取得をスキップする」が現在の egui 経路に実装を持つかは未確認である（レビュアが 4 経路を辿った範囲では instant ゲートが見つからなかったが、「存在しない」とは断定していない）。**pre-existing であり本 issue の射程外**。
- [x] 実装差分を確定させる（作業ツリーが計画どおりの形になっていることを `git diff` の引数 1 個の形で確認する）

## 未確定（実装前に潰す）

- [x] 敵対的調査（`workspace/adversarial-1124.txt`）の所見 — **壊せた項目 0 件。** 採否と理由は研究 §9 とセルフレビューの「要対処」に記録済み。機序の説明は逐語追認せず、`Cargo.toml` / `Cargo.lock` / `apply_migrations` の呼び出し経路 / 行番号を自分で実測して裁定した

## 条件別チェックの振り分け（`AGENTS.md`「条件別チェック」）

| トリガー | 該当 | 実施 |
|---|---|---|
| 関数・型を新規定義／改名／導入 | ✅ | 呼び出し元を **LSP `findReferences` で列挙済み**（研究 §2）。`/dry-check` 実施済み（下記）。旧 API 削除の移行漏れ検出器は `cargo check --workspace`（同一 workspace ゆえ全消費者が compile-fail になる）。新 API 導入と呼び出し点移行は 1 タスクに束ねた |
| 重複した読み・冗長に見える状態を束ねる／消す | ✅ | **DTO の `display` フィールドが「後で読まれる」ことに依存していないか** → 唯一の消費者（`launcher_controller` の Instant 枝）が `description` 非空のとき**捨てている**（実測）。他に読み手は無い（LSP 11 参照すべてが定義・`From` impl・テスト）。`name` / `description` は新関数がそのまま読む |
| フレーム内 live-read を追加／変更 | ✅ | `/race-check` は本文で「**計画段階では起動しない**（#784）」と定めているため、**実装後に起動する**（Phase 2 の作業項目に置いた） |
| ガバナンス文書（`SPEC.md`）を変更 | ✅ | `npm run governance:check`（Phase 2） |
| 網羅性が要件（DTO 消費者の全数） | ✅ | 母集団は rust-analyzer / cargo が知っている（LSP + `cargo check --workspace`）。独立再導出は Step 3b の敵対枠が担った |
| 永続形式・識別子／キー形式 | ❌ | `SearchResult` は永続化されない（`ui_types.rs` の `//!` が「永続形式にも入らない」と実測を記録） |
| 対称ペア／UI モード・ガード条件／件数 N・上限パラメータ／ファイル追加・削除 | ❌ | 非該当 |

### `/dry-check` の結果（実施済み）

grep パターン: `format!("{exe} {args}")` 系 / `InstantAction::Url` / `description.is_empty()` / `to_lowercase().starts_with`

| 候補 | 判定 |
|---|---|
| `snotra-settings/src/tabs/instant.rs:110-131`（display 導出の写し） | **[維持]** — 設計判断 2 の理由（表示規則が違う・`pub` 化で到達性の検出器を失う・`suspect_legacy` と融合）。写しの存在は research.md §8 に記録 |
| `snotra-core/src/config.rs:1119-1123`（`InstantAction` 全 3 variant の match） | **[維持]** — **別概念**。取り出すのは「変数展開のテンプレート」であり、`Exec` では `args` **のみ**（display は `exe args`）。片方だけが変わる将来が挙がる（展開対象に `exe` を含める／display に説明を足す） |
| `description.is_empty()`（2 件） | **[維持]** — settings 側は「両方表示」で優先ではない。**優先の判定は 1 箇所だけ**であり、それが新関数へ移る |
| 前方一致フィルタ（`to_lowercase().starts_with`） | **写し無し**（1 件のみ・`filter_instant_commands` 本体） |

## セルフレビュー

- リスク: 通常（`/plan-review` の高リスク条件に非該当——永続形式に触れず・並行性の機構を足さず・状態遷移を変えず・ガバナンス文書は `SPEC.md` の 1 行のみ・網羅性の母集団はコンパイラが持つ）
- plan-review: 未実施（通常リスク・自己レビューのみ）
- エージェント数: 1（Step 3b の敵対的調査のみ）
- 要対処: **敵対枠の 5 命題はいずれも壊せず（壊せた項目 0 件）。** 反映は 4 件——(1) `src-tauri` が bin crate ゆえ DTO は他 crate からリンク不能（研究 §2・より強い保証へ差し替え）、(2) `AppState` 不在の機序を成果物へ書き写さない方針を明記（研究 §5・plan の挙動差分節）、(3) `Legacy` の到達性を下限主張として記録しテストの位置づけを「防御的分岐」と明記（研究 §7・plan のテスト方針）、(4) 行番号のずれを修正（`914-931`）
- 未検証: tauri 2.11.5 と 2.11.4 のパッチ差分（**本計画はこの機序に依存しない**——依存しない形へ文言を変えることで潰した）

### 自己レビュー 5 点

1. **issue の全要件に作業項目が対応する** — 純関数化 ✅ / DTO 消費者を先に数える ✅（研究 §2・LSP 実測）/ `SPEC.md` §19.5 と突き合わせる ✅（§19.5 は不変・§19.2 を同期）/「速くなる」を目的にしない ✅（目的節に明記）
2. **境界条件と検証** — 0 件 / 空入力全件 / description 空・非空 / `Exec` の args 空・非空 / `Legacy` のすべてにテストを割り当てた
3. **新しい状態・リソース・プロセス** — 追加しない。純関数のみ。`AppHandle` の `clone()` は**減る**
4. **より単純な既存パターン** — `read_visual` / `visual_snapshot` と同型（研究 §3）。新機構を作らない
5. **壊してはならない不変条件の検知手段** — 「所有化を読みの中で終える」は型で強制（owned 戻り値）/「display 導出の 3 分岐」はテストで固定 / 旧シンボルの残存は grep で 0 件確認 / intra-doc link は `cargo doc`

## 人間レビュー

- [x] 承認済み — 2026-08-18 / 問い: "上の 2 点（`filter_instant_commands` の private 化 / `AppState` 不在時の fallback を空へ）について、承認または `workspace/plan.md` への注釈をお願いします。" / 回答: "OK"
