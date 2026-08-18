# research — #1133 SPEC §19.5「instant モード中はアイコン取得をスキップ」が egui 経路に実装を持たない

## issue の要約

`SPEC.md:1036`（§19.5）は「インスタントコマンドモード中はアイコン取得をスキップする（`path` がファイルパスではないため）」と宣言しているが、egui 経路にこれを実装する機構は無い。WebView2 経路にあった実装（`ResultsSection.tsx` の `skipIcons` prop）は #532 SU7（`15933afa`）で削除され、SPEC の当該行だけが残った。

**issue が明示的に未決としていた「どちらへ倒すか」は、本サイクルの冒頭でユーザーが決めた**（下記「決定」）。

## 決定（2026-08-18・ユーザー回答）

**案 C（行種別で決める）を採る。仕様変更として扱う**（`SPEC.md` → コード → ドキュメントの順）。

- 結果行のアイコン抽出の対象は「その行が指す実在ファイル」であり、抽出キーは行ごとに定まる
- instant 行: **exec 種別は exe のアイコンを出す**（args の有無・description の有無に依らない）／**URL 種別・Legacy 種別は抽出しない**
- 却下: 案 A（instant 全行スキップ＝SPEC の字面に合わせる）、案 B（as-built 追認＝文言だけ直す）

~~案 C を採ると、issue の ⚠未確認 3（instant 行の表示文字列が `icons.bin` のキーとして永続化される）は**キーが実 exe パスになることで自然に消える**。~~

**取り消し（3b の指摘を受けて一次証拠で裁定・下記 F10）。** ⚠未確認 3 は**今日も起きていない**ので、消える対象が最初から無い。

## 事実（一次証拠つき）

### F1. アイコン抽出のキーは `SearchResult.path` である（3 か所すべて）

| 用途 | 位置 | 現在の式 |
|---|---|---|
| 抽出要求 | `src-tauri/src/egui_shell/results_view.rs:179-186` | `needs_extraction(&r.path, ...)` / `wanted.push(r.path.clone())` |
| テクスチャ引き | `src-tauri/src/egui_shell/results_view.rs:485` | `icons.get(&result.path)` |
| 可視集合での剪定 | `src-tauri/src/egui_shell/results_view.rs:649` | `snapshot.rows.iter().map(|r| r.path.clone())` |

キーを行ごとに変えるなら**この 3 か所すべてを同じ導出へ寄せる必要がある**（1 つでも `path` のままだと、抽出したテクスチャを引けない／可視集合から漏れて毎世代 drop される）。

### F2. instant 行の `path` は「description 非空ならそれ、空なら display_text」である

`snotra-core/src/instant.rs:347-356`（`matching_results`）。issue の事実 3 は「exec 種別・args 空・exe が実在フルパス」と書いていたが、**`description` が空であることも合流条件に要る**（issue に無い条件）。`display_text`（同 `:322-334`）は `Url{url}` → url、`Exec{exe,args}` → args 空なら exe・非空なら `"exe args"`、`Legacy{command}` → command を返す。

### F3. `exe` は起動時に env 展開される

`src-tauri/src/commands/launch.rs:260`（`launch_exec_core`）が `expand_env(exe)` を通す。`expand_env` は `src-tauri/src/commands/launch.rs:226` にあり `pub(crate)`（snotra-core からは見えない）。ゆえに**アイコンキーも同じ展開を通さないと、`%LOCALAPPDATA%\...` 形の exe は起動できるのにアイコンだけ出ない**という食い違いが残る。

### F4. 失敗は latch されない（URL 行が再試行を反復する機序）

`SHGetFileInfoW` の 0 復帰は `IconFailure::ShellQueryFailed` で、`is_transient() == true`（`src-tauri/src/icon.rs:141`）。`needs_extraction`（`src-tauri/src/egui_shell/icon_textures.rs:56-62`）は `attempts < ICON_MAX_ATTEMPTS`（= 3・同 `:44`）まで再試行する。attempts は世代交代フレームで可視集合に刈られる（`results_view.rs:650-652`）ので、**別の文字を打って行集合が入れ替わるたびに 0 から積み直す**（issue の ⚠未確認 1・未実測）。

### F5. instant では perf ゲート（`input_idle`）が構造的に常時真になる

