# plan — issue #1239: rust-analyzer component を `rust-toolchain.toml` で保証し、LSP の沈黙する失敗を塞ぐ

調査: `workspace/research.md`。ブランチ: `fix/rust-analyzer-toolchain-component`。

## 目的と受け入れ条件

**目的**: Claude Code の LSP（`snotra-rust-lsp` → PATH の rustup shim → 1.98 toolchain の `rust-analyzer`）が、toolchain のパッチ更新や再インストールの後も動く状態を、手順ではなく `rust-toolchain.toml` の `components` で保証する。#1177 が「意図して外す」と決めた根拠（CI の追加費用）は、CI が毎 run 1.98 を丸ごと取得している現状では成り立たない（research.md「前提の変化」3）。

受け入れ条件:

1. `rust-toolchain.toml` の `components` に `rust-analyzer` が在り、ローカルでは toml 編集後に `cargo --version` を 1 回叩くだけで 1.98 に component が入る（`rustup component list --installed --toolchain 1.98-x86_64-pc-windows-msvc` に `rust-analyzer-x86_64-pc-windows-msvc`）
2. Claude Code 再起動後、`LSP findReferences` が `snotra-core/src/indexer/keys.rs` の `normalize_entry_key` に対して結果を返す（#1177 の「再起動すれば戻る」の追認）
3. CI が緑で、`Setup Rust toolchain` のログに `downloading 6 components` が出る（費用の実測。PR 本文のチェックリストで確認する——CI は PR ができてから走る）
4. `.claude/skills/deps-update/SKILL.md` の手動手順が消え、「toml が保証する」記述と再起動の注意だけが残る
5. `docs/hooks.md`「Claude Code の RA インスタンスと hook の分担」の壊れ方の表に「component 不在」の行がある
6. `checkLspConfig` が `rust-toolchain.toml` の `components` から `rust-analyzer` を外すと赤になり、故障注入テストがそれを固定する

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 |
|---|---|
| `rust-toolchain.toml` | `components = ["rustfmt", "clippy", "rust-analyzer"]`。ヘッダコメントの L17〜20（`components` を書く理由）に 1 段落: rust-analyzer は Claude Code の LSP（`.claude/lsp/`）が PATH の shim 越しに呼ぶもので、**無くても cargo / clippy / test は緑のまま LSP だけが落ちる**（沈黙する壊れ方）。#1177 は CI 費用を理由に手順へ逃がしたが、CI は pin された toolchain を毎 run 取得しており component 1 つ分しか変わらない（#1239 実測）。`channel` がパッチ版を追う限り toolchain は `channel` の行を変えずに入れ替わるので、手順では覆えない |
| `.claude/skills/deps-update/SKILL.md` L40〜46 | 「`channel` を上げたら `rustup component add rust-analyzer` を打つ」の段落を削り、「rust-analyzer は `components` が保証する（#1239）。**component が入り直した後も、既に LSP の起動失敗を数え切ったセッションは復帰しない——Claude Code を再起動する。再起動直後は索引が冷えており最初の 1 回は『見つからない』を返しうる**」だけを残す |
| `docs/hooks.md` L89〜90 の表 | 行を 1 つ足す: 「**component が無い**（toolchain の入れ替え・pin の変更）| 沈黙する——cargo / clippy / test は緑のまま `LSP` ツールだけが `crashed with exit code 1` を返す。守り手は `rust-toolchain.toml` の `components`と `checkLspConfig` の宣言検査」 |
| `.claude/hooks/lsp-config.mjs` `checkLspConfig` | `rust-toolchain.toml` を読み（`#` コメント除去は ratoml 検査と同じ扱い）、`components` 配列に `"rust-analyzer"` が無ければ violation: 「rust-toolchain.toml の components に rust-analyzer が無い — toolchain が入れ替わるたびに LSP が沈黙で落ちる（#1239）」。配置は ratoml 検査の直後・配送経路の検査の前（連鎖の早期 return に道連れにされない位置。既存コメントの理由と同じ）。**見るのは宣言だけ**——実際に入っているかは射程の外（CI の runner には無いので環境の実測は置けない）。その旨をコメントに書く |
| `.claude/hooks/lsp-config.test.mjs` | **`COPIED`（L23〜28）に `rust-toolchain.toml` を足す**（`materialize()` はこの配列のファイルだけを複製する——足し忘れると新テストは「ファイル不在」の枝を測る）。故障注入 2 本: 複製の `rust-toolchain.toml` から `"rust-analyzer"` を消すと赤（足 10）／`components` 行ごと消しても赤（足 10b）。無変異の複製が緑のままであることは既存 L70 が見る |
| `docs/hooks.md` L54〜57 の発火一覧 | `rust-toolchain.toml` → `hook-selftest` の行を足す。**この表は `G-hook-fires` が `selectChecks` と機械照合する**ので、コードだけ・表だけを直した形は `governance:check` が赤にする（L89 付近の壊れ方の表は散文のみで機械照合されない） |
| `.claude/hooks/post-edit.mjs` `CHECK_DEFINITION`（L77〜82） | Set に `"rust-toolchain.toml"` を足す（ルート限定・basename 完全一致で足りる。L166〜174 は `.claude/lsp/` と `rust-analyzer.toml` 用の正規表現ブロックで、こちらではない）。同 Set の注記「カナリアの無いファイルをここに足してはならない」は、`lsp-config.test.mjs` の実リポジトリ緑テスト + 故障注入 2 本がカナリアになることで満たす。**足さないと toml を触っても検査が走らず沈黙する** |
| `.claude/hooks/post-edit.test.mjs` | `selectChecks("rust-toolchain.toml")` が `["hook-selftest"]` を返す単体テスト 1 本（L164〜166 の `rust-analyzer.toml` の先例と同型） |

