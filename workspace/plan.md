# plan — #825（stale な主張の写し）+ #819（腐り検出器の射程）

> **エージェント実行者へ:** タスク単位で実行する。各ステップは `- [ ]` で追跡する。
> **`gh pr create` は未チェック項目が残っていると拒否される**（`.claude/hooks/pre-bash.mjs`）。
> やらないと決めた項目は削除して理由を記録すること。
>
> **本計画は 2 PR に分かれる。** Phase A（PR 1）を出し、マージ後に Phase B（PR 2）へ進む。
> 一次証拠は `workspace/research.md`。**マルチパースペクティブレビュー 4 体の結果**は
> `workspace/review-facts.md` / `review-completeness.md` / `review-mechanism.md` / `review-simplicity.md`
> に在り、本計画はその指摘を反映した第 2 版である（反映内容は末尾「セルフレビュー」）。
>
> **ID の正本はこのファイルのチェックボックスだけである。** 変更ファイル一覧の表は ID を持たない
> （第 1 版で表と手順が別の ID を指しており、実行者が取り違える形だった・review-simplicity）。

## 全体の目的

「現に在るものを『無い』と述べる記述」を消し（Phase A）、**同じ欠陥クラスの再発を機構で捕まえる**ところまで進める（Phase B）。#819 案 (A) は「拡張した検出器が最初に指すもの」であり、Phase B の受け入れ条件の一部を Phase A が満たす関係にある。

## PR の分け方

| PR | 中身 | 性格 |
|---|---|---|
| **1** | #825（`CLEAR_COLOR` の主張の写し **6 箇所**）+ #819 案 (A) | 事実訂正のみ・**機構不変**。リスク通常 |
| **2** | #819 案 (B)（`G-stale-identifiers` の射程拡大 + 拡大が指す件数の是正 + ADR 追記） | **判定を足す変更**。フォールトインジェクション必須・リスク高 |

分ける理由: 測定が「射程を広げない」に転んでも PR 2 が縮むだけで、#825 の事実訂正はその結果に人質を取られない。#819 案 (A) を PR 1 側へ置くのは、`docs/development-principles.md` の**同一節**（`:67` と `:71`）を触るためコンフリクトが構造的に消えるからである。（issue コメントの「#489 に従い単独 PR が要る」は過読み——#489 は「検査対象を変更しながら検査を走らせない」という順序の制約である。）

---

# Phase A（PR 1）— #825 + #819 案 (A)

## 受け入れ条件

1. 「一致に落ちる検査は無い」と読める記述が、**`.rs` / `.mjs` / `.md` / `.ps1` に 0 件**（`docs/superpowers/`・`.superpowers/`・`workspace/` を除く）。**全称で「リポジトリに 0 件」と書かない**——検証 grep が見る拡張子と一致させる（`AGENTS.md`「全称表現は前提条件とセットで書く」）
2. 「`[visual].background_color` は消費者ゼロ」と**現在形で**述べる記述が 0 件（歴史として過去形で述べるのは可）
3. **主張 A の系**（一致に検査が無い）について、機構の説明の正本が `snotra-egui-runtime/src/renderer.rs` の `CLEAR_COLOR` doc に定まり、他 4 箇所（`snotra-egui-runtime/CLAUDE.md` / `window_coordinator.rs` の doc / `visual-check-colors.ps1` / `build-commands.md`）が事実として正しい。**主張 B の系は条件 2 が受け持つ**——A4/A7 が書き換えるのは主張 B であり「機構を指す参照」にはならないので、条件 3 の母集団に混ぜると字義どおりの検証で必ず落ちる（review-completeness）
4. `docs/development-principles.md` に `G12_NO_LAUNCHER_READ` が 0 件
5. `npm run governance:check` が緑 / **`cargo test -p snotra`** が緑

## 正本の決定（実装判断・根拠つき）

**正本 = `snotra-egui-runtime/src/renderer.rs` の `CLEAR_COLOR` の doc コメント。** 値を変える人が必ず開くのは定数の定義位置であり、`CLAUDE.md` ではない（`docs/development-principles.md`「規範が効くのは変更者がその場所を通るときだけ」）。

**ただし他所を「正本を見よ」というポインタへ倒さない。** review-simplicity の指摘が決定的である——**機械照合される場所（`.md`）に照合されないポインタを書き、照合されない場所（rustdoc）に照合されうる情報（テストのパス）を書くのは向きが逆**である。`.md` からテストのパスを**直接**書けば、パスの実在は G-references が守り、テストを移した瞬間に CI が名指しで落ちる。ポインタ構造はその保証を一切与えず、読者に 1 ホップ課すだけである。

ゆえに**各所が事実を正しく述べ、`.md` 側はテストのパスを直接書く**。「正本」が意味するのは「由来と理由を最も詳しく書く場所」であって「他所が指す先」ではない。

## 変更ファイル一覧（ID は持たない。手順のチェックボックスが正本）

