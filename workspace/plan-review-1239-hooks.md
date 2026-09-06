対象 issue: #1239

# plan-review-1239-hooks — 観点1（hook 変更）・観点2（撤去と写し）

検証対象: `workspace/plan.md`。委譲元指示の2観点のみを見る。

## 観点1 — セーフティネット（hook）の変更

### selectChecks の現状分岐と rust-toolchain.toml の落ち先

- `rust-toolchain.toml` はリポジトリ直下のみに存在する単一ファイル（`git ls-files 'rust-toolchain.toml' '**/rust-toolchain.toml'` は 1 件）。
- `selectChecks`（`.claude/hooks/post-edit.mjs:128-178`）の既存分岐（`isRust` = `.rs`、`CARGO_MANIFEST` = `Cargo.toml`、`config.toml`/`tauri.conf.json`、`CHECK_DEFINITION` の4件、`.githooks/`、`.claude/lsp/`・`rust-analyzer.toml`）のどれにも `rust-toolchain.toml` はマッチしない。**現状は `checks = []` — 真の沈黙（「何も走らなかった」）である。** 計画のこの認識は正しい。

**判定: 軽微（現状認識は正しい）**

### 足す位置と形が計画の記述と一致するか

- 計画は「`.claude/hooks/post-edit.mjs` `selectChecks`」の変更を「L166〜174 の『検査の定義を変えるファイル』の並び」と記述している。
- しかし実測（`post-edit.mjs:152-174`）では:
  - **`CHECK_DEFINITION.has(rel)` のブロックは L152-156**（コメント「セーフティネットそのものと、検査の定義を変えるファイルを編集したときは…」＝計画が引用した文言そのもの）。`CHECK_DEFINITION` の Set 定義自体は L70-82 にあり、現在 `.claude/settings.json` / `package.json` / `vitest.config.ts` / `Cargo.toml` の4件（ルートのみ・basename 完全一致）。
  - **L166-174 は別ブロック**（`.claude/lsp/` と `rust-analyzer.toml` を basename アンカーで拾う正規表現ブロック。コメントの見出しは「Claude Code の rust-analyzer インスタンスへ渡す設定」）。
  - `rust-toolchain.toml` はルート限定・basename 完全一致で足りる（`rust-analyzer.toml` のような crate 直下再帰は不要——`rust-toolchain.toml` は cargo のワークスペース解決規則上、リポジトリ直下の1枚だけが効く）。この性質は `CHECK_DEFINITION` の想定（`Cargo.toml` はルートのみ、の項の反対側）と一致し、**本来の置き場所は `CHECK_DEFINITION` Set（L77-82）へ `"rust-toolchain.toml"` を足すことであり、L166-174 の正規表現ブロックではない**。
- **要対処**: 計画の「L166〜174」という行番号参照は誤りで、実装者がこの記述に従うと `rust-toolchain.toml` を `rust-analyzer.toml` と同じ basename 正規表現ブロックへ足してしまう可能性がある（crate 直下再帰は不要な性質のファイルに対して過剰な形になる。動作は壊れないが「検査の定義を変えるファイル」という計画自身の分類とコード上の実際の分類が食い違う）。実装前に `CHECK_DEFINITION`（L77-82）へ追加する形へ訂正すべき。

**判定: 要対処**

### docs/hooks.md への反映要否と機械照合

- `docs/hooks.md:54-57` の表（ヘッダ `| 編集したファイル（代表パス） | 走る検査 id |`）は **`scripts/governance/checks/G-hook-fires.mjs`** が `selectChecks` を実際に import して行ごとに照合する（`checkHookFires`）。表とコードが1文字でもずれると `governance:check` が赤になる。
  - 確認: `G-hook-fires.mjs:1-10`（`import { selectChecks } from "../../../.claude/hooks/post-edit.mjs"`）、`docs/hooks.md:54-57` に代表パス行の実例あり。
  - **したがって、`selectChecks` を変えて `docs/hooks.md` L54-57 の表に行を足さなければ PR は governance:check で確実に赤になる。** 計画はこの表への追記を「`docs/hooks.md` L56 付近の表に…足す」と明記しており（変更ファイル一覧・作業項目 Phase 3）、機構と整合する。
- 一方、`docs/hooks.md:89` 付近の「壊れ方の分類」表（「設定が届かない・上書きされる」など）は `G-hook-fires` の対象ではない（そのヘッダ行は「編集したファイル（代表パス）」ではなく別表）。`grep -n "壊れ方"` 等で該当検査を探したが見当たらず、**この表を機械照合する governance 検査は無い**（プロースの整合性チェックのみ、`governance:check` の対象外）。計画の「component が無い」行追加は妥当だが、こちらは untestedな散文であり「直さなくても governance:check は落ちない」——計画にはその区別（表54-57は機械照合・表89は非照合）が書かれていない。
- **判定: 軽微**（表89側の追記自体は良い判断だが、2つの表の性質差＝片方は機構が守り片方は守らないという事実を計画に明記した方が実装時の優先順位が明確になる）

