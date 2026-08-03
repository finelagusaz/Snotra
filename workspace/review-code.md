# code-reviewer: Phase A（#825 + #819 案 A）

対象: 作業ツリーの `git diff`（`workspace/plan.md` を除く 5 ファイル）。ブランチ `chore/stale-clear-color-claim`・コミット前。
**読み取り専用で実施した**（本ファイル以外を編集していない）。

打った検証コマンドと結果:

| コマンド | 結果 |
|---|---|
| `npm run governance:check` | 全検査 passed（検査 19 件 / 対象文書 64 件 / 見出し参照 206 件を 77 文書から / 散文の識別子 1 件を 25 文書から / ADR の短縮引用 162 件） |
| `cargo test -p snotra`（出力を捨てずに exit code を取得） | **EXIT=0 / 192 passed / 0 failed / 2 ignored**。`egui_shell::window_coordinator::tests::runtime_fallback_matches_config_default_background ... ok` を出力行で確認 |
| `cargo test -p snotra-egui-runtime` | ok. 24 passed / 0 failed（doc-test 0） |
| A11 の grep 3 本を独立に再実行 | 1 本目 0 件・2 本目 2 件（`development-principles.md:63` / `view.rs:352`）・3 本目 0 件。**plan の実測と一致** |
| `git grep -n "CLEAR_COLOR"`（`workspace/` と `docs/superpowers/` を除く全追跡ファイル） | 15 件。`.rs` / `.md` / `.ps1` のみで、他の拡張子に写しは無い |

---

## フェーズ 1: 実装検証

### 挙動変更の不在（確認済み）

`git diff` の全ハンクを行単位で読んだ。`.rs` の変更行はすべて `///` 始まり（`snotra-egui-runtime/src/renderer.rs:9-14` の doc、`src-tauri/src/egui_shell/window_coordinator.rs:698-701` のテスト doc）、`.mjs` の変更行はすべて `//` 始まり（`scripts/governance-check.mjs:1228-1230`）。**実行される式・リテラル・判定は 1 つも変わっていない**——`pub const CLEAR_COLOR: u32 = 0x0028_2828;`（`renderer.rs:15`）もテスト本体（`window_coordinator.rs:702-708`）も差分外。ゆえにシークレット露出・入力バリデーション・システム境界（IPC / Win32 / ファイル入力）の観点は**母集団が空**である。

テストカバレッジ: 本変更はテストを増減させない。実装者とは独立に `cargo test` を 2 crate 分回して緑を実測した（上表）。

### Critical / High: **0 件**

0 件と言える根拠は 2 点である——(1) 実行コードが 1 行も変わっていないことを差分の全行で確認した、(2) 主張を書き換えた 5 箇所すべてについて、主張が指す一次証拠（テストの実在と実行結果・`visual-check-colors.ps1` 本文・`.github/workflows/` の中身・`Cargo.toml` の依存・`config.rs` の `Default` 実装）に当たって真偽を確かめた。

### Medium 1 — 新しく書いた 3 つの主張は、**識別子の腐りを捕まえる機構を 1 つも持たない**

- 場所: `snotra-egui-runtime/CLAUDE.md:39` / `snotra-egui-runtime/src/renderer.rs:11-14` / `src-tauri/src/egui_shell/window_coordinator.rs:700-701`
- 壊れた不変条件: 3 箇所が `runtime_fallback_matches_config_default_background` という**テスト名**を名指すが、**その名前の実在を照合する検査はリポジトリに無い**。
  - G-references が買うのは**パスの実在だけ**である（`scripts/governance-check.mjs:29` の `REF_EXTENSIONS` と `:153-198` の述語＝「バッククォート内で `/` を含み拡張子が当たる」）。テスト名は `/` を含まないので照合されない。
  - G-stale-identifiers の述語は **camelCase 限定**である（`scripts/governance-check.mjs:1428` = `/^([a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)(\(\))?$/`）。Phase B が足すのは SCREAMING_SNAKE（`plan.md:214`）。**小文字 snake_case はどちらの述語にも当たらない**——母集団を広げても（Phase B が却下した M 軸を採っても）この参照は守られない。母集団の側も実測した——`staleIdentifierDocs`（`:1436-1438`）は `.claude/{skills,rules,agents}/*.md` のみ、`STALE_EXTRA_DOCS`（`:1426`）は `SPEC.md` のみで、`snotra-egui-runtime/CLAUDE.md` は今日の母集団外である。**述語と母集団のどちらの足からも届かない**。
