# 実装計画 — #1129 `AppState.config` を private にし、検知器は置かない

ブランチ: `chore/appstate-config-private` ／ 調査: `workspace/research.md`

## 目的

`AppState.config` の直読み（`state.config.read().unwrap()`）という残余を、**検知器ではなく可視性で**
塞ぐ。issue が自ら課した判断順序の 1（private 化できるか）が通ったので、順序 2（検知器の是非）へは
進まない。**製品の挙動は 1 行も変えない。**

## 受け入れ条件

1. `AppState` の `config` フィールドが `state.rs` の外から綴れない（**別モジュールへ注入した
   `state.config.read()` が `cargo check` で落ちることを実測し、エラーコードを doc へ記録する**）
2. `AppState` の構築が `AppState::new` の 1 か所へ集約され、`Engine::config_handle()` の呼び出し点が
   1 つになる（= #1032 の「同じ Arc」不変条件が規範から構造へ移る）
3. 残る残余 3 つが**正確に**記述されている——(a) `engine.lock().unwrap().config_handle().read()` が
   今も通ること〔既存・不変〕、(b) `state.rs` の内側（テストを含む）では綴れること〔**縮んだが消えていない**〕、
   (c) guard 内の錠・I/O は構造では止まらないこと〔既存・不変〕。
   **「直読みは書けなくなった」と無条件に書かない**
4. カテゴリ A の全コマンドと `npm run governance:check` が green
5. 検知器（grep によるガバナンス検査）を**置かない**（issue の順序に従った結論として ADR に記録する）

## 変更ファイル一覧と対象シンボル

| ファイル | 対象シンボル | 変更 |
|---|---|---|
| `src-tauri/src/state.rs` | `AppState.config`（フィールド） | `pub` を外して private にする |
| 〃 | `AppState::new`（**新規**） | `pub(crate) fn new(engine: Engine, initial_indexing: bool) -> Self` |
| 〃 | `AppState.config` の doc / `AppState::read_config` の doc | 射程の記述を更新（残余の 3 点を正確に） |
| 〃 | `mod tests::test_state` | `AppState::new` を使う |
| `src-tauri/src/main.rs` | `app_state` 束縛（240 行目付近） | `AppState::new(engine, initial_indexing)` |
| `src-tauri/src/commands/system.rs` | `mod tests::test_state` | `AppState::new(engine, indexing)` |
| `src-tauri/CLAUDE.md` | 「config の読みは `read_config` を通す」条項（57 行目） | 「ただし機構ではない（`pub` ゆえ直読みは通る）」を実態へ更新 |
| `docs/adr/ADR-appstate-config-visibility.md` | **新規** | 検知器を却下した否定の知識・案 G との関係 |

**触らない**: `snotra-core/src/engine.rs`（`config_handle` の可視性は crate をまたぐため変えられない）、
凍結 ADR 2 件（`ADR-adr-frozen-history`）、`main_visible` 等の他フィールド（射程外）。

## 実装順序

1. **Phase 1（コード）**: `AppState::new` の導入 → `config` を private 化 → コンパイルエラーになる
   構築点 2 件を `new` へ移行。**この順序が移行漏れ検出器そのものである**（private 化した時点で
   取りこぼしはコンパイルが落とす）
2. **Phase 2（実測）**: フォールトインジェクションで機構が効くことと、**閉じていない側**を同じ場で測る
3. **Phase 3（文書）**: state.rs の doc → `src-tauri/CLAUDE.md` の条項 → 新 ADR
4. **Phase 4（検証）**: カテゴリ A ＋ F ＋ `cargo doc`

## 不変条件と異常系

- **`config` は engine が持つのと同じ `Arc` である**（#1032）。`new` の中で `engine.config_handle()` を
  呼ぶ。**`Mutex::new(engine)` は engine をムーブする**ので、`let config = engine.config_handle();` を
  先に置く。既存テスト `app_state_config_is_the_same_arc_the_engine_holds` がこれを縛る（`new` を
  使うようにすると、そのテストが `new` 自体を測ることになる）
- **`read_config` の契約は変わらない**（read guard の中で錠も I/O も取らない）。錠の構成・読みの経路は
  一切変えないので `/race-check` の対象ではない
- **異常系は増えない**。`new` は失敗しうる処理を含まない（`config_handle` は `Arc::clone` のみ）
- **リソースの生成/破棄の非対称は生じない**（`Arc` の複製 1 つで、破棄は既存どおり drop）

## テスト方針と検証コマンド

- **新規テストは足さない。** 受け入れ条件 1 を測るのは**コンパイラ**であり、テストではない
  （`#[cfg(test)]` の中では綴れてしまうので、テストでは表現できない）
