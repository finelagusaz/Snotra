# research — #825 「CLEAR_COLOR の一致に検査は無い」という stale な主張

## issue の要約

`CLEAR_COLOR` と config 既定背景色の一致について「落ちる検査は無い（受容する残余）」と述べる記述が複数箇所に残っているが、**その検査は既に在る**（#802 で追加）。#795 が扱った「値の写し」と違い、こちらは**主張の写し**なので置換では消せず、正本を 1 か所に定めて他を参照へ倒す必要がある。

## 事実の確認（一次証拠）

### 検査は実在する

`src-tauri/src/egui_shell/window_coordinator.rs:702-707`

```rust
fn runtime_fallback_matches_config_default_background() {
    let d = snotra_core::config::VisualConfig::default().background_color;
    let c = egui::Color32::from_hex(&d).unwrap();
    let packed = ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32;
    assert_eq!(packed, snotra_egui_runtime::CLEAR_COLOR);
}
```

既存の doc コメント（`:698-700`）:

```rust
/// runtime のフォールバック（`set_clear_color` を呼ばなかったフレームの色）が config の
/// 既定背景色と一致することを**機構で**固定する。両 crate に依存するのはこの crate だけで、
/// 一致は今まで規約でしかなかった（`snotra-egui-runtime/CLAUDE.md` が受容した残余）。
```

**この doc は上流を逆参照している**——「`snotra-egui-runtime/CLAUDE.md` が受容した残余」は、CLAUDE.md:39 に「受容する残余」の記述が在ることを前提にしている。上流 4 箇所を直すと**この参照が宙に浮く**（指す先が消える）。ゆえに写しの網は 4 頂点ではなく **5 頂点**であり、テスト側の doc も同じ変更で書き換えねばならない。

- 追加コミット: `9c64c09` — `feat(egui): config の背景色を実際に効かせる（spec 決定 1〜4 の実装） (#802)`
- `src-tauri` は `snotra-core` と `snotra-egui-runtime` の両方に依存するので、**両者を突き合わせられる唯一の位置**である（`snotra-egui-runtime` 自身は `snotra-core` に依存しない＝当該 crate 内には置けない）
- 同コミットが `scripts/visual-check-colors.ps1`（`npm run check:colors`）も追加した（`git log --diff-filter=A` 実測）

### issue の行番号は移動している

| issue の記載 | 現在 |
|---|---|
| `snotra-egui-runtime/src/renderer.rs:11-12` | `:10-12`（doc コメント本体） |
| `snotra-egui-runtime/CLAUDE.md:38` | `:39` |
| `docs/development-principles.md:66` | `:67` |
| `scripts/governance-check.mjs:1067-1068` | `:1228-1229` |

## 主要な発見 1 — 「4 箇所の写し」は**2 つの別の主張**である

`CLEAR_COLOR` の grep に加えて、主張のマーカー語（`受容する残余` / `落ちる検査は無い` / `一致は規約` / `機構ではなく規約` / `消費者ゼロ` / `検査は無い`）でも数え直した（`docs/superpowers/` と `workspace/` は履歴資料・作業バッファゆえ除外）。結果、4 箇所は**同一の主張の写しではない**。

### 主張 A: 「一致に落ちる検査は無い」（2 箇所・#825 の本体）

| 箇所 | 逐語 |
|---|---|
| `snotra-egui-runtime/src/renderer.rs:11-12` | 「**`snotra-core` の `default_background_color()` と同値だが、この crate は同 crate に依存しない**——一致は機構ではなく規約であり、乖離したときに落ちる検査は無い（受容する残余）。」 |
| `snotra-egui-runtime/CLAUDE.md:39` | 「この定数は `snotra-core` の `default_background_color()` と同値だが、**当 crate は同 crate に依存せず一致は規約にすぎない**（乖離時に落ちる検査は無い・受容する残余）」 |

**どちらも偽**（上記テストが存在する）。前半（crate 依存が無い）は真であり、それは**検査が `src-tauri` 側に置かれている理由**である。

### 主張 B: 「`[visual].background_color` は消費者ゼロ」（2 箇所）

| 箇所 | 逐語 |
|---|---|
| `docs/development-principles.md:67` | 「実例: `[visual].background_color` の描画経路は消費者ゼロだが、既定 `#282828` が …`CLEAR_COLOR` と一致するため、既定のままでは正常に見え続ける。」 |
| `scripts/governance-check.mjs:1228-1229` | 「実例: `[visual].background_color` の描画経路は消費者ゼロのまま（実背景は renderer.rs の `CLEAR_COLOR` ハードコード）」 |