`view.rs:1159` が `input_idle = !self.controller.is_search_armed()` を snapshot へ載せ、`results_view.rs:660` の `if snapshot.input_idle` が抽出要求を包む。instant 枝は毎打鍵 `search_debounce.cancel()` を撃つ（`launcher_controller.rs:1397` 付近・§19.5「毎打鍵同期」）ため armed にならない。

**この `input_idle` には触らない。** doc（`results_view.rs:32-37`）が「perf ヒューリスティックであって正しさの述語ではない」「`is_unsettled` と同じ修正を当ててはならない」と明記しており、#1074 で意味が固定されている。案 C は**キー側**でスキップを表現するので、このゲートを迂回・改変せずに無駄仕事が消える。

### F6. 行は `SearchState.results` に一元的に置かれ、多くの経路が総入れ替えする

`set_results` / `put_rows` / `enter_tool`（`search_state.rs:422-451`）/ folder drain / search worker。**行と並ぶ別ベクタ（`Vec<Option<String>>`）でキーを運ぶ設計は、これら全経路で長さと順序を手で同期させる規約を生む**——`SearchResult` 自身へ持たせれば構造的に消える（ルート `CLAUDE.md`「表現不能にする」／memory `prefer-structural-over-documented-contract`）。

### F7. `SearchResult` は永続形式にも IPC にも入らない

`snotra-core/src/ui_types.rs:1-21` の `//!` が #836 の実測として「シリアライズする呼び出し点はリポジトリに 1 つも無く、永続形式にも入らない」と記録している（serde 派生は SU7 の残滓）。**フィールド追加は on-disk 互換の論点を持たない**（`/persistence-check` の対象外）。

構築点は `snotra-core`（`folder.rs` / `instant.rs` / `search.rs` / `search/scoring.rs`）と `src-tauri`（`search_state.rs` / `results_view.rs` / `platform/tray.rs`）に分布し、テストも含む。**件数は書かない**——`grep -c "SearchResult {"` の生値は struct 定義行や関数シグネチャを含み、数える者によって 19 / 22 / 23 と食い違った（3b と plan-review で実際に食い違った）。**尽きたことを決めるのはコンパイラ（`-D warnings` 下の E0063）であって、人の数え上げではない。**

### F8. tool 行の `path` は `exe` である（既に「path をファイルとして扱う」ことが機能している）

`search_state.rs:428-436`。SPEC §18.4（`SPEC.md:812-817`）はアイコンについて**一言も述べていない**。案 C の規則（抽出キーは行ごと・既定は `path`）は tool 行の現在の挙動を変えない。

### F9. instant 行の起動は `sel.name` で config を引き直す

`launcher_controller.rs:502-510`（`execute_instant_selected`）。行 → コマンドの対応は**名前**であり、`ResultsView` は config を読めない。ゆえに**アイコンキーは行の生成時（`matching_results`）に確定させて行と一緒に運ぶ**しかない。

### F10. 抽出に失敗した文字列は `icons.bin` のキーにならない（issue の ⚠未確認 3 は成立しない）

`src-tauri/src/commands/icon.rs:104-109`（Step 3）は `Ok(png) => c.insert(p, png)` / `Err(reason) => { failures.insert(p, reason); }` と書いており、**成功時だけ**キャッシュへ入る。ゆえに `SHGetFileInfoW` が失敗する文字列（URL・description・`"exe args"`）は**キーとして永続化されない**。

表示文字列がキーとして残る条件は「その文字列が実在ファイルを指すこと」であり、それは exec 種別・args 空・description 空の場合に `path == exe` となる経路だけである——**そのときキーは正当なファイルパスである**。issue の ⚠未確認 3 は as-built で成立しない。

（3b の指摘を受けて一次証拠で裁定。**採ったのは所見であって、添えられた「案 C の Explicit は改善方向」という説明ではない**——改善すべき害がそもそも無い。）

### F11. exec 種別の instant コマンドは、実 config にも既定 config にも 0 件である

- 既定: `snotra-core/src/config.rs:621-636` の `instant_commands` は 2 件で**両方 `InstantAction::Url`**（`g` / `gh`）
- 実 config（`%APPDATA%\Snotra\config.toml`・3b が読み取り専用で確認）: 2 件で**両方 url 型**

ゆえに**案 C の 2 枚看板は実運用点への届き方が非対称である**。