### 故障注入の強さ（複製 toml から削る/行ごと消す）

- `lsp-config.test.mjs` の既存ヘルパー `materialize()`（L21-36）は `COPIED` 配列（L23-28）に列挙されたファイルだけを複製する。**現在の `COPIED` に `rust-toolchain.toml` は無い。**
- 計画の変更ファイル一覧・作業項目は「`lsp-config.test.mjs` に故障注入2本」を挙げるが、**`COPIED` 配列へ `rust-toolchain.toml` を追加する旨が計画に明記されていない**。追加を忘れると `materialize()` が複製した temp ディレクトリに `rust-toolchain.toml` が存在せず、`checkLspConfig` 内の新規読み取りが「ファイルが無い」エラー（または既存コードの `try/catch` パターンに倣うなら早期 return）を返し、狙った変異（"rust-analyzer" を消す／`components` 行を消す）ではなく「ファイル不在」の枝を検査してしまう。
- **要対処**: `COPIED` への追加を計画に明記すべき（実装者が見落とす典型パターン——既存 `COPIED` は判定が読む5ファイルの完全な一覧であり、新しい読み取り対象を足したら同期する必要があることは自明ではあるが、計画のファイル一覧に無いことは実装漏れのリスクを上げる）。

- 変異の強さ自体（「`"rust-analyzer"` を消す」「`components` 行ごと消す」）は、TOML 構文としてはどちらも valid TOML のまま残る変異であり（配列から要素を1つ削る／キー行ごと削る＝配列自体が無くなる＝`undefined`）、**構文エラーにはならない**（`lsp-config.test.mjs` の既存パターンで見ると、「JSON として壊れる」形の変異〔足1・足5c〕は意図的に別カテゴリとして区別されている——これらは実際に起きうる回帰と同型の compile/parse-fail ではなく、意味的な変異である）。実際に起きうる回帰（「誰かが component を外す」＝配列から要素削除、「toml を書き直す」＝`components` キー自体を書き換え/削除）と一致しており、**強すぎる変異(構文エラーで落ちる形)にはなっていない**。

**判定: 要対処（COPIED 配列追加の明記漏れ）＋ 軽微（変異の強さ自体は妥当）**

### 検査の射程宣言（宣言だけを見る／実在は見ない）

- `lsp-config.mjs` 冒頭コメント（L1-16）は既存の `.lsp.json` 検査について「沈黙する壊れ方」と「CI には実測を置けない」系の射程宣言パターンを持つ。新設する `rust-toolchain.toml` 宣言検査にも同型のコメントが要る、という計画の指示（「見るのは宣言だけ、実際に入っているかは射程の外」「その旨をコメントに書く」）は、既存の設計哲学（例: `checkLspConfig` 内の ratoml 検査コメント L86-100「配送経路の検査より前に置く」の理由書き）と整合する。
- `docs/hooks.md` の分類表（L89-98 付近、「Claude Code の RA インスタンスと hook の分担」節）に射程を書く場所が計画に挙がっているかは、上述の L89 の行追加で部分的にカバーされる。ただし「見るのは宣言だけ」という射程の明示的な一文がその表の行、または本文中に必要——計画の記述は `lsp-config.mjs` のコード内コメントへ書く指示はあるが、`docs/hooks.md` 側にも同じ射程の一文を置くとまでは書いていない。
- **判定: 軽微**（コード側のコメントで足りるとも読めるが、`docs/hooks.md` にも一文添えるとより一貫する）

## 観点2 — 手順の撤去と写し

### `rust-analyzer` を巡る生きた層の grep（docs/adr, docs/superpowers 除く）

`git grep -n "rust-analyzer"` の全件を確認した（上のコマンド結果）。`.claude/skills/deps-update/SKILL.md:40-41` の削除対象以外で、手順や「意図して外してある」という**理由**を写している箇所は見当たらない。関連する記述:

- `.claude/hooks/post-edit.mjs:162-174`（コメント）と `docs/hooks.md:54-57,67-98`（正本の分担表）は「rust-analyzer は Claude Code の LSP 設定」という別の話題（設定配送）であり、「component を手で足す」手順の写しではない。**削除対象と無関係**。
- `docs/build-commands.md:13` は「これらのコマンドが走る Rust の版は `rust-toolchain.toml` が決める」という一般論であり、component の話は書いていない。**rust-analyzer 追加で偽にならない**。
- `.github/workflows/ci.yml:140-161` のコメントは「toml 側の `components` が正本」であることを既に明記しており（L157-161: 「同じ要求は toml 側にもある——…rust-toolchain.toml の components が決める（そちらが正本・2026-08-21 実測）」）、**rust-analyzer を toml に追加しても、この散文は依然として真**（内容を列挙していないため陳腐化しない）。計画は「据え置き」とは書いていないが、変更ファイル一覧にも挙げておらず、この判断（触らない）は正しい。
- `.vscode/settings.json` の `rust-analyzer.*` キー群は VS Code 拡張の設定名前空間であり、component とは無関係。触る必要なし。