## 実装順序

1. `rust-toolchain.toml` を編集 → `cargo --version` で component の自動取得を実測（受け入れ条件 1）→ `rustup component list --installed` で確認
2. 文書 2 枚（deps-update / hooks.md）
3. `lsp-config.mjs` + テスト（`COPIED` へ追加 → 先に赤 → 実装）→ `post-edit.mjs` の `CHECK_DEFINITION` + 単体テスト → `docs/hooks.md` の発火一覧
4. Claude Code 再起動後に `LSP findReferences` を実測（受け入れ条件 2）——**再起動は人間の操作**なので、PR 本文のチェックリストへ送る

## 不変条件と異常系

- **toml の `components` が正本で、CI の `components:`（preinstalled `stable` 用）には足さない**（`ci.yml` L157 のコメントどおり）
- **`.lsp.json` は触らない**（`command: "rust-analyzer"` のまま。絶対パスや `rustup run stable` へ逃がすと他マシン・worktree で壊れる・`ADR-claude-code-ra-lsp-plugin-delivery` の却下案と同型）
- **環境の実測を CI に置かない**（runner に rust-analyzer が入るのは toml が効いた後で、検査が toml の効果に依存する循環になる）。検査するのは宣言だけ、と `lsp-config.mjs` のコメントと `docs/hooks.md` に射程を書く
- 異常系: rustup が toml の追加 component を既存 toolchain へ反映しない版なら（制約 1 の実測が偽なら）、`rustup component add` を 1 回だけ手で打ち、その事実を PR 本文に書く。toml 自体は将来の入れ替えを守る
- 異常系: CI の `Setup Rust toolchain` が失敗する（component 名の誤り等）→ PR の CI で赤になる。名前は `rustup component list --toolchain 1.98-…` の出力 `rust-analyzer-x86_64-pc-windows-msvc`（toml には target 抜きの `rust-analyzer` と書く。`rustfmt` / `clippy` と同じ形）

## テスト方針と検証コマンド

| 局面 | コマンド | 期待 |
|---|---|---|
| toml 編集直後 | `cargo --version` | `info: installing component 'rust-analyzer'` が stderr に出る（出なければ異常系へ） |
| 同 | `rustup component list --installed --toolchain 1.98-x86_64-pc-windows-msvc \| grep rust-analyzer` | 1 行 |
| 同 | `rust-analyzer --version` | `rust-analyzer 1.98.1 …`（プロキシが解決する） |
| B・先に赤 | `npx vitest run .claude/hooks/lsp-config.test.mjs` | 新テスト 2 本が赤 → 実装後に緑（実リポジトリの緑テストは常に緑） |
| 各段 | `npm run governance:check` | 24 件緑（`docs/hooks.md` の表を触るため。G-heading-refs・散文の識別子） |
| PR | `Setup Rust toolchain` のログ | `downloading 6 components`・ステップ所要が前回（26 秒）から大きく変わらない |
| 再起動後（人間） | `LSP findReferences` | 結果が返る |

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: 不要（製品の挙動に触れない）
- `docs/build-commands.md` L13: 据え置き（版の決定元は変わらない）
- `docs/adr/`: 新設しない。#1177 の判断（手順に逃がす）を覆すが、却下した代替案は「手順のまま」で、その却下理由は toml のコメントと本 PR に残る。ADR は「否定の知識が生じた決定のみ」——ここで否定するのは既存の決定であり、`8fa8b5f` の commit message と本 PR がその歴史を持つ

## 作業項目

