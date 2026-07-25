# plan: #660 `snotra-egui-mvp` を除去する

**種別**: 挙動変更なし（削除 + 文書同期）。SPEC.md 更新は**不要**——`SPEC.md` に `mvp` 参照は 0 件（全リポジトリ grep で実測・plan-review の 3 体が独立に再確認）で、製品の意図に本 crate は現れない。

**ゴール**: ワークスペースから `snotra-egui-mvp` を消し、それを指す参照（マニフェスト・製品コード・hook・CI・ガバナンス文書）を残さない。**非ゴール**: 製品の挙動変更、`attach` への `#[must_use]` 付与、`pub` 露出の整理、`docs/superpowers/**`（歴史資料）の書き換え。

**本タスクの構造的性質（最重要）**: 当 crate は**被依存ゼロの leaf bin** ゆえ、通常最強の検知器＝コンパイラがほぼ全面的に盲になる。参照約 20 箇所のうち**機構が捕まえるのは 5 箇所だけ**（plan-review の独立再導出が `governance:check` の `runAll` へスナップショット注入して実測: G3 が 2 件・G5 が 3 件）。残りは手の列挙と最終 grep が唯一の担保である。

## フェーズと変更ファイル

### Phase 1 — 削除の前に教訓を抽出する（不可逆な順序）

`research.md` の 9 件棚卸しで孤立と判定した 2 件だけを移す。`git rm` より**前**に行う（消してから思い出すことはできない）。

1. **`PERFORMANCE.md`**（`## 計測と受け入れ基準`・:239-248 の箇条書き末尾へ 1 行追加）
   - warm frame の日次比較禁止。**一般則を主文に置き、実測を根拠として括弧に入れる**——「同一ホストでも warm frame は日によって 3 倍変わる。構成の比較は必ず同日・同条件で両方を測る（#532 Phase 1 の検証バイナリで実測: 2026-07-17 に 26-30ms、7/14 は 8-10ms）」。`standalone` の語は指示対象が消えるので使わない
   - 移設先に等価物が無いことは実測済み（「同日 / 日によって / 変動 / 再現性」grep が 0 件）
2. **`src-tauri/CLAUDE.md`**（新設 `## フォント登録（混在スクリプトのベースライン）`。**`## working set の能動回収` の後・`## Win32 / Tauri 注意事項` の前**——既存の並びで個別技術トピック節が集まる位置に合わせる）
   - 規則（jp_font / user_font の 2 枝）は `view.rs` の `font_definitions_*` 4 テストが機構化済みゆえ**繰り返さない**（テストは関数名で参照する。行番号は書かない——plan-review で :1651 が実際は :1697 とドリフトしていた）
   - 移すのは**因果と再発史**: softbuffer ラスタ（`fill_mesh`）はカバレッジ AA を持たず vertical metrics の分数差を整数 px へ丸めて顕在化させる（glow/wgpu は sub-pixel AA で吸収して隠す）／新規 bin を作るたび `push`（末尾 fallback）を再導入して再発した／型検査・clippy・単体テストを素通りし視覚スモークでのみ顕在化する
   - **#399 の原型は地の文で語り直さず `snotra-settings/CLAUDE.md` への参照に留める**（同じ事実を複数の `.md` に書かない・`docs/development-principles.md` の DRY）。受容残余は `SPEC.md` §フォントを参照

**不変条件**: 移設後、`warm frame` と「カバレッジ AA」の語が新しい場所で grep に掛かる（＝着地の確認）。Phase 2 の削除はこの確認後に行う。

### Phase 2 — crate 本体の削除とワークスペース整合

3. **`git rm -r snotra-egui-mvp/`**（追跡 7 ファイル: `.gitignore` / `CLAUDE.md` / `Cargo.toml` / `README.md` / `build.rs` / `src/main.rs` / `tauri.conf.json`）。**未追跡の `gen/schemas/*.json` 4 ファイル**（crate 内 `.gitignore` の `/gen/`）と `target/` 残骸は作業ツリーから消す
4. **`Cargo.toml`**: `members` から `"snotra-egui-mvp"` を削除（4 crate になる）
5. **`Cargo.lock`**: `cargo check --workspace` に再生成させ、**同一コミットに含める**（追跡ファイル）。**`tauri-plugin-global-shortcut` とその推移依存も落ちる**——当 crate が唯一の消費者だった（`tauri-plugin-updater` は `src-tauri/Cargo.toml:18` が使うので残る）。lock の diff が大きくなるのは想定内

