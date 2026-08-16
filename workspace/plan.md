# plan — issue #985: `temp_dir` テストヘルパーのプロセス一意化（残り 5 件）

調査は `workspace/research.md`。ブランチ: `fix/temp-dir-process-unique`。

## 目的

`snotra-core/src` のテスト用一時ディレクトリ名を**プロセス間で一意**にし、テストバイナリが複数プロセスに分かれて走る状況（`cargo test` と `cargo test --release` の重なり・別 worktree での並行実行）で、片方の `remove_dir_all` がもう片方へ割り込むことによる**コード変更と無関係な panic** を消す。

## 受け入れ条件

1. `binfmt.rs` / `config.rs` / `folder.rs` / `history.rs` / `window_data.rs` の各テスト用ディレクトリ名に `std::process::id()` が含まれる。
2. `indexer.rs` を含む `snotra-core/src` の 6 ヘルパーが**同じ形**（`{prefix}_{tag}-{pid}`）になる。
3. 各モジュールに、名前へ pid が入っていることを完全一致で pin する検知器がある。
4. `cargo test -p snotra-core` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check` がすべて green。

## 実装判断（issue が「どちらでもよい」とした 3 点＋1 点）

| 論点 | 決定 | 理由 |
|---|---|---|
| 区切り文字 | **`{prefix}_{tag}-{pid}`（pid の前だけ `-`）** | #982 の `indexer.rs` の逐語形。既に検知器つきで固定されている側へ 5 件を寄せるほうが、修正済みの 1 件を書き換えるより差分が小さく、検知器の期待値も動かさずに済む。**達成する一貫性は `snotra-core/src` の 6 ヘルパーの範囲**であり、リポジトリ全体ではない（`src-tauri/src/icon.rs` は `_{pid}` 形で、本 issue の射程外・変更しない） |
| 共有ヘルパーへ寄せるか | **寄せない** | issue が「寄せずに 5 か所へ pid を足すだけでも閉じる」と明記。寄せるには `#[cfg(test)]` の `pub(crate)` モジュール新設＋prefix 引数の追加が要り、さらに片付け作法（末尾 `remove_dir_all` の有無）がモジュール間で揃っていない差を先に整理する必要がある（`research.md`「技術的制約」）。size:S の射程を超える |
| 検知器を 5 件置くか | **置く（5 モジュールへ 1 本ずつ）** | issue の撤去条件「5 件すべてがプロセス一意」を保つ機構が他に無い。prefix がモジュールごとに違うため 1 本へ圧縮できず、DRY 違反にはならない。`indexer.rs:2579` の形を写す |
| `folder.rs` の prefix `snotra_test_` を `snotra_folder_test_` へ揃えるか | **揃える**（ユーザー判断・2026-08-16） | 他の 5 モジュールはすべて `snotra_<module>_test_` 形で、folder だけがモジュール名を落としている。**意図的な区別ではないことを導入コミットで確認した**——`snotra_test_` は `65d34e39`（Tauri 全面移行）由来で、`snotra_<module>_test_` 形を確立した `ca9a0f72`（#74）**より古い**。`folder.rs` の `//!` にも言及なし。「片方だけが変わる将来」（`AGENTS.md`「検証の作法」）を挙げられないため概念ではなく事故と判定。触る行は Phase 1 / Phase 3 で既に触るので追加費用はほぼ 0 |
| `folder.rs` の直書き固定名（issue 表の外） | **射程に入れる** | 同一ファイル・同一欠陥クラス（共有 `%TEMP%` 上の固定名）で 1 行。`AGENTS.md`「バグ発見時は同一パターン全コードパス検索を行う」の列挙結果を差分に反映する。ただし**この 1 件は issue の撤去条件には含まれない**ので、外しても issue は閉じる（人間レビューで外す判断があればそれに従う） |

