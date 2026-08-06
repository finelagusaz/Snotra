# レビュー（枠: 既存機構での代替可能性）— #900 `ctx.set_visuals` の禁止を clippy の `disallowed-methods` で機構化する

対象 issue: **#900**
レビュー日: 2026-08-06 / 枠: 「この repo が既に持っている機構で代替できないか」
制約: リポジトリのファイルは 1 件も変更していない（測定はすべて scratchpad の使い捨て crate と、
`governance-check.mjs` の export 関数への入力注入で行った）。

---

## 0. 結論の要旨（先に読むこと）

1. **問い 1 の答えは「代替できない」。** ただし理由は「近いものが無い」ではなく——
   **`clippy.toml` は新しい検査機構ではなく、この repo が既に両方の場所で走らせている
   ツールの設定ファイルだからである**（`ci.yml:126` と `.claude/hooks/post-edit.mjs:308-312`）。
   本枠の選好（「自作の写しより、ネイティブ機構・既存機構を既定の推奨にする」）は
   **`clippy.toml` を支持する側に立つ**。私が唯一見つけた実装可能な代替（`governance-check.mjs` へ
   `.rs` を grep する `G-*` を 1 本足す）は、clippy が型解決付きで既にやっていることの
   **手書きの写しである**。→ §1・§2

2. **問い 2 — 費用は「新種の検査機構が 1 つ増える」ではなく「新種の設定ファイルが 1 つ増える」である。**
   構造上の双子が既に repo 内に在る: ルート `Cargo.toml:21` の `[workspace.lints.rustdoc]`。
   どちらも「既存ツールの設定ノブ」で、新しい runner も新しい CI job も 0 本。
   「沈黙しうる経路をすべて塞ぐ」規範（ルート `CLAUDE.md`「フック」・#471）は**射程が
   `.claude/hooks/` の出力経路**であって、新種ファイルの追加を禁じるものではない。→ §3

3. **問い 3 — 塞げる。しかも一部は plan が気づかないまま既に塞がっている。**
   Phase 3 が `src-tauri/CLAUDE.md` へ書こうとしている 1 文が
   **`` `src-tauri/clippy.toml` `` というルート相対のバッククォート参照を含むなら、
   `G-references` がファイルの実在を既に検査する**（実測）。これは
   `plan.md:182,203-204` と独立レビュー §5-R8 の「消失は governance:check も緑」を
   **条件つきで反証する**。残る内容側（空配列化・パスの書き損じ）は
   `G-workspace-lints` と同じ形の検査 1 本で塞げる。→ §4

4. **未記録の沈黙経路を 1 本、自分で測って見つけた。**
   **`clippy.toml` は cargo の fingerprint に入らない**——`clippy.toml` だけを置いて
   同一コマンドを打つと、cargo はキャッシュを replay して**設定を一度も適用せずに exit 0 を返す**（実測）。
   幸い CI では効かない（`rust-cache` は workspace crate を保存しない・`ADR-smoke-build-time.md:14`）が、
   **手元の検証手順は影響を受ける**。→ §5・要-2

---

## 1. 問い 1 — 機構ごとの判定表

守りたい不変条件: **`src-tauri` の `run_ui` callback 内から `Context` 経由で global style を書かない**。

