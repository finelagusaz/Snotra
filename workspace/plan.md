# 実装計画 — issue #1128: 実測値を正本 1 か所へ寄せる

## 目的

`#1032` が残した 2 つの実測値を、それぞれ正本 1 か所へ寄せる。害の説明（錠越しに読むとフレームが返らない）は各所に残す。

| 値 | 生きた層の写し | 正本 | 出所 |
|---|---|---|---|
| `43,939 µs`（`read_window_width` の待ち max） | **4 か所** | `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」 | issue #1128 本文 |
| `40〜95 ms`（worker の engine lock 保持時間） | **8 か所 / 6 ファイル** | **`Engine::config_handle` の doc**（`snotra-core/src/engine.rs`） | 2026-08-20 のユーザー裁定（下記） |

**`40〜95 ms` は #1128 本文の射程外だったが、ユーザーの明示的な指示で今回に含めた。**

> 問い: 「40〜95 ms は PERFORMANCE.md に記録が無い（#1032 の計装は撤去済み）。8 か所の写しをどこへ寄せますか？」
> 回答: 「40〜95 ms も同じ形なら一緒に直そう」＋ 選択「engine.rs の doc を正本に（推奨）」

**`PERFORMANCE.md` に寄せられない理由（実測）**: `git grep "40〜95\|40-95" -- PERFORMANCE.md` が **0 件**
（`95 ms` を足すと 2155 行が 1 件出るが、それは `395 ms` への**部分一致**であって別の量である）。
#1032 の計装は同 issue の完了時に撤去済みで、この値は生きた層の 8 か所にしか存在しない。
ゆえに「`PERFORMANCE.md` へ 1 行足して正本にする」案は、**値の出所が既存の写しそのものになる**
（`AGENTS.md`「派生コピー同士の一致を完全性の証拠にしない」に触れる）ため採らなかった。

## 受け入れ条件

1. `git grep "43,939\|43939" -- . ':(exclude)workspace'` のヒットが **`PERFORMANCE.md` の 2 行だけ**になる（`docs/adr/` は元から 0 件）。
2. `git grep "40〜95" -- . ':(exclude)workspace'` のヒットが **`snotra-core/src/engine.rs` の 1 行（`config_handle` の doc）だけ**になる。
   - **`workspace/` を除くのは自己汚染を避けるためである**——`workspace/research.md` と `workspace/plan.md` は
     両方の数値を何度も含み、Step 6 でコミットされた瞬間に追跡ファイルになる。issue の言う「生きた層」に
     調査・計画の一時文書は入らない（`docs/adr/` を射程外と置いたのと同じ理由）。
   - **除外句は狙った以外まで落とす**ので、**除外あり/なしの差が `workspace/` の行だけ**であることまで測る。
3. 43,939 側の 4 か所が正準形 `` `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」 `` で正本を指す。
4. 40〜95 側の 7 か所が `Engine::config_handle` の doc を指す（`///` / `//!` からは **intra-doc link**、`//` と `.md` からは散文）。
5. **参照が機械照合に載っていることを実測する**（緑は証拠にならない）:
   - `G-heading-refs` の照合件数が **282 → 285**（`.rs` 101 → 104）へ進む。
   - `cargo doc` が intra-doc link を解決する（壊すと `broken_intra_doc_links` が deny で落ちる）。
   - 双方をフォールトインジェクションで確かめる。
6. 害の説明が全か所に残っている（数値だけが消える）。
7. `npm run governance:check` / カテゴリ A の全コマンドが緑。

## 変更ファイル一覧と対象シンボル

**6 ファイル・10 段落。**

| ファイル | 対象シンボル / 節 | 43,939 | 40〜95 |
|---|---|---|---|
| `snotra-core/src/engine.rs` | crate の `//!`（7-10 行） | — | 参照へ |
| `snotra-core/src/engine.rs` | `Engine::config_handle` の doc（259-262 行） | 数値を落とし節参照へ | **正本（残す・明示する）** |
| `src-tauri/src/egui_shell/mod.rs` | `read_config` の doc（418-421 行） | 数値を落とし節参照へ・**repoint** | 参照へ |
| `src-tauri/src/egui_shell/view.rs` | `plain_hidden` 直前の `//`（1128-1135 行） | — | 参照へ |
| `src-tauri/src/state.rs` | `AppState.config` フィールドの doc（17-20 行） | — | 参照へ |
| `src-tauri/src/state.rs` | `ui_reads_config_while_the_engine_lock_is_held` の doc（146-150 行） | 数値を落とし節参照へ | 参照へ |
| `src-tauri/CLAUDE.md` | 「モジュール構成」の config の読みの条項（57 行） | 数値の句を落とす | 参照へ |
| `docs/architecture.md` | Enter の bullet の内部ポインタ（228 行） | — | **ポインタの向き先を直す** |
| `docs/architecture.md` | #1032 の bullet（231 行） | — | 参照へ |

**触らないもの**: `PERFORMANCE.md`（529 / 566 行はどちらも測定記録そのもの）、`docs/adr/`（凍結された歴史）。

## 不変条件

- **害の説明を消さない。** 腐るのは数値であって「錠越しに読むとフレームが返らない」ではない。
- **参照は 1 物理行に収める。** `G-heading-refs` は `refScanLines` が返す**行単位**で照合し、
  `HEADING_REF` のラベル類は `[^「」\n]+` である。折られた参照は母集団へ入らず、**しかも finding が出ない**
  （実測: 現行 `mod.rs:420-421` の折られた参照は `matchAll` が 0 件を返す）。
  **`G-near-heading-refs` も行単位ゆえ同じ死角を持つ**——折れた参照は**二重に**見えていない（独立導出が実測）。
  **`git grep` も同じく殺される**（実測: `git grep "フレーム後半の帰属"` が当の参照を拾わなかった）。