## 変更ファイルと対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/src/binfmt.rs` | `tests::temp_dir` | 名前を `snotra_binfmt_test_{tag}-{pid}` へ／doc 1 行追加 |
| `snotra-core/src/binfmt.rs` | `tests::temp_dir_name_contains_process_id`（新規） | 検知器 |
| `snotra-core/src/config.rs` | `tests::temp_dir` | 名前を `snotra_config_test_{tag}-{pid}` へ／doc 1 行追加 |
| `snotra-core/src/config.rs` | `tests::temp_dir_name_contains_process_id`（新規） | 検知器 |
| `snotra-core/src/folder.rs` | `tests::temp_dir_with_contents` | 名前を `snotra_folder_test_{tag}-{pid}` へ（**prefix も揃える**）／doc 1 行追加 |
| `snotra-core/src/folder.rs` | `tests::list_folder_nonexistent_dir_returns_empty` | 直書き名へ pid を付す |
| `snotra-core/src/folder.rs` | `tests::temp_dir_name_contains_process_id`（新規） | 検知器 |
| `snotra-core/src/history.rs` | `tests::temp_dir` | 名前を `snotra_hist_test_{tag}-{pid}` へ／doc 1 行追加 |
| `snotra-core/src/history.rs` | `tests::temp_dir_name_contains_process_id`（新規） | 検知器 |
| `snotra-core/src/window_data.rs` | `tests::temp_dir` | 名前を `snotra_window_test_{tag}-{pid}` へ／doc 1 行追加 |
| `snotra-core/src/window_data.rs` | `tests::temp_dir_name_contains_process_id`（新規） | 検知器 |

**触らない**: `snotra-core/src/indexer.rs`（#982 で修正済み・形の正本）、`snotra-core/src/config.rs:3824`（`snotra-dedup-{pid}`・既に pid あり）、`snotra-core/tests/search_frame_cost.rs`、`src-tauri/src/icon.rs`（別 crate・既に pid あり・区切りは `_`）。

## 実装順序

### Phase 1 — 5 ヘルパーへ pid を足す

`format!` の引数を 1 つ増やすだけ。各ヘルパーの doc は **1 行**だけ足し、理由の本文は `indexer.rs` の `temp_dir` を正本として参照する（写しを増やさない・`AGENTS.md`「文書に事実の写しを増やす変更」）。

```rust
/// テスト用の作業ディレクトリを作り直して返す。
/// 名前に `std::process::id()` を含める理由は `indexer.rs` の `temp_dir` の doc を正本とする（#978 / #985）。
fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("snotra_binfmt_test_{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
```

- [ ] `binfmt.rs::temp_dir` → `snotra_binfmt_test_{tag}-{pid}`
- [ ] `config.rs::temp_dir` → `snotra_config_test_{tag}-{pid}`
- [ ] `folder.rs::temp_dir_with_contents` → `snotra_folder_test_{tag}-{pid}`（**prefix も `snotra_test_` から改める**。他 5 モジュールの `snotra_<module>_test_` 形へ合わせる）
- [ ] `history.rs::temp_dir` → `snotra_hist_test_{tag}-{pid}`
- [ ] `window_data.rs::temp_dir` → `snotra_window_test_{tag}-{pid}`

### Phase 2 — 検知器を 5 本置く

`indexer.rs:2579` の `temp_dir_name_contains_process_id` を各モジュールへ写す（prefix と tag だけ変える）。**完全一致**で pin することで、区切り文字の揺れも同時に捕まえる。

```rust
#[test]
fn temp_dir_name_contains_process_id() {
    let dir = temp_dir("process_unique");
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("temp dir name");
    assert_eq!(
        name,
        format!("snotra_binfmt_test_process_unique-{}", std::process::id()),
        "作業ディレクトリ名に自プロセスの pid が入っていない（#985）"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
```

タグ `process_unique` は 5 モジュールとも既存タグと衝突しない（`research.md` のタグ全列挙で確認済み）。

- [ ] `binfmt.rs` に検知器を追加
- [ ] `config.rs` に検知器を追加
- [ ] `folder.rs` に検知器を追加（呼ぶのは `temp_dir_with_contents`・期待値は `snotra_folder_test_process_unique-{pid}`）
- [ ] `history.rs` に検知器を追加
- [ ] `window_data.rs` に検知器を追加

### Phase 3 — `folder.rs` の直書き固定名（**issue 表の外・人間レビューで外せる**）

`list_folder_nonexistent_dir_returns_empty` の `snotra_test_nonexistent_zzz` を `format!("snotra_folder_test_nonexistent_zzz-{}", std::process::id())` にする（prefix も Phase 1 と揃える）。

