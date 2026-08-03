# レビュー: 一次証拠の検算

レンズ = 一次証拠の検算。`chore/stale-clear-color-claim`（未実装）に対し、`workspace/plan.md` と
`workspace/research.md` が主張する**事実**だけを独立に測った。実施日 2026-08-03。

**総括**: 行番号・シンボル・機構の実在はほぼ全て一致した。**不一致は 9 件**。
うち**実行を止めるのは 1 件**（`cargo test -p snotra --lib` が存在しないターゲットを指す）。
残りは数値の写し間違い（`13 → 12 → 9`）、分類の誤り（採用セルの「偽陽性 0」）、証拠の不在（採用セルが未測定だった）、理由付けの誤り 2 件など。

**採用射程 D-+E+`.yml`/`.json` の照合 43 / finding 9 は私が直接測って再現を確認した**（それまでどちらのスクリプトでも測られていなかった）。
ただし**その 9 件の内訳は「真 9 / 偽 0」ではない** — 下記「不一致」3 を参照。

---

## 検算した主張と結果

### 1. 行番号とシンボルの実在

| 計画書の主張 | 検算方法 | 結果 |
|---|---|---|
| `snotra-egui-runtime/src/renderer.rs:10-12` = `CLEAR_COLOR` の doc、`:13` = 定数 | `Read` | ✅ 一致（`:13` は `pub const CLEAR_COLOR: u32 = 0x0028_2828;`） |
| `src-tauri/.../window_coordinator.rs:698-700` = doc、`:702-707` = テスト本体 | `sed -n '690,712p'` | ✅ 一致（`fn` は `:702`、`assert_eq!` は `:706`、閉じ `}` は `:707`） |
| `snotra-egui-runtime/CLAUDE.md:39` = 主張 A の bullet | `grep -n "RuntimeFrame\|CLEAR_COLOR"` | ✅ 一致（`:39` のみ） |
| `docs/development-principles.md:55` = 節見出し「config の値は到達性の検出器を持たない」 | `grep -n ""` + `sed` | ✅ 一致 |
| 同 `:61` 導入文「この欠如は 3 つの形で現れる」・`:63` 「消費者ゼロ」・`:67` 主張 B・`:71` `G12_NO_LAUNCHER_READ` | 同上 | ✅ 4 件とも一致 |
| `scripts/governance-check.mjs:1228-1229` = G-config-reachability の説明コメント | `sed -n '1220,1235p'` | ✅ 一致（`:1228` が「消費者ゼロのまま」、`:1229` が「`CLEAR_COLOR` ハードコード」） |
| 同 `:1283-1295` = `NO_LAUNCHER_READ` の表 | `sed -n '1283,1300p'` | ✅ 一致（`:1283` が `export const`、`:1295` が `};`）。`VisualConfig.background_color` は**載っていない** ✅ |
| `docs/build-commands.md:77` = `:67` への引用元 | `sed -n '70,80p'` | ✅ 一致（原理の引用先は節見出し `:55`。実在するので空振りしない） |
| `snotra-core/src/config.rs:366` = `default_background_color()` | `grep -n` | ✅ 一致 |
| `snotra-egui-runtime/src/lib.rs:15` = `pub use renderer::CLEAR_COLOR;` | `grep -n` | ✅ 一致 |
| `scripts/governance-check.mjs:1419-1442` = G-stale-identifiers の現行射程 | `sed -n '1365,1450p'` | ✅ 一致（`:1419` `VOCAB_SOURCE_EXT`、`:1423` `VOCAB_TEST_FILE`、`:1425` `STALE_EXTRA_DOCS`、`:1427` `STALE_IDENT`、`:1435` `staleIdentifierDocs`、`:1441` `staleIdentifierTargets`） |
| `STALE_IDENT` の正規表現 = `/^([a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)(\(\))?$/` | `:1427` 逐語 | ✅ 一致 |
| `VOCAB_SOURCE_EXT = /\.(rs\|ts\|tsx\|mjs\|ps1\|toml)$/` / `VOCAB_TEST_FILE = /\.test\.(mjs\|ts\|tsx)$/` | `:1419` / `:1423` 逐語 | ✅ 一致 |
| plan B1「`runAll` の `staleDocs.length === 0` が fail-closed」 | `sed -n '1718,1730p'` | ✅ 一致（`:1725`。`:1723-1724` に「`STALE_EXTRA_DOCS` を混ぜると沈黙する」旨の doc あり） |
| plan B4「自称スコープの doc コメント `:1371-1417`」 | `sed -n '1369,1372p;1415,1418p'` | ⚠️ **ほぼ一致（誤差 1〜2 行）**。節の区切り `---` は `:1370` と `:1416`、散文は `:1371-1415`、`:1417` は空行 |
| `docs/development-principles.md` の SolidJS 期識別子の行（B10） | 測定スクリプト出力 | ✅ 一致（`viewKind` = `:78`,`:83` / `interpKind` = `:78`,`:84` / `assertNever` = `:81` / `isInstantPrefix` = `:84`）。詳細は §「集計ではなく分類」 |
| research「issue の行番号は移動している」表 4 行 | `gh issue view 825` 逐語 | ✅ 4 行とも一致。issue 本文は `renderer.rs:11-12` / `CLAUDE.md:38` / `development-principles.md:66` / `governance-check.mjs:1067-1068` と書いており、現在値との対応は research の表どおり |
| #819 が引く `development-principles.md:70`（現 `:71`） | `gh issue view 819` 逐語 | ✅ 一致。issue はほかに `governance-check.mjs:982`（現 `:1283`）・`:1107-1109`（現 `:1435-1437`）・`:1102`（現 `:1427`）も引いており、research の「引かれている行番号は #885 で移動済み」は全て真 |
| research「`snotra-egui-runtime` は `snotra-core` に依存しない」 | `grep -n "snotra-core" snotra-egui-runtime/Cargo.toml` → 0 件 | ✅ 一致 |
| research「追加コミット `9c64c09`（#802）が `visual-check-colors.ps1` も追加」 | `git show --stat 9c64c09` | ✅ 一致（`scripts/visual-check-colors.ps1 \| 236 +++`・`window_coordinator.rs \| 85 ++`） |
| research「`.md` → `.rs` のパス参照は G-references が照合（`REF_EXTENSIONS` に `.rs`）」 | `grep -n REF_EXTENSIONS` | ✅ 一致（`:30` = `/\.(md\|rs\|ts\|tsx\|mjs\|json\|toml\|yml\|ps1\|html\|css)$/`。research の「`:29-30`」は doc 行込みの範囲） |
| plan「`SPEC.md` は `CLEAR_COLOR` に言及していない（0 件）」 | `grep -c CLEAR_COLOR SPEC.md` → `0` | ✅ 一致 |