**どちらも偽。** 消費者は実在する（grep 実測）:

- `src-tauri/src/egui_shell/visual.rs:100` — `background: hex_or(&v.background_color, &d.background_color)`
- `src-tauri/src/egui_shell/view.rs:343` — `frame.set_clear_color(visual.background)`
- `src-tauri/src/egui_shell/mod.rs:273,281,292,308` — 窓生成の `.background_color` とネイティブブラシ

`NO_LAUNCHER_READ`（`scripts/governance-check.mjs:1283-1295`）に `VisualConfig.background_color` は**載っていない**＝G-config-reachability の「読まれている」側に正しく分類されている。**表そのものは正しく、腐っているのは同ファイルの説明コメントだけ**である。

**帰結**: 主張 B を直さないと、修正できる箇所は 4 → 2 に落ちる。issue の「「消費者ゼロ」の主張も同時に直すか」は選択ではなく、**同時に直さなければ issue が閉じない**。

## 主要な発見 2 — `docs/development-principles.md:67` には**第 3 の stale な主張**が同居する

同じ一文が「**色を変えて描画を検証するテストは無く、CI も手元の実行も既定 config で動く**」と述べる。

- 「色を変えて描画を検証する」検証は**在る**——`npm run check:colors`（`scripts/visual-check-colors.ps1`）が非既定色（既定 `#4A2B5C`）で起動し、main / results の**実ピクセルの最頻色**を期待色と突き合わせて **exit code で判定**する（`:16`, `:167`, `:304` 実測）
- ただし **CI には無い**（`grep check:colors .github/workflows/` は 0 件・`package.json:17` のみ）。GUI と非ロック画面を要する（`docs/build-commands.md`「画面がロックされていると実行できない」）

つまり「CI は既定 config で動く」は真、「手元の実行も既定 config で動く」は**偽**。この一文は当該節の主題文「**休眠を支えているのは、検証が既定値の下でしか走らないことである**」の**唯一の支え**であり、文字列の置換では済まない。節が死んだ実例で論じ続けることになるため、**実例の差し替えか「歴史である」旨の明示**が要る（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

## 主要な発見 3 — `snotra-egui-runtime/CLAUDE.md:39` にも第 3 の stale がある

同じ bullet が「**呼び忘れの検知はビルドでも検査でもなく目視だけである**」と述べる。`check:colors` は `set_clear_color` の呼び忘れ（＝背景が `CLEAR_COLOR` のままになる）を**非既定色下で自動判定する**ので、これも偽。ただし CI では走らないため、正確な言い換えは「ビルドでも自動テストでもなく、**手動で走らせる `check:colors` の実測**（と目視）だけが検知する」。

## 数え直しで**写しではないと分類した**箇所（触らない）

| 箇所 | 分類 |
|---|---|
| `docs/build-commands.md:77` | 「既定 `#282828` は `CLEAR_COLOR` と一致するため、色が届いていなくても正常に見える」——**今日も真**（値は一致しており、それが非既定色を要求する理由）。ただし「原理は `docs/development-principles.md`「config の値は到達性の検出器を持たない」」と**当該節を引用している**ため、:67 の書き換えで引用が空振りにならないか要確認 |
| `src-tauri/src/egui_shell/mod.rs:290` | インラインの `0x282828` + renderer.rs 参照。**値の写し**（#795 の類）であって主張の写しではない。#825 の射程外 |
| `src-tauri/src/egui_shell/view.rs:352` | 「`panel_fill` / `window_fill` は…消費者ゼロの死んだ書き込み**だった**」——過去形で、撤去済みの事実の記録。真 |
| `docs/development-principles.md:63` | 3 つの形の列挙にある「**消費者ゼロ**」——概念の定義であって実例ではない。真 |
| `docs/superpowers/**`（10 箇所） | 履歴資料（#589 で非規範化・`governanceDocs` の母集団外）。**触らない** |
| `受容する残余` の他 60 箇所 | 全く別の残余についての記述。同じ語彙を使うだけ |

## 関連ファイル・シンボル