**根拠を Step 2b の指摘で訂正した**: これは #985 の欠陥クラス（`remove_dir_all` が `create_dir_all` に割り込む）**ではない**——このテストは `create_dir_all` も `remove_dir_all` も呼ばない読み取り専用である。実際の危険は別で、**「存在しない」ことを共有 `%TEMP%` 上の固定名に期待している**点にある（誰かが同名を作れば落ちる）。pid を入れるとその可能性を構造的に潰せる。

- [ ] `list_folder_nonexistent_dir_returns_empty` の直書き名へ pid を付す（行番号は動くのでシンボル名で探す）

### Phase 4 — 検証

- [ ] `cargo test -p snotra-core` が green
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` が green
- [ ] `cargo fmt --all -- --check` が green
- [ ] **変異注入で検知器が発火することを確かめる**: 1 モジュール（`binfmt.rs`）で `format!` から `-{pid}` を一時的に外し、`cargo test -p snotra-core` が**そのモジュールの検知器で赤になる**ことを実測してから戻す（`AGENTS.md`「検知器を置き、呼び忘れを再現する変異で落ちることまで確かめる」）。**「変異なし＝緑」だけを証拠にしない**
- [ ] 全数の再照合: `git grep -n "env::temp_dir" -- '*.rs'`（除外句なし）を実行し、`snotra-core` 側の hit がすべて `std::process::id()` を含むことを目視で 1 行ずつ確認する

## 不変条件と異常系

- **不変条件 1**: 各ヘルパーが返すディレクトリは、同一マシン上の別プロセスが同時に走っても衝突しない。→ 検知器（Phase 2）が名前の形を pin する。
- **不変条件 2**: 挙動は変わらない。テストの assert 内容・対象コードには一切触れない。→ `cargo test -p snotra-core` の**成功テスト数が変わらないこと**（新規検知器 5 本の増加を除く）で確認する。
- **異常系**: `create_dir_all` が失敗すれば従来どおり `expect` で panic。変更前と同じ。
- **不変条件 3（新しい衝突を生まないこと）**: `-{pid}` の付与で新たな同名衝突は生まれない。衝突には `{tag_a}-{pid_a} == {tag_b}-{pid_b}` が必要だが、pid は純粋な数字列で同一プロセス内では `pid_a == pid_b` ゆえ `tag_a == tag_b` に帰着し、既存タグはモジュール内で相異なる（`research.md` のタグ全列挙）。なお `remove_dir_all` は完全パスに対して働くため、**接頭辞関係**（`snotra_test_basic` と `snotra_test_basic_x`）では巻き添えは起きない。
- **残余（消えないもの）**: pid は OS が再利用するため、古い同 pid のディレクトリを次のプロセスが `remove_dir_all` で消す。これは**同じディレクトリを同時に使う 2 プロセスが居ない**限り無害で、本 issue の解決を妨げない。`%TEMP%` に残骸が溜まる点も変更前後で同じ（3b が pid 付きの `indexer` 由来ディレクトリ 30 件以上の残存を実測した。**pid の付与はゴミの蓄積を減らさない——むしろ名前が毎回変わるぶん残骸の種類は増える**）。本 issue の撤去条件に含まれないため射程外とし、PR 本文にも「解決した」とは書かない。

## テスト方針と検証コマンド

TDD の Red は**変異注入**で取る（Phase 4）: 検知器を先に書けば pid が無い現状で赤になり、Phase 1 の修正で緑になる。実装順は Phase 1 → 2 の順に書くが、Phase 4 の変異で「検知器が本当に落ちる」ことを事後に実測する。

```
cargo test -p snotra-core
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## `SPEC.md`・関連文書の更新要否

- **`SPEC.md`: 更新不要。** テストヘルパーの命名は `SPEC.md` の射程（プロダクトの意図・挙動）に無い。挙動変更なし。
- **`snotra-core/CLAUDE.md`: 更新不要。** ファイルの追加・削除が無く、モジュール索引は変わらない。
- **`docs/`: 更新不要。** `temp_dir` の命名規約を持つ規範文書は存在しない（`research.md`「技術的制約」・.md の hit はすべて `docs/superpowers/plans/` 配下の過去計画＝履歴）。
- **`npm run governance:check`: 実行不要**（ガバナンス文書・`.rs` の見出し参照のいずれも触らない）。ただし PR では CI が常時実行するため、赤が出たら従う。

### `AGENTS.md`「条件別チェック」の該当判定