**不変条件**: `members` の stale エントリは cargo の hard error＝**自己検知**。逆（members から外してディレクトリを残す）は `--workspace` では沈黙するため、削除とマニフェスト更新は同一ステップで行う。

### Phase 3 — mvp のためだけに残していた製品コードの後片付け

6. **`snotra-egui-runtime/src/runtime.rs`**
   - `RuntimeFrame::close_window()`(:34-36) と `close_requested` フィールド(:29) と `apply_frame_commands` の `if frame.close_requested { self.window.close()? }`(:391-393) を削除。構築リテラルは `:324-327` の 1 箇所のみ（`Default` 実装なし）で、そこも追随する。**根拠**: 呼び出し元は mvp の 1 件のみ（`close_requested` の全参照は `:29,35,325,391` に閉じる。`snotra-settings/src/app.rs:375` の `close_requested()` は egui の別 API で無関係）。`docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md:162` が「#660 の削除まで温存する」と予定地を当 issue に置いている。`RuntimeFrame` は `drag_requested` 単独フィールドになる（構造体は残す——`drag_window` が `view.rs:1118` のタイトルバードラッグで現役）
   - `WindowWaker` の doc コメント(:91-93): 「`#[must_use]` は付けない」の**方針は維持**し、mvp を名指す理由節だけを落として「wake が不要な窓では戻り値を捨ててよい」を理由に据え直す。**削除後は「捨てる呼び出し元」が実在しなくなる**（`src-tauri` は `mod.rs:256,269` で必ず束縛する）ため、理由を実例に依存させない書き方にする。属性の追加は API 契約の変更ゆえ範囲外
7. **`snotra-egui-runtime/src/raster.rs:3`**: `spike snotra-egui-mvp/src/soft_host_main.rs から移植` → 既に存在しないファイルを指す stale。「#532 Phase 1 スパイクから移植」へ書き換え（死んだパスを残さない）
8. **`snotra-core/tests/search_frame_cost.rs:10-13`**: 合成索引の由来を「スパイク（`snotra-egui-mvp` の `build_verification_engine`）と同系」→ crate 名とパスを外し「#532 Phase 1 スパイクの検証 Engine と同系」へ。生成規則の記述自体（固定 6 件 + 連番 + 履歴ブースト 5 件）はこのファイルが単独で説明しているので保持

**不変条件**: 6 の削除は「呼び出し元ゼロの分岐の消去」であり製品の実行経路を変えない。`drag_window` 経路は無傷。**検知**: `cargo test -p snotra-egui-runtime` / `-p snotra` + PR で自動起動する Smoke。
**この削除を今やる理由**: `pub fn` の死は `dead_code` が発火せず clippy `-D warnings` でも鳴らない——**今後どの機構も二度と浮上させない**。下流が 1 crate しかなく今それが消えるこの瞬間が、唯一の低コストな機会である（AGENTS.md「旧 API の削除は下流の compile-fail を移行漏れ検出器に」の精神）。**PR 本文には「MVP 専用 API の同時撤去」として独立の項目で書き、黙って混ぜない**。
**異常系**: 将来 `close_window` が必要になれば再追加は 3 行（履歴に残る）。今残す理由は「いつか使うかも」＝YAGNI 違反。

### Phase 4 — セーフティネット（hook / CI）の同期

**9 と 10 は同一ステップで直す**——`selectChecks` と期待値の中間不整合は、`.claude/hooks/**` 編集で自動発火する hook-selftest がその場で red になる。

9. **`.claude/hooks/post-edit.mjs`**: 出力予算 `"egui-mvp-test"`(:38) / `selectChecks` の `snotra-egui-mvp/` 分岐(:115) / `case "egui-mvp-test"`(:275-276) の 3 箇所を削除
10. **`.claude/hooks/post-edit.test.mjs`**:
    - :104-114 の `it` は mvp と egui-runtime を**同時に** assert している。`it` ごと消さず、mvp の `expect` ブロック(:109-112)だけを削り、タイトル(:104)を egui 接着層側の主張に改題する。**runtime の正例（`snotra-egui-runtime/src/lib.rs`）と負例（`snotra-egui-runtime/README.md`・:113）は残す**——元のテストが証明していた命題を孤立させない（AGENTS.md ステップ 4）
    - :602 の members カナリア期待配列から `"snotra-egui-mvp"` を削除
    - :617 の `REPRESENTATIVE_EDITS` から `"snotra-egui-mvp/src/main.rs"` を削除。**残しても沈黙する**（`ids ⊆ BUDGETS` の片方向検査）ため機構では拾えない＝手で消す。:613 の由来コメント（「egui-mvp-test / egui-runtime-test 追加時に実際に起きた」）は**過去の事実の記述で今も真ゆえ保持**