- **触る段落は 1 段落 1 行へ直す**（`docs/comment-guidelines.md`「日本語の折返し」——適用は触った段落だけ）。
  規約の要求であり、同時に上の不変条件を満たす手段でもある。
- **`.rs` の doc コメントに行長の上限は無い**（実測: 100 **文字**超が 59 行・最長 551 文字。`rustfmt.toml` 不在）。
  **新文の長さ（コードポイント・全 7 段落を実測）**: engine.rs `//!` 185 / `config_handle` 234・169 /
  mod.rs 247 / view.rs 397 / state.rs 175・295 — **いずれも前例（551）の内側**。
  **初稿は 4,740 行 / 995 文字と書いていたが、それは `awk` の `length` が日本語をバイトで数えた値だった**
  （3b が指摘し、自分で再測定して訂正。経緯は `workspace/research.md`）。
- **`#[cfg(test)]` の中では intra-doc link を使わない。** rustdoc は `cfg(test)` の項目を組み立てないため、
  そこに書いた `[`Engine::config_handle`]` は**描画もされず検算もされない**。`state.rs:146` の段落は
  バッククォートの散文で書く（**検算が効かないことを明示的に受容する**）。
- **`//`（非 doc）コメントでも intra-doc link を使わない**（rustdoc が読まない）。`view.rs:1130` は散文で書く。

## 異常系

- 見出し参照が着地しない → `G-heading-refs` が「見出し参照が着地しない」で赤（実測済みの経路）。
- intra-doc link が解決しない → `cargo doc` が `broken_intra_doc_links`（root `[workspace.lints.rustdoc]` で deny）で赤。
- **参照が折れて照合母集団から落ちる → どの機構も鳴らない。** Phase 4 で人手で測る。
- **`.md` から `.rs` の doc への参照（`docs/architecture.md` / `src-tauri/CLAUDE.md`）はどの機構も検算しない。**
  **受容する残余**として明示する（`G-heading-refs` の対象は `<path>.md`「見出し」と `/skill-name` だけである）。

## 実装順序

**新旧の全文を以下に確定させる。実装者は追加判断をしない。**

### Phase 1 — `snotra-core/src/engine.rs`

#### 1-1. crate の `//!`（7-10 行）

旧:

```
//! **設定だけは外側の `Mutex` を経ずに読める**（#1032・`config_handle` の doc が正本）。
//! 検索は `&mut self` を要求するので外側の `Mutex` を長く握り、実運用点では 1 回の
//! `search` が 40〜95 ms 保持する。その間 UI が同じ `Mutex` 越しに設定を読んでいたのが
//! #1032 の主因だった。
```

新:

```
//! **設定だけは外側の `Mutex` を経ずに読める**（#1032・[`Engine::config_handle`] の doc が正本）。検索は `&mut self` を要求するので外側の `Mutex` を長く握り（実運用点での保持時間も同 doc が持つ）、その間 UI が同じ `Mutex` 越しに設定を読んでいたのが #1032 の主因だった。
```

#### 1-2. `Engine::config_handle` の doc（259-262 行）＝ **両方の値の分岐点**

旧（4 行）:

```
    /// **UI が毎フレーム行う live-read を、外側の `Mutex<Engine>` の外へ出すための口である。**
    /// 検索の worker は `search` の間じゅう外側の `Mutex` を握る（実運用点で 40〜95 ms）ため、
    /// UI が同じ錠越しに設定を読むと、そのフレームは worker の走査が終わるまで返らない
    /// （`read_window_width` 単独で 43,939 µs の待ちを実測した・`PERFORMANCE.md`）。
```

新（2 段落）:

```
    /// **UI が毎フレーム行う live-read を、外側の `Mutex<Engine>` の外へ出すための口である。** 検索の worker は `search` の間じゅう外側の `Mutex` を握るため、UI が同じ錠越しに設定を読むと、そのフレームは worker の走査が終わるまで返らない（`read_window_width` の待ちの実測値は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」）。
    ///
    /// **実運用点での保持時間は 40〜95 ms である**（#1032 実測）。**この値の正本はここであり、他の箇所はここを指す**——`PERFORMANCE.md` に記録の在る `read_window_width` の待ちと違って、この値は #1032 の計装（撤去済み）でしか測られておらず、寄せる先が他に無い。
```

**副次的な収穫**: 旧の `` `PERFORMANCE.md` `` は節を持たない裸の参照ゆえ `HEADING_REF` に当たらず、**今日まで一度も照合されていない**。

### Phase 2 — `src-tauri/src/egui_shell/`

#### 2-1. `mod.rs` の `read_config` の doc（418-421 行）

旧（4 行）:

```
/// 検索 worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点で 40〜95 ms）。
/// UI がその錠越しに config を読むと、フレームは worker の走査が終わるまで返らない
/// ——`read_window_width` 単独で 43,939 µs の待ちを実測した（`PERFORMANCE.md`「フレーム
/// 後半の帰属」）。
```

新（1 行）:

```
/// 検索 worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点での保持時間は [`snotra_core::engine::Engine::config_handle`] の doc）。UI がその錠越しに config を読むと、フレームは worker の走査が終わるまで返らない（`read_window_width` の待ちの実測値は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」）。
```

**参照先を「フレーム後半の帰属」から repoint する。これは意図的である**（4 か所を同じ節へ向けるのが #1128 の目的で、
issue 本文が向け先を名指ししている。両節とも 43,939 を持つため、どちらを指しても事実としては正しい）。
**repoint 後「フレーム後半の帰属」の参照は 0 になるが、節が参照されていることを要求する機構は無い**（測定ログの節である）。

**cross-crate の intra-doc link の前例**: `src-tauri/src/egui_shell/icon_textures.rs:115` の
`[`snotra_core::ui_types::IconSource::Explicit`]`。`snotra-core/src/lib.rs:16` が `pub mod engine`、
`config_handle` は `pub fn` ゆえ解決する（`cargo doc` で検算する）。

