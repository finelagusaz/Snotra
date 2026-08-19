# 調査 — #1134 列挙失敗行に、前の世代のフォルダアイコンが残って描かれる

## 1. issue の要約

フォルダ列挙に失敗した行（`is_error: true`）に、**本物のフォルダアイコンが描かれることがある**。

`SPEC.md`「3.4 アイコン」は「列挙失敗行（§6.6）はこの規則の対象外である。`path` が実在ディレクトリを指していても抽出せず、placeholder も描かない」と既に定めている。**ゆえにこれは仕様変更ではなくバグである**（`AGENTS.md`「開発ワークフロー」1 の判定: SPEC に記述があり、それに**合わせる**側）。

ユーザーの補足（issue 起票時の文言）:「フォルダ列挙に失敗したら、本物のフォルダアイコンは出さないのがいいと思う。フォルダ展開でフォルダの実態があったりアクセス許可があればアイコン取得、そうでないならアイコン出さないという動きになるとよさそう」——SPEC §3.4 の既存記述と一致する。**要求判断の確認は不要**と判断した（決めるべきは機構だけ）。

## 2. 経路（一次証拠つき・机上）

| # | 事象 | 一次証拠 |
|---|---|---|
| 1 | 実在フォルダ `P` の行が通常結果に出て、アイコンが抽出される | `icon_textures.rs:96` `wanted_icon_keys`（`is_error` でないので載る） |
| 2 | その行で**右カーソル**を押して展開する | `launcher_controller.rs:1301` `sel.is_folder && !sel.is_error`、`dir = sel.path.clone()` |
| 3 | 展開では行の世代が進まない（`icon_textures[P]` は生き残る） | `search_state.rs` `enter_folder` は `results` に触れない。テスト `rows_generation_is_stable_on_enter_folder` が固定 |
| 4 | `read_dir` が失敗（権限不足等・**ディレクトリ自体は実在**）してエラー行が作られる | `snotra-core/src/folder.rs:33` → `folder.rs:214` `error_result`。`path = P` / `is_error = true` / **`icon: IconSource::FromPath`**（`folder.rs:220`） |
| 5 | `folder_error` → `run_search_with` → `set_results(err.clone())` で世代が進む | `launcher_controller.rs:869-871`。**`set_results` は行を丸ごと差し替える**ので、この世代の行はエラー行 1 行だけである（`SPEC.md`「6.6 列挙失敗時」の「単一のエラー行」と一致） |
| 6 | 保持側が `icon_textures[P]` を残す | `icon_textures.rs:123` `visible_icon_keys` は `is_error` を見ない → `retain_visible` が `{P}` を残す |
| 7 | 描画側が引いて描く | `icon_textures.rs:132` `icon_for_row` が `Some(tex)` → `results_view.rs:353` の `Some(tex)` 枝に `is_error` ガードは無い |

**実機では再現していない**（机上で経路を辿っただけ・issue 本文の自認と一致）。権限不足のディレクトリを検索結果に出してから右カーソルで展開する手順が要る。**受容する残余**として扱い、接地は単体の連鎖テスト（下記 §6）で行う。

## 3. 関連ファイル・シンボル（実在を grep で確認済み）

| ファイル | シンボル | 役割 |
|---|---|---|
| `snotra-core/src/ui_types.rs:52` | `SearchResult::icon_key` | **キーの唯一の導出点**（#1133）。`IconSource` だけを見る |
| `snotra-core/src/ui_types.rs:36` | `IconSource`（`FromPath` / `Skip` / `Explicit`） | 行ごとのキーの出所（#1135 で導入） |
| `snotra-core/src/folder.rs:214` | `error_result` | エラー行の**唯一の生成点**（下記 §5-1） |
| `src-tauri/src/egui_shell/icon_textures.rs:96` | `wanted_icon_keys` | 要求側。`if r.is_error { continue }` を持つ |
| `src-tauri/src/egui_shell/icon_textures.rs:123` | `visible_icon_keys` | 保持側。`is_error` を**意図的に**見ない（#1133） |
| `src-tauri/src/egui_shell/icon_textures.rs:132` | `icon_for_row` | 引き側。`is_error` を見ない |
| `src-tauri/src/egui_shell/icon_textures.rs:68` | `retain_visible` | 可視集合に無いキーを drop |
| `src-tauri/src/egui_shell/results_view.rs:350-370` | `draw_result_row` のアイコン枝 | `Some(tex)`（:353）は無条件描画、`None if !result.is_error`（:368）で placeholder を抑止 |
| `src-tauri/src/egui_shell/launcher_controller.rs:869` | `run_search_with` の `ViewKind::Folder` 枝 | `folder_error` → `set_results` |