11. **`.github/workflows/ci.yml:85-86`**: `cargo test (snotra-egui-mvp)` ステップを削除

**不変条件**: 予算エントリと `selectChecks` の発行 id 集合は一致し続ける（片方だけ消すと、次に検査が失敗したとき hook が TypeError で落ち診断が届かない——:611-613 が記録する実際の事故）。**検知**: `npm test`。
**members カナリア（:577-607）が本タスクの錨である**: 実 `Cargo.toml` をディスクから読み厳密 5 件比較するため、4 の瞬間に red になり、失敗メッセージが更新先 4 箇所（`selectChecks` / `buildCommand` の case / `ci.yml` / `docs/build-commands.md`）を名指しする。**9〜11 + 14 がこれに 1:1 で写ることが完備性の無料の論証になる**。ガードを弱めずに済む＝これはフォールトインジェクションではなくガードの**行使**（`.claude/rules/safety-nets.md`）。

### Phase 5 — ガバナンス文書の同期（`governance:check` が閉じ役）

12. **`AGENTS.md`**: :10 の第2層列挙から `snotra-egui-mvp/src/*.rs` を削除／:20 の CLAUDE.md 列挙から `snotra-egui-mvp/` を削除。**どちらも行内編集に留める（行数を変えない）**——G10 の常時ロード予算は **216/216 行で余白ゼロ**（実測）。1 行を 2 行に割るだけで即 red になる
13. **`docs/architecture.md`**:
    - :11 のコメント「workspace（製品3 crate + egui検証2 crate）」→ 製品 4 crate へ（**件数の写し**）／:13「Tauri native Window向けegui/**wgpu**接着層」→ softbuffer（同じツリー図内で、削除する :14 の隣。SU1 以降 wgpu は不使用・`snotra-egui-runtime/Cargo.toml:11` は `softbuffer` のみ）／:14 の `snotra-egui-mvp/` 行を削除
    - :20 の散文末尾「`snotra-egui-mvp` は Issue #532 の採用判断に使った検証バイナリで…」の 1 文を削除（段落はその前の文で完結する）
    - :45-49 のレイヤー図「Issue #532 egui MVP（非配布）」ボックスを削除（`→ egui-wgpu/wgpu` の行も同時に消える）
    - :76 見出し「snotra-egui-runtime / snotra-egui-mvp（Issue #532 検証層）」→ `snotra-egui-runtime`（egui/softbuffer 接着層）へ改題（**見出し名・アンカーでの外部参照は 0 件**を実測済み）。:78 の散文から mvp 説明文と「**製品版へ統合する前の技術検証に限定し、release workflow の artifact には含めない**」を削除——runtime は `snotra.exe` に組み込まれて配布されるため、主語が runtime へ滑ると嘘になる。**同時に、書き換える当の文の `wgpu Surface／Device 復旧` を実装どおりに直す**: 再生成処理は存在せず（`recreate`/`reinit` は 0 件）、実体は「softbuffer Surface の描画失敗リトライ（指数バックオフ・上限 5 回=`MAX_PAINT_RETRIES`）」。:80 の参照先から `snotra-egui-mvp/CLAUDE.md` を削除（**G3 が拾う行**）
14. **`docs/build-commands.md`**（**7 箇所**）: :18 カテゴリ A のテスト行（**G5**）／:24「全 5 crate」→ 4／:25 の hook 発火パス列挙から `snotra-egui-mvp/**`／**:26「6 つ目の crate を追加したとき」→「5 つ目」**（件数の写し・plan-review の独立再導出が拾った漏れ）／:52 カテゴリ D の mvp 注記（`cargo run` に `-p` を付ける警告自体は**残す**——削除後も bin は `snotra` / `snotra-settings` の 2 本あり主張は真）／:87（**G5**）と :95（`cargo run -p` は G5 の対象外＝手で消す）／:128 CI/CD 対応表（**G5** +（11 を先にやれば）**G6**）
15. **`docs/development-principles.md:13`**: 責務集約先の列挙から `snotra-egui-mvp/CLAUDE.md` を削除（**G3 が拾う行**）
16. **`.claude/skills/retrospective/SKILL.md:65`**: 振り分け先 CLAUDE.md 列挙から `snotra-egui-mvp/` を削除

**フォールトインジェクション（`.claude/rules/safety-nets.md`「効いていることは一度は実測する」）**: **Phase 2 完了直後・Phase 5 着手前**に `npm run governance:check` を 1 回走らせ、**findings 5 件が期待どおりの行を指すこと**を確認する（`docs/architecture.md:80` と `docs/development-principles.md:13` = G3、`docs/build-commands.md:18` / `:87` / `:128` = G5）。稼働中のガードは無変更＝**ガードの行使**であり弱めていない。plan-review の独立再導出がメモリ上のスナップショット注入で同じ 5 件を先に実測しているので、ここでの観測はその再現確認になる。

**不変条件**: 消し忘れのうち **G3 が拾えるのは `.md` 拡張子付きパス参照のみ**。`snotra-egui-mvp/`（拡張子なし）・`snotra-egui-mvp/src/*.rs`（glob）・`cargo run -p`（G5 は `cargo test -p` だけを見る）・**件数の写し 3 件**・見出し・散文の表記ゆれ（「Phase 1 スパイク」「検証層」「独立検証バイナリ」）は**どの機構も検知しない**。ゆえに 12〜16 は `research.md` の列挙表を消化リストとして使い、最後に**全リポジトリ再 grep で 0 件**（`docs/superpowers/**` と `Cargo.lock` を除く）を確認する。`governance:check` の母集団数（対象文書 34 件 / rules 7 件 / skills 12 件）が削除後も同一であることも見る（母集団が痩せていない肯定的証拠）。

## 明示的に範囲外とするもの（別 issue 候補・plan-review で洗い出した）

| 対象 | 理由 |
|---|---|
| `attach` への `#[must_use]` 付与 | API 契約の変更。理由コメントの訂正（in-scope）とは別クラス。混ぜると diff が膨らむ典型経路 |
| `snotra-egui-runtime/src/runtime.rs:363,367` の `"...in the MVP runtime"` ログ文字列 | runtime **自身の成熟度**についての古い言い回しで、削除する crate への参照ではない（実物を読んで確認）。触ると無関係な文言へ diff が広がる |
| `docs/build-commands.md:17` の「Surface/Device復旧方針」 | `architecture.md:78` と同型の既存ドリフト（再生成処理は実装に無い）。**書き換える当の文の外**ゆえ広げない |
| `snotra-egui-runtime/CLAUDE.md` が G1 モジュール索引・G3 参照実在の母集団外 | 既存ギャップ。母集団を増やすと G1 の逆方向照合が 9 ファイル分の索引整合を要求し独立した検証が必要。**本 PR では指摘のみ** |
| `key_from_tao` / `modifiers_from_tao` / `is_renderable_extent` の過剰 `pub` | crate 内部と自テストが消費（mvp も使っていなかった）＝本 issue と因果関係のない既存事象 |
| `.claude/hooks/post-edit.test.mjs:619` の `"ui/src/App.tsx"` | SU7 フロント撤去の残り腐り。本 issue 由来でない |
| `src-tauri/CLAUDE.md` へ「宣言窓なし（`app.windows` は空）を保て」の規範を足す | 事実は `src-tauri/src/main.rs:259` のコメントと `tauri.conf.json:9-11` が既に記録済み。同じ事実を `.md` へ写すのは DRY 規約に反する（判断として記録） |

## テスト方針

**新規テストは追加しない。** 削除のみで挙動を変えないため。代わりに「削除で孤立する不変条件が無いこと」を `research.md` の 9 件表で先に示し、plan-review の 3 体（Rust 層・文書層・独立再導出）が独立に再確認した——**孤立 0 件**。消える 3 テストの帰着:

| 消えるテスト | 証明していた命題の行き先 |
|---|---|
| `verification_engine_searches_with_real_core_engine`（英語 / カタカナ / 漢字の 3 クエリ・`build_verification_engine(100)`） | `snotra-core/src/engine.rs`（`search_returns_matching_results` / `record_launch_and_search_boost` / `apply_prebuilt_index_rebuilds_kana_per_migemo`「ドキュメント」）+ `snotra-core/src/search/tests/migemo.rs` + `query.rs` のかな正規化テスト群。**`snotra-core/tests/search_frame_cost.rs` は行き先ではない**——`#[ignore]` の計測ハーネスで assert を持たない（plan-review の訂正） |
| `jp_font_is_registered_at_index_zero_for_both_families` | `src-tauri/src/egui_shell/view.rs` の `font_definitions_fallback_is_jp_single_stack`（#579 を名指しで固定）/ `font_definitions_honor_puts_user_first_jp_fallback` / `font_definitions_covered_user_font_omits_jp_entirely` / `font_definitions_registers_both_fonts_as_borrowed` の 4 テスト（SU4 の 2 枝仕様として mvp より詳細） |
| `updater_modes_preserve_install_capability_boundary` | 対象ごと消滅（`parse_update_mode` は `SNOTRA_EGUI_MVP_UPDATE_MODE` の env パーサ。製品は `config.toml` を serde で読む別経路で、対応する手書きパーサを持たない）。なお製品の `can_install = mode == Full` の**導出側**に単体テストが無いのは**本 issue が作るギャップではなく既存のギャップ**（対処は範囲外） |

**検証コマンド**（`docs/build-commands.md` カテゴリ A + セーフティネット）:

```
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo test -p snotra-egui-runtime
cargo test -p snotra
cargo test -p snotra-settings
cargo doc --workspace --no-deps --document-private-items
npm test
npm run governance:check
```

- `cargo doc` は **hook が発火しない＝沈黙は合格でない**（#562）。ただし `[snotra_egui_mvp` 形式の intra-doc link は全リポジトリ 0 件（実測）で、`raster.rs:3` の壊れたパス参照が今日まで一度も鳴っていないことが「バッククォート内の疑似パスは link ではない」ことの一次証拠。**それでも実行する**（削除で新たに切れる link が無いことの接地）
- `clippy` で `drag_requested` 単独フィールド化に新規 lint が出ないかを確認する（読み取り調査では既知の lint は無いが実測で決める）
- **カテゴリ C / D の扱い**: 手動 GUI 視覚スモーク（D）は非該当——製品の描画コードを 1 行も変えない（`close_window` は呼び出し元ゼロの分岐）。自動 smoke は **PR で勝手に走る**: root `Cargo.toml` と `Cargo.lock` が `e2e.yml` の paths（`'**/Cargo.toml'` / `'Cargo.lock'`）に合致し `smoke-egui` job が起動する。緑を確認する
- 最終確認として `snotra[-_]egui[-_]mvp|egui[ -]?mvp|SNOTRA_EGUI_MVP|MVP` の全リポジトリ grep（大文字小文字無視）が `docs/superpowers/**`（歴史資料・意図的に保持）・`Cargo.lock`・`runtime.rs:363,367`（範囲外と決めたログ文言）以外で 0 件