#### 2-2. `view.rs` の `plain_hidden` 直前の `//`（1128-1135 行）

旧（8 行の 1 段落）:

```
        // **受容する残余が 2 つある。** (1) この値は `indexing_raw` を読んだ時点のもので、
        // 表示ゲートとしては最大 1 フレーム古い——`on_enter` の同期 `engine.search` は engine lock を
        // 40〜95 ms 握る（#1032 実測）ので、その間に立つ余地がある。帰結は results 窓が隠れるのが
        // 1 フレーム遅れることだけで、**起動と表示は同じ値を見たまま**である。(2)
        // `run_search_with` の `indexing` 読みは live のままである（用途が違う——行をクリアするか。
        // **到達経路は数えない**——凍結より前に走るものも後に走るものも在り、足すたびに腐る）。
        // 食い違うと「Enter が 1 フレーム飲まれる」か「行が空で何も起きない」になり、どちらも
        // 次フレームの再検索が回復する。
```

新（1 行・397 字）:

```
        // **受容する残余が 2 つある。** (1) この値は `indexing_raw` を読んだ時点のもので、表示ゲートとしては最大 1 フレーム古い——`on_enter` の同期 `engine.search` は engine lock を握る（実運用点での保持時間は `Engine::config_handle` の doc）ので、その間に立つ余地がある。帰結は results 窓が隠れるのが 1 フレーム遅れることだけで、**起動と表示は同じ値を見たまま**である。(2) `run_search_with` の `indexing` 読みは live のままである（用途が違う——行をクリアするか。**到達経路は数えない**——凍結より前に走るものも後に走るものも在り、足すたびに腐る）。食い違うと「Enter が 1 フレーム飲まれる」か「行が空で何も起きない」になり、どちらも次フレームの再検索が回復する。
```

**`//`（非 doc）ゆえ intra-doc link を使わない**（rustdoc が読まない）。バッククォートの散文で書く。

### Phase 3 — `src-tauri/src/state.rs`

#### 3-1. `AppState.config` フィールドの doc（17-20 行）

旧（3 行）:

```
    /// **UI の毎フレームの live-read はこちらを読む**——`engine` の `Mutex` を経ると、
    /// 検索 worker が `engine.search` を走らせている間（実運用点で 40〜95 ms）フレームが
    /// そこで止まる。契約と、写しではないことの理由は `Engine::config_handle` の doc。
```

新（1 行）:

```
    /// **UI の毎フレームの live-read はこちらを読む**——`engine` の `Mutex` を経ると、検索 worker が `engine.search` を走らせている間フレームがそこで止まる。契約と、写しではないことの理由と、実運用点での保持時間は [`Engine::config_handle`] の doc。
```

（同ファイル 55 行に `[`Engine::config_handle`]` の前例がある。`Engine` は 12 行で `use` 済み。）

#### 3-2. `ui_reads_config_while_the_engine_lock_is_held` の doc（146-150 行）

旧（5 行）:

```
    /// worker は `engine.search` の間ずっと engine lock を握る（実運用点で 40〜95 ms）。
    /// その間に UI が同じ lock を取りに行っていたのが #1032 の主因で、`read_window_width`
    /// 単独で 43,939 µs の待ちを実測した。**この検査はその待ちが構造的に起きえないことを
    /// 測る**——engine lock を保持したまま別スレッドが config を読み切れることが受け入れ条件
    /// である。
```

新（1 行）:

```
    /// worker は `engine.search` の間ずっと engine lock を握る（実運用点での保持時間は `Engine::config_handle` の doc）。その間に UI が同じ lock を取りに行っていたのが #1032 の主因である（`read_window_width` の待ちの実測値は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」）。**この検査はその待ちが構造的に起きえないことを測る**——engine lock を保持したまま別スレッドが config を読み切れることが受け入れ条件である。
```

**`#[cfg(test)]` の中ゆえ intra-doc link を使わない**（rustdoc が組み立てない＝描画も検算もされない）。

### Phase 4 — ガバナンス文書

#### 4-1. `src-tauri/CLAUDE.md` 57 行の該当句

旧（同一行の中の 1 句）:

```
検索 worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点で 40〜95 ms）ため、その錠越しに設定を読むとフレームが走査の完了まで返らない——`read_window_width` 単独で 43,939 µs を実測し、60fps の予算を超えたフレームが 11 本あった（A/B は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」）。
```

新:

```
検索 worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点での保持時間は `Engine::config_handle` の doc）ため、その錠越しに設定を読むとフレームが走査の完了まで返らない（`read_window_width` の待ちと予算超過フレーム数の実測値は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」）。
```

**`60fps の予算を超えたフレームが 11 本` が同時に消えるのは意図的である**——43,939 と同じ 1 文の中に在る
A/B 表の派生値であり、残すと「数値を落として節を指す」が半分になる。
**行の他の部分（`口は 2 つ、…` 以降）には一切触れない。**

#### 4-2. `docs/architecture.md` 228 行の内部ポインタ

旧:

```
worker の走査（実運用点の値は下の #1032 の bullet が持つ）
```

新:

```
worker の走査（実運用点の値は `Engine::config_handle` の doc が持つ）
```

**これを直さないと 4-3 で偽になる**——228 行は「下の #1032 の bullet が値を持つ」と主張しており、
4-3 でその bullet から値が消える。**概念ラベルの grep で見つけた唯一の派生的な偽である。**

#### 4-3. `docs/architecture.md` 231 行の #1032 の bullet

旧（行の中の 1 句）:

```
worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点で 40〜95 ms）ため、
```

新:

```
worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点での保持時間は `Engine::config_handle` の doc）ため、
```

**同 bullet 末尾の `A/B の実測は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」` は触らない**（既に正しい形）。

### Phase 5 — 母集団の検算