- 「URL / Legacy はスキップ」は**実 config と既定の両方に直接効く**（現状、`https://github.com/search?q={query}` や `Google 検索` という文字列に対して `SHGetFileInfoW` が呼ばれ続けている）
- 「exec は本物のアイコン」は**exec 型を手で設定した利用者にしか届かない**（現状は 0 人）

**設計判断そのものは覆らない**（仕様はこの利用者の config ではなく製品の姿を定める）が、**検証労力を exec 側へ厚く配分してはならない**という配分の情報になる。

## 関連ファイル・シンボル

| ファイル | シンボル | 役割 |
|---|---|---|
| `snotra-core/src/ui_types.rs` | `SearchResult` | 行の型。アイコンキーの置き場（案 C） |
| `snotra-core/src/instant.rs` | `matching_results` / `display_text` | instant 行の組み立て。キーの決定点 |
| `snotra-core/src/config.rs:75-97` | `InstantCommand` / `InstantAction` | 種別（Url / Exec / Legacy）の出所 |
| `src-tauri/src/egui_shell/results_view.rs` | `request_icons_for_results` / `results_list_ui` / `draw_result_row` / `update` の剪定 | F1 の 3 か所 |
| `src-tauri/src/egui_shell/icon_textures.rs` | `needs_extraction` / `retain_visible` / `ICON_MAX_ATTEMPTS` | 再試行と剪定 |
| `src-tauri/src/egui_shell/launcher_controller.rs:928` | `matching_results` 呼び出し | env 展開関数を渡す点（F3） |
| `src-tauri/src/commands/launch.rs:226,260` | `expand_env` / `launch_exec_core` | 起動側の展開（キーを合わせる相手） |
| `SPEC.md:83-98` | §3.4 アイコン | 抽出規則の正本を置く先 |
| `SPEC.md:1036` | §19.5 | 書き換える当該行 |

## 再利用できる既存パターン

- **純関数へ env 展開を注入する**: `expand_exec_args(args, query, clipboard, env_expand: impl Fn(&str)->String)`（`snotra-core/src/instant.rs:296-308`）。`matching_results` も同じ形で `env_expand` を受け取れば snotra-core の純粋性とテスト可能性を保てる
- **列挙で「表現不能にする」**: `IconOutcome` / `IconFailure`（`src-tauri/src/icon.rs:110-146`）が既に「取れなかった」を潰さない形を採っている
- **snapshot のフィールド追加は compile-fail で守られる**: `RowsSnapshot::matches` 冒頭の分解束縛（`results_view.rs:53-58`）と doc「フィールドを増やしたらここも増やす」

## 技術的制約

1. **`ResultsView` は config を読めない**（別窓・`RowsSnapshot` が唯一の入力）。F9 の帰結
2. **`input_idle` の意味論を変えてはならない**（F5）
3. **`SearchResult` は snotra-core にあり、egui/tauri へ依存できない**。ゆえにキーは「文字列 or 無し」の素朴な形に留める（`expand_env` は呼び出し側から注入）
4. **平文検索行のキー導出に追加確保を持ち込まない**——`snapshot.rows.to_vec()` は既に全行の `String` を確保しており（`view.rs:1168`）、既定 200 行・設定次第 1000 行。既定を `Some(path.clone())` で表す設計は**確保を倍にする**ので採らない（既定は「`path` を使う」variant で表す）
5. `governance:check` の対象（`SPEC.md` は `*.md`）——PR CI の `governance-check` job が事後に見る

## 設計候補（plan で確定させる）

- **`SearchResult` に `icon: IconSource` を足す**（採用見込み）
  ```rust
  pub enum IconSource { FromPath, Skip, Explicit(String) }  // 既定 FromPath
  ```
  - `FromPath`: 既存の全行（検索・フォルダ・tool・履歴・トレイ）。追加確保なし
  - `Skip`: instant の Url / Legacy 種別
  - `Explicit(k)`: instant の Exec 種別（`k = env_expand(exe)`）
- F1 の 3 か所は 1 つの導出（`fn icon_key(r: &SearchResult) -> Option<&str>`）へ寄せる

## 3b 敵対的調査の結果（`workspace/adversarial-1133.txt`・sonnet 1 体）

**壊せた項目: 0。** 渡した 5 命題（F1 の 3 か所・F7 の compile-fail・Q2 の呼び出し元・F5 の結論・F3 の前提）はいずれも反証されず、一次証拠で追認された。

**壊せなかった項目（＝確認できた項目）と、その根拠**