| # | 機構 | できる／できない | 根拠（`file:line`） | 費用 |
|---|---|---|---|---|
| 1 | `scripts/governance-check.mjs` の `G-*` を 1 本足す（**禁止の一次検査として**） | **できない（近似のみ）** | 検査は Node の純関数で、入力は `snapshot.read(rel)` のテキストだけ（`:41-65`）。型解決を持たないので判定は必ず**字面**になる。この repo は `hasWorkspaceLintsOptIn` の doc で「**字面ではなく構文的位置で判定する**」を明示している（`:318-321`）。字面 grep は UFCS（`egui::Context::set_visuals(&ctx, v)`）を原理的に捕まえられない——独立レビュー §5-R5 が「UFCS でも clippy は発火する」を実測済みで、ここが**能力差の実体**である | 実装 ~40 行 + テスト。だが**発火は CI の `governance-check` job だけ**（`selectChecks` は `.rs` に governance を割り当てない・`post-edit.mjs:125-157`）ゆえ、書いた瞬間ではなく PR で初めて赤くなる |
| 1b | 同上（**`clippy.toml` の生存を守る二次検査として**） | **できる（推奨・§4）** | 同上。`snapshot.files` は拡張子で絞らず全ファイルを列挙する（`:41-53`・実測で `src-tauri/Cargo.toml` `src-tauri/tauri.conf.json` を確認） | 実装 ~30 行 + フィクスチャ 1 件 |
| 2 | `.claude/hooks/post-edit.mjs` の `selectChecks` | **できない（追加不要・既に配線済み）** | `.rs` を編集すると `clippy` が `--workspace --all-targets -- -D warnings` で走る（`:126-127`, `:308-312`）。**`clippy.toml` はこの既存の clippy にそのまま乗る**ので配線は 1 行も要らない。逆に `clippy.toml` 自身の編集には検査が 1 つも割り当たらない（`:125-157` に `.toml` の分岐は `CARGO_MANIFEST`（`:61` = `Cargo.toml` 限定）と `config.toml`（`:145`）だけ） | 0（変更不要） |
| 3 | `.githooks/` | **できない（射程外・規範に反する）** | 4 hook は分岐名だけを見る（`pre-commit` 全 8 行・判定は `current_branch_ref` の 1 比較）。cargo も node も起動しない。さらに `_lib.sh:10` が「**この層が生きているかを検知する仕組みは、意図的に作らない**」と宣言している——追跡ファイルゆえ fail-open で、最終防衛線は GitHub ruleset。ここへ内容検査を足すのは層の責務（main 保護）を壊す | 高（責務の混線） |
| 4 | `.github/workflows/ci.yml` の各 job | **できない（追加不要・既に配線済み）** | `rust-check` が `cargo clippy --workspace --all-targets -- -D warnings` を持つ（`:126`）。`clippy.toml` はここにそのまま乗る。`governance-check` job（`:51-64`）は `node scripts/governance-check.mjs` 1 本で、そこへ足す道は #1b と同一 | 0（変更不要） |
| 5 | `scripts/` 配下の既存 npm script | **できない** | `package.json` の 16 script のうち、Rust ソースを走査するのは `race:boundaries` のみ。同スクリプトは `/race-check` の**母集団を取る道具**であって gate ではなく、CI からも hook からも呼ばれない（`ci.yml` に `race:boundaries` は無い）。`verify` は `cargo check --workspace && npm test` で clippy を含まない | — |
| 6 | ルート `Cargo.toml` の `[workspace.lints]` | **できない（2 重の理由）** | (a) `disallowed-methods` は lint の**設定値**であって lint レベルではないので `[lints.clippy]` テーブルに書けない（依頼で確定済み・独立レビュー R10）。(b) 仮に書けても `[workspace.lints]` は全 member 共通（`Cargo.toml:21-24` + 各 member の `[lints] workspace = true`）ゆえ、`snotra-settings` の正当な 2 件を必ず巻き込む。**crate ごとに分けたいという要件そのものが workspace lints と反対を向いている** | — |
| 7 | Rust のユニットテスト（ソース走査型） | **できない（前例も無い）** | `include_str!` / `CARGO_MANIFEST_DIR` は**リポジトリの `.rs` 全域で 0 件**（実測）——ソースを読んで自分を検査する Rust テストの前例が無い。書けば #1 と同じ字面判定になり、加えて「テストがソースを読む」という**新しい種類の依存**を `.rs` 側へ持ち込む。`clippy.toml` より重い | 高 |
| 8 | 型で表現不能にする（newtype / trait 境界） | **できない（費用が非現実的）** | callback が受け取るのは `&mut egui::Ui` である（`snotra-egui-runtime/src/runtime.rs:25`）。`Ui` は egui の型で `ui.ctx() -> &Context` を公開しており、`Context` への到達を型で塞ぐには **`Ui` 全体をラップして egui の API 面を再輸出する**ほか無い。`setup` は `&egui::Context` を直接渡す（`:23`）ので、その口も別に塞ぐ必要がある。#536（`OwnedTimer`）のような「表現不能にする」転換が効く形ではない——**塞ぐ対象が自分の型ではなく上流の型だからである** | 非現実的 |
| 9 | `#[deprecated]` | **できない** | 外部 crate の item に属性は付けられない。`egui` は vendor していない（`Cargo.toml:11` の `egui = "=0.35.0"` は crates.io 依存） | — |

### 1.1 「代替できない」の内訳を一言で

**#2 と #4 は「できない」ではなく「もう在る」。** clippy はこの repo で
**CI と PostToolUse hook の両方**に、しかも `-D warnings` 付きで既に配線されている。
`clippy.toml` が足すのは runner でも job でもなく、**その既存 runner への入力**である。