- `git grep "43,939\|43939" -- . ':(exclude)workspace'` → `PERFORMANCE.md` の 2 行だけ。
- `git grep "40〜95" -- . ':(exclude)workspace'` → `snotra-core/src/engine.rs` の 1 行だけ。
- **別綴りも見る**: `git grep "40-95\|95 ms\|43\.9" -- . ':(exclude)workspace'`。
- **除外なしも打ち、差が `workspace/` の行だけ**であることを確かめる（除外句の副作用の検算）。

### Phase 6 — 参照が機械照合に載ったことの実測

**緑は「照合して緑」の証拠にならない**（折れた参照は沈黙する）。

1. **見出し参照の照合件数**: `npm run governance:check` の「見出し参照 N 件」が **282 → 285**（`.rs` 101 → 104）。
   - 内訳: engine.rs `config_handle` +1（裸 → 正準形）/ mod.rs +1（折れ → 1 行）/ state.rs 3-2 +1（新規）。
     `src-tauri/CLAUDE.md` と `docs/architecture.md` は既に 1 件ずつ持っており増減しない。
   - **`workspace/` の `.md` は照合母集団に入らない**（`headingRefDocs` が `workspace/` を除外・`scripts/governance/lib.mjs:513`）。
     ゆえに `plan.md` / `research.md` が同じ正準形を何度書いても 285 の予測は動かない。
2. **見出し参照のフォールトインジェクション**: **`src-tauri/src/egui_shell/mod.rs`**（＝今日まで不可視だった当のファイル）の
   節名を 1 文字崩し、`governance:check` が**「見出し参照が着地しない」で赤**になることを確かめ、戻す。
   **注入先を `.rs` に固定するのは腕別の件数予測（`.rs` 101 → 104）と 1 対 1 で照合するため**——
   `.md` へ注入すると md の腕が動いて内訳がずれる。
3. **intra-doc link のフォールトインジェクション**: `mod.rs` の
   `[`snotra_core::engine::Engine::config_handle`]` を存在しない綴りへ崩し、
   `cargo doc --workspace --no-deps --document-private-items` が `broken_intra_doc_links` で**赤**になることを確かめ、戻す。
   **これが「cross-crate の link が本当に解決されている」ことの唯一の証拠である。**
4. 各注入のあと `git diff` が注入前と一致することを確認する。

### Phase 7 — 変更後の検証

## テスト方針と検証コマンド

カテゴリは `docs/build-commands.md` の **A（`*.rs`）** と **F（ガバナンス文書）** に当たる。

| 何を | コマンド | 期待 |
|---|---|---|
| 母集団（43,939） | `git grep "43,939\|43939" -- . ':(exclude)workspace'` | `PERFORMANCE.md` の 2 行のみ |
| 母集団（40〜95） | `git grep "40〜95\|40-95" -- . ':(exclude)workspace'` | `snotra-core/src/engine.rs` の 1 行のみ |
| 母集団（除外の副作用） | 上の 2 本を `':(exclude)workspace'` なしで | 差が `workspace/` の行だけ |
| ガバナンス（F） | `npm run governance:check` | passed・見出し参照 285 件 |
| 書式（A） | `cargo fmt --all -- --check` | 緑（rustfmt はコメントを折り返さない） |
| 型（A） | `cargo check --workspace` | 緑 |
| lint（A） | `cargo clippy --workspace --all-targets -- -D warnings` | 緑 |
| テスト（A・触った crate） | `cargo test -p snotra-core -q` / `cargo test -p snotra -q` | 緑 |
| rustdoc（A・**hook は発火しない**） | `cargo doc --workspace --no-deps --document-private-items` | 警告なし・intra-doc link 解決 |

**`cargo doc` は hook も通常の沈黙も守らない**（`docs/build-commands.md` カテゴリ A の注記・#562）。
今回は intra-doc link を 3 本新設するので、**ここが唯一の検算**である。手で打つ。

## `SPEC.md`・関連文書の更新要否

- **`SPEC.md`: 不要。** 挙動もフローも状態遷移も変わらない（doc コメントとガバナンス文書の文言のみ）。
- **`docs/architecture.md`: 要**（4-2 / 4-3）。40〜95 ms の写しと、その値の在り処を指す内部ポインタを持つ。
- **`snotra-core/CLAUDE.md`: 不要。** 192 行が `config_handle` を「契約の正本」と名指すが、数値を持たない
  （むしろ今回の裁定と整合する）。
- **`RETROSPECTIVE.md`: 不要**（サイクル末に `/retrospective` が扱う）。

## 作業項目

### Phase 1 — `snotra-core/src/engine.rs`

- [x] crate の `//!` を 1-1 の新文へ差し替える
- [x] `Engine::config_handle` の doc を 1-2 の新文（2 段落）へ差し替える

### Phase 2 — `src-tauri/src/egui_shell/`

- [x] `mod.rs` の `read_config` doc を 2-1 の新文へ差し替える（repoint と cross-crate link を含む）
- [x] `view.rs` の `//` 段落を 2-2 の新文へ差し替える

### Phase 3 — `src-tauri/src/state.rs`

- [x] `AppState.config` フィールドの doc を 3-1 の新文へ差し替える
- [x] `ui_reads_config_while_the_engine_lock_is_held` の doc を 3-2 の新文へ差し替える

### Phase 4 — ガバナンス文書

- [x] `src-tauri/CLAUDE.md` 57 行の該当句を 4-1 の新文へ差し替える（行の他の部分は触らない）
- [x] `docs/architecture.md` 228 行の内部ポインタを 4-2 の新文へ直す
- [x] `docs/architecture.md` 231 行の該当句を 4-3 の新文へ差し替える（bullet 末尾の節参照は触らない）

### Phase 5 — 母集団の検算

- [x] 2 値それぞれの `git grep`（別綴り込み）を打ち、ヒットが正本の行だけであることを確かめ、出力を計画へ残す
- [x] 除外なしも打ち、差が `workspace/` の行だけであることを確かめる