- 既存テストはそのまま通る必要がある（`cargo test -p snotra`）
- 検証コマンドは `docs/build-commands.md` カテゴリ A（`.rs` 変更）＋ カテゴリ F（`*.md` 変更）が正本。
  **`cargo doc --workspace --no-deps --document-private-items` は hook が発火しない**ので手で打つ
  （doc コメントを触るため・`partial-automation-habituates`）

## `SPEC.md`・関連文書の更新要否

- **`SPEC.md`: 更新不要。** 本変更は crate 内部の可視性とコンストラクタの導入であり、`SPEC.md` が
  記述する挙動・フロー・状態遷移を 1 つも変えない（AGENTS.md「『fix』でも文書化された挙動を変えたら
  仕様変更」の逆側——挙動が変わらないので仕様変更ではない）
- **`docs/architecture.md`: 更新不要。** 231 行目は #1032 の経緯を書き、射程は
  `src-tauri/CLAUDE.md` の条項が正本と明記している（写しを置かない設計）
- **`PERFORMANCE.md`: 更新不要。** 性能特性は変わらない
- **更新が要るのは 2 件＋新 ADR 1 件**（上表）。**PR 本文も写しの母集団に入る**
  （`pr-body-is-outside-the-grep-population`）

## 作業項目

### Phase 1 — コード

**Phase 1 の途中で PostToolUse hook が赤くなるのは正常であり、直すべき欠陥ではない。** `new` を足した
直後は呼び出し元がまだ無く `dead_code` が `-D warnings` の下で立つ（機序は
`ADR-config-read-without-exception` 案 4 が記録している形）。`pub` を外した直後は未移行のファイルが
**E0451 で落ちる——それが移行漏れ検出器の作動そのものである**。**合否の判定点は Phase 末の
`cargo check --workspace` 1 回であり、編集ごとの赤ではない。** fix-forward を当てないこと。

- [ ] `src-tauri/src/state.rs` に `pub(crate) fn new(engine: Engine, initial_indexing: bool) -> Self` を追加する（`config_handle()` を `Mutex::new(engine)` より前に呼ぶ）
- [ ] `AppState.config` の `pub` を外す
- [ ] `src-tauri/src/main.rs` の構築を `AppState::new` へ移行する（**241 行目の説明コメント「engine が持つのと同じ Arc を渡す（写しではない）」は `new` の doc へ移す**——構造が規律を吸収した以上、そこが正しい置き場になる）
- [ ] `src-tauri/src/commands/system.rs` の `test_state` を `AppState::new` へ移行する
- [ ] `src-tauri/src/state.rs` の `test_state` を `AppState::new` へ移行する
- [ ] `cargo check --workspace` が green であることを確認する（＝ 移行漏れが無い）

### Phase 2 — 機構が効くことと、閉じていない側を、**実装後の形で**測り直す

計画段階（`new` を実装しない `pub` 外しのみ）で一度測ってある（値は「未確定」節）。**実装差分は
それ自体が誰の検算も受けていない**ため、`new` が入った形で同じ注入を再実施して値を確定させる。

- [ ] `commands/system.rs` の `#[cfg(test)]` 内へ `state.config.read()` を注入し、`cargo check --workspace --all-targets` が **E0616** で落ちることを確認して `git checkout --` で復元する
- [ ] 同じ場で `state.engine.lock().unwrap().config_handle().read()` が**可視性のエラーを出さない**ことを確認して復元する（#1123 と同じ両向きの測定・偽の全称を防ぐ）

### Phase 3 — 文書

- [ ] `state.rs` の `config` フィールド doc と `read_config` doc を、Phase 2 の実測値（エラーコード・日付）込みで更新する
- [ ] `src-tauri/CLAUDE.md` 57 行目の条項の「**ただし機構ではない**（`AppState.config` は `pub` ゆえ直読みは通る）」を実態へ更新する。**残余を 3 つとも書く**——(a) `engine.lock().unwrap().config_handle().read()` は今も通る〔既存・不変〕、(b) `state.rs` の中（`#[cfg(test)] mod tests` を含む）では綴れる〔**private 化後に残る形。「どこでも綴れる」から縮んだのであって消えてはいない**〕、(c) `read` へ渡すクロージャの中の錠・I/O は構造では止まらない〔既存・不変〕
- [ ] `docs/adr/ADR-appstate-config-visibility.md` を新設する。**構成は既存 ADR に倣う**（`ADR-config-read-without-exception.md` / `ADR-adr-frozen-history.md` の 2 枚が手本）——「文脈」「決定」「却下した代替案」「旧 ADR との関係」「帰結」。**H1 見出しは stem と一致させる**（`# ADR-appstate-config-visibility: …`。`G-adr-file-names` が形と見出しの一致まで照合する）
- [ ] 新 ADR への短縮引用を生きた層（条項）へ置く（G-adr-citations）

### Phase 4 — 検証