- 帰結: テストを改名・削除しても `npm run governance:check` は緑・`cargo test` も緑のまま、3 箇所が一斉に stale になる。**これは #825 が消そうとしている欠陥クラスそのもの**で、向きが「検査は無い」から「検査が在る」へ反転しただけである。
- 具体的な修正例（2 案・どちらもコードは触らない）:
  1. `workspace/plan.md:169` の不変条件行を正確化する。現行は「`.md` から書くパス参照は実在する | G-references … **A5 がテストのパスを直接書くのはこの保証を得るためである**」と読めるが、得られる保証は**ファイルの実在まで**である。「テスト名の実在は機械照合されない（述語が camelCase / SCREAMING_SNAKE 限定で snake_case を見ない）——受容する残余」を `:170`（rustdoc と `.ps1` の残余）の行と並べて明記する。
  2. Phase B の入力として記録する: 「snake_case を述語へ足すか」は一度も測っていない軸である（`plan.md:214` は SCREAMING_SNAKE のみ）。Phase A が新たに snake_case の参照を 3 本増やしたことは、その軸を測る動機になる。

### Low 1 — `renderer.rs:13-14`「検査は `src-tauri` にしか置けない」が前提より強い

- 場所: `snotra-egui-runtime/src/renderer.rs:13-14`「この crate は `snotra-core` に依存しないため、検査は**両方に依存する下流**（`src-tauri`）にしか置けない。」
- 根本原因: 前提（当 crate が `snotra-core` に依存しない）から結論（`src-tauri` にしか置けない）は出ない。**`snotra-egui-runtime` の `[dev-dependencies]` に `snotra-core` を足せば当 crate に置ける**——`snotra-core/Cargo.toml` は `snotra-*` を 1 つも持たない（実測）ので循環しない。事実として正しいのは「**今日**両 crate に同時に依存するのは `src-tauri` だけである」（`src-tauri/Cargo.toml:13-14` は両方、`snotra-settings/Cargo.toml:14` は `snotra-core` のみ・実測）。`AGENTS.md`「検証の作法（全タスク共通）」の全称表現の則に当たる。
- 修正例: 「…ため、**今日この 2 つに同時に依存するのは `src-tauri` だけである**（dev-dependency を足せば当 crate にも置けるが、依存を増やさない側を採った）」のように前提を名指す。`window_coordinator.rs:699-700` は前提（「両 crate に依存するのはこの crate だけ」）を書いているぶん穏当だが、「ここしか無い」の断定は同じ性質を残す。

### Low 2 — `development-principles.md:61`「下に挙げる実例はいずれも解消済みである」の射程が文面上は節末まで及ぶ

- 場所: `docs/development-principles.md:61`
- 根本原因: A7 の追記は正しい方向だが「下に挙げる実例」に境界が無い。節の**後半**には、解消されていない生きた残余が名指しされている——`:71`「enum variant のフィールド（`InstantAction::Url` の `url` 等）と generics 形の struct 定義は母集団に入らないため、そこは規範だけが頼りになる」。読者が「それも解消済み」と読む経路が開く。
- 検算した範囲（3 形の実例は確かに全て解消済み）: `[visual].background_color`（#802・テスト実行で実測）／ホットキー parser 二重化（#794）／`.unwrap_or(600.0)`（**`grep -rn "unwrap_or(600" --include=*.rs` が 0 件**＝現存しない）。
- 修正例: 「**下に挙げる 3 形の実例はいずれも解消済みである**」と数詞で閉じる（`:63` の 3 bullet と `:67` の段落に射程を限る）。

### Low 3 — `snotra-egui-runtime/CLAUDE.md:39`「検知するのは `check:colors` と目視である」は定常背景に限る

- 場所: `snotra-egui-runtime/CLAUDE.md:39`
- 根本原因: `scripts/visual-check-colors.ps1:18-22` 自身が「**自動判定するのは main と results の定常背景である**」と宣言し、show の一瞬のフラッシュと results リサイズ時のちらつきを目視へ残している。`set_clear_color` を**一部のフレームだけ**呼び忘れる形は自動判定では捕まらない（全フレーム呼び忘れなら捕まる）。
- 修正例: 「検知するのは `npm run check:colors`（**定常背景については**非既定色で実ピクセルを判定する…）と目視である」。

### Low 4 — テスト doc から「#802 で規約から機構へ移した」という**由来**が落ちた（ご指摘の点 6 への回答）