### Phase 6 — 機械照合に載ったことの実測

- [x] `npm run governance:check` の見出し参照件数が 282 → 285（`.rs` 101 → 104）へ進んだことを確かめ、出力を残す
- [x] `mod.rs` の節名を崩して `governance:check` が赤になることを実測し、戻して緑と `git diff` の一致を確かめる
- [x] `mod.rs` の cross-crate intra-doc link を崩して `cargo doc` が赤になることを実測し、戻して緑と `git diff` の一致を確かめる

### Phase 5・6 の実測記録（2026-08-20）

**母集団（Phase 5）** — 受け入れ条件 1・2 が立った:

```
$ git grep -n "43,939\|43939" -- . ':(exclude)workspace'
PERFORMANCE.md:529:| **`read_window_width` の lock 取得** | **911〜43,939** | mainwin |
PERFORMANCE.md:566:| `read_window_width` の読み max | 43,939 | **7** |

$ git grep -n "40〜95\|40-95" -- . ':(exclude)workspace'
snotra-core/src/engine.rs:258:    /// **実運用点での保持時間は 40〜95 ms である**（#1032 実測）。…

$ git grep -n "43\.9" -- . ':(exclude)workspace'
(0 件)
```

除外の副作用: 除外なしとの差は `workspace/` の 4 ファイル（`adversarial-1128.txt` /
`plan-review-1128-independent.md` / `plan.md` / `research.md`）だけで、生きた層に漏れは無い。

**見出し参照（Phase 6-1）** — **282 → 285。予測が的中した。**

```
governance:check — 全検査 passed（… 見出し参照 285 件を md 48 件 + .rs 101 件 + スクリプトのコメント 109 件から照合 …）
```

> **訂正: 「`.rs` 101 → 104」は計画の読み違いだった。** `md 48 件 + .rs 101 件 + スクリプトのコメント 109 件` は
> **走査した文書の数**であって腕ごとの参照件数ではない。今回はファイルを増減していないので 48 / 101 / 109 は不変で、
> 動くのは合計の 285 だけである。**接地に使えるのは合計であり、腕別の内訳ではない。**
> （なお Phase 6-2 の注入先を `.rs` に固定した判断自体は変わらない——`mod.rs` は今日まで不可視だった当のファイルである。）

**見出し参照のフォールトインジェクション（Phase 6-2）** — 検知器が発火した:

```
$ sed -i 's/「設定の読みを engine lock の外へ出す」/「…外へ出さない」/' src-tauri/src/egui_shell/mod.rs
$ npm run governance:check
governance:check — 1 件の不整合:
  src-tauri/src/egui_shell/mod.rs:418  見出し参照が着地しない: `PERFORMANCE.md`「設定の読みを engine lock の外へ出さない」
```

巻き戻しは**内容ハッシュで照合**した（`git hash-object` が前後とも `ef7b7ec4…`）。

**intra-doc link のフォールトインジェクション（Phase 6-3）** — cross-crate link が検算されている:

```
$ sed -i 's/config_handle`\]/config_handle_typo`]/' src-tauri/src/egui_shell/mod.rs
$ cargo doc --workspace --no-deps --document-private-items
error: unresolved link to `snotra_core::engine::Engine::config_handle_typo`
error: could not document `snotra`
exit=101
```

巻き戻しは内容ハッシュで照合（前後とも `ef7b7ec4…`）。**注入前の clean run も exit 0 を実測済み。**

### Phase 7 — 変更後の検証

- [x] カテゴリ A を実行し結果を残す（`cargo fmt --all -- --check` / `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra-core -q` / `cargo test -p snotra -q` / `cargo doc --workspace --no-deps --document-private-items`）
- [x] `///` の帰属が動いていないことを目視する（`docs/comment-guidelines.md`「rustdoc の様式」・#1106 型。
      今回は**アイテムを挿入しない**ので構造的に起きないが、`state.rs` 3-2 は `#[test]` 直前の doc ゆえ確かめる）
- [x] 実装差分を確定させる（`git diff` で上記 6 ファイル以外が動いていないことを確認）

## 委譲検証の結果（worktree・2 ラウンド）

**同じエージェント（`verify-1128`）を `SendMessage` で継続した。** 出力は
`workspace/verify-1128.txt`（R1）/ `workspace/verify-1128-round2.txt`（R2）——
どちらも委譲先の worktree に在り、この repo へは持ち込まない。

### ラウンド 1（アンカー `0a69c9b`）

- **カテゴリ A 全件 exit 0**（`fmt` / `check` / `clippy -D warnings` / `test -p snotra-core` 605 passed /
  `test -p snotra` 297 passed / `doc`）。**B・C・D・E 該当なし**（コメント行以外の差分 0 ゆえ表示経路・trace 名・hotkey に触れない）。
  **F 実行・見出し参照 285 件**。**人間への申し送りはゼロ。**
- **変異注入 5 件。** 指示した 2 件（見出し参照・intra-doc link）を独立に再現したうえ、**自選 3 件**を追加した。
- **Critical / High なし。**

### ラウンド 2（アンカー `dd7b544`）

- `git rev-parse dd7b544^` が `0a69c9b` と一致することを先に測り（rebase でないことの確認）、
  カテゴリ判定は継承せず `0a69c9b..dd7b544` の範囲で再導出した。**カテゴリ A 全件 exit 0・F 285 件（不変）。**
- **変異注入 7 サイクル。L-1 の表は 6/6 実測一致**——推論で書いていた `state.rs:18` の行も
  `cargo doc` **exit 101** で接地した（`--document-private-items` を外しても発火することまで確認）。
- **Critical / High / Medium なし。**

### 指摘と採否