### 2. `runtime_fallback_matches_config_default_background` は本当に「一致に落ちる検査」か

**測った（フォールトインジェクション）。** 変異は `renderer.rs:13` の `CLEAR_COLOR` を `0x0028_2828` → `0x0028_2829` に 1 文字。
`snotra-core` 側を変えると `config.rs` の既存 assert 群（`:1345`, `:1670`, `:1728` 等）も同時に落ちて赤の帰属が曖昧になるため、runtime 側へ当てた。

| 主張 | 検算方法 | 結果 |
|---|---|---|
| 乖離したら検査が落ちる | `CLEAR_COLOR` を `0x0028_2829` へ変えて実行 | ✅ **FAILED を実測**。`window_coordinator.rs:706` で `assertion left == right failed / left: 2631720 / right: 2631721` |
| 変異前に通る | 変異前に実行 | ✅ `test ... ok`（`193 filtered out`） |
| 復帰後に通る | 逆 Edit 後に再実行 | ✅ `test result: ok. 1 passed` |
| plan 受け入れ条件 5・A3・A12 の `cargo test -p snotra --lib` | そのまま実行 | ❌ **不一致。`error: no library targets found in package 'snotra'`**（下記「不一致」1） |
| 受け入れ条件 5「`cargo test` が緑」 | — | ⚠️ **未検証**。私が実行したのは当該 1 件のみ（`1 passed; 193 filtered out`）で、suite 全体（194 件）は走らせていない。コマンド形の誤りを指摘済みゆえ、緑の確認は実装時に `cargo test -p snotra` で行うこと |

作業後の状態: `git status --short` = ` M workspace/plan.md` / `?? workspace/research.md` の 2 件のみ＝**着手前のベースラインへ復帰**（元から clean ではない）。
`git diff --stat snotra-egui-runtime/src/renderer.rs` は空。

### 3. 「`background_color` の消費者は実在する」の裏取り

`grep -rn "background_color\|set_clear_color" src-tauri/src/egui_shell/` で経路を自分で辿った。

