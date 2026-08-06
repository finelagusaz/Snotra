# 調査 — issue #950: clippy.toml の空洞化を governance:check で捕まえる（G-clippy-disallowed）

## issue の要約

#951（= #900 の実装）が入れた `src-tauri/clippy.toml` は、**自分が死んだことを誰にも告げられない**。
clippy が exit 0 のまま沈黙する経路が 6 本あり、`disallowed-methods` の内容側は現在まったくの無防備である。
`governance:check` へ静的読み取りの検査を 1 本足して塞ぐ。

issue 本文の 6 経路（すべて起票者が 2026-08-06 に実測）:

| 経路 | clippy の挙動 |
|---|---|
| `clippy.toml` の削除 | 沈黙・exit 0 |
| `disallowed-methods` を空配列化 | 沈黙・exit 0 |
| エントリが 1 行だけ消える | 沈黙・exit 0 |
| メソッド名・型名の書き損じ | warning は出るが `-D warnings` でも exit 0 |
| crate 名の書き損じ（`eguii::`） | 診断そのものが出ない |
| `egui` 依存の消滅 | 診断そのものが出ない |

issue コメントが追加した**沈黙経路 0**（`disallowed_methods` は warn 既定で、赤くなるのは `ci.yml` と
`post-edit.mjs` が `-D warnings` を渡している間だけ）については、起票者判断で**選択肢 2（ルート
`[workspace.lints.clippy]` で deny 化して `-D warnings` 依存そのものを消す）**を採る（2026-08-06 に確認）。

## 関連ファイル・シンボル（grep で実在確認済み）

| パス | 対象 | 役割 |
|---|---|---|
| `src-tauri/clippy.toml` | `disallowed-methods`（7 エントリ） | 守る対象。冒頭に長い規範コメントを持つ |
| `src-tauri/Cargo.toml:15` | `egui.workspace = true` | 判定 C の入力。**dotted 形**である |
| `Cargo.toml:21-23` | `[workspace.lints.rustdoc]` | 判定 D を足す場所。clippy カテゴリは**現在無い** |
| `scripts/governance-check.mjs:316` | `REQUIRED_RUSTDOC_LINTS` | カナリアの手本・置き場所 |
| `scripts/governance-check.mjs:339` | `rustdocLintsAreDenied` | 判定 D の手本（level の 2 形を受ける） |
| `scripts/governance-check.mjs:321` | `hasWorkspaceLintsOptIn` | 判定 C の手本（字面でなく構文的位置） |
| `scripts/governance-check.mjs:263` | `tomlLine` | **再利用できない**（後述） |
| `scripts/governance-check.mjs:1575-1593` | `buildChecks` の registry | 新検査の登録先 |
| `scripts/governance-check.mjs:1614` | `evidence` 文字列 | 母集団の可視化 |
| `scripts/governance-check.test.mjs:296` | `describe("G-workspace-lints …")` | フィクスチャの手本 |
| `src-tauri/CLAUDE.md:51` | clippy.toml への言及 | **訂正不要**（`-D warnings` に言及していない） |
| `docs/build-commands.md:28` | rustdoc deny の説明 | 先例 `ca8afae` が同種の 1 文を足した場所 |

## 再利用できる既存パターン

- **`G-workspace-lints`（#713 / `ca8afae`）と同型**。「設定ノブが黙って無効化する」という同じ失敗に対し、
  必須要素を名指しするカナリア（`REQUIRED_RUSTDOC_LINTS`）を既に置いており、その根拠は
  `docs/development-principles.md`「8. 全称条件だけの検査は、集合が縮んだときに空振りする」。
- **先例 `ca8afae` が触ったファイル**: `governance-check.mjs` / `governance-check.test.mjs` /
  `docs/build-commands.md` / ADR / `post-edit.test.mjs`。**`SPEC.md` は触っていない**（`git show --stat` で実測）。
- **母集団欠落の定型**: `finding(path, 1, "… が読めない（G-* 母集団の欠落）")`。
- 検査は `snapshot.read(rel)` で任意の相対パスを読める（`makeSnapshot` は `fs.readFileSync` の薄いラッパで、
  拡張子でフィルタしない・`governance-check.mjs:43-66`）。`src-tauri/clippy.toml` は読める。

## 技術的制約（すべて実測。measured 2026-08-06）

### 1. `tomlLine` は再利用できない

`tomlLine = (raw) => raw.replace(/#.*$/, "").trim()` は**引用符の中を見ない**。clippy.toml の `reason` は
`（#751）` を含むため、実データのエントリ行は途中で切れる:

```
tomlLine: "{ path = \"egui::Context::set_visuals\", reason = \"root Ui が pass 冒頭で掴む Arc<Style> に間に合わない（"
```