| # | 指摘 | 採否 |
|---|---|---|
| R1 L-1 | 「`.md` → `.rs` doc は**どの機構も**検算しない」が偽 | **採用・修正**（`dd7b544`）。自分で対照変異を打って裁定した |
| R1 L-3 | 参照が「待ち」と書くが着地先の節に「待ち」が無い | **採用・修正**（`dd7b544`）。節内 grep 0 件を自分で実測してから直した |
| R1 M-1 | 正本化の逆行に検知器が無い | **宣言で受け、人へ返す**（受容する残余 2）。セーフティネットの新設を単独で決めない |
| R1 L-2 | repoint で「フレーム後半の帰属」の被参照が 0 件 | **宣言で受ける**（受容する残余 2b） |
| R1 ⚠️-2 | `view.rs` / `state.rs`(test) が不可視 | **採用・表へ明示**。R2 の注入で推論から実測へ接地した |
| **R2 L-4** | fix-forward が `state.rs:144` の「**その待ち**」から語彙的な先行詞を奪った | **直さない・受容。** 指摘者自身が「修正必須ではない」とし、指示対象は前文の「lock を取りに行っていた」から復元できる。**指示代名詞 1 語のために検証サイクルをもう 1 周回すのは釣り合わない**（同型が `docs/architecture.md:231` にも在り、そちらも据え置きで一貫する） |

**R2 が主エージェントの判断を 1 つ追認した**: `state.rs` の「その待ち」据え置きは正しい——
当該の「待ち」は**現象そのもの**を指し `PERFORMANCE.md` の表ラベルを指していない。
両方直すと現象の記述を測定ラベルへ引きずる誤りになる。

### 決着しなかった ⚠️（受容する）

- **⚠️-4**: 見出し参照 285 は**合計でのみ接地できる**（腕の内訳 48/101/109 は文書数ゆえ不動）。
  別 PR が別の内訳で同じ 285 を作れるため、接地としては弱い。
- **⚠️-5**: rustdoc の private 項目への link 検査が `--document-private-items` の有無によらず発火した。
  **1 例しか測っておらず、全 private 項目への一般化は測っていない。**
- **⚠️-6**: 兄弟エージェントとの `target/` 共有は未確認（委譲先は worktree 専用 target で測った）。

## 未確定（実装前に潰す）

- [x] **`PERFORMANCE.md` の 2 出現（529 / 566 行）を触るか** — **触らない**。
  529 行は帰属測定のレンジ（911〜43,939）、566 行は A/B の A 側 max であり、**別の量の測定記録**である。
  どちらも日付つきの測定ログであって現在の設計についての主張ではないため、再計測で腐るのは参照側だけである。
- [x] **`mod.rs` の参照先を repoint するか** — **する**。issue 本文が向け先を名指ししており、
  4 か所を同じ節へ向けるのが目的である（正しさではなく一様性で決まる判断）。
- [x] **参照ラベルが本当に着地するか** — **着地する**（実測）。
  `collectAnchors(PERFORMANCE.md).map(normAnchor)` に `normAnchor(ラベル)` を `startsWith` して **`true`**、
  該当アンカーは `設定の読みをenginelockの外へ出す—#1032のA/B（同じ器・同条件・3標本）` の **1 件のみ**。
- [x] **折れた参照が本当に照合母集団から落ちるか** — **落ちる**（実測）。
  現行 `mod.rs:420-421` の 2 行はそれぞれ `HEADING_REF.matchAll` が **0 件**。1 行に収めた形は正しく一致した。
  **`git grep` からも消えていた**（`git grep "フレーム後半の帰属"` が当の参照を拾わなかった）。
- [x] **`.rs` が `G-heading-refs` の母集団に入るか** — **入る**（`scripts/governance/lib.mjs:542` の
  `headingRefSourceDocs` は `f.endsWith(".rs")` のみで除外パターンを持たない）。
- [x] **ベースラインの照合件数** — **282 件**（md 48 + `.rs` 101 + スクリプトのコメント 109）。変更前の作業ツリーで実測。
- [x] **`.rs` の doc コメントに行長の上限があるか** — **無い**（コードポイントで 100 文字超 59 行・最長 551 文字）。
  **初稿の「4,740 行 / 995 文字」はバイト単位の値だった**——3b が単位の取り違えを指摘し、自分で再測定して訂正した。
- [x] **`40〜95 ms` を今回扱うか、正本をどこに置くか** — **扱う。正本は `Engine::config_handle` の doc**
  （2026-08-20 ユーザー裁定・上の逐語引用）。`PERFORMANCE.md` に寄せられないことは
  `git grep "40〜95\|40-95" -- PERFORMANCE.md` の **0 件**で実測済み
  （`95 ms` を足すと `395 ms` へ部分一致して 1 件出る。**緩い綴りで数えると「正本が在る」と誤読しかねなかった**）。
- [x] **cross-crate の intra-doc link は解決するか** — **解決するはず**（`snotra-core/src/lib.rs:16` が `pub mod engine`、
  `config_handle` は `pub fn`、前例が `icon_textures.rs:115` に在る）。
  **「はず」を残さないため Phase 6-3 のフォールトインジェクションで実測する。**
- [x] **`#[cfg(test)]` / `//` の中で intra-doc link が効くか** — **効かない**。
  rustdoc は `cfg(test)` の項目を組み立てず、`//` を読まない。
  → `state.rs` 3-2 と `view.rs` 2-2 は散文で書き、**検算が効かないことを受容する残余として明示する**。

## セルフレビュー

- リスク: **高**（`src-tauri/CLAUDE.md` / `docs/architecture.md` はガバナンス文書。加えて「全出現を寄せる」という**網羅性そのものが要件**である）
- plan-review: `/plan-review --deep`（網羅性が要件のため）
- エージェント数: **2**（3b の敵対的調査 1 体 + `/plan-review --deep` の独立導出 1 体）
- 要対処: **4 件**（独立導出 ∖ plan の漏れ候補。すべて反映済み。内訳は下の「plan-review 結果」）
- 未検証: **3 件**（282 → 285 の遷移 / cross-crate intra-doc link の解決 / PR 本文の写し。
  前 2 つは Phase 6 で測る。3 つ目は受容する残余として宣言）