- 場所: `src-tauri/src/egui_shell/window_coordinator.rs:698-701`
- 何が失われたか: 旧文の「一致は今まで規約でしかなかった」＝**このテストが在る理由**（規約では止まらなかったので機構へ移した）。新文は「どこに置けるか」は説明するが「なぜ在るか」を説明しない。
- ただし**リポジトリ全体では失われていない**: `docs/development-principles.md:67` が「#802 で消費者を与え、一致は `src-tauri` のテストが固定するようになった」と歴史を保持している。旧文が指していた先（`snotra-egui-runtime/CLAUDE.md` の「受容した残余」）は A5 で消えるので、**旧文をそのまま残す選択は宙に浮く参照を作った**——落とした判断自体は正しい。
- 修正例（任意・1 節句）: 「…**機構で**固定する（#802 で規約から機構へ移した）。両 crate に依存するのは…」。issue 番号は G-references の対象外なので腐らない。

### Low 5 — `governance-check.mjs:1230` のパスがリポジトリルート相対でない

- 場所: `scripts/governance-check.mjs:1230`「今は `egui_shell/visual.rs` が読むので下表に載らない。」
- 根本原因: 実体は `src-tauri/src/egui_shell/visual.rs`。`.mjs` は `governanceDocs`（`.md` のみ・`scripts/governance-check.mjs:1214-1216`）の母集団外なので G-references は見ないが、**同じ文字列が `.md` に在れば「実在しないパス」として赤になる**述語である（`/` を含み `.rs` に当たり、ルートから解決できない）。機械が助けない場所ゆえの取りこぼし。
- 修正例: `src-tauri/src/egui_shell/visual.rs` と書く。

### 参考（Low 相当・**対処不要**・下の件数には数えない）

`src-tauri/src/egui_shell/mod.rs:290` のコメントが `renderer.rs=0x282828` というリテラルの写しを持つ。**今日の値と一致しており主張も真**で、これは #795 の「値の写し」クラスであって #825 の「主張の写し」ではない。本 PR で触る理由は無い。

---

## フェーズ 2: 計画判断・SPEC.md 同期

### 2a. plan.md の不変条件との照合

| 不変条件（`plan.md:167-171`） | 実装箇所 | 判定 |
|---|---|---|
| `snotra-egui-runtime` は `snotra-core` に依存しない | `snotra-egui-runtime/Cargo.toml`（`snotra-core` の grep が 0 件・実測） | **守られている**（差分は `Cargo.toml` に触れていない） |
| `CLEAR_COLOR` の値と `default_background_color()` の値は一致する | `window_coordinator.rs:702-708` / `snotra-core/src/config.rs:457-473`（`VisualConfig::default()` は `background_color: default_background_color()` を**呼ぶ**・`:366-368` が `"#282828"`） | **守られている**。テストが読むのは `VisualConfig::default()` だが、それが `default_background_color()` を呼ぶので `renderer.rs` の doc が名指す関数と 1 ホップで一致する。**この 1 ホップを実際に読んで確認した**（doc が名指す実体とテストが読む実体がずれていないか、が唯一の実質的な懸念点だった。ずれていれば #795 クラスの二重経路になっていた） |
| `.md` から書くパス参照は実在する | `snotra-egui-runtime/CLAUDE.md:39` → `src-tauri/src/egui_shell/window_coordinator.rs`（実在・`ls` で確認）／`docs/build-commands.md`（実在） | **守られている**（`governance:check` 緑で二重に確認） |
| rustdoc と `.ps1` 内のパス参照は機械照合されない（受容する残余） | `renderer.rs:12` の `src-tauri/src/egui_shell/window_coordinator.rs`（実在するが未照合） | **記述どおり**。ただし**テスト名の非照合はこの行に含まれていない** → Medium 1 |
| `NO_LAUNCHER_READ` の表と実コードの双方向一致 | `scripts/governance-check.mjs:1284-1296`（差分外）・`checkConfigFieldReachability`（`:1326`・差分外） | **守られている**。差分は `:1228-1230` のコメント 3 行のみ。`governance:check` の「config フィールド 67 件の到達性」も緑 |

### 2b. 対称コードパスチェック

対称ペア（show/hide・生成/破棄・フラグ真偽）は本差分に**存在しない**（実行コード 0 行）。
本変更に固有の「対称」は**同一主張の写し同士**である。母集団の網羅を独立に検算した:

```
対称チェック: 主張「CLEAR_COLOR と config 既定色の一致に検査は無い」の全写し
候補の列挙: git grep -n "CLEAR_COLOR" -- ":(exclude)workspace" ":(exclude)docs/superpowers"  → 15 件
```

15 件の内訳と判定:

| 箇所 | 判定 |
|---|---|
| `renderer.rs:11-14`（doc）・`snotra-egui-runtime/CLAUDE.md:39`・`development-principles.md:67`・`governance-check.mjs:1229`・`window_coordinator.rs:700` | **修正済み**（本差分の 5 箇所） |
| `docs/build-commands.md:77` | **不要**——「既定色での確認はこの検証にならない」は今日も真。検査の不在を主張していない（逐語を読んで確認） |
| `scripts/visual-check-colors.ps1:7-9, 30, 80, 190` | **不要**——4 件とも「既定色 = `CLEAR_COLOR` ゆえ既定では観測できない」であり、検査の不在を主張していない |
| `src-tauri/src/egui_shell/mod.rs:290` | **不要**（下記「参考」・#795 クラス） |
| `renderer.rs:22, 26, 164` / `lib.rs:15` / `window_coordinator.rs:707` | **不要**（実装・`pub use`・テスト本体） |

**#825 本文が挙げた 4 箇所（`gh issue view 825`）に対し、計画は 6 箇所へ広げている**——`window_coordinator.rs` のテスト doc（issue に無い第 5 の頂点）と `visual-check-colors.ps1`（第 6）を足した点は列挙として正しい。私の独立列挙でも**取りこぼしは 0 件**だった。

### 2c. DRY / 関数カバレッジ

新規・変更した関数は無い。**主張の重複**という観点では、計画は意図的に「正本 + ポインタ」を採らず「各所が事実を述べる」形を選んでいる（`plan.md:44-46`）。機械照合の向きに対する論拠（`.md` からパスを直接書けば G-references が守る）は妥当で、実装もそれに従っている。ただしその論拠が買う保証はパス実在までである → Medium 1。

### 2d. リソースライフサイクル

生成/破棄の対を持つリソースは本差分に無い（実行コード 0 行）。

### 2e. SPEC.md 同期

```
SPEC 同期: 差分が変える挙動 → 無し（doc コメントと散文のみ）
SPEC 該当節: 記述なし（grep -n "CLEAR_COLOR|282828" SPEC.md = 0 件・実測）
判定: SPEC 対象外
```

`AGENTS.md`「バグか仕様変更かを判定する」の「fix でも文書化された挙動を変えたら仕様変更」に照らしても、状態遷移・返り値契約・スコア計算・設定キー・データフォーマットのいずれにも触れていない。**計画の「SPEC.md 不要」判断（`plan.md:304`）は正しい**。

### 2f. 「変更不要」判断の再評価

```
再評価: docs/build-commands.md:77（計画 A9 が「修正不要」と結論）
  ケース A: A6 が :67 の主題文を「自動で回る検証が〜」へ弱めた → :77 は「既定色での確認はこの検証にならない」としか言わず主題文に依存しない → OK
  ケース B: :77 が引く見出し「config の値は到達性の検出器を持たない」は :55 のまま不変 → G-heading-refs 緑（governance:check で実測）→ OK
結論: 変更不要を確認

再評価: scripts/visual-check-colors.ps1:7-9（計画 A10）
  ケース A: 引用先の節見出しが不変 → 空振りしない → OK
  ケース B: 「既定のまま起動しても欠陥は観測できない」は CLEAR_COLOR == #282828 が続く限り真。テストが機構化されてもこの命題は変わらない（テストは値の一致を固定するのであって、既定色での可視性を変えない）→ OK
結論: 変更不要を確認

再評価: NO_LAUNCHER_READ の表本体（計画 :61 が「触らない」）
  ケース A: background_color が表に無い＝「ランチャが読む」側 → visual.rs:100 が実際に読む（実測）→ 表は正しい → OK
  ケース B: preset は表に在り「ThemePreset を import すらしない」→ 表 :1295 の理由文と新コメントの文言が一致（実測）→ OK
結論: 変更不要を確認

再評価: フォールトインジェクション不要の判断（plan.md:179）
  ケース A: adrCitationDocs は scripts/governance-check.mjs を母集団に含みコメント本文を読む → 新コメントに ADR-<slug> 形の文字列は無い（読んで確認）→ G-adr-citations は影響を受けない
  ケース B: checkConfigFieldReachability / NO_LAUNCHER_READ / CONFIG_EXPECTED_STRUCTS はいずれも差分外 → 判定の実体が不変
  ケース C: governance:check を実際に走らせて全検査 passed（ADR の短縮引用 162 件が従前どおり照合された）
結論: 不要を確認（.claude/rules/safety-nets.md の要求は「判定を足す変更」に掛かるが、本差分は判定を足していない）
```