| トリガー | 該当 | 根拠 |
|---|---|---|
| 対称ペア | ✗ | `remove_dir_all` / `create_dir_all` の対は既存のまま。ペアの構造を変えない |
| 永続形式・識別子/キー形式を変更 | ✗ | `%TEMP%` の作業ディレクトリ名はディスク上の**永続**形式ではない（毎回作り直す使い捨て）。`index.bin` / `config.toml` / history キーのいずれにも触れない |
| 関数・型を新規定義／改名／導入 | △ | 新規は `#[cfg(test)]` の `#[test]` 関数 5 本のみ。呼び出し元は存在しない（test harness が呼ぶ）。改名・削除は無し。→ `/dry-check` は不要（5 本は prefix が異なり圧縮不能） |
| 並行性（worker・channel・listener 等） | ✗ | スレッド・チャネル・async を追加しない |
| 網羅性が要件 | ✓ | **5 件すべて**が要件。→ `research.md` で母集団を除外句なしの 2 経路 grep で確定済み＋ Phase 4 で再照合。3b の敵対的調査が独立再導出に当たる |
| セーフティネットを新設/変更 | ✗ | hook・CI・`.githooks/`・rules・skills・規範のいずれも触らない。検知器はテストであってセーフティネット機構ではない |
| ガバナンス文書を変更 | ✗ | `.md` を変更しない |
| ファイル（`.rs`）を追加/削除 | ✗ | 追加・削除なし |
| 件数 N・上限パラメータ・導出の入力を変更 | ✗ | 該当なし |

→ 該当する check スキルは無し。「網羅性が要件」に対する `/plan-review`「Step 2b」（独立再導出）は **Step 3b の敵対的調査 1 体**が担う。

## レビューへの申し送り（`/implement` の code-reviewer へ渡す）

**「重複に見えるが意図的に分けた構造」を先に渡す**（ルート `CLAUDE.md`「サブエージェント委譲と worktree」・#872）。渡さないと DRY 違反として必ず挙がり、採否の判断に毎回コストが乗る。

1. **検知器 5 本はほぼ同型だが圧縮できない。** 各モジュールの prefix（`snotra_binfmt_test_` / `snotra_config_test_` / `snotra_folder_test_` / `snotra_hist_test_` / `snotra_window_test_`）が異なり、ヘルパー自体が `#[cfg(test)] mod tests` のモジュール私的関数なので、共通化には `pub(crate)` の新設が要る。**意図的な分離である。**
2. **各ヘルパーの doc 1 行は写しではなく参照である。** 機序の全文は `indexer.rs::tests::temp_dir` の rustdoc が正本で、5 か所はそこを指すだけ（`AGENTS.md`「文書に事実の写しを増やす変更」に従った形）。「説明が薄い」は指摘として成立しない。
3. **`folder.rs` の prefix 変更（`snotra_test_` → `snotra_folder_test_`）は意図的である。** 他 5 モジュールの `snotra_<module>_test_` 形へ揃えるためで、`snotra_test_` が意図的な区別でないことは導入コミット `65d34e39` が規約確立の `ca9a0f72`（#74）より古いことで裏取り済み。ユーザーが明示的に採否を決めた（2026-08-16）。「issue に無い改名」としての指摘は不要。
4. **Phase 3（`folder.rs` の直書き固定名）は issue の表の外である。** 同一パターン全コードパス検索の結果を差分へ反映したもので、人間レビューで承認された意図的なスコープ。「issue に無い変更」としての指摘は不要。

## 未確定（実装前に潰す）