## コミット方針

**1 コミット・1 PR**（Phase 1〜5 を分けない）。理由: `members` の削除と参照の削除は互いに前提であり、中間状態はどれも `cargo check` か `npm test` か `governance:check` のいずれかが red になる。Phase 5 前のフォールトインジェクション観測は**コミット前の作業ツリー**で行う（コミットは全 green を確認してから）。

PR 本文には (a) crate 削除、(b) **MVP 専用 API（`close_window`）の同時撤去**、(c) 教訓 2 件の移設、(d) 文書・hook・CI 同期 を独立の項目として書く。`Closes #660` を書き、`closingIssuesReferences` の確認手順（ルート `CLAUDE.md`）をマージ時に適用して **#532 が一覧に混ざらないこと**を確認する（本文で #532 に言及するときは closing keyword を伴わせない）。

## セルフレビュー

### Step 5a — `/plan-review`（Explore 3 体 + 独立再導出 1 体）

**要対処として反映した漏れ**（導出 ∖ plan）:

1. `docs/build-commands.md:26`「6 つ目の crate を追加したとき」——**件数の写し**。当初計画は :24 の「全 5 crate」しか挙げていなかった → 項目 14 に追加（6 箇所 → 7 箇所）
2. `docs/architecture.md:13` の `egui/wgpu 接着層`——削除するツリー図行(:14)の隣で同じ嘘 → 項目 13 に追加
3. `docs/architecture.md:78` の「release workflow の artifact には含めない」——主語が mvp から runtime へ滑ると嘘になる（runtime は製品バイナリに入る）→ 削除対象として明記
4. `docs/architecture.md:78` の「Device 復旧」——`wgpu → softbuffer` の置換だけでは**実装に無い挙動の記述が残る**（再生成処理は 0 件・実体は上限 5 回のリトライ）→ 書き換え文言を実装どおりに指定
5. `post-edit.test.mjs` の編集範囲を :104-112 → **:104-114**（`snotra-egui-runtime/README.md` の負例 :113 を残すことを明記）
6. **`view.rs` の font テストの行番号ドリフト**（計画の :1651-1694 は実際 :1697-1756・テストは 3 本ではなく **4 本**）→ 行番号を捨てて関数名参照へ。教訓として「移設先・引用先は行番号で書かない」を Phase 1 に明記
7. **帰着表の誇張**: `search_frame_cost.rs` を「同型の合成索引を持つテスト」として命題の行き先に挙げていたが `#[ignore]` の計測ハーネスで assert を持たない → 行き先を `engine.rs` / `migemo.rs` / `query.rs` の実テストへ訂正
8. **G10 の常時ロード予算が 216/216 で余白ゼロ** → `AGENTS.md` の編集は行内に留める制約を項目 12 に明記
9. `Cargo.lock` から `tauri-plugin-global-shortcut` + 推移依存が落ちる（当 crate が唯一の消費者）→ 項目 5 に明記
10. **`#399` を新設節で地の文再述しない**（DRY）・**新設節の位置を `working set の能動回収` の後へ**・**warm frame 移設文で `standalone` を使わない** → Phase 1 に反映