残る #1・#5・#7 はいずれも「clippy が型解決込みでやっていることを、字面で手書きし直す」道であり、
**これが本枠の選好が名指しで避けよと言っている形**である
（`docs/development-principles.md`「6. 検出は構造化された信号で行い…」・
`governance-check.mjs:318`「字面ではなく構文的位置で判定する」）。

---

## 2. 唯一の実装可能な代替案（`G-*` の grep 検査）を、あえて評価する

「新種のファイルを増やさない」を最優先に置くなら、次の形は**実装できる**:

> `src-tauri/src/**/*.rs` を走査し、`\.(set_visuals|set_visuals_of|style_mut_of|set_style_of|
> global_style_mut|set_global_style|all_styles_mut|style_ui|settings_ui)\(` に**コメント行以外で**
> 一致したら finding を出す。

**偽陽性は現時点で 0 件である**（実測）。リポジトリ全域の `.rs` に上の正規表現を当てると、
`snotra-settings/src/app.rs:52` と `snotra-settings/src/style.rs:81` の**正当な 2 件しか出ない**——
`view.rs` にある 16 行のコメント言及は、いずれも `ctx.set_visuals` を**開き括弧なしで**書いており
当たらない。`src-tauri/src/` に絞れば 0 件になる。

**それでも一次検査としては推さない。** 理由は 3 つで、いずれも clippy との能力差である。

1. **UFCS を落とす。** `egui::Context::set_visuals(&ctx, v)` は上の正規表現に当たらないが、
   clippy は発火する（独立レビュー §5-R5 の実測）。**抜け道の集合が clippy より広い。**
2. **発火が遅い。** `selectChecks`（`post-edit.mjs:125-157`）は `.rs` の編集に
   `fmt` / `clippy` / `tauri-test` を割り当てるが `governance:check` は割り当てない。
   grep 検査は **PR の CI でしか鳴らない**。clippy は**書いた直後の hook で鳴る**。
3. **偽陽性 0 件は今日の性質であって不変条件ではない。** 誰かが doc コメントに
   `ctx.set_visuals(visuals)` と括弧つきで例示した瞬間に赤くなり、次の人の最も安い直し方は
   「検査を緩める」になる。clippy はコメントを見ないので構造的にこの問題を持たない。

→ **結論: 一次検査は clippy。grep 検査は不要。** ただし §4 で述べるとおり、
**同じ `G-*` の枠は「`clippy.toml` の生存」を守る二次検査としてなら価値がある**——
そちらは字面判定で十分であり（対象が設定ファイルそのものだから）、能力差が問題にならない。

---

## 3. 問い 2 — `clippy.toml` を新設する費用

### 3.1 増えるものの正体

| 観点 | `clippy.toml` が増やすもの |
|---|---|
| 新しい runner | **0**（`cargo clippy` は `ci.yml:126` と `post-edit.mjs:308-312` に既在） |
| 新しい CI job | **0** |
| 新しい npm script | **0** |
| 新しい**ファイル種別** | **1**（`.md` でも `.rs` でも `Cargo.toml` でもない） |
| 新しい**判定ロジック** | **0**（判定は clippy 本体が持つ） |

**構造上の双子が既に repo 内に在る。** ルート `Cargo.toml:21-24` の `[workspace.lints.rustdoc]` は
「既存ツール（`cargo doc`）の設定ノブを置き、既存の CI step がそれを実効させる」という
**まったく同じ形**である。違いは、それが `Cargo.toml` という既知の種別に収まった点だけで、
`disallowed-methods` にその道は無い（→ §1 の #6）。

### 3.2 「沈黙しうる経路をすべて塞ぐ」規範との関係 — 抵触しない

ルート `CLAUDE.md`「フック」の当該文を字義どおり読む:

> **沈黙しうる経路はすべて塞いであり、その閉塞を壊す変更を `.claude/hooks/` に入れてはならない**

**射程は `.claude/hooks/` の出力経路である**（「検出は exit code、出力は証拠」・#471）。
`clippy.toml` の新設は `.claude/hooks/` を 1 行も変えないので、この禁止には当たらない。

隣接する規範のほうが関係する:

> 沈黙が「合格」なのは `selectChecks` に検査が割り当てられたファイルだけである（#497）。
> `*.md` 全般・`SPEC.md`・`scripts/` 配下の非 TS ファイル・`.github/workflows/`・`Cargo.lock` の
> 沈黙は「何も走らなかった」である