### 5a 自己照合

1. **issue の全要件に作業項目が対応する** — 43,939 の 4 か所が Phase 1-2 / 2-1 / 3-2 / 4-1 に 1 対 1。
   40〜95 の 8 か所が Phase 1-1 / 1-2（正本）/ 2-1 / 2-2 / 3-1 / 3-2 / 4-1 / 4-3 に 1 対 1。
   「`docs/adr/` を触らない」は変更ファイル一覧が明示的に除外。
2. **境界条件と検証** — (a) 見出し参照の着地（Phase 6-1・6-2）、(b) 折返しによる母集団落ち（同上）、
   (c) intra-doc link の解決（Phase 6-3）、(d) `PERFORMANCE.md` を誤って触らないこと（Phase 7 の `git diff`）、
   (e) `src-tauri/CLAUDE.md` の行の他の部分を壊さないこと（同上）、(f) 別綴りの取りこぼし（Phase 5）。
3. **新しい状態・リソース・プロセス** — 無し（doc とガバナンス文書の文言のみ）。
4. **より単純な既存パターン** — 有り、それを採る: `docs/architecture.md:231` 末尾が既にこの形
   （害の説明 + 数値なしの節参照）。40〜95 側も `state.rs:20` / `engine.rs:7` が既に
   「`Engine::config_handle` の doc」を指しており、**その既存の向きに値を合流させるだけである**。
5. **壊してはならない不変条件と検知手段** — 「見出し参照が着地する」は `G-heading-refs`、
   「intra-doc link が解決する」は `cargo doc` が検知する。
   **「参照が照合母集団に入っている」と「`.md` → `.rs` doc の参照」はどの機構も検知しない**——
   前者は Phase 6 で人手で測り、後者は**受容する残余**として宣言する。

### 5a-7 概念ラベルでの grep（この変更で偽になる散文の探索）

`git grep <ラベル> -- . ':(exclude)workspace'` を 5 本。

| ラベル | 結果 | 判断 |
|---|---|---|
| `フレーム後半の帰属`（repoint 元の節名） | `PERFORMANCE.md:511` の見出しだけ。参照 0 件 | repoint 後も参照 0 のまま。節の被参照を要求する機構は無い。変更不要 |
| `config_handle` | `snotra-core/src/engine.rs:7` と `snotra-core/CLAUDE.md:192` が「契約の正本は同メソッドの doc」と名指す | **偽にならない**——むしろ今回の裁定（値の正本も同 doc）と整合する。`CLAUDE.md:192` に数値は無い |
| `ui_reads_config_while_the_engine_lock_is_held` | 定義（`state.rs:155`）のみ | 変更不要 |
| `設定の読みを engine lock の外へ出す`（参照先の節名） | `PERFORMANCE.md:557`・`docs/architecture.md:231`・`src-tauri/CLAUDE.md:57` | いずれも扱い済み |
| `実運用点の値は下の #1032 の bullet が持つ` | `docs/architecture.md:228` | **偽になる。4-2 で直す**（この grep が見つけた唯一の派生的な偽） |

**副産物の実測**: `git grep "フレーム後半の帰属"` が `mod.rs:420-421` の現行参照を**拾わなかった**。
折られた参照は `G-heading-refs` だけでなく **`git grep` からも消えている**——
`docs/comment-guidelines.md` の「折返しは `grep` を壊す」が、まさにこの変更対象の上で再現した。

## plan-review 結果

- リスク: **高**
- レビュー方式: **独立導出 1 体**（Step 2b・`/plan-review --deep`）
- エージェント数: **2**（3b の敵対的調査 1 体 + 独立導出 1 体）
- 成果物: `workspace/plan-review-1128-independent.md`（438 行）。
  **報告なしで idle になったため、呼び出し側が指定した出力パスから回収した**
  （`reviewer-subagents-fail-to-report` の作法）。

### 導出 ∖ plan（漏れ候補）— **4 件すべて採用**

| # | 所見 | 反映先 |
|---|---|---|
| 1 | **`G-near-heading-refs` も行単位ゆえ同じ死角を持つ**——折れた参照は**二重に**見えていない | 不変条件へ追記 |
| 2 | `PERFORMANCE.md:385` の **`55〜96 ms` は別の量**（`SearchEngine` 構築コスト・#1003）。40〜95 ms の正本と誤認しうる | 下の「近傍の紛らわしい値」へ記録（自分で 382-390 行を読んで確認した） |
| 3 | `docs/comment-guidelines.md:51`——`///` の帰属は**直後のアイテム**に付く。#1106 の事故 | Phase 7 の確認項目へ（今回は**アイテムを挿入しない**ので構造的に起きないが、目視する） |
| 4 | ⚠️ `43,939` が PR 本文・過去の commit message に写っている可能性は未測定 | **受容する残余**として宣言（下記） |

### plan ∖ 導出（スコープ過剰候補）— **2 件とも意図的**

| # | 差分 | 裁定 |
|---|---|---|
| 1 | 計画は **`40〜95 ms` の 8 か所**と `docs/architecture.md` を含む。導出は「#1128 では直さない・別 issue 推奨」 | **ユーザーの明示的な裁定が優先する**（2026-08-20「40〜95 ms も同じ形なら一緒に直そう」）。導出は issue 本文だけを根拠にしており、その範囲では正しい |
| 2 | 計画は触った段落を**1 段落 1 行**へ結合する。導出は「**文途中で折らない**だけで必要十分・1 段落 1 行は射程外の整形」 | **1 段落 1 行を採る。** `docs/comment-guidelines.md` の規範文が「**文途中で物理改行を入れない**（1 段落 1 行）」と括弧で自ら言い換えており、字面がこちらである。実践もある（300 字超の doc コメント行が 16 行・実測）。**ただし導出の指摘は正しい**——機械照合に載せるだけなら「文途中で折らない」で足りる |