### 計画（Phase A）と実装の差

| 項目 | 判定 |
|---|---|
| A1 / A2 / A4 / A5 / A6 | **計画の逐語スニペットと実装が完全一致**（`plan.md:70-74, 82-85, 99-101, 111, 119` を diff と 1 行ずつ突き合わせ） |
| A7 | 計画は「弱めるか、解消済みである旨を足す」（**or**）→ 実装は**両方**を行った。過剰実装ではなく計画の許容範囲内。ただし追記側の射程が甘い → Low 2 |
| A8 | `docs/development-principles.md:71` が `NO_LAUNCHER_READ` へ。`G12_` の残存 0 件を独立に確認（`grep -rn "G12_" docs/ scripts/ *.md`）|
| A9 / A10 | ファイル無変更・結論を `plan.md` へ記録。**私も一次証拠に当たって同じ結論**（上記 2f） |
| A11 | 述語の修正（`一致は規約` → `一致は規約にすぎ` + `一致は今まで規約` の追加）は**正しい判断**。訂正後の文に部分一致する述語は「直した/直していない」を区別できず検算として不成立、という理由付けも妥当。3 本を独立に再実行して plan の実測と一致 |
| A12〜A16 | 実測値（192 passed / governance:check 全検査 passed / diff 3+2 行）を**私も独立に再現**した |
| **計画に在って実装に無い項目** | **無し** |
| **実装に在って計画に無い項目** | **無し**（`workspace/plan.md` 自身の更新は計画の運用手順どおり） |

---

## フェーズ 3: パフォーマンス

**該当なし。** 性能に敏感な箇所（`search.rs` / `folder.rs` / 検索結果レンダリング / アイコンキャッシュ / egui の `update()` 内）に触れていない。差分の `.rs` 変更は `snotra-egui-runtime/src/renderer.rs` の `pub const` に付く doc コメントと `src-tauri/src/egui_shell/window_coordinator.rs` の `mod tests` 内の doc コメントのみで、**生成されるコードは変わらない**（コメント行だけの差分であることを全行で確認済み）。

---

## 確かめられなかったこと（推測で埋めない）

1. **`npm run check:colors` の実行そのもの**——GUI と非ロック画面を要するため本セッションでは走らせていない。確かめたのは**スクリプト本文の逐語**である: 既定色 `#4A2B5C`（`:48`）・最頻色による判定（`:154-167`）・`exit 1`（`:304`）・非既定色で実ピクセルを測ること。したがって新記述の「非既定色で実ピクセルを判定する」は**ソースに接地している**が、実行結果としては未確認（`plan.md:355` の「未検証」と同じ範囲）。
2. **「CI には無い」**——`.github/`（workflows 5 本を含む）に `check:colors` / `visual-check-colors` の参照が 0 件であることを grep で確認した。各ワークフローの中身を 1 本ずつ読んではいない（間接起動の可能性は grep で潰れる範囲までしか見ていない）。
3. **`runtime_fallback_matches_config_default_background` のフォールトインジェクション**（`0x0028_2829` へ 1 文字変異させて赤になること）——`plan.md:168` は review-facts の実測として記録しているが、**私は再測していない**（読み取り専用の指定のため）。テストが実際に実行され緑であることまでは実測した。
4. **`docs/development-principles.md` の節全体の意味的整合**——A6/A7 が触った段落と前後（`:55-60` / `:69-73`）のつながりは読んで矛盾が無いことを確認したが、`/health-check` 相当の意味的監査は実施していない。

---

## 総括

- **Critical: 0 件 / High: 0 件 / Medium: 1 件 / Low: 5 件**（ほかに対処不要の参考 1 件）

Medium 1（新設した 3 つのテスト名参照が、どの検出器の述語にも当たらない小文字 snake_case である）だけが**構造の話**で、残りは文言の精度である。いずれも**本 PR をブロックしない**——Medium 1 は「新たな腐りを作った」のではなく「腐りうる箇所を機構が守っていない事実の記録漏れ」であり、`workspace/plan.md:169-170` の不変条件表に 1 行足せば閉じる。Low 1〜5 は 1 節句ずつの修正で、まとめて 1 コミットに収まる。

**#825 の目的（現に在るものを「無い」と述べる記述の除去）は達成されている**——独立に列挙した 15 件の `CLEAR_COLOR` 言及すべてについて今日の実装に照らして真偽を確かめ、偽の主張が 0 件であることを確認した。#819 案 (A) も `G12_` 残存 0 件で完了している。
