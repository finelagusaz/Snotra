# plan — #900 `ctx.set_visuals` の禁止を clippy の disallowed-methods で機構化する

調査は `workspace/research.md`。レビューは 4 枠を独立に走らせた（→「マルチパースペクティブレビュー結果」）。
ここには実装者が追加判断せず実行できる形だけを置く。

## 目的

#751 が新設した「`Context` 経由で global style を書いてはならない」という規範のうち、
**名前つき API の直呼びだけ**を `src-tauri` 限定の `clippy.toml` で機構へ吸収する。
**順序の不変条件（適用は visuals を読む最初の操作より前）は本 issue では機構化しない**——
`src-tauri/CLAUDE.md` の「この順序に検知手段は無い」は**書き換えない**（issue 明記。
これを消せる設計案は別 issue へ送った）。

## 受け入れ条件

1. `src-tauri` で禁止対象 7 メソッドのいずれかを呼ぶと `cargo clippy --workspace --all-targets
   -- -D warnings` が exit 101 で落ちる。**7 件を個別に測る**（書き損じた行は黙って死ぬため）。
2. `cargo clippy -p snotra-settings --all-targets -- -D warnings` は exit 0 のまま
   （同 crate の `set_visuals` / `all_styles_mut` の正当な使用 2 件を巻き込まない）。
3. 最終形の作業ツリーで `cargo clippy --workspace --all-targets -- -D warnings` が exit 0。
4. `src-tauri/CLAUDE.md` が **規範の広さと機構の広さの差**を明示している——
   規範は `Context` 経由の global style 書き込み全般に及び、**機構が守るのは列挙した 7 メソッドに限る**
   （`options_mut` / `memory_mut` からの直書きは通る）。既存の「この順序に検知手段は無い」は残っている。
5. **`src-tauri/CLAUDE.md` の当該文が `` `src-tauri/clippy.toml` `` というルート相対形で、
   コードフェンスの外に在る。** これは体裁ではなく機構である——この形のときだけ `G-references`
   （`REF_EXTENSIONS` に `.toml` を含む）がファイルの実在を検査し、`git rm clippy.toml` 単独の
   コミットが CI で赤くなる。**`` `clippy.toml` `` と縮めると保護が黙って消える。**
6. `npm run governance:check` が全検査 passed。

## 禁止集合 — 7 メソッド（確定）

選別の規則は **「`Options` の style へ着地する、global style 書き込み専用の名前つき API か」**。

| # | メソッド | 着地 |
|---|---|---|
| 1 | `egui::Context::set_visuals` | → `style_mut_of` |
| 2 | `egui::Context::set_visuals_of` | → `style_mut_of` |
| 3 | `egui::Context::style_mut_of` | `options_mut` → `Arc::make_mut(dark/light_style)` |
| 4 | `egui::Context::set_style_of` | `options_mut` → `dark/light_style = style` |
| 5 | `egui::Context::global_style_mut` | `options_mut` → `Arc::make_mut(opt.style_mut())` |
| 6 | `egui::Context::set_global_style` | `options_mut` → `*opt.style_mut() = style` |
| 7 | `egui::Context::all_styles_mut` | `options_mut` → dark/light 両方 |

7 件は doc が目的を明示する同族で（"Mutate the currently active `Style` …"）、**説明を要しない最小の閉包**である。
`src-tauri` での現在の使用は全件 0（コメント内の言及のみ）。

**含めないもの**（理由は `clippy.toml` のコメントが正本）:

- **`style_ui` / `settings_ui`** — 内部で `set_style_of` / `options_mut` を呼ぶが、これらは
  **egui 組み込みのスタイル編集パネルを描くウィジェット API** である（doc: "Edit the `Style`" /
  "Show a ui for settings"）。呼ぶ人は inspector を出したいのであって #751 の誤りを犯していない。
  禁止すれば**偽陽性**になり、解消手段は `#[allow(clippy::disallowed_methods)]` しか無い——
  **機構を完全に無効化する逃げ道を、正当な理由で打鍵させる訓練になる。**
  失われる守りはほぼゼロ（config の色は inspector から来ないので #751 は再現しない）。