**スコープ過剰（plan ∖ 導出）**: 検出なし。`close_window` 削除は独立再導出も「含めるべき（ただし PR 本文に独立項目として書く）」と結論——一致。

**一致（盲点が無いことの能動的証拠）**: 参照の全数（B〜E 表）・G1 編集不要・`governanceDocs` が mvp CLAUDE.md を含まない・G3 述語の限界（`.md` 拡張子のみ）・不変条件 9 件の帰着（孤立は warm frame の 1 件のみ）・テスト 3 件の帰着・`.claude/rules` の glob に mvp 対象なし（G7 無影響）・`.github/workflows` は ci.yml のみ・`scripts/` は 0 件・`SPEC.md` / `README*` / `CONTRIBUTING.md` / `RETROSPECTIVE.md` / `docs/adr` / `docs/hooks.md` は 0 件——いずれも 3 体が独立に再確認して一致した。

**要対処なし**と 3 体が結論（Rust 層・セーフティネット層・文書層）。

### Step 5b — plan-review が扱わない 3 観点

1. **境界条件**: 「削除の中間状態」が境界である。(a) crate 削除 + members 未更新 → cargo hard error、(b) members 更新 + ディレクトリ残置 → 沈黙（ゆえに同一ステップ）、(c) hook 分岐削除 + テスト期待値残置 → hook-selftest red、(d) 予算エントリ残置 → **沈黙**（片方向検査ゆえ手で消す）、(e) 文書側だけ修正 → G5/G9 red、(f) ci.yml だけ修正 → G6 は逆方向を見ないので沈黙するが cargo が hard error。各ケースの検知手段を上に明記した
2. **シンパル化の挑戦**: 新しい状態・抽象は 1 つも導入しない（純減）。むしろ `RuntimeFrame` のフィールドが 2 → 1 に減り、公開 API が 1 つ減る。教訓の移設先も既存節への追記 1 行 + 新設節 1 つに留め、新しい文書は作らない
3. **破壊不変条件 + 検知手段**:
   - **製品の起動・表示経路が壊れない** → 検知: `cargo test -p snotra` / `-p snotra-egui-runtime` + PR 自動起動の `smoke-egui`（`smoke:startup` + `smoke:egui -RequireResults`）
   - **PostToolUse hook が検査を配送し続ける**（沈黙 = 合格の前提が壊れない）→ 検知: `npm test`（members カナリア + `selectChecks` ルーティング + 予算完全性）
   - **ガバナンス文書に宛先のない参照が残らない** → 検知: `npm run governance:check` findings 0 + 母集団数不変 + 最終 grep 0 件
   - **削除で失う知識が 0 件** → 検知: 移設後の grep 着地確認（Phase 1 の不変条件）と `research.md` の 9 件表
   - **どの機構も見ない残余**: 件数の写し・散文の表記ゆれ・見出し・doc コメント内の疑似パス。**これらは人の目と最終 grep だけが担保する**——受容する残余として明記する
