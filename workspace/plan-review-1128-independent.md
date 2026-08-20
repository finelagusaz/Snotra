# 独立再導出 — issue #1128（`43,939 µs` を `PERFORMANCE.md` の 1 節へ寄せる）

対象 issue: **#1128**
導出者: 独立枠（コードと文書のみから導出。`workspace/` は読取・grep 走査の双方から除外した）
ブランチ: `chore/perf-value-single-source` / 実測日: 2026-08-20

---

## 0. 母集団の数え上げ（item 1）— 実測

### 除外句の差を測った（除外あり/なしで差ゼロ）

```
git grep -c "43,939" -- .                          → 5 ファイル
git grep -c "43,939" -- . ':(exclude)workspace'    → 5 ファイル（同一）
```

差がゼロであることは**証明できる**——`git status --porcelain` の untracked は `?? workspace/` の
1 件だけであり、`git grep` は追跡ファイルしか見ないので `workspace/` は元から母集団の外にある。
ゆえにこの除外句は結果を変えていない（`grep-exclusion-drops-more-than-intended` 型の
取りこぼしはこの走査では起きていない）。

### 別綴りの走査（全件ゼロを実測）

| 綴り | 走査 | 結果 |
|---|---|---|
| `43,939` | 全追跡ファイル | **5 ファイル 6 行**（下表） |
| `43939` / `43 939` / `43.94` / `43.9 ms` | `git grep -nE` | 0 件 |
| 全角数字（`４３` / `９３９` / `４４` / `３９`） | **ASCII フィルタを通さない裸の grep** | **0 件** |
| `44 ms` / `約 4[34]` | 全追跡ファイル | 2 件だが**別物**（`PERFORMANCE.md:80` は再表示レイテンシ、`scripts/occupy-hotkey.ps1:98` は CPU 負荷） |

**全角の走査は ASCII フィルタを外して単独で回した**——最初の走査は `\| grep -E "43\|44"` を挟んで
おり、全角しか含まない行はそこで落ちていた（同じ形の穴が訂正差分に残らないよう測り直した）。

`.ps1` / `.mjs` / `.toml` / `.yml` / `.json` / `.github/` / `.claude/` / `SPEC.md` /
`RETROSPECTIVE.md` はすべて **0 件**（上の全追跡ファイル走査に含まれる）。`Cargo.lock` の
`43939` 様のヒットは checksum の 16 進であり無関係。

### 生きた層の母集団 — 確定 4 か所

| # | file:line | シンボル / 節 | 現在の綴り |
|---|---|---|---|
| 1 | `snotra-core/src/engine.rs:262` | `Engine::config_handle` の rustdoc | `` （`read_window_width` 単独で 43,939 µs の待ちを実測した・`PERFORMANCE.md`）。 `` |
| 2 | `src-tauri/src/egui_shell/mod.rs:420-421` | `egui_shell::read_config` の rustdoc | `` ——`read_window_width` 単独で 43,939 µs の待ちを実測した（`PERFORMANCE.md`「フレーム↵後半の帰属」）。 `` |
| 3 | `src-tauri/src/state.rs:148` | `mod tests` の `ui_reads_config_while_the_engine_lock_is_held` の rustdoc | `` 単独で 43,939 µs の待ちを実測した。 `` |
| 4 | `src-tauri/CLAUDE.md:57` | 「モジュール構成」節の **config の読みは `read_config` を通す** 条項 | `` `read_window_width` 単独で 43,939 µs を実測し、60fps の予算を超えたフレームが 11 本あった `` |

正本側（**触らない**）: `PERFORMANCE.md:529`（`911〜43,939`・「フレーム後半の帰属」表）と
`PERFORMANCE.md:566`（`43,939`・A/B 表）。
射程外（**触らない**）: `docs/adr/`（凍結された歴史）。実測でも `docs/adr/` に `43,939` は 0 件。

---

## 1. 正本の節の特定（item 2）

**`PERFORMANCE.md:557`**