- **`set_debug_on_hover`** — クロージャが egui 内部で `style.debug.debug_on_hover` に固定されており
  visuals を書けない。`#[cfg(debug_assertions)]` 付きゆえプロファイル依存の沈黙経路にもなる。
- **`options_mut` / `memory_mut`** — 汎用アクセサ。**どちらも `Options` の `dark_style` / `light_style`
  （`pub`）へ直接書けるので、この機構の穴である**（`memory_mut` は `src-tauri` に focus 操作の
  実使用が 4 件あり禁止できない）。受容残余。
- **`set_theme`** — 書くのは `theme_preference` であって style の中身ではない。src-tauri は使わない。
- **`set_fonts` / `add_font`** — style ではなくフォント定義。`font_stack.rs:192` に正当な実使用がある。

## 変更ファイル一覧（2 ファイル）

| ファイル | 変更 |
|---|---|
| `src-tauri/clippy.toml` | **新規作成**。`disallowed-methods`（7 エントリ）＋ 否定の知識と残余のコメント |
| `src-tauri/CLAUDE.md` | 「テーマ色・font・行高の読みは 1 フレーム 1 回（#673 spec 決定 4）」項の禁止記述を書き換え |

**触らない**: `src-tauri/src/egui_shell/view.rs`（**`//!` の追記は削った**——「全域 grep で 0 件」は真のままで、
消えても何も壊れず、戻れば clippy が止める。触らないことで注入の撤去が常に安全になる）・
`docs/adr/ADR-visuals-application-target.md`（凍結された歴史）・`.claude/rules/safety-nets.md`・
`snotra-settings/` ・`snotra-egui-runtime/`・`.github/workflows/ci.yml`・`docs/build-commands.md`・
`SPEC.md`・`src-tauri/CLAUDE.md` のモジュール索引（`G-module-index` の母集団は `.rs` 系で `clippy.toml` は対象外）。

## 実装順序

### Phase 1 — `src-tauri/clippy.toml` を作成する

`disallowed-methods` に上表の 7 パスを置く。`reason` は 7 件とも同一で、**要件だけを書く**:

> `"root Ui が pass 冒頭で掴む Arc<Style> に間に合わない（#751）。run_ui の中では ui.visuals_mut() を使う"`

**`#[allow]` を示唆しない。** #775 の実測（`30bbf1d` の retro）が逐語でこう記録している——
「**要件ではなく検出範囲を教えると、読者は検出範囲を最適化する**」。逃げ道の存在はコメントに書き、
診断には要件だけを載せる。

**ただし「`run_ui` の中では」という条件は残す。** #775 が戒めたのは**どの経路が検出されないか**の
名指しであり（具体例は「別判定に当たらない `\` を選べばよい」）、適用範囲の明示はそれに当たらない。
**そして条件を落とすと、`EguiView::setup(&mut self, context: &egui::Context)` で lint を踏んだ人が
「存在しない `ui` を使え」という助言を読むことになる**——そこは 3-B が構造的に正当と確定した地点である。

**ファイル冒頭のコメントがこの判断の正本になる**（他に置き場所が無い）。書くのは次の 5 点。
**それ以外は書かない**——`clippy.toml` が package スコープであることや `snotra-settings` の正当性は、
このファイルを開いた人が推論で辿り着くか、そもそもこのファイルを読まない人の関心である。

1. **なぜ 7 件か**（1 行）— root `Ui` の `Arc<Style>` snapshot に間に合わない書き込み口。#751。
2. **`style_ui` / `settings_ui` は inspector ウィジェットゆえ意図的に除外**（1 行）——
   禁止すると偽陽性になり `#[allow]` を訓練するため。
3. **`options_mut` / `memory_mut` からの直書きは通る**（1 行・受容残余）。意図的な除外だと
   書いておかないと「漏れ」と読まれ、次の人が足して正当な用途を巻き込む。
4. **沈黙経路が 3 本ある**（下記・**この機構の最重要の弱点**）。
5. **egui のピンを動かしたら、この表を注入で測り直す**（1 行）。
   `egui = "=0.35.0"` は `b59f1fe`(2026-07-22) 以来動いていない。**パス解決の失敗は警告でも
   型エラーでもないので、`/deps-update` も CI も原理的に捕まえられない。**
   「撤去・再測定の合図を成果物自身の doc へ書く」（`AGENTS.md` 条件別チェック・#872）の形である。