| research の主張 | 結果 |
|---|---|
| `visual.rs:100` — `background: hex_or(&v.background_color, &d.background_color)` | ✅ 逐語一致 |
| `view.rs:343` — `frame.set_clear_color(visual.background)` | ✅ 逐語一致 |
| `mod.rs:273,281,292,308` — 窓生成の `.background_color` とネイティブブラシ | ✅ 4 行とも一致 |
| `NO_LAUNCHER_READ` に `VisualConfig.background_color` が載っていない | ✅ 一致（表 `:1284-1294` の 11 エントリに無い） |
| `npm run governance:check` が緑 | ✅ 実測「全検査 passed（検査 19 件 / … / config フィールド 67 件の到達性 / 散文の識別子 1 件を 25 文書から照合）」 |

**経路は閉じている**: config `VisualConfig.background_color` → `visual.rs:100` が `VisualSnapshot.background` へ畳む → `view.rs:343` が `frame.set_clear_color` で描画へ渡す。
つまり「消費者ゼロ」は現在形では偽 ✅（research の帰結を支持）。

### 4. research.md「追補」の測定表の再現

**まず述語の照合**（スクリプトが本物の写しとして正しいか）。`scripts/governance-check.mjs` の実装と 1 行ずつ突き合わせた。

| 要素 | 本物 | proxy 2 本 | 判定 |
|---|---|---|---|
| `CAMEL` | `STALE_IDENT`（`:1427`） | 逐語同一 | ✅ |
| `SCREAM` | （新規・本物に無い） | `/^([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)(\(\))?$/` = plan §採る射程 2 と逐語同一 | ✅ |
| `EXTERNAL_CMD_LINE` | `:1429` | 逐語同一 | ✅ |
| `linesOutsideFences` | `:67-78` | 変数名のみ違う同一実装 | ✅ |
| `raw.includes("/"\|" "\|".")` の除外 | `:1478` | 同一 | ✅ |
| `inVocab` の `\b<id>\b` メモ化 | `:1464-1467` | 同一 | ✅ |
| `base()` | `staleIdentifierTargets`（`:1441-1443`） | 同一（`staleIdentifierDocs` は本物を import） | ✅ |
| `currentVocabulary` の `#`/`//` 除去分岐 | `:1453`（`ps1\|toml` は `#`、他は `stripRustComments`） | measure 側は本物を import。vocab-widen 側は `ps1\|toml\|yml` を `#` 側へ | ⚠️ 差分（下記「不一致」5） |
| `text == null` で母集団欠落の finding | `:1470-1472` | proxy 2 本とも `continue` | ⚠️ 差分（`SPEC.md` は実在するので今日は影響 0。実在を実測） |

**次に数値**。両スクリプトをそのまま実行した。

| research の表 | 実測 | 結果 |
|---|---|---|
| ベースライン 1 / 0 | `docs=25 照合=1 finding=0` | ✅ |
| E 単独 7 / 0 | `照合=7 finding=0` | ✅ |
| D 単独 69 / 35 | `docs=60 照合=69 finding=35` | ✅ |
| D- 単独 18 / 8 | `docs=32 照合=18 finding=8` | ✅ |
| M 単独 9 / 2 | `docs=31 照合=9 finding=2` | ✅ |
| D+E 107 / 40 | `照合=107 finding=40` | ✅ |
| D-+E 43 / 12 | `照合=43 finding=12` | ✅ |
| **D-+E + `.yml`/`.json`（採用）43 / 9 / 真 9 / 偽 0** | **どちらのスクリプトも測っていなかった。**私が直接測って `docs=32 照合=43 finding=9` を再現 | ✅ 件数は再現（証拠は本レビューが初出）／❌ **内訳「真 9 / 偽 0」は不一致**（不一致 3） |
| D-+M+E + 同語彙 73 / 13 / 真 10 / 偽 3 | `照合=73 finding=13`。内訳 = 採用セルの 9 + `iconCacheSize`(真) + `WM_SETCURSOR`/`numFonts`/`MARKER_DONT_FOCUS`(偽 3) | ✅ 件数一致（真/偽の総数は不一致 3 と連動して変わる） |
| M 軸の分類「真 1（`iconCacheSize`）: 偽 3（いずれもソースのコメントにしか現れない外部語彙）」 | 下記 grep で 4 語を実測 | ✅ **逐語で一致**（§「測ったが計画書に無い事実」10 に生出力） |
| 本文「実測: **13 → 12 → 9** の推移」 | vocab-widen の実出力は **16 → 14 → 13**（D-+M+E）。D-+E で測ると **12 → 10 → 9** | ❌ **不一致**（下記 3） |
| 「`.yml`/`.json` で偽陽性 3 件が消え、真の腐りは 1 件も沈黙しない」 | ✅ 実測。`.yml` で `GITHUB_TOKEN` ×2、`.json` で `CLAUDE_PROJECT_DIR` が消え、真の腐り 9 件は 3 セルとも不変 | ✅ **結論は正しい**（数値の写しだけが誤り） |
| 「D 単独の finding 35 件中 27 件が ADR」 | D 単独セルの出力を `grep "docs/adr/" \| wc -l` → `27` | ✅ |
| 偽陽性の実在場所: `GITHUB_TOKEN` は `.github/workflows/label-sync.yml` にのみ | `grep -rln GITHUB_TOKEN` → `./.github/workflows/label-sync.yml` 1 件のみ | ✅ |
| `CLAUDE_PROJECT_DIR` は `.claude/settings.json` にのみ実在（`post-edit.mjs` にはコメントとしてのみ） | `grep -rn` → `post-edit.mjs:91`（`* 「存在するか」だけを見る。CLAUDE_PROJECT_DIR にも…` ＝ doc コメント）+ `settings.json:9,21` | ✅ **逐語で正しい** |
| B10「`backgroundThrottlingPolicy` はどの `.json` にも存在しない」 | `grep -rn --include=*.json` → 0 件 | ✅ |
| B9「今日 `*.test.json` は存在しない（実測してから書くこと）」 | snapshot 上で `/\.test\.json$/` → **0 件** | ✅（先に測っておいた） |
| M 母集団の 6 ファイルが実在するか（過小でないか） | snapshot 照会 | ✅ 6 件すべて実在（`CLAUDE.md` / `AGENTS.md` / `snotra-core` / `snotra-egui-runtime` / `src-tauri` / `snotra-settings`） |
| ADR の「歴史としてそのまま残す」記述 | `grep -rn` → `docs/adr/ADR-stale-identifier-detector-scope.md:110` | ✅ 実在 |