| ファイル | 対象 | 種類 |
|---|---|---|
| `snotra-egui-runtime/src/renderer.rs` | `CLEAR_COLOR` の doc（`:10-12`） | 正本を書く |
| `src-tauri/src/egui_shell/window_coordinator.rs` | テストの doc（`:698-700`） | 宙に浮く逆参照を直す |
| `snotra-egui-runtime/CLAUDE.md` | `:39` の bullet | 主張 A +「目視だけ」を訂正 |
| `docs/development-principles.md` | `:61` 導入文・`:67` 段落 | 主張 B + 検証状況を再接地 |
| `docs/development-principles.md` | `:71` | `G12_NO_LAUNCHER_READ` → `NO_LAUNCHER_READ`（#819 案 A） |
| `scripts/governance-check.mjs` | `:1228-1229` の説明コメント | 主張 B を訂正 |
| **`scripts/visual-check-colors.ps1`** | `:7-9` | **第 6 の写し**（review-completeness が発見）。引用の空振り確認 |
| `docs/build-commands.md` | `:77` | 読み直しのみ（引用の空振り確認） |

`NO_LAUNCHER_READ` の表本体（`:1283-1295`）は**触らない**——`background_color` が載っていないのが正しい状態である。

## 実装順序

### A-Phase 1 — 正本と機構（`.rs` 2 ファイル）

- [ ] **A1** `snotra-egui-runtime/src/renderer.rs:10-12` の doc を差し替える

  ```rust
  /// view が色を決める前（起動直後の 1 枚目）と `set_clear_color` 呼び忘れのフォールバック。
  /// **`snotra-core` の `default_background_color()` と同値であり、一致は機構が固定する**
  /// ——`src-tauri/src/egui_shell/window_coordinator.rs` の
  /// `runtime_fallback_matches_config_default_background` が両者を突き合わせる。この crate は
  /// `snotra-core` に依存しないため、検査は**両方に依存する下流**（`src-tauri`）にしか置けない。
  ```

  - 定数行 `pub const CLEAR_COLOR: u32 = 0x0028_2828;` は変えない

- [ ] **A2** `src-tauri/src/egui_shell/window_coordinator.rs:698-700` の doc を差し替える

  ```rust
  /// runtime のフォールバック（`set_clear_color` を呼ばなかったフレームの色）が config の
  /// 既定背景色と一致することを**機構で**固定する。両 crate に依存するのはこの crate だけなので、
  /// 突き合わせられる位置がここしか無い。**`snotra-egui-runtime` の `CLEAR_COLOR` の doc と
  /// `snotra-egui-runtime/CLAUDE.md` がこのテストを名指す**——改名・移動するなら両方を直す。
  ```

  - 旧文の「一致は今まで規約でしかなかった（`snotra-egui-runtime/CLAUDE.md` が受容した残余）」を落とす。A-Phase 3 で指す先が消えるため
  - **A2 の完了は A11 の grep では検算できない**——旧文は「一致は**今まで**規約でしか**なかった**」でマーカー語に部分一致しない（review-facts 実測）。**A12 の目視確認が唯一の担保である**

- [ ] **A3** **`cargo test -p snotra`** で当該テストが通ることを確認する
  - **`--lib` を付けてはならない**——`src-tauri/Cargo.toml` は `[lib]` を持たないバイナリ crate で、`cargo test -p snotra --lib` は `error: no library targets found in package 'snotra'` で止まる（実測）。SSOT は `docs/build-commands.md:19` / `:148`

### A-Phase 2 — `scripts/governance-check.mjs`

- [ ] **A4** `:1228-1229` の実例を差し替える

  ```js
  // 実例: `VisualConfig.preset` はランチャが `ThemePreset` を import すらしていない。
  // （`[visual].background_color` は #802 以前が同じ形で、描画経路の消費者がゼロだった。
  //  今は `egui_shell/visual.rs` が読むので下表に載らない。）
  ```

  - 判定ロジック・`NO_LAUNCHER_READ` の表・関数は**一切触らない**

### A-Phase 3 — 文書（`.md` 2 ファイル + `.ps1` 1 ファイル）