**判定: 未検証→軽微**（全件確認したが写しは deps-update の1箇所のみで、計画の削除対象と一致。ただし `docs/architecture.md` や `RETROSPECTIVE.md` のような不定期に読まれる文書に古い言及が無いかは grep 済みで0件——**この母集団に限れば網羅は取れている**）

### rust-toolchain.toml ヘッダコメント書き換えで偽になる散文

- `rust-toolchain.toml` の L17-20（`components` を書く理由の段落）は計画通りの位置（確認済み・上記 grep 出力の行番号と一致）。ここに1段落追記する計画の記述は正確。
- 書き換えによって**他ファイルの散文が偽になるか**を検討: `ci.yml:157-161` は「component は toml 側が正本」で内容を列挙しないため偽にならない。`docs/build-commands.md:13` も同様に列挙なしで偽にならない。`docs/hooks.md` にも `rust-toolchain.toml` の中身を逐語で引用した記述は無い。**偽になる散文は見当たらなかった**。

**判定: 軽微（未検出であり、これは母集団を検討した上での「無い」という結論）**

### 手順撤去後も真であるべき注意が計画に残っているか

- 削除対象の段落（`SKILL.md:40-46`）には次の3つの情報が同居している:
  1. 「`channel` を上げたら `rustup component add rust-analyzer` を打つ」の**手順**（削除対象そのもの）
  2. 「セッションが既に LSP の起動失敗を数え切っていると…Claude Code を再起動すれば戻る」の**注意**
  3. 「再起動の直後は索引が冷えており、最初の1回は『見つからない』を返しうる…温まるまで待って測り直す」の**注意**
- 計画の該当行（変更ファイル一覧の2行目）は明示的に「**component が入り直した後も、既に LSP の起動失敗を数え切ったセッションは復帰しない——Claude Code を再起動する。再起動直後は索引が冷えており最初の1回は『見つからない』を返しうる**」を残す形で書かれている。**2と3の注意は計画に明記されて残されている。**

**判定: 軽微（適切に保持されている）**

## 総括

| # | 項目 | 分類 |
|---|---|---|
| 1 | `selectChecks` 現状分岐: `rust-toolchain.toml` は真の沈黙 | 軽微（認識正しい） |
| 2 | `selectChecks` の追加位置＝L166-174 という行参照が誤り。実際は L166-174 は `.claude/lsp/`/`rust-analyzer.toml` 用の正規表現ブロックで、「検査の定義を変えるファイル」という計画自身の分類名が指すのは `CHECK_DEFINITION` Set（L77-82・使用箇所 L152-156）である | **要対処** |
| 3 | `docs/hooks.md` L54-57（発火一覧）への追記は `G-hook-fires` が機械照合するため必須。計画はこれを予定しており整合 | 軽微 |
| 4 | `docs/hooks.md` L89 の「壊れ方」表は machine-checked ではない（散文のみ）。計画はこの区別を明記していない | 軽微 |
| 5 | `lsp-config.test.mjs` の `COPIED` 配列（L23-28）に `rust-toolchain.toml` を足す必要があるが、計画のファイル一覧に明記が無い。忘れると故障注入が「ファイル不在」の枝を検査してしまう | **要対処** |
| 6 | 故障注入2本の変異の強さは実際に起きうる回帰と同型で、構文エラー等の過剰な変異にはなっていない | 軽微 |
| 7 | 検査の射程宣言（宣言だけを見る）はコード側コメントで書く指示があり妥当。`docs/hooks.md` 側にも一文あるとより一貫する | 軽微 |
| 8 | `rust-analyzer` の生きた層 grep で、`deps-update/SKILL.md` 以外に手順・理由の写しは見つからず | 軽微（網羅済み） |
| 9 | `rust-toolchain.toml` ヘッダ書き換えで偽になる散文（`ci.yml`・`docs/build-commands.md`）は無い | 軽微 |
| 10 | 手順撤去後も真であるべき注意（再起動・索引の温まり）は計画に保持されている | 軽微 |
| ⚠️11 | `.claude/hooks/post-edit.test.mjs:164-168` に既存の `rust-analyzer.toml` 用 `selectChecks` 単体テストがある（`docs/hooks.md` 照合とは別に、この直接呼び出しテストのファイルにも先例がある）。計画は `lsp-config.test.mjs` の故障注入は挙げるが、`post-edit.test.mjs` 側に `rust-toolchain.toml` の `selectChecks` 単体テストを足す指示が無い。`governance:check`（G-hook-fires）が表との整合は担保するので機能的な抜けではないが、既存の慣習（rust-analyzer.toml に倣った直接単体テスト）との一貫性という観点では確信が持てない | ⚠️軽微〜要対処（判断が割れうる） |

## 結論

「要対処」は2点（#2 の行番号ミス、#5 の COPIED 配列追加漏れ）。どちらも実装前に計画側を修正すれば解消する軽微な訂正で、設計方針自体（Phase 3 の宣言検査・射程限定・故障注入の考え方）は既存の hook 設計パターンと整合している。観点2（撤去と写し）側に実質的な懸念は見つからなかった。