いまは `path` が `reason` より前に在るため結果的に生き残るが、**順序に依存した偶然**である。
引用符を意識した除去（`stripTomlComment`）を新設する。

### 2. 素朴な per-line 単発 match は、最も起きやすい空洞化を緑で通す

| 抽出方式 | 実データ（複数行） | 1 行形 | `#` でコメントアウトされたエントリ |
|---|---|---|---|
| per-line 単発 `match` | 7 件 | **1/2 件** | **在ると誤認（false green）** |
| quote-aware + `matchAll` | 7 件 | 2 件 | 0 件 |

コメントアウト形は、このファイルの「コメントで長く説明する」文化ではもっとも自然な「一時的に無効化する」
操作であり、**issue が塞ごうとしている空洞化そのもの**である。ゆえに quote-aware + 全域 match を採る。

### 3. 選択肢 2（workspace lints で deny）は実効であり、他 crate に無害である

`src-tauri/src/main.rs` へ `ctx.set_visuals(...)` を注入して測った（測定後に `git checkout --` で復帰済み・
作業ツリーは差分ゼロを確認）:

| 条件 | 結果 |
|---|---|
| 現状 + `cargo clippy -p snotra --all-targets`（`-D warnings` **無し**） | `warning: use of a disallowed method` / **exit 0** / `#[warn(clippy::disallowed_methods)] on by default` |
| ルートへ `[workspace.lints.clippy] disallowed_methods = "deny"` を足して同じコマンド | `error: use of a disallowed method` / **exit 101** / `requested on the command line with -D clippy::disallowed-methods` |
| 同じ状態で `cargo clippy -p snotra-core -p snotra-egui-runtime -p snotra-settings --all-targets` | 診断なし / **exit 0** |

3 crate が無害なのは `clippy.toml` を持たず禁止集合が空だからで、これは同時に
**「deny にしても `clippy.toml` が空なら禁止するものが無い」＝判定 A/B が依然として必要**であることの実測でもある
（issue コメントの主張を裏づける）。

### 4. 判定 D 自身も沈黙する

deny の 1 行がルート `Cargo.toml` から消えれば warn へ戻り、また黙る。**沈黙を移しただけにしないため、
deny の実在も検査に含める**。member 側の opt-in（`src-tauri` の `[lints] workspace = true`）は
既存の `G-workspace-lints` が全 member について見ているので**重複させない**（依存関係だけ書き残す）。

### 5. 述語の実測（19 ケース・すべて期待どおり）

判定 B: 実データ 7 件 / 1 行形 2 件 / 空配列 0 件 / コメントアウト 0 件 / 配列ごと消滅 `null` /
1 行だけ消える 6 件 / メソッド名の書き損じ・crate 名の書き損じ → カナリア欠落を検知 / `reason` 先行でも 1 件。
判定 C: 実データ true / `snotra-egui-runtime` だけ false（部分文字列の誤爆なし）/ 素の `egui = "…"` true /
`[package]` 配下の同じ字面 false。
判定 D: 現ルート false / 文字列形・テーブル形 deny true / `warn` false / 別カテゴリ配下 false /
**ハイフン形 `disallowed-methods` は false（赤へ倒れる）**。

## 未解決の疑問 → 解消済み

- ~~`tomlLine` を使ってよいか~~ → **不可**（制約 1・実測）。`stripTomlComment` を新設する。
- ~~選択肢 2 は本当に `-D warnings` 依存を消すか~~ → **消す**（制約 3・実測）。
- ~~他 crate を巻き込まないか~~ → **巻き込まない**（制約 3・実測）。
- ~~`SPEC.md` の同期が要るか~~ → **不要**。CI の静的検査であり製品の意図ではない。先例 `ca8afae` も触っていない。
- ~~G-* の id を列挙する索引文書があるか~~ → **無い**。`docs/build-commands.md:132` の散文要約は
  `G-workspace-lints` / `G-ci-table` / `G-adr-*` も列挙しておらず、索引ではない。

## 残る注意点（計画へ持ち込む）

- ルート `Cargo.toml` へ clippy カテゴリを足すと、`governance-check.mjs` の `G-workspace-lints` 冒頭にある
  **受容する残余の一文が偽になる**（「`[workspace.lints.clippy]` 等が降格されてもこの検査は鳴らない…
  workspace テーブルが担っていない」）。同じ変更で訂正する。
- `src-tauri/clippy.toml` 冒頭の**沈黙経路 0 の段落も偽になる**。あわせて、
  「`[workspace.lints]` は…使えない」の一文は**内容（禁止集合）**を論じているのに**レベル**まで否定して読める。
  「内容は移せない／レベルは移した」と鋭くする。