### 判断の不一致 — なし

参照先の節の選択（repoint）は導出も「A/B 節への正規化を推奨」で一致した（⚠️ 付き・逆の裁定も成立すると併記）。

### 導出が独立に実測した、計画を強化する証拠

本物の `scanHeadingRefs` を注入スナップショットへ当てて対照実験を行っている:

- 提案 4 文面: `checked = 4 / findings = 0`（正準形として拾われ、着地する）
- 対照 1（折返し形＝現行 `mod.rs`）: `checked = 0 / findings = 0` ← **緑ではなく「数えられてすらいない」**
- 対照 2（裸の対象名＝現行 `engine.rs`）: `checked = 0 / findings = 0`
- 対照 3（実在しない見出し）: `checked = 1 / findings = 1` ← **検知器が発火しうることの実証**

**対照 3 が `measure-whether-detector-can-fire` の作法を満たしている**——Phase 6-2 の
フォールトインジェクションは、これを実物の `governance:check` で再現する位置づけになる。

### 近傍の紛らわしい値（**正本と取り違えない**）

- `PERFORMANCE.md:385` の **`55〜96 ms`** は `SearchEngine` の**構築コスト**である（#1003・「構築コストの見積もりが 4〜5 倍外れた」）。
  40〜95 ms（engine lock の保持時間）とは**別の量**。数字が近いだけで、寄せ先にしてはならない。
  （自分で 382-390 行を読んで確認した。）
- `95 ms` という緩い綴りで grep すると `PERFORMANCE.md:2155` の **`395 ms`** に部分一致する。

### 受容する残余（**宣言して止める**）

1. **`Engine::config_handle` を指す 6 か所は、綴りの実在を見る機構が在るものと無いものに割れる。**
   **計画の初稿は「`.md` → `.rs` doc の参照はどの機構も検算しない」と書いたが、これは偽だった**——
   委譲レビューの指摘（L-1）を受けて対照変異で測り直した結果が下表である。
   （**この欄は「だから機構を足さない」の根拠として使われる形なので、宣言は測った射程で書く**。）

   | 参照の在り処 | 綴りの実在を見る機構 | 実測 |
   |---|---|---|
   | `docs/architecture.md`（228 / 231 行） | **`G-stale-identifiers`** | 誤綴りを注入 → `governance:check` が exit 1「散文に、現行語彙に無い識別子が残っている」 |
   | `src-tauri/src/egui_shell/mod.rs`（`///` の intra-doc link） | **`cargo doc`** | 誤綴りを注入 → exit 101 `unresolved link` |
   | `src-tauri/src/state.rs:18`（`///` の intra-doc link） | **`cargo doc`** | 同じ機構に載る |
   | `src-tauri/CLAUDE.md`（57 行） | **無い** | 誤綴りを注入 → exit 0 の沈黙（モジュール `CLAUDE.md` は `G-stale-identifiers` の母集団外） |
   | `src-tauri/src/state.rs:144`（`#[cfg(test)]` の doc） | **無い** | rustdoc が `cfg(test)` を組み立てない |
   | `src-tauri/src/egui_shell/view.rs`（`//`） | **無い** | rustdoc が `//` を読まない |

   **どの機構も「指し先がその値を持つか」までは見ない**——見るのは綴りの実在だけである。

2. **正本化そのものの逆行を止める機構は無い**（委譲レビューの M-1）。
   `engine.rs` の doc は「他の箇所はここを指す」と宣言するが、**5 番目の `43,939` や 9 番目の `40〜95 ms` が
   明日書かれても何も鳴らない**。委譲エージェントが自選した変異 3（`src-tauri/CLAUDE.md` の正準形参照を
   2 物理行へ折る）が同じ形で実証している——`governance:check` は exit 0 のまま、照合件数だけが 285 → 284 へ静かに落ちた。
   **検知器を置くかはセーフティネットの新設ゆえ、この計画では決めない**（ルート `CLAUDE.md`
   「セーフティネットの変更は合意してから」）。**人へ返す。**
2b. **repoint により `PERFORMANCE.md`「フレーム後半の帰属」を指す参照が 0 件になった**（委譲レビューの L-2）。
   孤立節を赤にする検査は無く、判断としては計画どおり。**ただし当該節は本差分の主張の一次証拠を持つ**ので、
   節ごと消す変更が来たときに気づく仕掛けは無い——**受容する。**

3. **PR 本文・過去の commit message の `43,939` は `git grep` の母集団外である**
   （`pr-body-is-outside-the-grep-population`）。#1128 の射程は「生きた層」であり、
   マージ済みの記録は `docs/adr/` と同じく凍結された歴史として扱う。**測っていないことを明示する。**
4. **`#1139` の編集時 reminder はこの PR で何も保証しない**（`PERFORMANCE.md` は `governanceDocs` の外）。
   **沈黙を合格と読まない**——`npm run governance:check` だけが緑を出す。

### 要対処 → 反映済み

- 導出 ∖ plan の 4 件をすべて計画へ反映した（上表）。
- 導出の「要対処」5 件は、計画の Phase 1-2 / 2-1 / 3-2 / 4-1 / 6-1 と 1 対 1 で一致した（**追加の漏れなし**）。

### 未検証

- **見出し参照 282 → 285 は導出であって実測ではない**（Phase 6-1 で測る。動かなければ正準形が 1 行に収まっていない）。
- **cross-crate intra-doc link の解決は未実行**（Phase 6-3 で測る）。
- **PR 本文・commit message の `43,939`**（上の受容する残余 3）。

### 判断

- **実装着手: 可**（人間の承認後）。

## 人間レビュー

- [x] 承認済み — 2026-08-20 / 問い: "計画が固まりました。**承認をお願いします**（`workspace/plan.md` への注釈でも構いません）。" / 回答: "承認"