- [ ] **A5** `snotra-egui-runtime/CLAUDE.md:39` の bullet を差し替える

  ```markdown
  - **`RuntimeFrame` の埋めないフィールドは既定値が黙って効く**（`RawInput` と同型）——`set_clear_color` を呼ばなかったフレームは `renderer.rs` の `CLEAR_COLOR`（`0x0028_2828`）へ落ちる。**呼び忘れはビルドでも自動テストでも落ちない**——検知するのは `npm run check:colors`（非既定色で実ピクセルを判定する。GUI を要するため CI には無く、人が手で走らせたときだけ動く）と目視である（`docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」）。**`CLEAR_COLOR` と `snotra-core` の既定背景色の一致は規約ではなく機構が固定する**——`src-tauri/src/egui_shell/window_coordinator.rs` の `runtime_fallback_matches_config_default_background`（由来と理由は `renderer.rs` の `CLEAR_COLOR` の doc）
  ```

  - **テストのパスを直接書く**のが要点（G-references がパスの実在を守る。`REF_EXTENSIONS` に `.rs` を含む・実測）

- [ ] **A6** `docs/development-principles.md:67` の段落を差し替える

  ```markdown
  **休眠を支えているのは、自動で回る検証が既定値の下でしか走らないことである。** 非既定の config を組むテストは在るが（`engine.rs` の検索系）、**見た目に効く値は既定の下でしか描かれない**——色を変えて描画を検証する経路は `npm run check:colors` の 1 本だけで、GUI と非ロック画面を要するため **CI には無く、人が手で走らせたときにしか動かない**（`docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」）。乖離を露出させる入力——ユーザーが既定と違う値を設定すること——は、放っておいて回る側の検証には現れない。**この形で実際に休眠していたのが #802 以前の `[visual].background_color` である**: 描画経路の消費者がゼロだったが、既定 `#282828` が `snotra-egui-runtime` の `CLEAR_COLOR` と一致するため既定のままでは正常に見え続けた（#802 で消費者を与え、一致は `src-tauri` のテストが固定するようになった）。
  ```

  - **主題文を「検証が既定値の下でしか走らない」→「自動で回る検証が〜」へ弱める**のが要点。`check:colors` の存在で旧主題文は全称として偽になる

- [ ] **A7** **同じ節の `:61` 導入文を直す**（A6 とは独立の項目。第 1 版では A6 の bullet に埋め込まれており、実装者が差し替え文だけ貼って読み飛ばす経路が開いていた・review-completeness）
  - `:61`「この欠如は 3 つの形で**現れる**」は現在形で「3 形が今も生きている」と主張するが、A6 が唯一の現行実例を歴史へ移す。「現れうる」「現れてきた」等へ弱めるか、実例が解消済みである旨を足す
  - `:63` の 3 形の**名前**（消費者ゼロ / 導出が 2 経路 / 既定値の偶然の一致）は概念の定義ゆえ変えない
  - **異論の記録**: review-simplicity は「`:61` は概念の説明であって実例の主張ではないので削れ」とした。review-completeness は「必須であって任意ではない」とした。**後者を採る**——「3 つの形で現れる」は現況についての現在形の命題であり、支える実例が全て歴史になった節でそのまま置くのは #825 が消そうとしている欠陥そのものだからである

- [ ] **A8** `docs/development-principles.md:71` の `G12_NO_LAUNCHER_READ` を `NO_LAUNCHER_READ` へ直す（**#819 案 (A)**）。1 語のみ
- [ ] **A9** `docs/build-commands.md:77` を読み直し、`docs/development-principles.md`「config の値は到達性の検出器を持たない」への引用が空振りしていないことを確認する（節見出し `:55` は変えないので G-heading-refs は緑。確認は意味の側）
- [ ] **A10** **`scripts/visual-check-colors.ps1:7-9` を読み直す**（第 6 の写し）。逐語は「config の既定色 `#282828` は `snotra-egui-runtime` の `CLEAR_COLOR` と一致するため、既定のまま起動しても「色が届いていない」欠陥は観測できない（`docs/development-principles.md`「config の値は到達性の検出器を持たない」）」——**今日も真**なので事実は直さなくてよいが、A6 が引用先の節の主題文を動かすので空振りしないか確認する。**`.ps1` は `governanceDocs` の母集団外ゆえ G-heading-refs にも G-references にも守られていない**（実測）——機械は助けてくれない

### A-Phase 4 — 検算と検証

- [ ] **A11** 受け入れ条件 1・2・4 を grep で検算する

  ```bash
  grep -rn "落ちる検査は無い\|一致は規約\|機構ではなく規約" \
    --include=*.rs --include=*.mjs --include=*.md --include=*.ps1 . \
    | grep -v "^./target" | grep -v node_modules | grep -v "^./docs/superpowers/" \
    | grep -v "^./.superpowers/" | grep -v "^./workspace/"
  grep -rn "消費者ゼロ" --include=*.rs --include=*.mjs --include=*.md --include=*.ps1 . \
    | grep -v "^./target" | grep -v node_modules | grep -v "^./docs/superpowers/" \
    | grep -v "^./.superpowers/" | grep -v "^./workspace/"
  grep -rn "G12_NO_LAUNCHER_READ" docs/
  ```

  - **「検査は無い」をパターンに入れてはならない**——全く別の残余を述べる文を巻き込む（`.superpowers/sdd/plan/spec-inventory-duplication.md:451`「本棚卸しの findings に対応する検査は無い」が実例・実測）。**第 1 版が書いた「二重に数える」という理由は誤り**（`grep` の alternation は 1 行を 1 回しか出さない・review-facts 実測）
  - **除外行はすべて実効的である**——`docs/superpowers/` には `消費者ゼロ` が 1 件実在し（`specs/2026-07-28-config-background-color-design.md:71`・実測）、`.superpowers/` は `grep -rn` の視界に入る（`WALK_EXCLUDE_PREFIXES` は `governance:check` の走査にしか効かない）。第 1 版の「防御的」という説明は誤り
  - 1 本目の期待: **0 件**
  - 2 本目の期待: `development-principles.md:63`（3 形の名前）・`view.rs:352`（過去形）・A6 と A4 が書き換えた 2 箇所（過去形）のみ
  - 3 本目の期待: **0 件**
- [ ] **A12** **A2 の完了を目視で確認する**（A11 では検算できない・A2 の注記参照）
- [ ] **A13** `npm run governance:check`
- [ ] **A14** `cargo test -p snotra`
- [ ] **A15** `git diff scripts/governance-check.mjs` で `NO_LAUNCHER_READ` の表と判定関数に差分が無いことを確認する
- [ ] **A16** `cargo fmt --check` / `cargo clippy` は PostToolUse hook が `.rs` 編集で自動実行する（沈黙 = 合格）

## Phase A の不変条件