### 5. `npm run check:colors` が「非既定色で自動判定する」か（実行はしていない）

`scripts/visual-check-colors.ps1`（304 行）を読んだ。

| research / plan の主張 | 一次証拠 | 結果 |
|---|---|---|
| 既定が非既定色 `#4A2B5C` | `:30` doc「既定は `#4A2B5C`（既定色とも `CLEAR_COLOR` とも明確に違う紫）」/ `:48` `[string]$Color = '#4A2B5C'` | ✅ |
| 実ピクセルの**最頻色**を期待色と突き合わせる | `:152-153` コメント「**1 点ではなく最頻色で判定する。**」/ `:167` `$ok = ($mode -eq $ExpectedKey)` | ✅（research が引く `:167` は今日の行番号） |
| **exit code で**判定 | `:16` doc「**exit code で** 返す」/ `:304` `if (-not $succeeded) { exit 1 }` | ✅（`:16` `:304` とも今日の行番号） |
| main / results の両方を測る | `:238` `$mainResult = Measure-WindowBackground …` / `:254` `$resultsResult = …` | ✅ |
| 色を config へ書き込んでから起動する | `:106` `background_color = "$Color"` | ✅（＝`set_clear_color` 呼び忘れなら背景が `CLEAR_COLOR` のまま残り、最頻色が期待色と外れて exit 1 になる。CLAUDE.md:39 の「目視だけ」は偽） |
| CI には無い | `grep -rn "check:colors" .github/workflows/` → **0 件** / `package.json:17` にのみ | ✅ |
| GUI・非ロック画面を要する | `docs/build-commands.md:80`「**画面がロックされていると実行できない**（#866）」 | ✅ |
| plan A6/A7 が引用する「`[visual]` の色を変える変更は、**非既定色で**目視する」 | `docs/build-commands.md:69` の**見出し**として実在 | ✅（G-heading-refs の照合対象になる形） |

⚠️ **実行はしていない**（GUI と非ロック画面を要するため。指示どおり）。上記はすべてスクリプト本文の静的読み取り。

---

## 不一致（要修正）

### 1. ❌ `cargo test -p snotra --lib` は存在しないターゲットを指す（受け入れ条件 5・A3・A12 の 3 箇所）

```
$ cargo test -p snotra --lib
error: no library targets found in package `snotra`
```

`src-tauri/Cargo.toml` は `[package] name = "snotra"` で **`[lib]` セクションを持たない**（バイナリ crate）。当該テストは `src/main.rs` のユニットテストとして走る。
正しい形は `cargo test -p snotra`（`docs/build-commands.md:19` と `:148` が SSOT として書いている形）。`--bins` でも通る:

```
$ cargo test -p snotra --bins runtime_fallback_matches_config_default_background
     Running unittests src\main.rs (target\debug\deps\snotra-914fd415245da96f.exe)
running 1 test
test egui_shell::window_coordinator::tests::runtime_fallback_matches_config_default_background ... ok
```