**コメントに見出し参照の正準形・行番号を書かない**（`.toml` は `governance:check` の走査元に入らず、
腐っても誰も気づかない）。指すのは issue 番号と **PR #901 / `7562cef`**（#751 を実際に修正した
コミット。squash subject が `#751` を含まないので `git log --grep` では辿れない）に留める。

- [ ] `src-tauri/clippy.toml` を上記の形で新規作成する

### Phase 2 — フォールトインジェクション（7 件すべてを個別に測る）

**この順序は必須である。** 撤去は `git checkout -- src-tauri/src/egui_shell/view.rs` で行う。
`view.rs` は成果物に含まないので、この撤去は常に安全である。

**1 件だけ測って「効いた」と読んではならない**——書き損じた残りが黙って死ぬ（下の沈黙経路 1・2）。
`safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」（#858）そのものの状況である。
注入は `let c = ui.ctx().clone();` で書く（**7 件は `ui` を別に借りないので `.clone()` 無しでも
通る見込みだが未測定である**——借用が割れればコンパイルエラーで大声で落ちるので、詰まったら clone する）。

注入先は `view.rs` の `let visuals = ui.visuals_mut();` の直前。**この注入はガードの行使であって
「稼働中のガードを弱める」ではない**（`safety-nets.md` が #482 で明示的に対象外としている）。

- [ ] 7 呼び出しを `view.rs` へ注入 → `cargo clippy --workspace --all-targets --message-format short -- -D warnings`
      が exit 101 で、**7 メソッド分の `use of a disallowed method` が行番号つきで全部出る**（受け入れ条件 1）
- [ ] `git checkout -- src-tauri/src/egui_shell/view.rs` で撤去し、`git diff --stat` が空になることを確認する

### Phase 3 — `src-tauri/CLAUDE.md` の禁止記述を、規範と機構の広さの差ごと書き直す

現在の「**`ctx.set_visuals` を使ってはならない**」は主語がメソッド 1 つだが、**理由節は既に
「そこ（global style）への書き込みは次の pass からしか効かず」と類で書かれている**。
すなわち規範は #751 の時点から類の広さを持ち、**太字の主語だけが狭い**——`ctx.global_style_mut` を
`run_ui` 内に書けば今日でも同じ欠陥を踏むのに、文書は警告しない。

- 太字の主語を類へ広げる（`ctx.set_visuals` は代表例として残す）。**規範としてはこれが正しい**——
  `memory_mut` 直書きも同じ欠陥を踏むので、規範を機構の広さに合わせて狭めてはならない。
- **同じ文で機構の広さを限定する**: 禁止は `` `src-tauri/clippy.toml` `` の `disallowed-methods` が
  守るが、**それは列挙した 7 メソッドに限る**（`options_mut` / `memory_mut` の直書きは通る）。
  **これを書かないと機構より強い偽の契約になる**（`AGENTS.md`「全称表現は前提条件とセットで書く」）。
- **機構化されたのは禁止だけで、順序は依然として規範しか持たない**ことを明示する。
- 既存の「**この順序に検知手段は無い**」は**そのまま残す**。上の 1 文が直前に来ることで非対称が読める。
- **メソッド名を CLAUDE.md へ写さない**（列挙の正本は `clippy.toml`。写せば 2 か所で腐る）。
- **`` `src-tauri/clippy.toml` `` はルート相対形・コードフェンスの外**（受け入れ条件 5。機構の条件）。
- 面積 cap の制約は無い（`research.md` 測定 5: `src-tauri/CLAUDE.md` はどちらの面にも算入されない）。

- [ ] `src-tauri/CLAUDE.md` の当該箇所を書き換える（既存の順序記述は変更しない）

### Phase 4 — 残りの検証

**`clippy.toml` 単独の変更を測るときは `.rs` を触るか `cargo clean -p snotra` を挟む**（沈黙経路 3）。
Phase 2 の注入が有効なのは `view.rs` を書き換えて fingerprint が動くからであり、
**Phase 1 の直後に clippy を打っても無意味な緑が返る。**

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` → exit 0（受け入れ条件 3。
      **Phase 2 の注入撤去で `.rs` の mtime が動いた後に測る**）