| 不変条件 | 壊れたときの検知手段 |
|---|---|
| `snotra-egui-runtime` は `snotra-core` に依存しない | 本変更は依存関係に触らない（`Cargo.toml` に `snotra-core` 0 件・実測） |
| `CLEAR_COLOR` の値と `default_background_color()` の値は一致する | `runtime_fallback_matches_config_default_background`。**乖離で実際に落ちることをフォールトインジェクションで実測済み**（`0x0028_2829` へ 1 文字変異 → `assertion left == right failed` / 復帰後 ok・review-facts） |
| `.md` から書くパス参照は実在する | G-references（`REF_EXTENSIONS` に `.rs` を含む）。**A5 がテストのパスを直接書くのはこの保証を得るためである** |
| **rustdoc と `.ps1` 内のパス参照は機械照合されない** | `governanceDocs` は `.md` のみ。**受容する残余**——A1/A2 の相互参照と A10 の引用は規範でしか守られない |
| `NO_LAUNCHER_READ` の表と実コードの双方向一致 | G-config-reachability。表を触らないので影響なし（A15 で確認） |

異常系: 無し（挙動を変える変更を含まない。`.rs` の変更は doc コメントのみ）。

## Phase A のセーフティネット手順

`scripts/governance-check.mjs` を触るので `.claude/rules/safety-nets.md` が配送されるが、**Phase A に同 rule の仕事は無い**:

- **フォールトインジェクション: 不要。** 根拠は「コメントしか変えないから」**ではない**——`adrCitationDocs` は `scripts/governance-check.mjs` を母集団に含み、**コメント本文を読む**（review-mechanism が発見）。正しい根拠は「**判定の実体（`checkConfigFieldReachability` と `NO_LAUNCHER_READ` 表）を変えないから**」であり、A15 がそれを担保する
- **`/norm-review`: 不要。** 起動条件は「判定を足す変更」であり「種が書けない変更（索引の追随・改名）には仕事が無い」。加えて 2026-07-27 のユーザー裁定で指摘の採用は既定で見送り

---

# Phase B（PR 2）— #819 案 (B) 腐り検出器の射程拡大

**Phase A のマージ後に着手する。** 着手時点で `:71` は直っているので finding は 1 件減る。

## 測定（着手前ゲート）

proxy snapshot で測った（稼働中のガードは触っていない）。**レビュー 3 体が独立に再現し、うち 1 体は `scripts/governance-check.mjs` の export 関数そのものへ問うて全行一致を確認した**（`AGENTS.md`「列挙も SSOT のツール自身に問う」）。

| 述語 | 照合 | finding | 真の腐り | 外部語彙 |
|---|---|---|---|---|
| ベースライン（現行） | 1 | 0 | 0 | 0 |
| E 単独（SCREAMING_SNAKE） | 7 | 0 | 0 | 0 |
| D 単独（`docs/**`） | 69 | 35 | 8 | 27（**全て ADR の却下記録**） |
| D-（`docs/**` − `adr/`） 単独 | 18 | 8 | 7 | 1 |
| M 単独（モジュール `CLAUDE.md`） | 9 | 2 | 1 | 1 |
| D-+E | 43 | 12 | 8 | 4 |
| **D-+E + 語彙源に `.yml`（採用）** | 43 | **10** | **8** | **2** |
| （参考）D-+E + `.yml`/`.json` | 43 | 9 | 8 | 1 |
| （参考）D-+M+E + `.yml`/`.json` | 73 | 13 | 9 | 4 |

**第 1 版から 2 点訂正した:**

1. **「偽陽性 0」は成立しない。** `docs/development-principles.md:128` の `backgroundThrottlingPolicy` は**真の腐りではなく外部語彙**である——`tauri.conf.json` のキー名を「Windows 非対応でビルドエラーになる」と**在ってはならないことを述べるために**名指している。現行語彙に無いのは腐ったからではなく在ってはならないからで、検出器が要求する向きが逆になっている。**M 軸を却下した理由と同じクラスであり、同じ現象に軸ごとに別の分類規則を当てていた**（review-completeness / review-facts が独立に到達）
2. **採用セルは第 1 版では測られていなかった。** 2 本のスクリプトのうち一方は現行語彙のみ、他方は M 入りの母集団しか出しておらず、「9」は**引き算による推論**だった。review-facts が直接測って再現を確認した（`docs=32 照合=43 finding=9`）。本文の「13 → 12 → 9」も写し間違いで、正しくは **12 → 10 → 9**（D-+E 基準）

## 採る射程（測定に基づく決定）

1. **検査対象に `docs/**.md` を足す。ただし `docs/superpowers/` と `docs/adr/` を除く**
   - `docs/adr/` を除くのは、ADR が**否定の知識＝もう存在しない案**を書く場所だからである。D 単独の finding 35 件中 27 件が ADR 自身の却下記録で、うち 20 件が `ADR-stale-identifier-detector-scope.md` 自身のもの——**ADR がその ADR を赤にする**
   - **非対称を明記する**: `governanceDocs`（G-references の母集団）は `docs/adr/` を**含む**。「ADR は歴史ゆえ母集団外」は**この述語に固有の判断**であってリポジトリ全体の不変条件ではない。ADR が指すパスは今日も在るべきだが ADR が語る識別子は消えていてよい、という正しい非対称である。**書かないと次の人が統一しにいく**