**`clippy.toml` はこの列挙に加わる**（`selectChecks` に `.toml` の分岐は `Cargo.toml` と
`config.toml` の 2 つしか無い・`post-edit.mjs:61,145`）。plan は `plan.md:171-173` で
この事実を正しく認識している。

ただし**非対称が 1 つある**——列挙された既存の面々（`*.md`・`.github/workflows/`）と違い、
`clippy.toml` は**他のファイルを編集したときに間接的に行使される**。`.rs` を 1 行でも触れば
hook の clippy が走り、そのとき `clippy.toml` は必ず読まれる。ゆえに
「`clippy.toml` は編集時に沈黙するが、**日常の `.rs` 編集で常に行使されている**」という
**既存の列挙より良い性質**を持つ。この差は plan にも issue にも書かれていない（→ 軽-1）。

**ただしこの性質には §5 の但し書きが付く**（fingerprint に入らないため、`.rs` を触らない限り
キャッシュが replay されうる）。

### 3.3 残る本当の費用

1. **新種ゆえ、次に `clippy.toml` を読む人が「これは誰が検査しているのか」を自力で調べる。**
   plan はその答え（誰も検査していない）を `clippy.toml` 自身のコメントへ書く設計で、これは正しい。
2. **`governance:check` の走査元に入らない。** `G-references` / `G-heading-refs` /
   `G-stale-identifiers` の**走査元**は `.md` と `.rs` に限られる（`governanceDocs` `:1111-1120` /
   `headingRefDocs` / `headingRefSourceDocs`）。ゆえに `clippy.toml` の**コメントに書いた参照は腐っても
   誰も気づかない**。plan `:107-109` はこれを正しく認識し「見出し参照の正準形・行番号を書かない」と
   自己制約している——**この制約は守られる限り正しい対処である**（規範であって機構ではないので、
   守られなかったときに鳴るものは無い）。
3. **R7（ルート `clippy.toml` の遮蔽）**。独立レビューが実測済み。新種ファイル固有の費用で、
   plan はコメントへ残す設計。妥当。

**判定: `clippy.toml` の新設は、この repo の規範に抵触しない。**
費用は「新種のファイル 1 つ」であって「新種の検査機構 1 つ」ではない。

---

## 4. 問い 3 — `clippy.toml` の生存を既存機構で守れるか

### 4.1 実在は **`G-references` が既に守る**（条件つき・実測）

`REF_EXTENSIONS`（`governance-check.mjs:30`）は `.toml` を**含む**。
`governanceDocs`（`:1111-1120`）は `src-tauri/CLAUDE.md` を**含む**（実測で確認）。
`checkReferences`（`:161-214`）のバッククォート述語は「`/` を含む・glob 無し・`\` 無し・
拡張子が `REF_EXTENSIONS`・`workspace/` 配下でない」で、**`src-tauri/clippy.toml` はこれを満たす**。

`checkReferences` へ入力を注入して測った（リポジトリ無改変）:

```
A(clippy.toml が在る):   []
B(clippy.toml を消した): [{"file":"src-tauri/CLAUDE.md","line":2,
                          "message":"バッククォート参照のパスが実在しない: src-tauri/clippy.toml"}]