| 命題 | 判定 | 追認の根拠（枠が独立に採ったもの） |
|---|---|---|
| F1: キーを読むのは 3 か所 | 確認 | `egui_shell/` 全体 + `icon.rs` + `commands/icon.rs` を横断。drain（`results_view.rs:580-589`）と `load_icon_pngs` は**下流の消費点であって導出点ではない**と分類。`platform/tray.rs` の `path` はアイコン抽出に使わない（トレイアイコンは自プロセス exe 由来）。`snotra-settings` / `snotra-egui-runtime` は `SearchResult` を参照しない（0 件） |
| F7: 23 構築点が compile-fail | 確認 | 19 件のリテラル構築を個別に読み、`..` / `..Default::default()` / deserialize 経由が**1 件も無い**ことを確認。`SearchResult` は `Default` を derive していないので言語的にすり抜け経路が無い |
| Q2: crate 外の呼び出し元は 1 か所 | 確認 | `snotra-settings` を含めて 0 件 |
| F5: キー側だけで無駄が消える | 確認（⚠ 1 件つき） | `wanted` が空なら `spawn_icon_load` は呼ばれない（`results_view.rs:188-190`）。剪定も同じキー概念に寄せれば取り残しを作らない |
| F3: `expand_env` の意味論 | 確認（実測で補強） | 未定義変数は字面のまま素通り・孤立した `%` もそのまま・panic も空文字列も返さない（`[Environment]::ExpandEnvironmentVariables` で 3 ケース実測） |

**採った所見（採否と理由）**

1. **F10（⚠未確認 3 は成立しない）— 採用。** ただし**機序の説明は採らない**。枠は「案 C の `Explicit(exe)` は `"exe args"` キーを exe 単体へ正規化するので改善方向」と添えたが、`"exe args"` は抽出に失敗するのでそもそもキーにならない（F10 を一次証拠で自分で裁定した結果）。**改善すべき害が無い**
2. **F11（exec 型 0 件）— 採用。** 実 config と既定 config の両方を読んだ枠だけが到達できた事実で、**計画とコードだけを見ていては原理的に出ない**（`.claude/skills/start-issue` が「測定環境そのものを疑え」を必須にしている理由がそのまま再現した）
3. **⚠ 毎打鍵の `expand_env` コスト（未測定）— 採用（受容として記録）。** instant 枝は毎打鍵 `matching_results` を呼ぶ（`launcher_controller.rs:910` のコメント「毎打鍵同期」）ので、exec 型コマンド 1 件につき `ExpandEnvironmentStringsW` が 1 回増える。**額は測っていない**——「速くなる」も「無視できる」も書かない。同じ関数が毎打鍵で全マッチ行ぶんの `String` を確保している既存の桁と比べる、という**下限の言い方**に留める
4. **⚠ `IconSource` の derive 不足 — 採用。** `SearchResult` は `Debug, Clone, PartialEq, Eq, Serialize, Deserialize` を derive している（`ui_types.rs:14`）ので、`IconSource` が同じ集合を持たないと derive が壊れる。計画のスケッチを修正した

**枠が見ていない項目（枠自身の宣言）**: `cargo check` による compile-fail の実測（E0063 が 19 点で出ること）、`icons.bin` の実機での変化、F4 / F6 / F8 の敵対的検証。**前者 2 つは実装フェーズで自然に測る**（構築点の移行はコンパイラが数える・`icons.bin` は U5 の判断どおり実機確認を行わない）。

## 未解決の疑問（plan の未確定欄へ持ち越す）

- Q1. `IconSource` の名前と variant 名、置き場（`ui_types.rs` か新モジュールか）
- Q2. `matching_results` のシグネチャ変更（`env_expand` 引数の追加）が snotra-settings 側の呼び出し元を壊さないか（`crate` 外へ出しているのは `matching_results` のみ・要 findReferences）
- Q3. 検証の形——issue が設計した実機 2 アームプローブは**利用者の同意が要る**（`tauri_plugin_single_instance` が利用者の実インスタンスをトグルする）。単体テストで決定的に測れる範囲がどこまでかを先に確定させる
- Q4. ADR を起こすか（案 A・B を却下した否定の知識が生じている）
- Q5. `SPEC.md` の正本の置き場——§3.4 に抽出キーの規則を書き、§19.5 はそこを参照する形（写しを作らない）でよいか