```
#### 設定の読みを engine lock の外へ出す — #1032 の A/B（同じ器・同条件・3 標本）
```

**この節を選ぶ理由**（`43,939` は 2 節に跨がって載っているので、明示的に裁定する）:

- issue の文面が「#1032 の **A/B 実測値**」と名指している。`43,939` を A 側の値として持つのは
  この節の表（`PERFORMANCE.md:566`）である。
- `src-tauri/CLAUDE.md:57` と `docs/architecture.md:231` の既存参照が**すでにこの節を指している**
  ——寄せ先を変えると、腐っていない参照 2 件を無用に動かすことになる。
- もう一方の候補「フレーム後半の帰属」（`PERFORMANCE.md:513`・`911〜43,939` の帰属表）も
  アンカーとしては着地する。**`src-tauri/src/egui_shell/mod.rs:420` だけが現にこちらを指している**
  ので、**A/B 側へ正規化する**ことを提案する（下の #2 参照）。理由は「4 か所が同じ 1 節を指す」
  ほうが正本が 1 つに定まるためで、帰属の議論を読みたい読者は A/B 節の直前の兄弟節へ辿れる。
  ⚠️ ここは判断であり、issue が節名を名指していないため**逆の裁定（帰属側へ揃える）もありうる**。

### 参照の「正準形」— 機構から導いた

`scripts/governance/lib.mjs:168`