2. **述語に SCREAMING_SNAKE を足す** — `/^([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)(\(\))?$/`。`_` を 1 つ以上要求する形で、camelCase 述語が「こぶを 1 つ以上要求する」のと同じ構造（ADR 却下 3「単語 1 つの識別子」とは矛盾しない）
3. **`VOCAB_SOURCE_EXT` に `.yml` だけを足す。`.json` は採らない**（**第 1 版から変更**）
   - `.github/workflows/*.yml` は**追跡され・人が書き・CI が実際に実行する**。「語彙を寄付してよいのは『現に動いている実装』だけである」（`scripts/governance-check.mjs:1387-1388`）に当たる
   - **`.json` 一括は 3 つの契約を破る**（review-mechanism の実測）: (i) 同じ原則——`src-tauri/gen/schemas/*.json` 306 KB は**生成物**、`package-lock.json` 49 KB は依存メタデータ（新規語彙 118 個の大半が integrity ハッシュの base64 断片で `npm install` のたびに入れ替わる）、(ii)「テストコードは語彙を寄付しない」——`test-results/.last-run.json` が `failedTests` を寄付する、(iii) ファイル冒頭の「決定的」契約——`test-results/.last-run.json` と `.claude/settings.local.json` は **gitignore 済みで CI に存在せず、手元と CI で語彙が割れる**（`.superpowers/` を走査から外した理由と同型・#722）
   - **「手書きの `.json` 3 本だけ」に絞る案も採らない。** 測定上は等価だが、それは**リスト**であり冒頭契約「免除注記の機構は設けない」と ADR 却下 2 に当たる。`.test.<ext>` が許されているのは**ファイル名の形**だからである
   - **`.json` を採らない代償は finding 1 件**（`docs/hooks.md:67` の `CLAUDE_PROJECT_DIR`）。これは B10 と同じ**文書側の記述の正確化**で処理する（下記 B11）——外部語彙は語彙源を広げて免罪するのではなく文書側の書き方で外す、というのが `closingIssuesReferences` に対して採られた前例であり、**M 軸を却下した原理とも一貫する**（第 1 版は同じ性質の問題に 2 つの原理を当てていた・review-simplicity）
   - `.yml` も `GITHUB_ENV` / `GITHUB_OUTPUT` / `TAG_NAME` / `TAURI_SIGNING_PRIVATE_KEY` という **GitHub 提供の外部語彙**を寄付する。これは受容するが **ADR へ残余として書く**
4. **`.json` を採らないので、`currentVocabulary` のコメント除去分岐は `.yml` を `#` 側へ回すだけでよい**（`.ps1`/`.toml` と同じ扱い）
5. **母集団に足すもの / 足さないものを、測ったうえで決める**（**第 1 版に無かった。review-completeness が実測**）

   | 母集団 | camel finding | SNAKE finding | 決定 |
   |---|---|---|---|
   | `snotra-settings/SETTINGS-DESIGN.md` | 0 | 0（照合 31） | **足す**——コスト 0 で、設定 UI の識別子は腐りやすい面 |
   | ルート `CLAUDE.md` / `AGENTS.md` | 0（照合 4） | 0 | **足す**——M 軸の偽陽性 3 件は**すべてモジュール側**に出ており、M をひと括りで却下したことで無害な半分まで落ちていた |
   | モジュール `CLAUDE.md`（4 本） | 1 真 | 3 外部語彙 | **足さない**——ラップ対象の外部 API（Win32 / tao / TTC）を語る場所ゆえ外部語彙の**密度**が高い |
   | `.github/**.md` | 0 | 9 | **足さない**——`OPENAI_API_KEY` 等は GitHub の secret / variable でリポジトリに実体が無く、`.yml` を語彙へ入れても消えない |
   | `PERFORMANCE.md` | 8 | 0 | **足さない**——8 件すべてが「この節の具体例は WebView2 期のものである」という**既存の免責注記の中で名指しされた語**（`:3-6`）。`docs/adr/` と同じ歴史クラス |
   | `src-tauri/capabilities/README.md` | 0 | 1 | **足さない**——`CAPABILITY_FILE_EXTENSIONS` は Tauri のビルド時定数 |
6. **`docs/design/` の扱いを意識的に決める。** `docs/design/2026-05-31-coherence-staleset.md`（`status: Agreed` / 日付スラグ）は D- の母集団に入るが、`docs/superpowers/` と同じ「日付付き設計書＝歴史記録」の性質を持つ。**今日は finding 0 だが、それは規則が守っているのではなく、たまたま腐った識別子が書かれていないだけである。** 除外へ足すか、受容する残余として ADR に書く

## Phase B の作業項目

### B-Phase 1 — 検出器の改修（`scripts/governance-check.mjs`）

- [ ] **B1** `STALE_EXTRA_DOCS` の経路で新母集団を検査対象へ足す。**`staleIdentifierDocs` へは入れない**——`runAll:1725` の `staleDocs.length === 0` が `.claude/**` の消滅を見ており、混ぜると長さが常に 1 以上になり**その検知が永久に沈黙する**
- [ ] **B2** **新母集団自身の fail-closed を足す**（**第 1 版に無かった。レビュー 3 体が独立に指摘・review-mechanism が実測**）
  - **実測**: `docs/` が丸ごと消えた proxy に計画どおりの実装を当てると `finding 0 / 照合 1 / exit 0`＝**緑で沈黙する**。検出器は拡大前の射程へ黙って戻る
  - **既存の 3 つの 0 件検知はどれも代替にならない**——`ctx.docs`（`governanceDocs`）も `ctx.refDocs`（`headingRefDocs`）も `ctx.staleDocs`（`.claude/**` 24 本）も他の母集団で埋まったまま非空である
  - **守られているのは照合 1 件を寄付する母集団で、守られていないのが 36 件を寄付する母集団である**（per-file 内訳の実測）
  - `runAll` へ既存 `staleDocs.length === 0` と**対称な 1 行**を足す。`STALE_EXTRA_DOCS` を動的化すると `SPEC.md` の「実在を問わず加える」性質（静的リテラルゆえ読めなければ鳴る）が失われ、**`docs/**` が 0 件でも `SPEC.md` の 1 件で埋まる**ことに注意