- [ ] `cargo clippy -p snotra-settings --all-targets -- -D warnings` → exit 0（受け入れ条件 2）。
      **exit code をパイプ越しに読まない**——出力はファイルへ落として `echo $?` を直後に取る
- [ ] `npm run governance:check` → 全検査 passed（受け入れ条件 6）
- [ ] **`G-references` の保護が実際に効くことを測る**（受け入れ条件 5 の機構検証）:
      `clippy.toml` を一時退避（`mv`）して `npm run governance:check` が
      「バッククォート参照のパスが実在しない: src-tauri/clippy.toml」で**赤くなる**ことを確認し、戻す
- [ ] `git status --short` の追跡対象の差分が**ちょうど 2 ファイル**
      （`src-tauri/clippy.toml` / `src-tauri/CLAUDE.md`）であることを確認する
      （`workspace/` は未追跡ディレクトリとして別に現れる）

**PostToolUse hook の沈黙を合格と読まないこと。** `selectChecks` は `src-tauri/clippy.toml` にも `.md` にも
検査を割り当てない。**このタスクで hook が自動で走るのは `view.rs` を注入・撤去したときだけ**である。

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| src-tauri で禁止 7 メソッドの直呼びが 0 件である | **本 issue で新設する clippy**（CI・hook の両方で `-D warnings`）。ただし下の沈黙経路つき |
| `options_mut` / `memory_mut` からの global style 直書きが無い | **無い（受容残余）**——`memory_mut` は正当な実使用が 4 件あり禁止できない |
| `snotra-settings` の正当な 2 件が巻き込まれない | Phase 4 の負の測定（既存コードがそのまま検体になる） |
| 適用は visuals を読む最初の操作より前である（順序） | **無い（受容残余）**——本 issue で変わらない。消す設計案は **#949** へ |
| `clippy.toml` の**実在** | **`G-references`**（受け入れ条件 5 の形で書いた場合のみ・Phase 4 で実測する） |
| `clippy.toml` の**内容**（空配列化・パスの書き損じ・egui 依存の消滅） | **無い**——**#950 へ送った**（受容残余ではなく先送りである） |

### 沈黙経路（3 本・すべて実測）

1. **メソッド名・型名の書き損じ** → warning は出るが **`-D warnings` でも exit 0**（CI は緑）。
2. **crate 名の書き損じ／egui 依存の消滅** → **診断そのものが出ない**（`eguii::Context::set_visuals` は
   1 行も警告しなかった）。
3. **`clippy.toml` は cargo の fingerprint に入らない** → `clippy.toml` だけを変更して同一コマンドを
   打つと、キャッシュ replay で**設定を適用せずに exit 0** を返す（`Finished in 0.01s`）。
   CI では `rust-cache` が workspace crate を保存しないため効かないが、**手元と hook では効く**。

しかも **hook は exit code で検出し成功時は何も出力しない**契約（`CLAUDE.md`「検出は exit code、
出力は証拠」）なので、1・2 の warning はエージェントにも届かない。**沈黙は二重である。**

ゆえに **Phase 2 の 7 件注入が、この機構が生きていることの唯一の検知点**である。
ただし**それは実装時 1 回の測定であって継続的な検知器ではない**——6 か月後に 1 行消えたとき
鳴るものは、別 issue の `G-clippy-disallowed` が入るまで実在しない。

その他の抜け道（すべて受容残余）: `#[allow(clippy::disallowed_methods)]`（実測で完全に抑止する）/
他 crate に薄いラッパーを置く（`ADR-visuals-application-target` が既に却下した形）。
**UFCS は抜け道ではない**——完全修飾呼び出しでも発火する（実測）。

## 別 issue へ送るもの（ユーザー裁定・2026-08-06・起票済み）

1. **#949** — **3 値の適用を `search_input_ui`（`view.rs:220`）へ吸収し、初回 pass の子 `Ui` を実コードで
   観測するテストを置く。** `hint` クロージャが子 `Ui` の中で呼ばれるので観測点になる。
   これが通れば「`ui.visuals_mut()` を子 Ui 生成後へ移す」「`ctx.set_visuals` へ戻す」
   「3 値の一部を適用し忘れる」が同じテストで落ち、**順序の受容残余そのものが消える**。
   `ADR-visuals-application-target` が却下した 2 案（上流制限を固定するテスト・visuals reader 一覧に
   依存する静的検知器）とは別物である——**製品関数の同一 pass 出力を測る**形だからである。