```js
export const HEADING_REF = /`([^`\n]+)`\s*(?:§\s*[\d.]*\s*)?「([^「」\n]+)」/g;
```

すなわち **`` `<path>.md`「<見出し>」 ``**（対象は `<path>.md` か `/skill-name`・
`isRefTargetSpelling`）。規範側の宣言は `.claude/rules/governance-docs.md`:

> 他を指すときは正準形 `` `<対象>`「<見出し>」 ``（対象は `<path>.md` か `/skill-name`）で書く。
> **この形だけが `governance:check` の G-heading-refs で照合され**、見出しの改名・消滅は
> 参照元を名指しして CI が落とす。**走査元は `.md` だけではない——コード（`.rs`）のコメントに
> 書いた参照も同じ検査に載る**（#925）

照合は `normAnchor`（`` ` ``・`*`・`「」`・空白を除去）後の**前方一致**なので、後置の括弧注記
（`— #1032 の A/B（同じ器・同条件・3 標本）`）は書かなくてよい。**実測**:

```
"設定の読みを engine lock の外へ出す" -> 着地 1 件 | 設定の読みをenginelockの外へ出す—#1032のA/B（同じ器・同条件・3標本）
"フレーム後半の帰属"                   -> 着地 1 件 | フレーム後半の帰属—#1032（2026-08-11・release・実運用点312,180件・3標本×2巡）
```

どちらも**着地は 1 件ちょうど**（曖昧化しない）。

---

## 2. 機械照合されるための条件（item 3）— **本レビュー最大の所見**

母集団は `scripts/governance/lib.mjs` の 3 本の腕（`allHeadingRefDocs`）:

| 腕 | 関数 | 4 か所のうち | 走査する行 |
|---|---|---|---|
| `.md` | `headingRefDocs` — `docs/superpowers/` / `workspace/` / `docs/adr/` を除外 | #4 `src-tauri/CLAUDE.md` | フェンス外の**全行** |
| `.rs` | `headingRefSourceDocs` — **全 `.rs`。テストコードを外さない**（明文） | #1 #2 #3 | フェンス外の全行 |
| コメント族 | `headingRefCommentDocs`（`.mjs` / `.ps1` 等） | 該当なし | コメント行のみ |

`resolveRefTarget` は「文書ディレクトリ基準 → リポジトリルート → 一意な suffix 一致」の順で解決
するので、`snotra-core/src/engine.rs` から書いた `` `PERFORMANCE.md` `` はルート直下の
`PERFORMANCE.md` へ解決する（#4 が現に緑であることが実証）。

### 実測 — 現状 4 か所のうち **機械照合されているのは 1 か所だけである**

実際の `HEADING_REF` を 4 ファイルへ当てた結果:

```
src-tauri/src/egui_shell/mod.rs: PERFORMANCE.md への正準形マッチ 0 件
snotra-core/src/engine.rs:       PERFORMANCE.md への正準形マッチ 0 件
src-tauri/src/state.rs:          PERFORMANCE.md への正準形マッチ 0 件
src-tauri/CLAUDE.md:57  target=PERFORMANCE.md label=設定の読みを engine lock の外へ出す
```

機序は 2 つ:

1. **#1 と #3 は見出しを持たない**（裸の `` `PERFORMANCE.md` `` / 参照そのものが無い）ので、
   `HEADING_REF` の `「…」` 群に当たらない。
2. **#2 は正準形に見えるが物理改行でまたいでいる。** `HEADING_REF` の両群は `[^…\n]+` で
   改行を許さず、しかも `scanHeadingRefs` は `refScanLines` が返す**行ごと**に `line.matchAll`
   を掛ける。ゆえに `` 「フレーム↵/// 後半の帰属」 `` は**検査の視界に入らない**。
   `G-near-heading-refs` も同じく行単位なので、**こちらも拾えない**（二重の死角）。

**対照実験（フォールトインジェクション）で確かめた**——本物の `scanHeadingRefs` を注入
スナップショットへ当てた:

```
提案 4 件:            照合件数 checked = 4 / findings = 0     ← 4 件とも正準形として拾われ、着地する
[対照1] 折返し形:      checked = 0 / findings = 0             ← 現行 #2。検査は「見なかった」だけで緑
[対照2] 裸の対象名:    checked = 0 / findings = 0             ← 現行 #1 / #3。同上
[対照3] 実在しない見出し: checked = 1 / findings = 1
        → 見出し参照が着地しない: `PERFORMANCE.md`「存在しない節の名前」（PERFORMANCE.md に該当する見出し・リード文が無い）
```

対照 3 が赤くなることで、**この検知器が実際に発火しうる**ことを確かめてある（
`measure-whether-detector-can-fire` の作法）。対照 1・2 の `checked = 0` は
「緑だったのではなく数えられてすらいなかった」の実証である。

### ゆえに書き換え時の**必須条件**

> **正準形 `` `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」 `` の全体を、
> 1 本の物理行に収めること。** `///` の中でも折り返してはならない。

これは偶然の作法ではなく、`docs/comment-guidelines.md`「日本語の折返し」が
**すでに規範として持っている**（下記 item 4）。#2 の現行の壊れ方は、その規範が予告していた害
（「折返しは `grep` を壊す」）が**機械検査にも当たっていた**実例である。

---

## 3. 守るべき規約（item 4）

### `docs/comment-guidelines.md`

- **`docs/comment-guidelines.md:57`「日本語の折返し」**: 「**文途中で物理改行を入れない**
  （1 段落 1 行）。**適用は新規に書くコメントと、その変更で触った段落だけである**」
  → 4 か所とも「触った段落」になるので、**その段落は 1 行へ畳む**。
  「**とくにコードスパン（バッククォートで囲んだ識別子・コマンド）を行またぎさせない**」が
  正準形にそのまま当たる。
- **`docs/comment-guidelines.md:31`**: 「直すときは、根拠を腐らない形へ移す: **過去の行為**
  （「〜する」→「〜した」）・**過去の観測**（日付つきの実測）・**参照（正本を指す）**」
  → #1128 がやろうとしているのは、この 3 番目そのものである。
- **`docs/comment-guidelines.md:96`**: 「**実測値には条件を添える**（日付・エントリ数・試行回数
  など）。条件のない数値は再検証できない」→ 4 か所の `43,939` はいずれも条件（実運用点 312,180
  件・3 標本・2026-08-11）を伴っておらず、**規約違反の状態にある**。落とすのが正しい。
- **`docs/comment-guidelines.md:51`**: `///` の帰属が動いていないか目視する（挿入は既存アイテムの
  **後ろ**へ）。今回は削減側なので低リスクだが、#3 は `#[test]` 直前の doc なので確認する。

### `.claude/rules/governance-docs.md`

- 「(1) **かぶりなく**——同じ主張が別の場所に無いか grep で確かめ、あれば**正本 1 か所に寄せて
  他は参照にする**」— #1128 の指示そのもの。
- 「**既に消滅した節の名前を正準形で書かない**」— 今回は消滅しないので該当しないが、
  節を**改名する**選択肢を採らないことの根拠になる（改名すれば 4 か所を同時に直す必要が出る）。

### リポジトリに実在する**先例の言い回し**（新造しない）

`額は `PERFORMANCE.md`「<見出し>」` は既に定着した綴りである:

- `src-tauri/src/icon.rs:45` — `` `PERFORMANCE.md`「撤去: アイコン剪定そのもの」が正本（**数値をここへ写さない**——再測定された日にこの行だけが古くなる） ``
- `snotra-core/src/search/path_store.rs:19` — `` **現在値は `PERFORMANCE.md` を正本とする**——ここに絶対値を書き足すと、次の反復のたびに 2 か所を直すことになる ``
- `snotra-core/src/index_tree.rs:148` / `search/scoring.rs:362` / `search/footprint.rs:206` /
  `search/build.rs:462` / `search.rs:223` — いずれも「額は `PERFORMANCE.md`「…」」

**この綴りへ寄せる**（訳語規則の観点でも「額」「正本」はこのリポジトリの既存語彙であり、造語ではない）。

---

## 4. 各所の書き換え後の文面案

> 4 案とも**本物の `scanHeadingRefs` に通して `checked = 4 / findings = 0` を実測済み**である
> （計画に書いた判定は実装前に代表入力で測る・`AGENTS.md`「検証の作法」）。

### #1 `snotra-core/src/engine.rs:259-262`（`Engine::config_handle`）

現行:
```rust
    /// **UI が毎フレーム行う live-read を、外側の `Mutex<Engine>` の外へ出すための口である。**
    /// 検索の worker は `search` の間じゅう外側の `Mutex` を握る（実運用点で 40〜95 ms）ため、
    /// UI が同じ錠越しに設定を読むと、そのフレームは worker の走査が終わるまで返らない
    /// （`read_window_width` 単独で 43,939 µs の待ちを実測した・`PERFORMANCE.md`）。
```
案:
```rust
    /// **UI が毎フレーム行う live-read を、外側の `Mutex<Engine>` の外へ出すための口である。**
    /// 検索の worker は `search` の間じゅう外側の `Mutex` を握る（実運用点で 40〜95 ms）ため、UI が同じ錠越しに設定を読むと、そのフレームは worker の走査が終わるまで返らない（額は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」が正本である——数値をここへ写さない）。
```
落としたもの: **`43,939 µs` だけ。** 害の説明（錠越しに読むとフレームが返らない）も
`（実運用点で 40〜95 ms）` も残す——後者は #1128 の射程外だからである（§6a の裁定。
#2 / #3 も同じ理由で残しており、4 か所の扱いが揃う）。

### #2 `src-tauri/src/egui_shell/mod.rs:418-421`（`egui_shell::read_config`）

現行（**正準形が改行をまたいで死んでいる**）:
```rust
/// 検索 worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点で 40〜95 ms）。
/// UI がその錠越しに config を読むと、フレームは worker の走査が終わるまで返らない
/// ——`read_window_width` 単独で 43,939 µs の待ちを実測した（`PERFORMANCE.md`「フレーム
/// 後半の帰属」）。
```
案:
```rust
/// 検索 worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点で 40〜95 ms）。
/// UI がその錠越しに config を読むと、フレームは worker の走査が終わるまで返らない——その待ちの額は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」が正本である。
```
落としたもの: `43,939 µs`。**加えて、参照先を「フレーム後半の帰属」→「設定の読みを engine lock の
外へ出す」へ正規化し、正準形を 1 行へ畳んで機械照合の視界へ入れる。**
⚠️ 節の正規化は §1 の裁定に依る。帰属側を保つ判断を採るなら
`` その待ちの額は `PERFORMANCE.md`「フレーム後半の帰属」が正本である。 `` と書けばよい
（**1 行に収める点は、どちらの節を選んでも必須である**）。

### #3 `src-tauri/src/state.rs:146-150`（`ui_reads_config_while_the_engine_lock_is_held`）

現行:
```rust
    /// worker は `engine.search` の間ずっと engine lock を握る（実運用点で 40〜95 ms）。
    /// その間に UI が同じ lock を取りに行っていたのが #1032 の主因で、`read_window_width`
    /// 単独で 43,939 µs の待ちを実測した。**この検査はその待ちが構造的に起きえないことを
    /// 測る**——engine lock を保持したまま別スレッドが config を読み切れることが受け入れ条件
    /// である。
```
案:
```rust
    /// worker は `engine.search` の間ずっと engine lock を握る（実運用点で 40〜95 ms）。
    /// その間に UI が同じ lock を取りに行っていたのが #1032 の主因である（待ちの額は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」が正本）。
    /// **この検査はその待ちが構造的に起きえないことを測る**——engine lock を保持したまま別スレッドが config を読み切れることが受け入れ条件である。
```
落としたもの: `43,939 µs`。**この doc の主題（何をこの検査が測るか）は一字も動かしていない。**
`#[cfg(test)]` の内側だが `headingRefSourceDocs` は Rust のテストコードを**意図的に外していない**
ため（#925 が見つけた腐り 1 件は現に `#[cfg(test)]` の内側だった）、ここも照合対象に載る。

### #4 `src-tauri/CLAUDE.md:57`（「モジュール構成」節・**config の読みは `read_config` を通す**）

現行（該当部分のみ）:
```
その錠越しに設定を読むとフレームが走査の完了まで返らない——`read_window_width` 単独で 43,939 µs を実測し、60fps の予算を超えたフレームが 11 本あった（A/B は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」）。
```
案:
```
その錠越しに設定を読むとフレームが走査の完了まで返らない——`read_window_width` の待ちで 60fps の予算を超えたフレームが実在した（額と A/B は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」が正本）。
```
落としたもの: `43,939 µs` **と `11 本`**（後者も同じ A/B 表 `PERFORMANCE.md:571` の値であり、
**同じ 1 文の中の写しである**——§6 の判断を参照）。
既存の正準形は綴りを変えていないので、**この行の照合は今までどおり成立する**。

---

## 5. 検証コマンド（item 7）

`docs/build-commands.md`「変更後の検証チェックリスト」より:

| カテゴリ | 根拠 | 対象 | コマンド |
|---|---|---|---|
| **A. Rust ファイル（`*.rs`）** | `docs/build-commands.md:11` | #1 #2 #3 | カテゴリ A のコードブロック（fmt / clippy / test）。**ただし PostToolUse hook が自動実行するので沈黙 = 合格** |
| **A の手動追加** | `docs/build-commands.md:29` | #1 #2 #3 | **`cargo doc` は CI（rust-check）でのみ発火し PostToolUse フックは発火しない**。「**doc コメント（`///` / `//!`）を触ったらローカルで上記コマンドを手動実行してリンク切れを確認する**」——**3 か所とも rustdoc なので、これは必須である** |
| **F. ガバナンス文書** | `docs/build-commands.md:160` | #4（`src-tauri/CLAUDE.md`）**および #1〜#3**（`.rs` のコメントも G-heading-refs の母集団） | `npm run governance:check` |

**カテゴリ F を `.rs` の変更でも回す必要がある**のがこの PR の非自明な点である——
`docs/build-commands.md:167` が明記するとおり `.rs` の hook の沈黙は
「fmt / clippy / test の合格であって**見出し参照の着地を含まない**」（#925）。

### 受け入れ条件（数で接地する）

変更前のベースライン（**実測**）:

```
governance:check — 全検査 passed（検査 20 件 / 対象文書 36 件 / … /
  見出し参照 282 件を md 48 件 + .rs 101 件 + スクリプトのコメント 109 件から照合 / …）
```

**変更後は「見出し参照 285 件」になるはずである。** 内訳:
#1 が 0 → 1、#2 が 0 → 1（折返しを畳んで初めて数えられる）、#3 が 0 → 1、#4 は 1 → 1 で不変。

**`md 48 件 + .rs 101 件 + スクリプトのコメント 109 件` のほうは動かない。**
`scripts/governance/evidence.mjs:106` を読むと、この 3 つは `ev.refDocs.length` /
`ev.refSourceDocs.length` / … すなわち**走査した文書の件数**であって参照の件数ではない
（48 + 101 + 109 = 258 ≠ 282 が傍証）。動くのは `ev.headingRefs` の 282 だけである。
**「`.rs` が 104 になるはず」と待つと、正しく直っているのに失敗と読んでしまう。**

⚠️ 285 という数は導出であって実測ではない。**この数が動かなかったら、正準形が 1 行に
収まっていない**（対照 1 の形に戻っている）ことを疑う。

---

## 6. 同型パターンの走査（item 5）— 他の「測定値を生きた層へ写した」箇所

### (a) `40〜95 ms` — **8 か所。正本が `PERFORMANCE.md` に存在しない**

| file:line | 文脈 |
|---|---|
| `snotra-core/src/engine.rs:9` | モジュール `//!` |
| `snotra-core/src/engine.rs:260` | `config_handle` の rustdoc |
| `src-tauri/src/egui_shell/mod.rs:418` | `read_config` の rustdoc |
| `src-tauri/src/egui_shell/view.rs:1130` | 行コメント（`40〜95 ms 握る（#1032 実測）`） |
| `src-tauri/src/state.rs:19` | `AppState` フィールドの doc |
| `src-tauri/src/state.rs:146` | テストの rustdoc |
| `src-tauri/CLAUDE.md:57` | 同じ条項 |
| `docs/architecture.md:231` | 横断パターン |

**`git grep -nE "40〜95" -- PERFORMANCE.md` は 0 件である。**
つまりこの数値には**寄せ先の節が無い**——`PERFORMANCE.md` の最も近い値は `:385` の
`55〜96 ms` で、これは別の測定（`drop` 後の解放）である。

**今回直すべきか → 直さない（軽微・別 issue 推奨）。** 根拠 2 つ:
1. issue の文面が名指しているのは `43,939 µs` の 1 値であり、`40〜95 ms` は名前に含まれない。
2. 寄せ先が無い以上、**`PERFORMANCE.md` へ節（または既存節への行）を足す作業が先に要る**。
   それは #1128 の「正本を定めて残りを参照へ寄せる」より 1 段大きい変更であり、
   `AGENTS.md`「スコープを勝手に広げない」に当たる。
   ⚠️ ただし `PERFORMANCE.md` に無い数値が生きた層に 8 部あるのは #1128 と**同型の害であり、
   しかも今回のほうが悪い**（正本が無いので、どれが正しいか誰も裁定できない）。**issue を切ることを推奨する。**

### (b) `11 本`（60fps 予算超過フレーム数）— **1 か所**

`src-tauri/CLAUDE.md:57` のみ。正本は `PERFORMANCE.md:571` の A/B 表（`| **11 本** | **0 本** |`）。

**今回直すべきか → 直す（要対処に含めた）。** 根拠: これは #1128 が名指す A/B 表の値であり、
しかも `43,939` と**同じ 1 文の中に、同じ「A/B は `PERFORMANCE.md`「…」」の注記を共有して**
並んでいる。片方だけ数値を落として隣に残すと、その文は「参照へ寄せた」とも「数値を持つ」とも
言えない中間状態になる。§4 #4 の案はこれを落としてある。

### (c) `16,700 µs`（60fps 予算）— **同型ではない。直さない**

`snotra-core/tests/path_query_cost.rs` / `src-tauri/src/icon.rs` / `results_view.rs` 等に多数。
これは**測定値ではなく導出定数**（1/60 秒）であり、再測定で動かない。#1128 の「腐るのは数値」に
当たらない。

### (d) 既に正しい形になっている先行例（**触らない**）

`docs/architecture.md:231` は同じ #1032 の話を書きながら **`43,939` を持たず**、
`` A/B の実測は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」 `` で閉じている。
**この行が #1128 の到達目標の実物である**——4 か所をここへ揃えればよい。

---

## 7. この変更で偽になる散文（item 6）

**概念ラベルでの grep を実施した**（識別子だけでなく、参照先として名指される節見出しの名前で）:

```
git grep -n "設定の読みを engine lock の外へ出す" → PERFORMANCE.md:557（見出し本体）/ docs/architecture.md:231 / src-tauri/CLAUDE.md:57
git grep -n "フレーム後半の帰属"                   → PERFORMANCE.md:513（見出し本体）/ src-tauri/src/egui_shell/mod.rs:420
```

**偽になる散文は見つからなかった。** 個別に確認したもの:

| 箇所 | 主張 | 判定 |
|---|---|---|
| `snotra-core/src/engine.rs:7` | 「設定だけは外側の `Mutex` を経ずに読める（#1032・`config_handle` の doc が正本）」 | **真のまま**。正本宣言は*契約*についてであり、数値についてではない |
| `snotra-core/CLAUDE.md:192`「engine.rs のロック最小化パターン」 | 「契約の正本は同メソッドと `config` フィールドの doc」 | **真のまま**。数値を含まない（実測で確認） |
| `docs/architecture.md:231` | 「射程と、規範を守る機構は `src-tauri/CLAUDE.md`「モジュール構成」の当該条項が正本——ここに言い換えを置かない」 | **真のまま**。#4 の書き換えは*機構*の記述（`read_config` の 2 口・E0616 / E0451 / E0599）を一字も動かさない |
| `docs/hooks.md:117` | 「`governanceDocs()` の外の `.md` は参照実在を見ない——`PERFORMANCE.md`…」 | **真のまま**。`PERFORMANCE.md` は編集しない。なお**この事実により、#1139 の編集時 reminder はこの PR で一度も鳴らない**——沈黙を合格と読まないこと |
| `PERFORMANCE.md:529` / `:566` / `:571` | 数値そのもの | **触らない**（正本） |

⚠️ **未検証**: `43,939` が PR 本文・過去の commit message に写っている可能性は検めていない
（`pr-body-is-outside-the-grep-population`）。ただし #1128 の指示は「リポジトリの**生きた層**」で
あり、マージ済み PR 本文は書き換え対象にならないので、実害は無いと判断する。

---

## 8. 3 分類

### 要対処

| # | file:line | シンボル / 節 | やること |
|---|---|---|---|
| 1 | `snotra-core/src/engine.rs:262` | `Engine::config_handle` の rustdoc | `43,939 µs` を落とし、`` 額は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」が正本 `` へ |

**折返しについて、どの読みを適用したか**: `docs/comment-guidelines.md`「日本語の折返し」は
**文途中で物理改行を入れない**ことを禁止の実体とし、括弧内で「1 段落 1 行」と補っている。
§4 の案文は**前者（文途中で折らない）**を適用し、文の切れ目では改行している——正準形を
機械照合の視界へ入れるにはこれで必要十分であり（実測 `checked = 4`）、既存の段落構造を
必要以上に動かさないためである。より厳しい「1 段落 1 行」へ揃えたいなら、それは
#1128 の射程外の整形として別に判断すること。
| 2 | `src-tauri/src/egui_shell/mod.rs:420-421` | `egui_shell::read_config` の rustdoc | `43,939 µs` を落とす。**加えて折返しを畳んで正準形を 1 物理行に収める**——現行は改行をまたいでおり `G-heading-refs` / `G-near-heading-refs` の**双方の視界の外にある**（実測 `checked = 0`）。参照先を A/B 節へ正規化 |
| 3 | `src-tauri/src/state.rs:148` | `mod tests::ui_reads_config_while_the_engine_lock_is_held` の rustdoc | `43,939 µs` を落として正準形の参照を 1 行で置く。`#[cfg(test)]` 内でも `headingRefSourceDocs` の母集団に載る |
| 4 | `src-tauri/CLAUDE.md:57` | 「モジュール構成」節・**config の読みは `read_config` を通す**条項 | `43,939 µs` **と `11 本`** を落とす。既存の正準形はそのまま |
| 5 | （検証） | — | `npm run governance:check`（カテゴリ F）を `.rs` 変更でも回す。**`cargo doc` を手動実行**（カテゴリ A の hook 対象外・`docs/build-commands.md:29`）。見出し参照 **282 → 285** を接地に使う |

**害の説明は 4 か所とも残す**（「engine の錠越しに設定を読むとフレームが返らない」）——
issue の明示的な指示であり、腐るのは数値であって害の説明ではない。

### 軽微

| 項目 | 内容 | 判断 |
|---|---|---|
| `40〜95 ms` が生きた層に 8 部 | `engine.rs:9` / `engine.rs:260` / `egui_shell/mod.rs:418` / `view.rs:1130` / `state.rs:19` / `state.rs:146` / `src-tauri/CLAUDE.md:57` / `docs/architecture.md:231` | **#1128 では直さない。** issue が名指すのは `43,939` のみ。かつ `PERFORMANCE.md` に**寄せ先の節が存在しない**（実測 0 件）ため、正本を作る作業が先に要る。**別 issue を推奨**——正本が無い写し 8 部は #1128 と同型かつより悪い |
| 参照先の節の選択（#2） | 現行「フレーム後半の帰属」→「設定の読みを engine lock の外へ出す」への正規化 | ⚠️ **判断**。issue が「A/B 実測値」と書いていることと、他 3 か所が A/B 節を指すことを根拠に正規化を推奨。逆の裁定も成立する。**どちらでも「1 物理行に収める」は必須** |
| `16,700 µs` の多数の写し | `path_query_cost.rs` / `icon.rs` / `results_view.rs` 他 | **同型ではない**（測定値でなく 1/60 秒の導出定数）。触らない |
| `docs/architecture.md:231` | 既に数値なし・正準形で節を指す | **触らない。到達目標の実物として参照する** |

### 未検証

| 項目 | 何が未検証か |
|---|---|
| ⚠️ 見出し参照 **282 → 285** の予測 | 導出であって実測ではない。実装後に `governance:check` の出力で確かめる。動かなければ正準形が 1 行に収まっていない |
| ⚠️ PR 本文・commit message 内の `43,939` | `git grep` の母集団外。#1128 の射程は「生きた層」なので実害無しと判断したが、測ってはいない |
| ⚠️ `cargo doc` のリンク切れ | 本レビューは読み取り専用のため未実行。`[`crate::AppState::read_config`]` 等の intra-doc link は触らない案にしてあるが、#2 の段落を畳む際に隣接行を巻き込まないこと |
| ⚠️ `docs/comment-guidelines.md:51`（`///` の帰属） | #3 は `#[test]` 直前の doc なので、削減で行数が減ったとき帰属が動かないことを目視すること（#1106 型） |
| ⚠️ `governance:check` 変更後の実行 | 本レビューは読み取り専用のため、ベースライン（282・緑）しか測っていない。285 への遷移は実装者が測ること |

---

## 付録: 実測に使った治具

`C:/Users/Eoh/AppData/Local/Temp/claude/C--workspace-Snotra/6337f4fe-6046-4af8-8fcf-28bed1632a9e/scratchpad/probe.mjs`
——本物の `scanHeadingRefs` を注入スナップショットへ当て、提案 4 文面の着地・現行 2 形の
不可視・実在しない見出しでの発火（フォールトインジェクション）を 1 回で測る。