| パス | シンボル / 行 | 役割 |
|---|---|---|
| `snotra-egui-runtime/src/renderer.rs` | `CLEAR_COLOR`（`:13`）+ doc（`:10-12`） | 定数の定義。**正本の候補** |
| `snotra-egui-runtime/src/lib.rs` | `pub use renderer::CLEAR_COLOR;`（`:15`） | 再エクスポート（テストはこれ経由で参照） |
| `snotra-core/src/config.rs` | `default_background_color()`（`:366`） | 既定値 `#282828` の正本 |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `runtime_fallback_matches_config_default_background`（`:702`）+ doc（`:698-700`） | **一致を固定する機構**。doc が上流 CLAUDE.md を逆参照している |
| `snotra-egui-runtime/CLAUDE.md` | `:39` | 主張 A + 発見 3 |
| `docs/development-principles.md` | `:67`（節「config の値は到達性の検出器を持たない」） | 主張 B + 発見 2 |
| `scripts/governance-check.mjs` | `:1228-1229`（G-config-reachability の説明コメント） | 主張 B |
| `docs/build-commands.md` | `:77` | :67 への引用元（読み直しのみ） |

## 再利用できる既存パターン

- **`AGENTS.md`「条件別チェック」の「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」** — 本 issue が明示的に引く規範
- **`.md` → `.rs` のバッククォート参照はパス実在が機械照合される** — `G-references` の述語は「`/` を含む・glob なし・拡張子が `REF_EXTENSIONS`（`.rs` を含む・`:29-30` 実測）」。ゆえに `` `src-tauri/src/egui_shell/window_coordinator.rs` `` と書けばパスの実在は `governance:check` が守る。**シンボル名（テスト関数名）までは見ない**——そこは規範のみ
- **`prefer-structural-over-documented-contract`（メモリ）** — 機構と記述を近づける。テスト側にも「この定数 doc が引く機構である」旨の 1 行を置く

## 技術的制約

- **`snotra-egui-runtime` は `snotra-core` に依存しない**（`Cargo.toml` 実測の前提。これが「検査が下流に在る」理由そのもの）。この非依存を崩す修正は取らない
- **`scripts/*.mjs` と `*.md` は PostToolUse hook の沈黙が「合格」を意味しない**（ルート `CLAUDE.md`）。`npm run governance:check` を明示的に走らせるのが唯一の機械検証
- **`scripts/governance-check.mjs` はセーフティネット**（`.claude/rules/safety-nets.md` の `paths` に `scripts/*.mjs` が在る）。ただし今回の変更は**説明コメントの文言のみで判定を足さない**ため、同 rule の「フォールトインジェクション」も `/norm-review` も**仕事が無い**（「種が書けない変更（索引の追随・改名）には仕事が無い」）。`NO_LAUNCHER_READ` の表本体は正しいので触らない
- **#489 は PR 分割の規則ではない** — 「検査対象を変更しながら検査を走らせない」という**順序の制約**である。ゆえに `governance-check.mjs` の編集と `governance:check` の実行を重ねなければ 1 PR で足りる（issue コメントの「#489 に従い単独 PR が要る」は過読み）
- **`/norm-review` は起動しない** — 2026-07-27 のユーザー裁定（メモリ `norm-review-findings-low-value`）

---

# 追補 — #819 を束ねる判断（ユーザー回答: 「A と B をフェーズ分けてやろう」）

## #819 案 (B) の現状（引かれている行番号は #885 で移動済み）

`G-stale-identifiers` の現在の射程（`scripts/governance-check.mjs:1419-1442` 実測）:

- **検査対象**: `.claude/(skills|rules|agents)/**.md` + `SPEC.md`（`STALE_EXTRA_DOCS`）。**`docs/**` は入らない**
- **述語**: `STALE_IDENT = /^([a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)(\(\))?$/` — camelCase 限定。**SCREAMING_SNAKE は見ない**
- **現行語彙**: production のソース（`VOCAB_SOURCE_EXT = /\.(rs|ts|tsx|mjs|ps1|toml)$/`）の**非コメント本文**。`.test.<ext>` は除外

`G12_NO_LAUNCHER_READ` は `docs/development-principles.md` に在る SCREAMING_SNAKE ゆえ、**母集団と述語の両方**を広げないと届かない。#819 の記述は現状でも成立する。

## 軸の測定（proxy snapshot・稼働中のガードは触っていない）