- [ ] **B3** `STALE_IDENT` の隣に SCREAMING_SNAKE の述語を足し、`scanStaleIdentifiers` が両方を試すようにする
- [ ] **B4** `VOCAB_SOURCE_EXT` に `yml` を足し、`#` コメント除去の側へ回す
- [ ] **B5** **自称スコープの doc コメントを改訂する**（`:1371-1415`。区切り `---` は `:1370` と `:1416`）。「見るのは `.claude/**` の散文と `SPEC.md` の中の**バッククォート内 camelCase 識別子**だけである」は B1・B3 の瞬間に**偽**になる。新母集団が何を含むか（`docs/` は設計原則・ビルド手順・フック契約・アーキ説明という性質の違う 4 種を含む）を 1 文で書き、`docs/adr/` を外した理由と M を採らなかった理由も同節へ置く
- [ ] **B6** finding のメッセージ文字列を、拡大後の母集団と整合する文言へ直す
- [ ] **B7** `scripts/governance-check.test.mjs` の G-stale-identifiers 関連テストを改訂する
- [ ] **B8** **新母集団の配線テストを新設する**（**第 1 版に無かった・review-mechanism**）。`governance-check.test.mjs:959` の `describe("G-stale-identifiers の配線 …")` と**同じ形**。同ファイル `:955-958` の論証——「`staleTargets` を `staleDocs` へ戻しても実リポジトリの finding は 0 / 照合 1 のまま変わらないため、dogfood テストも証跡の印字も気づけない」——が新母集団にそのまま当たる。**B9〜B10 は実装時 1 回きりの測定であって、後日の退行を捕まえる面ではない**

### B-Phase 2 — フォールトインジェクション（`.claude/rules/safety-nets.md` 必須）

- [ ] **B9** **述語だけを切り分けて測る。** 種は**既存母集団**（`.claude/rules/*.md`）へ蒔く——`docs/**` へ蒔くと述語と母集団を同時に変異させることになり、失敗時にどちらが原因か切り分けられない（第 1 版の欠陥・review-mechanism）
  - **種は実在の欠陥にする**: `G12_NO_LAUNCHER_READ`（Phase A が消す語・SCREAMING_SNAKE 述語が捕まえる唯一の実測例）を赤フィクスチャに、**`NO_LAUNCHER_READ` を緑の対にする**。「架空の識別子」は `governance-check.test.mjs:874-875` の作法（「赤フィクスチャは実際に検出された `createObjectURL`」）から外れる
  - **逆向きを必ず測る**（第 1 版は順方向しか書いていなかった）——語彙に在る SCREAMING_SNAKE（`CLEAR_COLOR` / `NO_LAUNCHER_READ` / `AREA_BUDGET`）が鳴らないこと
- [ ] **B10** **母集団を切り分けて測る。** B9 の種を `docs/**` へ移して捕まること、**`docs/adr/` へ移して捕まらないこと**を両方向で確認する（`.claude/rules/safety-nets.md`「検査の入力集合を、具体対象で検算する」）。B2 の fail-closed も同時に測る（`docs/` を空にした proxy で鳴ること）
- [ ] **B11** `.yml` を語彙源へ足したことによる免罪の範囲を測り、**実際に語彙へ入るファイルを列挙して**受容する残余として明記する

### B-Phase 3 — 拡大が指す件数の是正（**同じ PR に束ねる**）

検出器の射程拡大と、それが指す件数の是正は 1 タスクに束ねる（未修正なら `governance:check` が赤のまま）。

- [ ] **B12** `docs/development-principles.md` の finding を是正する。**Phase A 後は 7 件 / 相異なる識別子 5 個**（`viewKind` = `:78`,`:83` / `interpKind` = `:78`,`:84` / `shouldShowResults` = `:39` / `assertNever` = `:81` / `isInstantPrefix` = `:84`）。**識別子の個数と finding の件数を取り違えないこと**
  - **一律の方針で処理できない**（review-facts）: `:78` / `:81` / `:83` / `:84` は撤去された SolidJS フロントの語を**教訓の出典**として書いている。`:39` の `shouldShowResults` は**参照の作法を説く節の例示**で文脈が違う。歴史として書くならバッククォートを外して散文にする（`.claude/rules/governance-docs.md`「歴史を書くならバッククォートを外して散文にする」）か、現行の等価物へ差し替える