C(ベア名 `clippy.toml`): []          ← `/` を含まないので述語が弾く
D(誤った 2 セグメント):  [finding]   ← `snotra/clippy.toml` は解決しない
```

さらに実 repo の `makeSnapshot` を走らせ、walker が拡張子で絞らず
`src-tauri/Cargo.toml` `src-tauri/tauri.conf.json` を列挙することを確認した（345 ファイル）。
ゆえに `src-tauri/clippy.toml` も `snapshot.files` に入る。

**finding が CI を赤にすることも確認した**——`governance-check.mjs:1622-1628` の `isMain` ブロックは
`findings.length > 0` のとき `process.exitCode = 1` を立てる（`:1627`）。
ゆえに finding は「印字されるだけ」ではなく `ci.yml:64` の step を落とす。

**帰結**: Phase 3 の 1 文が **`` `src-tauri/clippy.toml` `` というルート相対形**で、かつ
**コードフェンスの外**に書かれるなら（`checkReferences` は `linesOutsideFences`（`:67-79`）を通した行しか見ない）、
`git rm src-tauri/clippy.toml` だけのコミットは **CI の `governance-check` job（`ci.yml:51-64`）で赤くなる**。

**これは `plan.md:182`・`:203-204` と独立レビュー §5-R8 の「`clippy.toml` の消失は
CI も hook も governance:check も緑のまま通す」を条件つきで反証する。** 条件は 2 つ:

- **(必須)** 文言が `` `src-tauri/clippy.toml` `` という**ルート相対形**で、かつ**コードフェンスの外**に在ること。
  plan `:142` の文言案「`src-tauri/clippy.toml` の `disallowed-methods` が機構で守る」は満たすが、
  **ケース C が示すとおり `` `clippy.toml` `` と縮めた瞬間に保護が消える**し、
  同じ文をコードフェンスの中へ移しても消える（`linesOutsideFences`・`:67-79`）。
  **どちらも黙って消える**。これは plan に書かれていない前提であり、
  **規範ではなく機構として固定するには §4.2 が要る**。
- **(残余)** ファイルと文の**両方**を消すコミットは通る。これは残余として正しく受容できる
  （2 か所を同時に消すのは偶発ではなく意図である）。

### 4.2 内容は `G-clippy-disallowed` 1 本で塞げる（推奨）

**塞ぐ範囲の決め方は repo に前例がある。** `G-workspace-lints` のヘッダは
「**塞ぐのは cargo が exit 0 で沈黙した次の 6 経路だけである**」（`:294`）と
「**沈黙しない経路に見張りは置かない**」（`:300`）でスコープを定めている。
`clippy.toml` は `[workspace.lints.rustdoc]` と**同じ失敗の形**（設定ノブが黙って無効化する）を
持つので、同じ規則を当てればよい。

**`.githooks/` との違いも明確である**——あちらが「生きているかの検知を意図的に作らない」のは
**最終防衛線（GitHub ruleset）を持つから**（`_lib.sh:8-11`）。`clippy.toml` に最終防衛線は無い。

#### 何を入力に、何を判定するか

| 項目 | 内容 |
|---|---|
| 入力 1 | `snapshot.read("src-tauri/clippy.toml")` |
| 入力 2 | `snapshot.read("src-tauri/Cargo.toml")` |
| カナリア | `export const REQUIRED_DISALLOWED_METHODS = [ …9 パスの文字列… ]`（`REQUIRED_RUSTDOC_LINTS`（`:316`）と**同じ位置・同じ根拠**。同 doc が「名指しは意図的である——片方の行が消えた形が緑を通る」とハードコードを既に弁護している） |
| 判定 A | `clippy.toml` が読めない → finding（`G-*` の「母集団の欠落」定型） |
| 判定 B | `path = "..."` の値を行単位で抽出（`tomlLine` / `rustdocLintsAreDenied`（`:337-357`）と同じ書き方）し、カナリア 9 件が**すべて在る**こと |
| 判定 C | `src-tauri/Cargo.toml` が egui 依存を宣言していること（全パスが解決する前提そのもの） |

#### この検査が塞ぐ経路（＝ 発火しうるか）

| 経路 | clippy 側の挙動 | この検査 |
|---|---|---|
| ファイル削除 | 沈黙・exit 0 | **鳴る**（判定 A。§4.1 と二重になるが、あちらは文言に依存する） |
| `disallowed-methods` を空配列化 | 沈黙・exit 0 | **鳴る**（判定 B） |
| 9 件のうち 1 行だけ消える | 沈黙・exit 0 | **鳴る**（判定 B） |
| メソッド名・型名の書き損じ | warning のみ・**exit 0**（実測済み） | **鳴る**（判定 B——書き損じた文字列はカナリアと一致しない） |
| crate 名の書き損じ（`eguii::`） | **診断ゼロ**（実測済み） | **鳴る**（判定 B） |
| `egui` 依存の消滅 | **診断ゼロ** | **鳴る**（判定 C） |
| `reason` 文言の変更 | — | 鳴らない（意図的に射程外） |
| `#[allow]` による迂回 | 迂回できる | 鳴らない（射程外・lint に内在する性質） |

**「走る場所で発火しうるか」**（`measure-whether-detector-can-fire`）: 走る場所は
`node scripts/governance-check.mjs`（`ci.yml:64`・全 PR で常時実行・`skip-ci` 非対象）で、
**cargo のキャッシュを一切介さない Node の静的読み取り**である。上表の 6 経路すべてが、
その入力（`clippy.toml` のテキスト）の変化として現れる。**clippy 自身が沈黙する 6 経路が、
この検査では 6 経路とも入力の差分になる**——それがこの検査を冗長でなくしている性質である。

#### 塞ぐ価値の判定 — **在る**