**plan.md の 3 箇所（`:33`, `:87`, `:144`）を `cargo test -p snotra` へ直すこと。** そのまま実行すると A3 / A12 が exit 101 で止まる。

### 2. ❌ 採用セル「D-+E + 語彙源に `.yml`/`.json`」は、どちらのスクリプトでも測られていなかった

- `measure-stale-axes.mjs` は語彙に `currentVocabulary(snapshot)`（**現行語彙のまま**）しか使わない。9 セルすべて現行語彙。
- `vocab-widen.mjs` の母集団は `[...base, ...docsNoAdr, ...mod]` = **D-+M+E**（M 入り）。3 行とも M 入りで、D-+E は 1 行も出力しない。

plan.md の Phase B 着手前ゲート（finding 9 → PR 1 後に 8）はこの行に載っているので、未測定は実質的な穴だった。
**本レビューで `verify-adopted-cell.mjs` を書いて直接測り、`docs=32 照合=43 finding=9` を再現した**ため、表の数値そのものは正しい。証拠が計画書のどこにも無かった点だけが不一致。

再現スクリプト: `C:/Users/Eoh/AppData/Local/Temp/claude/C--workspace-Snotra/2b2184b6-6635-4cc7-862f-51ae76be5b70/scratchpad/verify-adopted-cell.mjs`

生出力（採用セル・逐語）:

```
=== D-+E / vocab=+yml+json (採用) === docs=32 照合=43 finding=9
  docs/development-principles.md:39   `shouldShowResults`
  docs/development-principles.md:71   `G12_NO_LAUNCHER_READ`
  docs/development-principles.md:78   `viewKind`
  docs/development-principles.md:78   `interpKind`
  docs/development-principles.md:81   `assertNever`
  docs/development-principles.md:83   `viewKind`
  docs/development-principles.md:84   `isInstantPrefix`
  docs/development-principles.md:84   `interpKind`
  docs/development-principles.md:128  `backgroundThrottlingPolicy`
```

（同スクリプトの現行語彙行は `照合=43 finding=12`＝上記 9 件 + `docs/build-commands.md:215` の `GITHUB_TOKEN` ×2 + `docs/hooks.md:67` の `CLAUDE_PROJECT_DIR`。`+yml` 行は `finding=10`。）

### 3. ❌ 採用セルの「真 9 / 偽 0」は、計画書自身の偽陽性基準に照らすと成り立たない

計画書が M 軸を却下した基準は「いずれも**ソースのコメントにしか現れない外部語彙**である。モジュール文書はラップ対象の外部 API を語る場所で、`docs/**` とは母集団の性質が違う」（plan `:194`）。
**同じ基準を採用セルの 9 件へ当てると、`backgroundThrottlingPolicy` は真の腐りではない。**

```
$ grep -rn "backgroundThrottlingPolicy" --include=*.json --include=*.rs --include=*.ts --include=*.tsx \
    --include=*.mjs --include=*.ps1 --include=*.toml --include=*.md --include=*.yml . \
    | grep -v "^./target" | grep -v node_modules | grep -v "^./workspace/"
./docs/development-principles.md:128:- `tauri.conf.json` や platform 固有ファイルに設定を追加する際は、その設定が Windows で
  サポートされているか事前に確認する（例: `backgroundThrottlingPolicy` は Windows 非対応でビルドエラーになる）
```

これは Tauri の設定キー＝**外部語彙**であり、しかも「Windows 非対応だから足すな」という**反例として**散文に置かれている。撤去された自前の語（`viewKind` 等）とは性質が違う。
**内訳は「真 8 / 偽 1」であり、「偽陽性 0」ではない。** 影響は 3 点:

- **表と ADR の「偽陽性 0」というラベルが偽になる**（`AGENTS.md`「全称表現は前提条件とセットで書く」）
- **この偽陽性は語彙源の拡大では吸収できない。** `GITHUB_TOKEN` / `CLAUDE_PROJECT_DIR` は `.yml` / `.json` に実在するから構造で消えたが、`backgroundThrottlingPolicy` は**リポジトリのどのファイルにも存在しない**。「免除注記の機構を設けない」契約（Phase B 不変条件表）の下では、**文書側を書き換える以外に消す手が無い**
- **B12 が書く「否定の知識」に、この非対称が抜けている。** 「語彙源の拡大で偽陽性が構造的に消える」は 3 件中 2 件についてのみ真であり、3 件目は**新しい受容残余**として記録が要る

なお B10 は `backgroundThrottlingPolicy` を「撤去されたフロントの語」に含めず「リポジトリのどの `.json` にも存在しない（実測）」とだけ書いており、**性質の違いには気づいている**。是正方針（散文へ倒すか）を決めるところまで書けば整合する。