- [ ] **B13** `docs/development-principles.md:128` の `backgroundThrottlingPolicy` を**外部語彙として**処理する。**前 5 個とは処方が違う**——現行の等価物が存在せず、存在してはならない。散文化は可能だが `tauri.conf.json` のキー名を散文で書くと読者が検索できなくなるので、書き方を個別に決める
- [ ] **B14** `docs/hooks.md:67` の `CLAUDE_PROJECT_DIR` を処理する（`.json` を語彙源へ入れない帰結）。**`EXTERNAL_CMD_LINE` は `gh|npm|cargo|git|node|pwsh|npx` にしか当たらないので `closingIssuesReferences` のときのコマンド行化は使えない**（実測）。候補: バッククォートを外して散文にする / `${CLAUDE_PROJECT_DIR:-.}` の形で書く（`.` を含むので述語が構造的に外し、`.claude/settings.json` の実際の記法とも一致する＝記述の正確化になる）
- [ ] **B15** `npm run governance:check` が緑になるまで反復する

### B-Phase 4 — ADR 追記と検証

- [ ] **B16** `docs/adr/ADR-stale-identifier-detector-scope.md` へ追記節を足す（既存の「その後（#735 完了後・射程を広げた）」と**同じ形**。原文は 1 文字も書き換えない）。書く内容:
  - (a) 測定表（**「偽陽性 0」ではなく「外部語彙 2 件」と正しく分類したもの**）
  - (b) `docs/adr/` を母集団から外した理由 + **`governanceDocs` は `docs/adr/` を含むという非対称**とその理由
  - (c) **M を却下した理由**（否定の知識）。ただし「外部語彙は `docs/**` には出ない」ではない——**実測で出ている**。本当の理由は「モジュール文書は外部語彙の密度が高い」である
  - (d) **`.json` を語彙源へ入れない判断**（否定の知識。測定上は等価でありながら 3 つの契約を破るため）と、`.yml` が寄付する GitHub 提供語彙を受容する残余として記録
  - (e) 「述語は camelCase しか見ない」という既存の受容残余の**更新**（SCREAMING_SNAKE の追加を反映）
  - (f) `docs/design/` の扱い（除外へ足す / 受容する残余として測定の射程を明記）
  - (g) B9〜B11 のフォールトインジェクション結果