- [ ] `docs/build-commands.md` カテゴリ A のコマンドをすべて実行する
- [ ] `cargo doc --workspace --no-deps --document-private-items` を実行する（hook 非発火）
- [ ] `npm run governance:check` を実行する（カテゴリ F・`*.md` の沈黙は合格ではない）

## 未確定（実装前に潰す）

- [x] **別モジュールから private field を読んだときの実エラーコード** — 2026-08-18 に注入して実測（詳細と表は `workspace/research.md`「実測」節）。**直読み = E0616**、**構築リテラル = E0451**、
  `engine.lock().unwrap().config_handle().read()` は**可視性のエラーを出さず今も通る**。
  注入は `git checkout --` で復元済み（`git status` で確認）
- [x] **新 ADR を書くか** — **書く**。検知器を却下した判断が否定の知識であり、かつ
  `ADR-config-read-without-exception`「帰結」が残余として挙げた「`AppState.config` の直読みが
  書けること」が本変更で偽になるが、**凍結ゆえあちらを直せない**ので生きた層に受け皿が要る
  （#1123 が案 G に対して取ったのと同じ作法）

## セルフレビュー

- リスク: **高**（`src-tauri/CLAUDE.md` の規範条項を書き換えるためガバナンス区分）
- plan-review: 独立レビュー 1 体（`--deep` は不要——網羅性はコンパイラが担保し、ガバナンス文書の
  移動・圧縮・分割を含まない）
- エージェント数: 2（3b の敵対枠 1 体 ＋ plan-review 1 体）
- 主エージェントの自己照合（Step 5a の 5 点）:
  1. **issue の全要件に作業項目が対応する** — issue が課した判断順序 1（private 化の可否）は測定済み、
     2（検知器の是非）は 1 が通ったので評価に進まない。この結論自体を ADR へ残す作業項目がある
  2. **境界条件と検証** — 境界は「どこから綴れるか」の 1 軸で、内側（`state.rs`）/ 外側（他モジュール）/
     迂回（`config_handle`）の 3 点すべてを Phase 2 の注入が実測する
  3. **新しい状態・リソース・プロセスの正常/失敗/破棄経路** — 増えない（`new` は `Arc::clone` のみで
     失敗経路を持たず、破棄は既存の drop のまま）
  4. **より単純な既存パターンで置き換えられないか** — 検知器案が「より単純」に見えるが**下段**であり、
     issue 自身の判断順序が private 化を先に置いている。`new` を作らず `config` だけ private にする案は、
     他モジュールの構築が不可能になるので成立しない
  5. **壊してはならない不変条件に検知手段がある** — #1032 の「同じ Arc」は既存テスト
     `app_state_config_is_the_same_arc_the_engine_holds` が縛る（`new` を使う形にすると `new` 自体を測る）
- 要対処: **0 件**
- 未検証: なし

## plan-review 結果

- リスク: **高**（ガバナンス文書の変更）
- レビュー方式: 計画準拠レビュー 1 体（Step 2。網羅性はコンパイラが担保するので Step 2b は選ばない）
- エージェント数: 1（3b の敵対枠を含めると 2）
- 成果物: `workspace/plan-review-appstate-config.md`

### 要対処

- **なし**（2 観点とも計画を差し戻す欠陥は無し）

### 軽微（**3 件とも計画へ反映済み**）

- Phase 3 の文言が残余を「2 つ」としており、private 化後に残る `state.rs` 内側の形を名指ししていなかった
  → 受け入れ条件 3 と Phase 3 の両方を **3 つの明示列挙**へ書き換えた
- 新 ADR の作業項目が「決定・却下代替案・旧 ADR との関係」だけを挙げ、既存 ADR の典型構成
  （文脈・帰結）を欠いていた → 手本 2 枚と構成、`G-adr-file-names` の見出し一致要件を明記した
- `main.rs:241` の説明コメントが移行で失われる → **`new` の doc へ移す**ことを作業項目に明記した

### レビュアが独立に確認した事実（本計画の根拠を補強するもの）

- **`G-adr-citations` は引用側しか見ない**——新 ADR が未引用でも `governance:check` は赤くならない。
  計画の「引用を置く」項目は必須条件ではなく良い実務（**この事実は計画を弱めない**ので項目は残す）
- `Engine::config_handle` の doc は呼び出し点の**数**を主張していない → 「触らない」判断は正しい
- `docs/architecture.md` / `PERFORMANCE.md` / `SPEC.md` を実 grep し、いずれも `AppState.config` の
  可視性を記述していないことを確認 → 更新不要の判断は正しい

### 判断

- 実装着手: **可**（人間の承認後）

## 人間レビュー

- [x] 承認済み — 2026-08-18 / 問い: "`workspace/plan.md` の計画で実装へ進めてよろしいですか — 承認、または plan.md への注釈をお願いします" / 回答: "OK"