### 4. ❌ research.md「実測: 13 → 12 → 9 件の推移」が再現しない

`vocab-widen.mjs`（= 唯一この推移を出しうるスクリプト）の実出力:

```
=== D-+M+E / vocab=現行 (rs|ts|tsx|mjs|ps1|toml) === 照合=73 finding=16
=== D-+M+E / vocab=+yml                        === 照合=73 finding=14
=== D-+M+E / vocab=+yml+json                   === 照合=73 finding=13
```

D-+E（M 抜き）で測り直した生出力のヘッダ 3 行:

```
=== D-+E / vocab=現行 (rs|ts|tsx|mjs|ps1|toml) === docs=32 照合=43 finding=12
=== D-+E / vocab=+yml                          === docs=32 照合=43 finding=10
=== D-+E / vocab=+yml+json (採用)              === docs=32 照合=43 finding=9
```

つまり **12 → 10 → 9**。「13 → 12 → 9」は**どちらの系列でもない**——`12` と `9` は D-+E 系列の両端、`13` は D-+M+E 系列の末尾で、3 つの数字が別々の行から拾われている。
消える件数（3 件）と「真の腐りは不変」という**結論は両系列で正しい**が、括弧内の数値は写し間違いなので、`(12 → 10 → 9)`（D-+E 基準）へ直すこと。

なお research の表「D-+M+E + 同語彙 | 73 | 13」は**正しい**（`同語彙` = `+yml+json` の行）。誤っているのは本文の推移だけ。

### 5. ❌ plan A10「下のパターンでは両方とも 0 件だが（実測）」が偽

`消費者ゼロ` は `docs/superpowers/` に 1 件ある:

```
docs/superpowers/specs/2026-07-28-config-background-color-design.md:71:
  消費者ゼロを確認したうえで削除する（§1）。…
```

（`落ちる検査は無い\|一致は規約\|機構ではなく規約` の方は `docs/superpowers/` + `.superpowers/` とも 0 件 ✅。）
つまり `docs/superpowers/` の除外は**防御的ではなく実効的**である。「防御的だが 0 件」と書くと、除外を外した人が理由を誤解する。

### 6. ❌ plan A10「`grep` は部分一致ゆえ「落ちる検査は無い」を**二重に数える**」が偽

単一の alternation パターン（`"落ちる検査は無い\|検査は無い"`）を `grep -rn` へ渡すと、**マッチした行は 1 回しか出力されない**。実測:

```
$ grep -rn "検査は無い" --include=*.rs --include=*.mjs --include=*.md . | grep -v target | grep -v node_modules | grep -v "^./workspace/"
./.superpowers/sdd/plan/spec-inventory-duplication.md:451: … 本棚卸しの findings に対応する検査は無い。 …
./snotra-egui-runtime/CLAUDE.md:39: …
./snotra-egui-runtime/src/renderer.rs:12: …
```

3 行＝重複なし。**「全く別の残余を述べる文を巻き込む」という指摘の方は正しく、実例（`spec-inventory-duplication.md:451`）も逐語で実在する** ✅。理由の前半だけを削ること。

### 7. ❌ research「`受容する残余` の他 60 箇所」— 実測 49 件

```
$ grep -rn "受容する残余" --include=*.rs --include=*.mjs --include=*.md . \
    | grep -v "^./target" | grep -v node_modules | grep -v "^./docs/superpowers/" \
    | grep -v "^./.superpowers/" | grep -v "^./workspace/" | wc -l
49
```

（除外なし・`target`/`node_modules` のみ落とすと 135 件。）分類の主張（「全く別の残余についての記述」）は妥当だが、件数は 49。

### 8. ⚠️ plan B4 の範囲 `:1371-1417` は誤差 1〜2 行

節の区切り `// ---` は `:1370` と `:1416`、散文は `:1371-1415`、`:1417` は空行。改訂対象は `:1371-1415`。

### 9. ⚠️ 受け入れ条件 1 のマーカー語 grep は A2（第 5 の頂点）を検算できない

`window_coordinator.rs:700` の逐語は「一致は**今まで**規約でしか**なかった**」であり、パターン `一致は規約` に部分一致**しない**（実測: マーカー語 grep のヒットは `CLAUDE.md:39` と `renderer.rs:12` の 2 件のみで、`window_coordinator.rs` は現れない）。
つまり A10 が緑でも「A2 をやり忘れた」状態と区別がつかない。A2 の完了は目視か別の検算（例: `一致は今まで規約` / `が受容した残余` を足す）で担保すること。