2. **#950** — **`G-clippy-disallowed`**（`governance-check.mjs` へ 1 本）。`clippy.toml` が沈黙する 6 経路
   （削除・空配列化・1 行欠落・メソッド名の書き損じ・crate 名の書き損じ・egui 依存の消滅）が、
   すべて **Node の静的読み取りの入力差分**として現れる。カナリアは `REQUIRED_RUSTDOC_LINTS` と
   同じ位置・同じ根拠。

## PR 本文のチェックリストへ送る項目

`safety-nets.md`「CI の実測は PR が在って初めて行える」——`plan.md` に置くと `gh pr create` が
未チェック `- [ ]` で拒んで循環する（#749）。

- CI（`ci.yml` rust-check）の `cargo clippy` が緑であることを PR で確認する
- **`ADR-visuals-application-target` は「`ctx.set_visuals` が届かないことを固定する対のテスト」を
  「上流が直した日に緑のビルドが赤になる」という理由で却下していた。本変更は同じ命題を別の道具で
  固定する行為である**ことを PR 本文に自覚的に書く（壊れる向きが逆なので却下理由は当たらないが、
  書かないと後日 ADR を読んだ人が矛盾と読む）

## 未確定（実装前に潰す）

- [x] **禁止対象の集合** — **7 件**。`research.md` 測定 1 が直接 writer 7 件を確定し、独立導出レビューが
  名前から選べない 3 件（`style_ui` / `settings_ui` / `set_debug_on_hover`）を指摘、
  縮小レンズと codex が**その 3 件を落とすべき**と独立に結論した。決め手は egui 自身の doc——
  `style_ui` / `settings_ui` は**ウィジェットを描く API** であり、呼ぶ人は #751 の誤りを犯していない。
  **却下**: issue 原案の 1 件（`ctx.style_mut_of(ctx.theme(), …)` が素通りする）・10 件案（偽陽性で
  `#[allow]` を訓練する）。**「1 件で足りる」の論証は縮小レンズが明示的に試みて構築できなかった。**
- [x] **`.claude/rules/safety-nets.md` の `paths` へ足すか** — **足さない**。`paths` は配送のトリガーで
  あって検知器ではない。`clippy.toml` の生存は `G-references`（実在）と別 issue（内容）が担う。
- [x] **`snotra-egui-runtime` にも置くか** — **置かない（射程外）**。同 crate は `run_ui` の呼び出し側で、
  `EguiWindow::new` が `run_ui` より前に global style を書くのは正当である（`runtime.rs:380-386`）。
- [x] **`EguiView::setup` を塞ぐか** — **塞ぐ**。`setup`（`view.rs:373` / `results_view.rs:513`）は
  `run_ui` の外で呼ばれるので欠陥を持たず、歴史上そこに style 書き込みは一度も無い（`git log -L` で確認）。
  crate 全体の禁止はこの正当な地点も塞ぐが、**必要になれば `#[allow]` + 理由コメントで開けられる**。
  `reason` にその逃げ道を書かないのは #775 の教訓による（Phase 1）。
- [x] **codex の choke point 案 / `G-clippy-disallowed` を本 PR に含めるか** — **どちらも別 issue**
  （ユーザー裁定・2026-08-06）。

## マルチパースペクティブレビュー結果

4 枠を**道具ごと分けて**独立に走らせた（成果物はすべて `workspace/`）。