### `icon_key` の呼び出し元の母集団

**LSP（rust-analyzer）は本セッションで応答しなかった**（`findReferences` / `hover` とも "not fully indexed"）。`.claude/rules` の既定は LSP だが、**無い環境では grep へ落とす**規定に従い grep で列挙した。`icon_key` は `SearchResult` の inherent method なので、`.icon_key` の文字列一致が呼び出し構文をすべて覆う。

- 製品コード: `icon_textures.rs:107`（`wanted_icon_keys`）、`:125`（`visible_icon_keys`）、`:136`（`icon_for_row`）の **3 箇所のみ**
- テスト: `snotra-core/src/instant.rs:1112,1136,1148,1157,1168`
- **`icon_textures` の HashMap を `icon_for_row` を通さずに引く箇所は無い**（`icons.get(` の grep は `icon_for_row` の中身 1 件のみ）

### `is_error: true` の生成点の母集団

`is_error:`（値を問わない形）で全リポジトリを走査した結果、**製品コードで `true` を入れるのは `folder.rs:219`（`error_result`）だけ**である。他はすべて `false` のリテラルで、変数値を入れる箇所は 1 つも無い（`grep -rn "is_error:"` の全件を分類済み。`snotra-settings` の `message_is_error` は別型の別フィールド）。`SearchResult` は `Default` を導出しておらず、serde 経由の構築点も無い（`ui_types.rs` の `//!` が #836 で「シリアライズする呼び出し点は 1 つも無い」と実測記録）。

## 4. 再利用できる既存パターン

- **キーの単一導出**（#1133）: `icon_textures.rs:74` のブロックコメントが「キーを導くのは `SearchResult::icon_key` ただ 1 つである」と宣言している。要求・保持・引きが同じ導出を通ることが「抽出したものを引ける」の根拠。
- **表現不能にする構造**（`docs/development-principles.md`・ルート `CLAUDE.md` の設計選好）: 文書契約より、誤った状態を作れなくする構造を好む。
- 既存テスト `wanted_icon_keys_never_loads_icons_for_error_rows`（`icon_textures.rs:259`）は**要求側だけ**を測る。issue 本文が指摘するとおり、これは連鎖の半分である。

## 5. 技術的制約

1. **エラー行の生成点は 1 つだが、それは規約であって構造ではない**——`error_result` が `IconSource::Skip` を入れる案（下記 D）は、将来別の生成点が `FromPath` で書けば破れる。
2. **`visible_icon_keys` を `is_error` で狭める案は #1133 が明示的に却下している**（`icon_textures.rs:120` の doc: 「狭いと、抽出した直後の世代交代でテクスチャを落として積み直す往復になる」）。この記録を覆す変更をするなら、**理由と費用を測って上書きする**必要がある。
3. **エラー行の世代は 1 行だけである**（§2 の #5 で一次証拠を確認）。ゆえに「エラー行がキーを出さない」ことで落ちるテクスチャは**高々 1 件**であり、往復は「列挙に失敗したフォルダから Escape で戻ったときの再抽出 1 回」に限られる。制約 2 の懸念（通常行の往復）はこの経路には掛からない。
4. **描画側の `None if !result.is_error` ガードは別の事実である**（placeholder を描かない）。SPEC §3.4 が「抽出せず、placeholder も描かない」と 2 つを並べており、片方を消してはならない。
5. `wanted_icon_keys` の doc に「**この issue が閉じたらこの段落も更新すること**」が明記されている（`icon_textures.rs:87-95`）。撤去対象の散文が既に名指しされている。
6. `.rs` の doc コメントが `SPEC.md` の見出しを参照するため、`npm run governance:check`（カテゴリ F）の対象になる。