`.claude/rules/safety-nets.md`「複製に変異を当てる」に従い、述語と母集団をスクラッチパッドのスクリプト内へ複製して変異させた（`scratchpad/measure-stale-axes.mjs`・`scratchpad/vocab-widen.mjs`）。`docs/adr/ADR-stale-identifier-detector-scope.md` の「その後」節と同じ表の形で記録する。

軸: **D** = 検査対象に `docs/**.md`（`docs/superpowers/` 除く）/ **D-** = D から `docs/adr/**` も除く / **M** = モジュール `CLAUDE.md` + ルート `CLAUDE.md`/`AGENTS.md` / **E** = 述語に SCREAMING_SNAKE

| 述語 | 照合 | finding | 真の腐り | 偽陽性 |
|---|---|---|---|---|
| ベースライン（現行） | 1 | 0 | 0 | 0 |
| E 単独 | 7 | 0 | 0 | 0 |
| D 単独 | 69 | 35 | 8 | **27** |
| D- 単独 | 18 | 8 | 8 | 0 |
| M 単独 | 9 | 2 | 1 | 1 |
| D+E | 107 | 40 | 9 | 31 |
| D-+E | 43 | 12 | 9 | 3 |
| **D-+E + 語彙源に `.yml`/`.json`** | 43 | **9** | **9** | **0** |
| D-+M+E + 同語彙 | 73 | 13 | 10 | 3 |

### 読み取れたこと

1. **`docs/adr/**` は `docs/superpowers/` と同じ扱いが要る。** D 単独の finding 35 件のうち 27 件が ADR 自身の**却下記録**である（`folderState` / `resetForShow` / `createObjectURL` 等）。ADR は「否定の知識＝もう存在しない案」を書く場所で、死んだ識別子が載るのは正しい。`ADR-stale-identifier-detector-scope.md` 自身が「歴史としてそのまま残す」と明記している行が鳴る
2. **偽陽性 3 件は語彙源の穴で、除外リスト無しに構造で消える。** `GITHUB_TOKEN`（`.github/workflows/label-sync.yml` にのみ実在）・`CLAUDE_PROJECT_DIR`（`.claude/settings.json` にのみ実在。`post-edit.mjs` にはコメントとしてしか無い）。`VOCAB_SOURCE_EXT` に `.yml` / `.json` を足すと**真の腐りを 1 件も沈黙させずに** 3 件が消えた（実測: 13 → 12 → 9 件の推移で真の腐りは不変）。`.json` の不在は ADR が「受容する残余」として自認していた穴である
3. **M（モジュール `CLAUDE.md`）は採らない。** 真 1（`iconCacheSize`・撤去済み SolidJS フロントの語）に対し偽 3——`WM_SETCURSOR`（Win32 メッセージ）・`MARKER_DONT_FOCUS`（tao 内部定数）・`numFonts`（TTC ヘッダのフィールド）。いずれも**ソースのコメントにしか現れない外部語彙**である。モジュール文書はラップ対象の外部 API を語る場所であり、`docs/**` とは母集団の性質が違う。**これは否定の知識ゆえ ADR へ記録する**
4. **D-+E が拾う 9 件は、#825 と同じ欠陥クラスである。** 内訳: `docs/development-principles.md` の SolidJS/WebView2 期の識別子 8 件（`shouldShowResults` / `viewKind` ×2 / `interpKind` ×2 / `assertNever` / `isInstantPrefix` / `backgroundThrottlingPolicy`——最後の 1 件はリポジトリのどの `.json` にも存在しない）と、`G12_NO_LAUNCHER_READ` 1 件（＝#819 案 A の対象）。**#819 案 (A) は「拡張した検出器が最初に指すもの」であり、案 (B) の受け入れ条件そのものである**

## 束ね方の決定（実装判断）

- **PR 1 = #825 + #819 案 (A)** — 事実訂正のみ・機構不変。`docs/development-principles.md` の同一節（`:67` と `:71`）を触るので hunk が 1 つに収まる
- **PR 2 = #819 案 (B)** — 検出器の射程拡大 + 拡大が指す残り 8 件の是正。判定を足す変更ゆえ**フォールトインジェクションが要る**（`.claude/rules/safety-nets.md`）。PR 1 の後に測り直す（PR 1 が `:71` を直すので、PR 2 の時点の finding は 8 件になる想定）

分ける理由は具体的である: 測定が「D- は採らない」に転んだ場合でも PR 2 が縮むだけで、#825 の事実訂正はその結果に人質を取られない。