plan は「`clippy.toml` 自体が生きている | **無い（受容残余）**」（`:182`）を受容し、
そのうえで **Phase 2 の 9 件注入を「この機構が生きていることの唯一の検知点」**と書く（`:201`）。
だが**フォールトインジェクションは実装時の 1 回きりの測定であって、継続的な検知器ではない**。
6 か月後に誰かが 1 行消したとき鳴るものは、この検査が無ければ本当に 0 である。

#### 正直な費用

- **9 パス文字列が 2 か所に写る**（`clippy.toml` と `governance-check.mjs`）。
  これは `REQUIRED_RUSTDOC_LINTS` と同じ受容済みの形で、**ズレたらこの検査自身が赤くなる**ので
  腐りは検知される。ただし「正当に 10 件目を足すとき 2 か所を直す」費用は実在する。
- **発火は CI のみ。** `selectChecks` は `.md` にも `.toml` にも検査を割り当てない。
- 実装 ~30 行 + `governance-check.test.mjs` のフィクスチャ 1〜2 件。

**ただし本枠の裁定は「価値は在る」までである。** issue #900 の作業項目に無い追加であり、
本 PR に含めるか別 issue へ送るかは呼び出し側の判断（→ 要-3）。

---

## 5. 私が新しく測った沈黙経路 — `clippy.toml` は cargo の fingerprint に入らない

**誰も測っていない。** scratchpad の使い捨て crate で、`-D warnings` を含む**同一コマンド**を
前後で打って測った（cargo/clippy 0.1.94）。

```
A: clippy.toml 無し                          → EXIT 0 / disallowed 0 行
B: clippy.toml を置いた直後（.rs 無改変）    → EXIT 0 / disallowed 0 行 / "Finished in 0.01s"
C: clippy.toml を消した直後（.rs 無改変）    → EXIT 0 / disallowed 0 行
D: clippy.toml + touch src/lib.rs            → EXIT 101 / disallowed 4 行
E: D の直後にもう一度（.rs 無改変）          → EXIT 101 / disallowed 4 行
```

B が 0.01s で終わって設定を適用していないのに対し、D は同じ設定・同じコマンドで赤くなる。
**設定は正しい。cargo が `clippy.toml` の変化を fingerprint に含めないだけである。**
（独立レビュー §3-A が `CLIPPY_CONF_DIR` の測定で `-A clippy::needless_return` を
「fingerprint を変えて再リントさせるためだけ」に付けていたのは、同じ現象への回避策である。）

### 5.1 CI では効かない（確認済み）

**`rust-cache` が workspace crate を保存しない限り**、CI の全 run で `snotra` はゼロから
コンパイルされ、`clippy.toml` は必ず適用される。この前提は repo 内に実測として記録がある
（`docs/adr/ADR-smoke-build-time.md:14`・`.github/workflows/e2e.yml:99`）が、
**どちらも smoke／release 側の記録であって `rust-check` job について測った記録ではない**——
性質は `Swatinem/rust-cache` 一般のものと思われるが、私はそれを一次で測っていない。
前提が成り立つ限り **CI 側にこの穴は無い**。要-2 の対処は手元の経路についてのものなので、
この前提の成否に関わらず有効である。

### 5.2 手元と hook では効く（要対処）

- **`clippy.toml` だけを編集した後の `cargo clippy` は、キャッシュ replay で緑を返しうる。**
  `selectChecks` は `clippy.toml` に検査を割り当てないので hook も走らない。
  **手元の「緑」が二重に空振りする形**である。
- plan Phase 5 の受け入れ条件 3（clean な作業ツリーで exit 0）は、直前の
  `git checkout -- view.rs` が `.rs` の mtime を動かすので**たまたま有効**である。
  だが「Phase 1 の直後に clippy を打って exit 0 を確認する」順序で回すと**無意味な緑**になる。
- **Phase 2 の 9 件注入は有効である**（`view.rs` を書き換えるので fingerprint が必ず動く）。
  plan の順序は結果として正しいが、**正しい理由が書かれていない**。

**対処**: plan の検証手順に「`clippy.toml` 単独の変更を測るときは `.rs` を触るか
`cargo clean -p snotra` を挟む」を 1 行足す。`clippy.toml` のコメントの「沈黙経路」節にも
3 本目として並べる価値がある（既に 2 本を並べる設計なので置き場所は在る）。

---

## 6. `clippy.toml` より良い道具は在るか

**無い。**