- [x] 母集団（`research.md` の 11 hit）が `.rs` 全数か — **潰した**。3b が独立に 2 grep を取り直し、加えて `TempDir` 型 / `CARGO_TARGET_TMPDIR` / `env::var("TEMP"|"TMP"|"TMPDIR")` / `GetTempPath` / `dirs::` を個別検索して 0 hit。**壊れなかった**
- [x] タグがモジュール内で相異なるか — **潰した（訂正あり）**。`config` は 9 ではなく **12**（`load_from_dir_repairs_and_saves_invalid_hotkey` のループが動的に 3 タグを作る・`config.rs:3349` を自分で読んで確認）。12 個は相異なるので結論は変わらず、検知器タグ `process_unique` とも衝突しない
- [x] 欠陥が実在するか（理論だけでないか） — **潰した**。テストバイナリを 2 プロセス同時に起動し、pid なしの `binfmt::` で赤（`exit=101` / `20 passed; 1 failed`）を**自分で直接観測**した。pid ありの `indexer::` は同条件 13 ラウンドで赤なし。詳細と 3b との数の食い違いは `research.md`「欠陥の実測」
- [x] `folder.rs` の直書き固定名（issue 表の外）を射程に入れるか — **入れる**と決定。同一ファイル・同一欠陥クラスで 1 行、`AGENTS.md`「バグ発見時は同一パターン全コードパス検索を行う」の列挙結果を差分へ反映する。**issue の撤去条件には含まれない**ので、人間レビューで外す判断があれば Phase 3 ごと削除する（判定で分岐する作業を作業項目に残さないため、既定は「入れる」に固定してある）

## 人間レビュー

- [x] 承認済み — 2026-08-16 / 問い: "この内容で承認いただけますか。承認後に `workspace/` をコミット・push し、実装は `/implement` へ渡せます。" / 回答: "承認。Phase 3 も入れて進めて"

**注釈として反映した判断**（同日・逐語: "folder.rs の prefix もそろえたほうがいいと思う。あとから見たときにprefix の違いに何か意味があるかも、と考える人もいそうだから。どう思う？"）: `folder.rs` の prefix を `snotra_folder_test_` へ揃える（「実装判断」表・「レビュー後に入った差分」節）。

## plan-review 結果

- **リスク: 高**（`/plan-review`「リスク判定」の「網羅性そのものが要件である」に該当。issue の撤去条件が「5 件すべて」）
- **レビュー方式: 独立導出1体**（Step 2b。`workspace/plan.md` / `research.md` / `adversarial-985.txt` を読ませず、issue の WHAT だけを渡してコードから独立に導出させた）
- **エージェント数: 1**（Step 2b）／本 issue 全体では 2（Step 3b の敵対的調査 1 体を含む）
- 成果物: `workspace/plan-review-temp-dir-pid.md`

### Step 1（主エージェント自身の照合）

| # | 項目 | 結果 |
|---|---|---|
| 1 | issue の各要件に作業項目がある | ✅ 5 ヘルパー → Phase 1 の 5 項目。「5 件で揃える」→ 形を 1 つに固定。3 つの判断点 → 「実装判断」表 |
| 2 | 変更ファイル・シンボルが実在する | ✅ `git grep "fn temp_dir"` で 5 ヘルパー、`temp_dir_name_contains_process_id` は `indexer.rs` にのみ存在（新規 5 本と衝突しない） |
| 3 | 不変条件・異常系・テスト期待値が具体化されている | ✅ 不変条件 1〜3 ＋ 残余、検証は Phase 4（変異注入を含む） |
| 4 | `SPEC.md` と関連文書の更新要否が正しい | ✅ 更新不要。Step 2b が独立に 8 母集団を当てて同結論（最強の一次証拠: PR #982 の同型修正は `indexer.rs` 1 ファイルのみ・文書 0 枚） |
| 5 | 未確定欄に未チェック項目が残っていない | ✅ 4 件すべて `- [x]` |
| 6 | タスク分割の境界がトリガーを跨いでいない | ✅ 新 API の導入・移行を伴わないため `dead_code` の中間状態は生じない。Phase 1 と Phase 2 の間で `-D warnings` が壊れる経路は無い |
| 7 | 変更で偽になる散文を含むファイルが一覧に載っている | ✅ ヘルパー名 5 種を逐語で持つファイルは対象 5 ファイル以外に 0 件。概念ラベル「一時ディレクトリ / 作業ディレクトリ」の hit は `.claude/rules/safety-nets.md`（hook の変異テストの話・無関係）と `indexer.rs`（正本・変更しない）のみ |

### 要対処

