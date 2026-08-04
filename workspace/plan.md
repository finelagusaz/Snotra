# plan — #838 SPEC §6.3 の as-built 明文化（2 点）

## 目的

`SPEC.md` §6.3 に、実装と食い違う 2 点を as-built として書き足す。**挙動は一切変えない**（`SPEC.md` と `search_state.rs` のテストだけを触る）。

## 受け入れ条件（issue より）

1. §6.3 の記述が実装と一致する（エラー行表示中の挙動を含む）
2. `SPEC.md` §6.1 / §6.3 / §6.6 の間で同じ事実を二重に書かない
3. 挙動は変えない（純粋核テストで固定できる範囲は固定する）

## 明示した仮定（実装を止めない裁定）

- **ずれ 1 の事実は §6.3 だけに置き、§6.6 は無改変とする。** issue の提案が §6.3 を名指しし、AC 2 が二重記述を禁じるため。§6.6 の「右/左/Escape は通常どおり有効」が閉じた列挙に読める点は残るが、それは §6.6 の側の書き換え（＝射程外の再構成）を要するので取らない。Step 5c で人間へ 1 問だけ確認する
- **AC 3 の「固定する」は 2 点のうち 1 点にしか当たらない。** ずれ 2（選択リセット）は純粋核テストで固定できるが、ずれ 1（エラー行の filter 非適用）は `LauncherController` の driver 側にあり `AppHandle` を要するため固定できない（`research.md`「技術的制約」）。**「両点をテストで固定した」とは書かない**

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 内容 |
|---|---|---|
| `SPEC.md` | §6.3「フォルダ展開中の検索」（現行 `SPEC.md:247-251`） | 箇条書きを 2 行追加（見出しの増減なし） |
| `src-tauri/src/egui_shell/search_state.rs` | `mod tests`・`#[test] fn folder_filter_typing_resets_selection_to_first_row`（新規） | ずれ 2 を固定する純粋核テスト 1 本 |

**触らない**: `launcher_controller.rs`（実装・挙動不変）／`SPEC.md` §4・§6.1・§6.6・§6.7／`docs/`／`CLAUDE.md` 各種。

## 実装順序

### Phase 1 — SPEC §6.3 の加筆

`SPEC.md` §6.3 の既存 3 行の**後ろ**へ 2 行を足す。文言案:

```markdown
- **絞り込みが効くのは列挙に成功した候補に対してである**。列挙に失敗しているとき（§6.6）のエラー行は絞り込みの対象外で、打鍵しても候補行は変わらない（as-built）
- 絞り込みの打鍵は選択を 1 行目へ戻す（as-built）
```

文言の制約（逸脱したら書き直す）:

- **「打鍵しても何も起きない」と書かない** — 打鍵ごとに行の差し替え自体は走る（`rows_generation` は進む）。ユーザーが観測できる粒度＝「候補行は変わらない」で書く
- **`rows_generation` / `folder_error` / `set_folder_filter` などの実装識別子を書かない**（#885 / #899 / #902 が SPEC から落とした類）
- **到着時のクランプを再説しない**（§6.1:237 の所有）／**プレースホルダの消失を再説しない**（§6.7:276 の所有）／**エラー行からの復帰キーを再説しない**（§6.6:271 の所有）

### Phase 2 — 純粋核テストの追加

`search_state.rs` の `mod tests`、**`arriving_empty_rows_leaves_no_selectable_row`（`:873-882`）の直後**へ、独立した小ブロックとして追加する。

**#743 のブロック（`:785-882`）の内側へ入れてはならない** — 冒頭コメントが「この **5 本** が固定するのは…」と本数を名指ししており、内側へ足すとその記述が stale になる（本数を直すのは #743 の射程への介入になる）。

テストの骨格（AC 3・`:831-832` の教訓に従う）:

```rust
/// フォルダ内の絞り込みの打鍵は選択を 1 行目へ戻す（#838・SPEC §6.3 の as-built）。
/// **非ゼロから始める**——0 から始めると `enter_folder` の初期値と区別が付かない。
/// **`enter_folder` を先に通す**——`set_folder_filter` は `folder` が `None` でも
/// 黙って `selected = 0` を撃つため、突入を忘れると folder 中の挙動を何も実証しない。
#[test]
fn folder_filter_typing_resets_selection_to_first_row() {
    let mut s = SearchState::new();
    s.enter_folder("C:\\a".into());
    s.set_results(vec![res("a"), res("b"), res("c")]);
    s.move_selection(2);
    assert_eq!(s.selected(), 2);          // 前提: 非ゼロ
    s.set_folder_filter("b".into());
    assert_eq!(s.selected(), 0, "絞り込みの打鍵で先頭へ戻る");
    assert_eq!(s.view_kind(), ViewKind::Folder, "folder 中の挙動を測っている");
}
```

## 不変条件と異常系

- **挙動不変**: `src-tauri/src/**` の非テストコードに 1 行も差分を入れない（`git diff --stat` で確認する）
- **`res()` / `ViewKind` は同 `mod tests` のスコープに既存**（`:762` 等で使用中）— 新しい import は要らない
- 異常系は無い（文書とテストのみ）。テストが赤くなる場合、それは「実装が想定と違う」ことを意味するので、テストを緩めず調査へ戻る

## テスト方針と検証コマンド

| 対象 | コマンド | 根拠 |
|---|---|---|
| `search_state.rs`（カテゴリ A） | `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` | PostToolUse hook が自動実行（沈黙 = 合格） |
| 新規テスト単体 | `cargo test -p snotra folder_filter_typing` | Red→Green の確認 |
| `SPEC.md`（カテゴリ F） | `npm run governance:check` | **SPEC.md は hook の検査割り当てが無く沈黙 = 「何も走らなかった」**（ルート `CLAUDE.md`）ので手で回す |