## 6. 設計の選択肢と採否

| 案 | 内容 | 採否 |
|---|---|---|
| **A** | `draw_result_row` の `Some(tex)` 枝に `is_error` ガードを 1 行足す | **却下**。テクスチャは引けたまま残り、不変条件が要求側・描画側の 2 か所に分かれる。何より**検知器が描画層（kittest）にしか置けない**——issue が要求する「`icon_textures` に既にキーがある状態からの連鎖」を単体で測れない |
| **B** | `icon_for_row` で `is_error` を見る | **却下**。単体で測れる利点は C と同じだが、不変条件が要求側と引き側の 2 か所に残り、保持側は無駄なテクスチャを持ち続ける |
| **C** | `SearchResult::icon_key` が `is_error` のとき `None` を返す | **採用**。3 つの消費者（要求・保持・引き）すべてを**1 つの導出**で覆う。#1133 が宣言した「キーを導くのは `icon_key` ただ 1 つ」と同じ向きであり、エラー行に本物のアイコンを引く状態が**表現不能**になる |
| **D** | `error_result` が `IconSource::Skip` を入れる | **却下（C の補完としても採らない）**。C があれば同じ事実の 2 つ目の表現になり、単一導出の原則に反する。単独では制約 1 のとおり将来の生成点で破れる |

### C を採ることの費用（測って書く）

- **保持側が実質的に狭まる**。ただし `visible_icon_keys` に `is_error` の条件を足すのではなく、**エラー行がキーを持たなくなる**結果として狭まる。#1133 が守ろうとした「通常行の往復」は一切起きない（制約 3）。
- 具体的な費用は「列挙に失敗したフォルダの行のテクスチャ 1 件が落ち、Escape で戻ったときに 1 回だけ再抽出される」。**その 1 件以外は、成功時と同じく世代交代でどのみち落ちる**（フォルダ展開が成功した場合も展開前の行のテクスチャは全部落ちる）。
- 意味論の変化: `icon_key` が `IconSource` だけでなく `is_error` も見るようになる。issue 本文が `icon_for_row` 案について述べた「『キーの導出』と『行の性質』を 1 か所に混ぜる」はここにも当たる。**ただし `icon_key` の意味は「この行のアイコンをどこから取るか」であり、「取らない」は既に `Skip` として表現されている**——エラー行が `Skip` 相当になるのは同じ語彙の中の話である。
- `wanted_icon_keys` の `if r.is_error { continue }` が**冗長になる**。同じ事実の 2 つ目の表現を残さないため**撤去する**（既存テスト `wanted_icon_keys_never_loads_icons_for_error_rows` は `icon_key` 経由で通り続け、**導出が要求側まで届いていることの検知器**へ意味が変わる）。

## 7. 新しく生きる分岐（`AGENTS.md`「分岐を決める値の出所を変更」トリガー）

`icon_key` の返り値がエラー行で `Some → None` へ変わることで、**1 行も変えていないのに初めて走る行**:

1. `wanted_icon_keys` の `let Some(key) = r.icon_key() else { continue }`（従来エラー行はその手前の `is_error` で落ちていた。撤去後はここが受ける）
2. `visible_icon_keys` の `filter_map` がエラー行を落とす → `retain_visible` が `icon_textures[P]` を **drop する**（従来は残していた）
3. `icon_for_row` の `row.icon_key()?` が `None` で早期 return → `draw_result_row` の `None if !result.is_error` 枝（placeholder を抑止する既存の枝）がエラー行で**必ず**通る

## 8. 未解決の疑問（計画の「未確定」へ引き継ぐ）