- 一次検査（禁止そのもの）: clippy の `disallowed_methods` が**この repo で唯一、型解決を持ち、
  かつ既に CI と hook の両方へ配線済みの道具**である。代替候補（§1 の #1・#5・#7）は
  すべて字面判定で、抜け道の集合が広く、発火も遅い。
- 型で表現不能にする道（この repo が #536 で好んだ形）は、塞ぐ対象が**上流の型**であるため
  費用が非現実的（§1 の #8）。
- 唯一の追加提案は**代替ではなく補完**である: `clippy.toml` の生存を守る `G-clippy-disallowed`（§4.2）。

---

## 7. 所見の 3 分類

### 要対処（3 件）

| # | 所見 | 根拠 | 対処 |
|---|---|---|---|
| **要-1** | **`plan.md:182,203-204` と独立レビュー §5-R8 の「`clippy.toml` の消失は governance:check も緑」は、条件つきで偽である。** Phase 3 の文が `` `src-tauri/clippy.toml` `` の形なら `G-references` が実在を検査する。**ただし `` `clippy.toml` `` と縮めた形では保護が消える**——この依存は plan のどこにも書かれていない | `governance-check.mjs:30`（REF_EXTENSIONS に `.toml`）/ `:161-214`（述語）/ `:1111-1120`（`src-tauri/CLAUDE.md` が母集団）/ 入力注入の実測 A〜D / `makeSnapshot` の実測 | plan の残余記述を「**文言がルート相対形である限り**実在は `G-references` が守る／両方消すコミットは通る」へ訂正する。**Phase 3 の文言をルート相対形に固定することを受け入れ条件へ格上げする**（縮めると黙って保護が消えるため） |
| **要-2** | **`clippy.toml` は cargo の fingerprint に入らない（新規実測）。** `clippy.toml` 単独の変更後に同一コマンドを打つと、キャッシュ replay で**設定を適用せずに exit 0** を返す。`selectChecks` が `clippy.toml` に何も割り当てないため hook も走らず、**手元の緑が二重に空振りする** | §5 の A〜E（scratchpad 実測）/ `post-edit.mjs:61,125-157` / CI 側は前提つきで安全（`ADR-smoke-build-time.md:14`「rust-cache は workspace crate を保存しない」——ただし §5.1 の但し書き） | plan の検証手順へ「`clippy.toml` 単独の変更を測るときは `.rs` を触るか `cargo clean -p snotra` を挟む」を明記。Phase 2 の注入が有効な**理由**（`.rs` を書き換えるから）も 1 句で残す。`clippy.toml` の沈黙経路の列挙へ 3 本目として足す |
| **要-3** | **`clippy.toml` の生存を守る `G-clippy-disallowed` は実装可能で、価値が在る。** clippy が沈黙する 6 経路すべてが、この検査では入力の差分として現れる。plan は「フォールトインジェクション＝唯一の検知点」（`:201`）と書くが、それは実装時 1 回の測定であって継続的な検知器ではない | `governance-check.mjs:294,300`（`G-workspace-lints` のスコープ規則＝同じ失敗の形への前例）/ `:316`（カナリアのハードコード弁護）/ `:337-357`（TOML 行パースの書き方）/ `.githooks/_lib.sh:8-11`（検知器を置かない場合の条件＝最終防衛線の存在。`clippy.toml` にはそれが無い） | §4.2 の形で 1 本足す。**ただし #900 の作業項目に無い追加である**——本 PR に含めるか別 issue へ送るかは呼び出し側の判断 |

### 軽微（3 件）

| # | 所見 |
|---|---|
| 軽-1 | **`clippy.toml` は「沈黙する新種ファイル」の中で最も良い性質を持つ**——`.rs` を 1 行でも触れば hook の clippy が読むので、日常的に行使される（`post-edit.mjs:126-127,308-312`）。ルート `CLAUDE.md` が列挙する `*.md` / `.github/workflows/` にこの性質は無い。plan にも issue にも書かれていないので、`clippy.toml` のコメントか PR 本文に 1 句あるとよい（**ただし要-2 の但し書き付き**） |
| 軽-2 | **`clippy.toml` のコメント内の参照は `governance:check` の走査元に入らない**（`.md` と `.rs` のみ）。plan `:107-109` は正しく自己制約しているが、**これは規範であって機構ではない**——守られなかったときに鳴るものは無い。要-3 の検査を足すなら、`reason` 文言や参照の照合まで射程を広げない判断も同時に明示するとよい（私は広げないことを推す。`G-workspace-lints` の「沈黙しない経路に見張りは置かない」と同じ理由ではなく、**費用対効果で**） |
| 軽-3 | 字面 grep の `G-*` 検査は**現時点で偽陽性 0 件**（実測: リポジトリ全域の `.rs` に 9 メソッドの `\.name\(` を当てて、正当な 2 件しかヒットしない）。一次検査としては推さないが、この測定値は「代替案を検討して落とした」ことの一次証拠として plan の「より単純な既存パターンで置き換えられないか」（セルフレビュー 4）へ残す価値がある |