**`--lib` を付けない** — `snotra` は `[lib]` を持たず `cargo test -p snotra --lib` は常に失敗する（`src-tauri/CLAUDE.md`）。

**カテゴリ C / D は不要**（明示的に落とす）: trace イベント名・hotkey 登録・表示経路・フォント登録のいずれにも触れず、実行コードの差分がゼロだから。

## SPEC.md・関連文書の更新要否

- `SPEC.md`: **要**（本タスクの本体）
- `.claude/rules/spec.md`「セクション番号整合」: **非該当**（箇条書きの追加のみで `###`／`##` の増減なし。確認済み）
- `docs/architecture.md` / 各 `CLAUDE.md` / `docs/adr/`: **不要**（アーキ・横断パターン・否定の知識のいずれも生じない）
- `RETROSPECTIVE.md`: 不要（サイクル末の `/retrospective` の管轄）

## フェーズごとの作業項目

### Phase 1 — SPEC §6.3

- [ ] `SPEC.md` §6.3 へ 2 行を追加する（上の文言案・文言の制約を満たすこと）
- [ ] §6.1 / §6.6 / §6.7 と読み比べ、同じ事実の二重記述が無いことを確認する（AC 2）
- [ ] `npm run governance:check` を実行する

### Phase 2 — 純粋核テスト

- [ ] `search_state.rs:882` の直後へ `folder_filter_typing_resets_selection_to_first_row` を追加する
- [ ] `cargo test -p snotra folder_filter_typing` で green を確認する
- [ ] `set_folder_filter` の `self.selected = 0;` を一時的に消して **red になること**を確認し、戻す（無効テストで緑になる罠の排除・`:793-794`）
- [ ] `git diff --stat` で非テストコードの差分が 0 であることを確認する（挙動不変・AC 3）

### Phase 3 — 仕上げ

- [ ] カテゴリ A（fmt / clippy / test）が緑であることを確認する
- [ ] コミット（`fix(spec): ...`）→ push → PR 作成（`Closes #838`）

## 未確定（実装前に潰す）

（なし）

## セルフレビュー

- リスク: **通常**
  - `/plan-review`「リスク判定」の 6 条件を実測で照合: 永続形式・設定キー・公開 API・状態遷移の変更なし／worker・channel・listener・共有状態・非同期の変更なし／hook・CI・rules・skills の変更なし／網羅性は要件でない（対象は §6.3 の 2 点に限定）／モジュール間インターフェースの新設なし／ユーザーの `--deep` 指定なし
  - 「ガバナンス文書」該当性の裁定: `SPEC.md` は**製品の意図（3 層分担の第 1 層）**であって、エージェントの行動を律する規範（`.claude/rules/`・skills・ルート `CLAUDE.md` / `AGENTS.md`）ではないため**非該当**とする。ただし `AGENTS.md` 条件別チェック表の `*.md` トリガーには当たるので `npm run governance:check` は実行する（上の検証コマンド表）
- plan-review: **未実施（通常リスク）**
- エージェント数: **0**
- 自己照合（`/plan-review` Step 1 の 7 項目）:
  1. issue の全要件に作業項目が対応 — AC 1 → Phase 1、AC 2 → Phase 1 の 2 項目め、AC 3 → Phase 2（**ただし固定できるのは 2 点のうち 1 点**・「明示した仮定」に明記）
  2. 変更ファイル・シンボルの実在 — `SPEC.md:247-251`・`search_state.rs:348-351` / `:873-882` を Read で実測
  3. 不変条件・異常系・テスト期待値の具体化 — 済（骨格コードと assert 文言まで確定）
  4. `SPEC.md` と関連文書の更新要否 — 済（上節）
  5. 未確定欄 — 空
  6. タスク分割の境界 — Phase 1/2 はどちらも独立にコンパイル・検査が通る（新 API 導入と呼び出し点移行のような分割不可の組は無い）
  7. 変更で偽になる散文 — `folder_filter` / 「絞り込み」/ 「列挙失敗」で grep 済み。`launcher_controller.rs:760` のコメント「列挙失敗行（filter 非適用）」と `search_state.rs:785-794` のブロック冒頭コメント（本数の名指し）が該当し、**前者は今回の SPEC 文言と一致するので無改変・後者は「ブロックの外へ足す」ことで stale 化を回避**する（Phase 2 に明記）
- 要対処: 0 件
- 未検証: **ずれ 1（エラー行の filter 非適用）に自動検知手段が無いこと**は受容する残余。理由は driver 側で `AppHandle` を要するため（`research.md`「技術的制約」）。腐り検知の錨は `launcher_controller.rs:760` の既存コメントのみ

## 人間レビュー

- [x] 承認済み — 2026-08-04 / 問い: "この計画で実装へ進んでよろしいですか？（`workspace/plan.md` へ直接の注釈も歓迎します）" / 回答: "承認する"
- [x] §6.6 の扱いを裁定 — 2026-08-04 / 問い: "ずれ 1（列挙失敗のエラー行は絞り込みの対象外）を §6.3 だけに書き、§6.6 は無改変とする計画です。この扱いでよろしいですか？" / 回答: "§6.3 だけに書く（計画どおり）"
  - → 上の「明示した仮定」1 点目が確定した。§6.6 への参照行の追加は行わない（実装を変える差分ではないため `/plan-review` の再実行は不要）