---

## 測ったが計画書に無い事実

1. **`set_clear_color` の消費者は 4 経路目がある** — `src-tauri/src/egui_shell/results_view.rs:466` も `frame.set_clear_color(visual.background)` を呼ぶ。research は `view.rs:343` しか挙げていない。「消費者ゼロは偽」の結論はむしろ強まる。

2. **偽陽性 3 件の所在は M 側ではなく D- 側である** — `docs/build-commands.md:215`（`GITHUB_TOKEN` が**同一行に 2 回**）と `docs/hooks.md:67`（`CLAUDE_PROJECT_DIR`）。research は「偽陽性 3 件」とだけ書いて所在を記していないが、これは「`docs/**` を母集団へ入れたことで生まれた偽陽性」であり、M の偽陽性 3 件（`WM_SETCURSOR` / `numFonts` / `MARKER_DONT_FOCUS`）とは**別の 3 件**である。ADR へ書くときに取り違えると論証が崩れる。

3. **`shouldShowResults` の finding は `:39` で、B10 が挙げる `:78-84` 帯の外にある** — `docs/development-principles.md:39`「インラインコードしかない箇所（例: `shouldShowResults` memo）を参照するなら…」。ここは**参照の作法を説く節**で、SolidJS 期の語を**例示**として使っている。B10 が言う「教訓の**出典**として書かれている」形とは文脈が違い、「バッククォートを外して散文にする」で足りるかは個別判断が要る（`:128` の `backgroundThrottlingPolicy` は別カテゴリ＝「不一致」3）。**8 件を一律の方針で処理できない。**

4. **B10 の内訳は完全に正しい** — 実測 8 件 / 相異なる識別子 6 個。`viewKind` = `:78`,`:83` / `interpKind` = `:78`,`:84` / `shouldShowResults` = `:39` / `assertNever` = `:81` / `isInstantPrefix` = `:84` / `backgroundThrottlingPolicy` = `:128`。件数だけでなく行の帰属まで一致した。

5. **`vocab-widen.mjs` は `.json` を `stripRustComments` 側へ回している**（`#` 除去は `ps1|toml|yml` のみ）。JSON 文字列中の `//`（URL 等）以降が落ちるため語彙が**減る**方向＝finding が**増える**方向で、「偽陽性 0」は保守側の測定になっている ✅。ただし **plan B3 は `.yml` の行き先しか書いておらず `.json` の扱いが未確定**。実装時に測定と違う側（`#` 除去）へ回すと数値が変わりうるので、B3 に「`.json` は `stripRustComments` 側」と明記すべき。

6. **proxy 2 本は `text == null` を finding にせず `continue` する**（本物は「母集団の欠落」として鳴らす・`:1470-1472`）。`STALE_EXTRA_DOCS` の `SPEC.md` は snapshot 上に実在するため今日の測定への影響は 0（実測）。ただし B8 の逆向き検算をこのスクリプトで行うなら、この差分が「読めないファイルを黙って飛ばす」形で効きうる。

7. **ベースラインの `governance:check` は緑** — 「全検査 passed（検査 19 件 / 対象文書 64 件 / rules 7 件 / skills 14 件 / … / **散文の識別子 1 件を 25 文書から照合** / …）」。この「1 件 / 25 文書」が測定表のベースライン行（照合 1・docs 25）と一致する＝**proxy の母集団が本物と同じ**ことの独立な裏取りになっている。

8. **`.superpowers/` の二重防御は実在する** — `.gitignore:25` に `.superpowers/`、`scripts/governance-check.mjs:37` の `WALK_EXCLUDE_PREFIXES = ["workspace", ".claude/worktrees", ".superpowers"]`。ただし `grep -rn` はこれらを見ないので（実測で `.superpowers/` の行がヒットした）、plan A10 の除外行は必要 ✅。

9. **`measure-stale-axes.mjs` の生出力（現行語彙 9 セル・逐語）**:

   ```
   === ベースライン（現行） === docs=25 照合=1 finding=0
   === E 単独（SCREAMING を述語へ） === docs=25 照合=7 finding=0
   === D 単独（docs/** を対象へ・camelCase のみ） === docs=60 照合=69 finding=35
   === D- 単独（docs/** − adr・camelCase のみ） === docs=32 照合=18 finding=8
   === M 単独（モジュール CLAUDE.md・camelCase のみ） === docs=31 照合=9 finding=2
   === D+E === docs=60 照合=107 finding=40
   === D-+E === docs=32 照合=43 finding=12
   === D-+M+E === docs=38 照合=73 finding=16
   === D+M+E（最大） === docs=66 照合=137 finding=44
   ```

   `vocab-widen.mjs` の生出力（ヘッダのみ・逐語）:

   ```
   === D-+M+E / vocab=現行 (rs|ts|tsx|mjs|ps1|toml) === 照合=73 finding=16
   === D-+M+E / vocab=+yml === 照合=73 finding=14
   === D-+M+E / vocab=+yml+json === 照合=73 finding=13
   ```