- **タグの数え上げの根拠が誤っていた** — `research.md` を修正済み。リテラル一致の grep が動的生成タグを落としており、`config` は 9 → **12**、`folder` は 11 → **20**。**結論（モジュール内で相異なる）は両方とも生き残る**ため計画の作業項目は変わらない。再照合の根拠: `config.rs:3349` のループと `folder.rs:460` / `:532` の `format!` を自分で読んだ。根拠の作り方を「タグの数」から「**ヘルパーが 1 か所であること**」へ差し替えた
- **Phase 3 の根拠が誤っていた** — 計画を修正済み。`list_folder_nonexistent_dir_returns_empty` は `create_dir_all` も `remove_dir_all` も呼ばない読み取り専用で、#985 の欠陥クラス（remove が create に割り込む）には**該当しない**。実際の危険は「共有 `%TEMP%` 上の固定名に不在を期待している」点。再照合の根拠: `folder.rs:355-361` を読み、FS への書き込みが無いことを確認
- **区切り文字の選択が先例間の割れを解消しない件** — 計画の判断は維持する。Step 2b が「`_` を選ぶと唯一の検知器を持つ `indexer.rs` とその完全一致アサーションも同時修正が必要」と実測で示しており、`-` を選ぶ計画の理由（差分が小さく検知器の期待値を動かさない）を裏づけている

### 軽微（採らない・理由つき）

- ~~**`folder.rs` の prefix `snotra_test_` だけがモジュール名を含まない**（Step 2b の B2）~~ → **要対処へ昇格・採用**（2026-08-16 ユーザー判断）。判断の根拠と導入コミットの裏取りは「実装判断」表を参照
- **`snotra-core/CLAUDE.md` へ規約を書き足す案**（B4）。正本は `indexer.rs::tests::temp_dir` の rustdoc であり、書くと写しが増える（`AGENTS.md`「文書に事実の写しを増やす変更」）。→ 書かない
- **共有ヘルパーへ寄せる案**（B5）。Step 2b は「片付け作法は 5 件とも均一で技術的には可能」と報告したが、issue が「寄せずに閉じる」と明記し、置き場所の新設を要する。→ 寄せない（「実装判断」表のとおり）
- **`binfmt.rs` が固定 tmp 名（`data.bin.tmp`）での tmp→rename を直接叩くテストを 14 本持ち、5 件中で最も危険**（A7）。作業内容は変わらないが、**自分の A/B 実測で赤が出たのが実際に `binfmt` だった**ことと整合する。→ Phase 4 の変異注入の対象を `binfmt.rs` にしてあるのはこの理由でもある

### 未検証（受容する残余）

- **`folder.rs` / `config.rs` / `history.rs` / `window_data.rs` での並行再現は測っていない**（`binfmt.rs` でのみ赤を観測）。5 件は同一の 4 行の形で、修正も一律なので、再現の有無で分岐する作業は無い
- **CI で実際に衝突が起きるか**は未測定。3b は「windows-latest は使い捨て VM ゆえ CI 同士の衝突は起きにくく、現実的なトリガーは同一開発機での並行実行」と述べ、確信度は低いと申告した。self-hosted runner の有無は双方未確認。**issue を閉じる判断はこれに依存しない**（欠陥の実在は開発機で直接観測済み）
- **`%TEMP%` の残骸蓄積**は本修正で解決しない（「不変条件と異常系」の残余節）。射程外

### レビュー後に入った差分（2026-08-16）

`folder.rs` の prefix を `snotra_test_` → `snotra_folder_test_` へ揃える変更をユーザー判断で採用した。**追加の `/plan-review` は実行しない。** 理由:

1. Step 2b の再実行条件は「issue の要件、または変更対象のファイル・シンボル集合が変わった場合」（`/plan-review`「Step 2b — 独立導出による網羅性レビュー」）。**どちらも変わらない**——同じ `temp_dir_with_contents`、同じ新規検知器、変更ファイル 5 枚も不変。変わるのは `format!` の文字列リテラル 1 つと、それを写す新規検知器の期待値だけである。
2. **この案は Step 2b 自身が B2 として挙げた所見である。** 未レビューの新規差分ではなく、レビューが出した所見の採用であり、再レビューは同じ枠組みの再実行になる（`AGENTS.md`「レビュー指摘へ修正を当てた」が求める再実行は**修正差分に対する別枠組みの検算**であり、それは Phase 4 の変異注入と `cargo test` が担う）。
3. 意図の裏取りは主エージェントが一次証拠で実施済み（導入コミット `65d34e39` が規約確立の `ca9a0f72`（#74）より古いこと・`folder.rs` の `//!` に言及が無いこと）。

### 判断

- **実装着手: 可**（人間の承認後）