- [ ] **B17** `npm run governance:check` / `node --test scripts/governance-check.test.mjs`
- [ ] **B18** PR 本文へ「CI での実測」をチェックリストとして送る（`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」）

## Phase B の不変条件

| 不変条件 | 検知手段 |
|---|---|
| `.claude/**` が空なら母集団欠落として鳴る | `runAll` の `staleDocs.length === 0`。B1 が `STALE_EXTRA_DOCS` 経路を使うことで保つ |
| **新母集団（`docs/**` 等）が空なら鳴る** | **B2 で新設する**（現状は存在せず、実測で緑に沈黙する） |
| 配線を戻すと鳴る | **B8 で新設する**（既存の配線テストと同じ形。実リポジトリの finding は 0 のままなので dogfood も証跡も気づけない） |
| `docs/adr/` の歴史記述は鳴らない | B10 の逆向き検算 |
| 免除注記の機構を設けない（ファイル冒頭の契約） | 除外リストを追加しない。**`.json` を採らない判断もこの契約から出ている** |
| テストコードは語彙を寄付しない | `VOCAB_TEST_FILE` の除外を維持。**`.json` を採らないので `test-results/.last-run.json` の経路は開かない** |
| 判定は決定的（手元と CI で同じ） | **`.json` を採らないので gitignore 済みファイルが語彙へ入る経路は開かない** |

## Phase B のセーフティネット手順

- **フォールトインジェクション: 必須**（B9〜B11）
- **`/norm-review`: 起動条件には当たる**が、2026-07-27 のユーザー裁定により指摘の採用は既定で見送り。起動そのものを省く判断は Phase B 着手時にユーザーへ確認する
- **`/plan-review`: Phase B 着手前に 1 回実行する**（リスク高）

---

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要**（両 Phase とも）。`SPEC.md` に `CLEAR_COLOR` は 0 件（実測）
- **ADR**: Phase A は不要（否定の知識が生じない）。**Phase B は必須**（B16）
- **`RETROSPECTIVE.md`: 不要**（サイクル末に `/retrospective` が扱う）

## 未確定（実装前に潰す）

（なし——#819 の束ね方はユーザー回答で確定し、射程の各軸は着手前ゲートの実測とレビュー 4 体の再現で確定した）

## 人間レビュー

- [x] 承認済み — 2026-08-03 / 問い: "引き続き、`workspace/plan.md` への注釈または明示的なご承認をお待ちしております。" / 回答: "承認"

## セルフレビュー

- リスク: **Phase A = 通常 / Phase B = 高**
- plan-review: 未実施。代わりに**マルチパースペクティブレビュー 4 体**（一次証拠の検算 / 列挙の完全性 / セーフティネットの機構 / やりすぎ検分）を実施。**エージェント数 4**、成果物は `workspace/review-*.md`
- **要対処として反映した件数: 18 件**

### レビューが見つけた、第 1 版の欠陥（すべて反映済み）

| # | 指摘 | レンズ | 反映 |
|---|---|---|---|
| 1 | **`cargo test -p snotra --lib` は存在しないターゲット**（`[lib]` を持たないバイナリ crate。実測で `error: no library targets found`） | facts | 3 箇所を `cargo test -p snotra` へ |
| 2 | **変更ファイル表と手順で ID が別のものを指す**（表の A3 = CLAUDE.md、手順の A3 = cargo test） | simplicity | 表から ID 列を削除し、チェックボックスを唯一の正本に |
| 3 | **採用セルはどちらのスクリプトでも測られていなかった**（引き算による推論だった） | facts | 直接測定で再現を確認し、その旨を明記 |
| 4 | **「13 → 12 → 9」は 3 つの数字が別々の行からの写し間違い** | facts | 12 → 10 → 9（D-+E 基準）へ訂正 |
| 5 | **`backgroundThrottlingPolicy` は真の腐りではなく外部語彙**。「偽陽性 0」は成立しない | completeness / facts | 測定表を「真の腐り / 外部語彙」で再分類。B13 を独立項目に |
| 6 | **新母集団に fail-closed が無い**（`docs/` 消滅で exit 0 の緑・実測） | mechanism / simplicity / completeness | **B2 を新設** |
| 7 | **新母集団の配線テストが無い**（戻しても実リポジトリは緑のまま） | mechanism | **B8 を新設** |
| 8 | **`.json` 一括追加は 3 つの契約を破る**（生成物 306 KB・lockfile・gitignore 済み 2 本で手元と CI が割れる）。手書き 3 本へ絞る案も「リスト」ゆえ冒頭契約に当たる | mechanism / simplicity | **`.json` を却下**し `.yml` のみ採用。代償 1 件は B14 の記述正確化で処理 |
| 9 | **外部語彙の扱いが D 軸と M 軸で非対称**（片や語彙源拡大で免罪、片や却下） | simplicity | 文書側の記述正確化へ統一（`closingIssuesReferences` の前例に揃える） |
| 10 | **第 6 の写し `visual-check-colors.ps1:7-9` が母集団に無い** | completeness | 変更ファイル表と A10 に追加 |
| 11 | **受け入れ条件 3 が 2 つの別主張を混ぜており字義どおり検証すると必ず落ちる** | completeness | 主張 A の系に限り、主張 B は条件 2 へ |
| 12 | **受け入れ条件 1 が全称なのに検証 grep が `.ps1` 等を見ない** | completeness | 条件を拡張子と揃え、grep に `--include=*.ps1` を追加 |
| 13 | **A11 では A2（第 5 の頂点）を検算できない**（旧文がマーカー語に部分一致しない） | facts | **A12（目視確認）を新設**し、A2 に注記 |
| 14 | **「`grep` が二重に数える」は誤り**（alternation は 1 行 1 回） | facts | 理由を「別の残余を巻き込む」だけに訂正 |
| 15 | **「`docs/superpowers/` は 0 件ゆえ防御的」は誤り**（`消費者ゼロ` が 1 件実在） | facts | 「除外行はすべて実効的」へ訂正 |
| 16 | **Phase A の「フォールトインジェクション不要」の根拠が誤り**（`adrCitationDocs` は `governance-check.mjs` を含みコメント本文を読む） | mechanism | 根拠を「判定の実体を変えないから」へ差し替え |
| 17 | **Phase B の母集団に測っていない群が 4 つ**（`.github/**.md` 9 件・`PERFORMANCE.md` 8 件・`capabilities/README.md` 1 件・`SETTINGS-DESIGN.md` 0 件）。ルート `CLAUDE.md`/`AGENTS.md` は偽陽性 0 なのに M ごと落ちていた | completeness | 採る射程 5 に**測定つきの採否表**を新設 |
| 18 | **B9〜B11 が述語と母集団を同時に変異させ、逆向き検算が無く、種が架空だった** | mechanism | 述語（B9）と母集団（B10）へ分離し、逆向きを明記、種を `G12_NO_LAUNCHER_READ` へ |

### レンズ間で割れた論点（裁定を記録する）

| 論点 | simplicity | completeness / mechanism | 裁定 |
|---|---|---|---|
| `docs/development-principles.md:61` の弱化 | **削れ**（概念の説明であって実例の主張ではない） | **必須**（現在形の命題を支える実例が全て歴史になる） | **残す**（A7 として独立項目化）。「3 つの形で現れる」は現況についての現在形の命題であり、支える実例が全て歴史になった節でそのまま置くのは #825 が消そうとしている欠陥そのものである |
| 「正本 1 + 参照 n」構造 | **平坦化せよ**（機械照合の向きと逆） | 言及なし | **平坦化を採る**——`.md` からテストのパスを直接書けば G-references が守る。「正本」は「由来と理由を最も詳しく書く場所」の意味に留める |
| `.json` の扱い | **却下**（買うのは 1 件） | **手書き 3 本へ narrow**（測定上等価） | **却下**——narrow もリストであり冒頭契約に当たる。代償 1 件は文書側で処理 |

### 未検証（正直に残す）

1. **`npm run check:colors` の実行そのもの** — GUI と非ロック画面を要する。参照する事実（非既定色 `#4A2B5C` で起動・最頻色を期待色と突き合わせ・exit code で判定・CI には無い）は 2 体がスクリプト本文の逐語で確認済み
2. **Phase B のフォールトインジェクション**（B9〜B11）— 実装時に行う
3. **CI での実測** — PR が無いと行えない（B18 で PR 本文へ送る）
4. **`.yml` 追加後の採用セル（照合 43 / finding 10）** — `.json` を落とす決定が第 2 版で初めて入ったため、この行は測定表の「D-+E」12 件と「+`.yml`」10 件から導いた。**Phase B 着手時に直接測り直すこと**（第 1 版で採用セルが未測定だったのと同じ形の穴を作らないため）