- `icon_key` の doc と `wanted_icon_keys` / `visible_icon_keys` の doc をどう書き換えるか（撤去対象の段落は制約 5 で名指し済み）。
- 連鎖テストの置き場所（`icon_textures.rs` の `mod tests`）と、変異注入（`icon_key` の `is_error` フォールドを戻す）で赤になることの実測。
- `SPEC.md` の更新要否——§3.4 は既に意図を書いており**変更不要**の見込み。ただし「抽出のキーは結果行ごとに定まる」の直下に列挙失敗行の例外が箇条書きで在るので、機構が `icon_key` へ移ったことを**書き足すかどうか**は判断が要る。

## 9. 敵対的調査（Step 3b・`workspace/adversarial-1134.txt`）の所見と採否

`general-purpose` / `sonnet` を 1 体、命題 8 件と「測定環境そのものを疑う」指示つきで起動した。

### 壊せた項目

**0 件。** 命題 1〜8 のいずれも一次証拠では偽にできなかったと報告された。

### 壊せなかった項目（相手が当たった手段つき）

| 命題 | 相手が当たった手段 |
|---|---|
| 1 エラー行の世代は 1 行 | `set_results` → `put_rows`（`self.results = rows` の 1 箇所）が唯一の書き換え点であることまで辿った。`view.rs` の snapshot 経路にマージ層が無いことも確認 |
| 2 `icon_key` の呼び出し元は 3 箇所 | `.icon_key(` に加え**ドット無しの裸参照・re-export 経由**も別途 grep |
| 3 `is_error: true` は `error_result` のみ | `is_error` の全ヒットを目視分類、shorthand・struct update 経由も grep |
| 4 `enter_folder` は世代を進めない | 本体を読み、テスト `rows_generation_is_stable_on_enter_folder` の**内容**まで照合 |
| 5 SPEC 変更不要 | §3.4 / §6.5 / §6.6 / §4.7 / §19.5 を実読 |
| 6 `Some(tex)` 枝にガード無し | 該当行を読み、`None if !result.is_error` が別分岐であることを確認 |
| 7 C で表現不能になる | `icon_textures` の HashMap を読む経路が `icon_for_row` 1 箇所、挿入も drain 1 箇所でキーの再導出が無いことを確認（**未実装ゆえ演繹**・⚠️） |
| 8 `wanted_icon_keys` の `continue` は撤去可 | 同上（**未実装・未実測**・⚠️） |

### ⚠️（相手が確信を持てないと宣言した所見）

1. **行番号のズレ 3 件**（`retain_visible` 69→68、`icon_for_row` 135→132、描画のアイコン枝 361-370→350 台）。
2. 命題 7 / 8 は C 実装後の演繹であり `cargo test` 未実行。
3. §6 の費用見積もり（Escape での再抽出 1 回）は独立に再トレースしたが実機実測ではない。

### 採否

- **⚠️-1（行番号）: 採用。自分で測って裁定した**（`grep -n` で定義行を直接測定）——`retain_visible` は 68、`icon_for_row` は 132、アイコン枝は 350-370（`Some(tex)` は 353、`None if !result.is_error` は 368）が正しい。本文を訂正済み。相手の指摘は正しかったが、**相手の数値をそのまま写さず自分で測った**（相手は「351-369」と報告したが実測は 350-370）。
- **⚠️-2: 採用。** 命題 7 / 8 が実装後にしか測れないことは正しい。**そのための検知器と変異注入を計画の Phase 1 / Phase 4 に置く**——演繹のままにしない。
- **⚠️-3: 受容する残余として明示。** 実機再現は issue 本文も行っておらず、接地は連鎖の単体検知器で行う（§2 末尾と同じ判断）。
- **「壊せた項目 0 件」そのものは、正しさの証拠として扱わない。** 相手は `cargo` を走らせておらず（本人が申告）、命題 7 / 8 は原理的に実装後にしか測れない。**この調査で確定したのは命題 1〜6（現在のコードに対する事実）だけ**であり、7 / 8 は計画の検証項目として残る。

### 自分で追加した裁定（相手に渡していない命題）

- `commands/icon.rs` の `load_icon_pngs`（:49-69）は `IconCache` を先に引くため、**C の代償である再抽出はキャッシュヒットであって `SHGetFileInfoW` の呼び直しではない**。C の費用はさらに下がる（worker 往復 1 回と PNG decode 1 回）。