### 未検証（3 件）

| # | 所見 | 何を測れば決着するか |
|---|---|---|
| 未-1 | ⚠ **`G-references` による保護は、`src-tauri/CLAUDE.md` 側の文言に依存する規範的な結合である。** 私が測ったのは `checkReferences` の述語であって、Phase 3 の**実際の文**ではない（まだ書かれていない） | Phase 3 実装後に `npm run governance:check` を打ち、その後 `clippy.toml` を一時的に `git stash` して**赤くなること**を実測する。plan の検証手順へ 1 行として足せる |
| 未-2 | ⚠ **要-3 の検査の TOML 行パースが、実際の `clippy.toml` の書式で機能するかは未測定。** `disallowed-methods` は複数行のインラインテーブル配列になる見込みで、`rustdocLintsAreDenied`（`:337-357`）の行単位パーサとは形が違う（`{ path = "...", reason = "..." }` が 1 行に収まるか複数行に割れるかで抽出が変わる） | 実装時に `path\s*=\s*"([^"]+)"` を全行に当てる形で書き、`governance-check.test.mjs` へ「1 行形」「複数行形」の 2 フィクスチャを置いて測る |
| 未-3 | ⚠ **fingerprint の測定は依存 0 の単一 crate で行った。** `snotra` のような依存の多い workspace member で、`Cargo.lock` や build script の絡む条件でも同じかは測っていない。**向きは安全側に倒れない**（依存が多いほうがキャッシュが効きやすい＝空振りしやすい）ので、要-2 の対処は依存の有無に関わらず有効 | 実 repo で `clippy.toml` を置いた後に `.rs` を触らずに clippy を打ち、`Finished in <1s` かつ `disallowed` 0 件になることを確認する（`clippy.toml` の作成を伴うため本レビューでは実施していない） |

---

## 付録: 本レビューで実施した測定（すべてリポジトリ無改変）

| 測定 | 方法 | 結果 |
|---|---|---|
| `G-references` が `src-tauri/clippy.toml` を見るか | `checkReferences` へ入力注入（4 ケース） | 在る→[] / 消す→**finding** / ベア名→[] / 誤パス→finding |
| walker が `.toml` を列挙するか | 実 repo で `makeSnapshot` を実行 | 345 ファイル。`src-tauri/Cargo.toml` `src-tauri/tauri.conf.json` を確認 |
| `src-tauri/CLAUDE.md` が `governanceDocs` に居るか | 同上 | **true** |
| `clippy.toml` が fingerprint に入るか | scratchpad の使い捨て crate・同一コマンド前後 5 回 | **入らない**（置いた直後は 0.01s で exit 0・.rs を touch すると exit 101） |
| 字面 grep の偽陽性 | リポジトリ全域の `.rs` に 9 メソッドの `\.name\(` | **2 件のみ**（どちらも `snotra-settings` の正当な呼び出し。`src-tauri` は 0） |
| ソース走査型 Rust テストの前例 | 全域 grep（`include_str!` / `CARGO_MANIFEST_DIR`） | **0 件**（前例が無い） |
| `selectChecks` の `.toml` 分岐 | `post-edit.mjs:61,125-157` を読解 | `Cargo.toml` と `config.toml` のみ。`clippy.toml` は 0 検査 |
| `G-*` 検査の総数と登録形 | `governance-check.mjs:1575-1592` | 18 本。`{ id, run }` の配列に 1 行足す形 |
| `.githooks/` の判定内容 | `pre-commit`（8 行）・`_lib.sh`（全文） | 分岐名の比較 1 つのみ。cargo も node も起動しない |
| CI の clippy / governance 配線 | `ci.yml:126` / `:51-64` / `:105-109` | clippy は既在。governance は Node のみ。rust-cache は workspace crate 非保存（`ADR-smoke-build-time.md:14`） |
| callback が受け取る型 | `snotra-egui-runtime/src/runtime.rs:22-25` | `update(&mut self, ui: &mut egui::Ui, …)`・`setup(&mut self, _context: &egui::Context)` |