10. **M 軸の分類「真 1 : 偽 3」は逐語で正しい**（`iconCacheSize` はどこにも無く、他 3 語はすべて `//` / `///` コメント行にしか無い）:

    ```
    $ grep -rn "iconCacheSize\|WM_SETCURSOR\|MARKER_DONT_FOCUS\|numFonts" \
        --include=*.rs --include=*.ts --include=*.tsx --include=*.mjs --include=*.ps1 --include=*.toml . \
        | grep -v "^./target" | grep -v node_modules
    ./snotra-egui-runtime/src/runtime.rs:492:        // 最後に呼んだ者が勝ち、マウス静止中は `WM_SETCURSOR` が来ないので OS の復元も
    ./snotra-egui-runtime/src/runtime.rs:499:        // 一度ずれうる。マウスを動かせば `WM_SETCURSOR` で窓ごとの値へ復帰する。
    ./snotra-settings/src/font.rs:171:    // TrueType Collection header: "ttcf" tag, then numFonts (u32 BE) at offset 8.
    ./snotra-settings/src/font.rs:239:        b.extend_from_slice(&num_fonts.to_be_bytes()); // numFonts at offset 8
    ./snotra-settings/src/font.rs:273:        // Claims to be a collection but is too short to hold numFonts → reject.
    ./src-tauri/src/egui_shell/results_window.rs:95:    /// tao 内部で `SW_SHOWNOACTIVATE` に至る唯一の経路（`MARKER_DONT_FOCUS`）は窓生成時に
    ```

    `iconCacheSize` は 0 件＝真の腐り ✅。ADR へ書く「否定の知識」の根拠は測定で立つ。

11. **ただし「コメントにしか現れない」は D- 側にも当てはまる語がある** — `shouldShowResults` と `interpKind` は `src-tauri/src/egui_shell/search_state.rs:464`（`/// SolidJS \`shouldShowResults\`（search.ts: \`interpKind()==="instant" || !indexing()\`）の鏡写しで、`）・`:1133`・`view.rs:762` に**コメントとして実在する**。`assertNever` と `isInstantPrefix` は 0 件（完全に不在）。
    つまり M 軸と D- 軸を分ける線は「コメントにしか現れないか」ではなく「**外部 API か / 自前の撤去済み語か**」である。plan `:194` の論拠文（「いずれもソースのコメントにしか現れない外部語彙である」）は両方の条件を並べているので誤りではないが、**識別力を持つのは後半（外部語彙）だけ**である。ADR へ書くときは前半を根拠にしないこと。

12. **`.claude/rules/safety-nets.md` の引用は逐語で正しい** — `paths` に `scripts/*.mjs` が実在（frontmatter `:8`）、`:28`「種が書けない変更（索引の追随・改名）には仕事が無い」、`:22`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」、`:33`「検査の入力集合を、具体対象で検算する」。Phase A で `/norm-review` とフォールトインジェクションを不要とする plan の論拠は、rule の文言に接地している ✅。

---

## 実行したコマンド（一次証拠の索引）

| 目的 | コマンド |
|---|---|
| ベースライン | `git status --short` / `npm run governance:check` |
| 行番号 | `sed -n '690,712p' src-tauri/src/egui_shell/window_coordinator.rs` ほか `grep -n ""` + `sed` |
| フォールトインジェクション | `cargo test -p snotra --bins runtime_fallback_matches_config_default_background`（変異前 / 変異後 / 復帰後の 3 回） |
| 測定表 | `node …/measure-stale-axes.mjs` / `node …/vocab-widen.mjs` / `node …/verify-adopted-cell.mjs`（新規） |
| 語彙源 | `grep -rln GITHUB_TOKEN` / `grep -rn CLAUDE_PROJECT_DIR .claude/` / `grep -rn --include=*.json backgroundThrottlingPolicy` |
| check:colors | `grep -n "4A2B5C\|exit \|最頻" scripts/visual-check-colors.ps1` / `grep -rn "check:colors" .github/workflows/`（0 件） |
| issue | `gh issue view 825` / `gh issue view 819` |

**リポジトリは未変更のまま**（`workspace/plan.md` の既存変更と `workspace/research.md` の untracked、および本ファイルのみ）。