| 枠 | 道具 | 成果物 | 主な発見 |
|---|---|---|---|
| 独立導出（`--deep`） | コード + egui ソース | `plan-review-clippy-disallow-set-visuals.md` | 名前から選べない 3 メソッド・`EguiView::setup`・沈黙経路 1/2 |
| codex | 別モデル（codex-cli 0.146.0） | `review-codex-design.md` | **`memory_mut` 経由の抜け道**・choke point 案・件数基準の揺れ |
| 縮小（KISS） | 計画と issue の突き合わせ | `review-lens-minimal.md` | **`style_ui` / `settings_ui` は偽陽性**・`view.rs` 追記は不要・コメント削減 |
| 歴史（逆向きの監査） | `git log -S` / `blame` | `review-lens-history.md` | **#775「検出範囲を教えると最適化される」**・egui ピン再測定の面が無い・#706 の先例 |
| 既存機構 | 既存機構の実装読解 | `review-lens-native.md` | **fingerprint の沈黙経路**・`G-references` が実在を守る・`G-clippy-disallowed` の設計 |

**独立に到達した一致**（→ すべて採用）: `style_ui` / `settings_ui` を削る（minimal・codex）／
`view.rs` を触らない（minimal・codex）／`clippy.toml` の生存に検知器が要る（native・history）／
egui バージョン結合の負債（codex・history）。

**対立と裁定**:

- **道具そのもの**（codex「choke point + テストが上位、clippy は補助」vs native「clippy.toml が最善、
  代替は型解決の手書きの写し」）→ **排他ではない**と判定。codex 案は #751 の不変条件全体を守る別の
  設計であり、実現しても `ctx.set_visuals` への巻き戻しは止まらない。**#900 は clippy で実施し、
  codex 案は別 issue**（ユーザー裁定）。
- **`CLAUDE.md` を類へ広げるか**（codex「`memory_mut` があるので偽の契約になる」vs minimal
  「規範は既に類の広さを持ち、太字の主語だけが狭い」）→ **両立させた**。規範は類のまま広く書き、
  **同じ文で機構の広さを 7 件に限定する**（Phase 3）。`memory_mut` が `pub` な `Options::dark_style` へ
  書けることは自分で再照合した。
- **`reason` の文言**（私の当初案は `#[allow]` の逃げ道を読ませる設計）→ **history の指摘を採用**。
  #775 の逐語「要件ではなく検出範囲を教えると、読者は検出範囲を最適化する」と形が一致する。
  要件だけを書き、逃げ道はコメントへ。

**要対処のうち採用しなかったもの**: 無し（別 issue へ送った 2 件を含め、すべて反映した）。

## セルフレビュー

- リスク: **高**（`src-tauri/CLAUDE.md` は `governanceDocs` の母集団・禁止集合の網羅性が要件）
- plan-review: **独立導出 1 体**（Step 2b・`--deep`）＋ **ユーザー要請によるマルチパースペクティブ 4 枠**
- エージェント数: **4**（うち 1 は codex CLI）
- 自己照合（5 点）:
  1. **issue の全要件に作業項目が対応する** — 作業項目 1 → Phase 1、2 → Phase 3、3 → Phase 2/4 と未確定 2、
     4 → Phase 2。「順序の記述を書き換えない」制約は Phase 3 と「触らない」に明記
  2. **境界条件と検証** — 正（7 件が赤）・負（settings が緑）・clean（exit 0）・**沈黙する経路 3 本**・
     **`G-references` の保護条件**を列挙し、それぞれ Phase 2/4 に検証がある
  3. **新しい状態・リソース・プロセス** — 無し（設定ファイル 1 枚）
  4. **より単純な既存パターンで置き換えられないか** — 既存機構 9 通りを実装読解で評価済み
     （`review-lens-native.md` §1）。`clippy.toml` は新しい機構ではなく**既に CI と hook の両方で
     走っている clippy への入力**であり、代替候補はすべて型解決を持たない字面判定の写しである
  5. **壊してはならない不変条件に検知手段があるか** — 上の表。順序と `clippy.toml` の内容は
     **受容残余ではなく別 issue へ先送り**と明示した
- 要対処: **11 件**（4 枠合計。すべて根拠を自分で再照合し、計画へ反映または別 issue へ送付）
- 未検証: **2 件** — 実 repo での赤の確認（Phase 2）と `G-references` の実発火（Phase 4）。
  どちらも実装時に測る手順として置いてある

## 人間レビュー

- [x] 承認済み — 2026-08-06 / 問い: "`workspace/plan.md` をご確認のうえ、**注釈を書き加えていただくか、明示的に承認**をいただけますか。承認まで実装には入りません。" / 回答: "OK"