### Phase 1 — toml
- [x] `rust-toolchain.toml` に `rust-analyzer` を足し、ヘッダコメントに理由を書く
- [x] `cargo --version` で自動取得を実測し、`rustup component list --installed` で確認（リポジトリの toml で `cargo 1.98.1`・`rust-analyzer-x86_64-pc-windows-msvc`・`rust-analyzer 1.98.1`・override の出所は `rust-toolchain.toml`。component 自体は計画段階の scratch 実測で既に入っていた）

### Phase 2 — 文書
- [x] `.claude/skills/deps-update/SKILL.md` の手動手順を「toml が保証する」形へ改める（再起動の注意は残す）
- [x] `docs/hooks.md` の壊れ方の表に「component が無い」行を足す（編集時の語彙 reminder で `pluginUsage` を散文へ直した）
- [x] `npm run governance:check` が緑（24 件）

### Phase 3 — 宣言検査
- [x] `lsp-config.test.mjs` に故障注入 2 本（先に赤）（hook-selftest が 2 本の赤を報せた・2026-09-06 19:52）
- [x] `checkLspConfig` に toml の宣言検査を足す（射程のコメントつき）
- [x] `lsp-config.test.mjs` の `COPIED` に `rust-toolchain.toml` を足す
- [x] `post-edit.mjs` `CHECK_DEFINITION` に `rust-toolchain.toml` を足し、`post-edit.test.mjs` に `selectChecks` の単体テスト 1 本。`docs/hooks.md` の発火一覧（`G-hook-fires` 照合）へ 1 行（単体テストも先に赤を確認）
- [x] `npx vitest run .claude/hooks` が緑（329 件）

## 未確定（実装前に潰す）

- [x] 選択肢 A（toml + 文書）か B（A + 宣言検査 1 本）か — 2026-09-06 人間の判断: **B**（問い「#1239 の対処の幅を決めてくださいませ。…」への回答「B: toml + 文書 + 宣言検査（推奨）」）
- [x] rustup 1.29.1 が toml の `components` 追加を既存 toolchain へ即時反映するか — 2026-09-06 に scratch ディレクトリの toml で実測: `cargo --version` 1 回で 1.98 に `rust-analyzer-x86_64-pc-windows-msvc` が入り `rust-analyzer 1.98.1` が解決した（research.md「主エージェントの実測」）。**ローカルの 1.98 には既に component が入っている**ので、Phase 1 の実測は「リポジトリの toml でも同じ出力になる」の確認に縮む（サンドボックスを外して実行すること——中では rustup の HTTPS が失敗する）

## 人間レビュー

- [x] 承認済み — 2026-09-06 / 問い: "`workspace/plan.md`（#1239）を承認しますか。承認後は workspace をコミット・プッシュし、実装は `/implement` へ渡します。" / 回答: "承認する"

## plan-review 結果

- リスク: 高（hook・skills・ガバナンス文書を変更する）
- レビュー方式: 計画準拠レビュー 1 体（`workspace/plan-review-1239-hooks.md`・観点: セーフティネットの変更／手順の撤去と写し）
- エージェント数: 1（ほかに /start-issue 3b の敵対的調査 1 体）

### 要対処（計画へ反映済み）
- `selectChecks` の追加位置の誤り — 計画が指した L166〜174 は `.claude/lsp/` と `rust-analyzer.toml` 用の正規表現ブロックで、「検査の定義を変えるファイル」は `CHECK_DEFINITION` Set（L77〜82）だった — 再照合: `post-edit.mjs` L70〜82・L152〜156 を読んで確認。Set へ足す形へ修正
- `lsp-config.test.mjs` の `COPIED` に `rust-toolchain.toml` を足す指示が無かった — 再照合: L23〜28 を読んで確認。作業項目へ追加

### 軽微（反映済み）
- `docs/hooks.md` の 2 表のうち発火一覧（L54〜57）は `G-hook-fires` が機械照合し、壊れ方の表（L89）は散文のみ — 変更ファイル表へ明記
- ⚠️ `post-edit.test.mjs` の `selectChecks` 単体テストの先例（L164〜166）— 同型を 1 本足す形で採用
- 観点 2: `rust-analyzer` の生きた層の写しは `deps-update/SKILL.md` 以外に無し。toml ヘッダの書き換えで偽になる散文も無し。再起動・索引の温まりの注意は保持

### 未検証
- なし

### 判断
- 実装着手: 人間の裁定待ち

## セルフレビュー

- リスク: 高
- plan-review: 計画準拠レビュー 1 体
- エージェント数: 2（敵対的調査 1 + 計画準拠レビュー 1）
- 要対処: 2 件（上記・反映済み）
- 未検証: なし
