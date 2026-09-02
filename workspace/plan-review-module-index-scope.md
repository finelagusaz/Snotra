# 計画準拠レビュー: G-module-index 逆方向照合のスコープ絞り込み

対象 issue: #1214
読んだ計画: `C:/workspace/Snotra/workspace/plan.md`
読んだ調査: `C:/workspace/Snotra/workspace/research.md`

対象コード: `C:/workspace/Snotra/scripts/governance/checks/G-module-index.mjs`
関連: `C:/workspace/Snotra/scripts/governance/lib.mjs`（`sectionOf`, 382-431行）、
`C:/workspace/Snotra/scripts/governance/edit-findings.mjs`

**検証方法**: 稼働中のガードは一切変更していない。読み取り専用スクリプトを
`%TEMP%/.../scratchpad/verify-section-scope.mjs` と `verify-mutation.mjs` / `verify-mutation2.mjs`
へ書き、実物の `makeSnapshot` / `sectionOf` / `MODULE_INDEX_CRATES` / `moduleIndexSources` を
`file://` URL で import して、メモリ上でしか変異させていない（`snapshot.read` をラップして
1 ファイルだけ差し替え）。追跡ファイルへの書き込みは本レポートのみ。

---

## 観点1 — 逆向きの監査（差分が消す不変条件・偽陽性/沈黙経路・「触らない」宣言の追随）

### 独立再測（自分で実行し、計画・research.md の値と照合）

1. **現行ツリーでの掃除の費用**: `verify-section-scope.mjs` を実行し、4 crate 全体で
   `text.includes` 版と `section.includes` 版の両方を回した。
   結果: 両方とも findings 0 件、**section 版だけが新たに赤くする実ファイルは 0 件**
   （`新規に赤くなるファイル数: 0`）。plan.md の受け入れ条件2「現行ツリーで governance:check が緑のまま」
   と一致。

2. **残る死角（1′ ケース）の再現**: `snotra-core/CLAUDE.md:77`（`autostart.rs` の索引行）だけを
   メモリ上で削除して `verify-mutation.mjs` を実行。
   結果: `text` 版・`section` 版とも `赤か? false`（緑のまま）。これは同じ節内の
   `snotra-core/CLAUDE.md:78`（`win_registry.rs` のエントリ本文にある `` `autostart.rs` `` という言及）が
   救っているためで、plan.md 表の「1′」行・research.md の「案 C の残余」節と一致した。

3. **新たに捕まる側（+12 の実例）**: `snotra-settings/CLAUDE.md:21` の `` `style.rs` `` 索引行だけを
   メモリ上で削除して `verify-mutation2.mjs` を実行。
   結果: `text` 版は `赤か? false`（`## スタイルシステム` 節内、`snotra-settings/CLAUDE.md:30`
   付近の `` `src/style.rs` が実装 `` という節外の言及に text 全体照合が救われている）、
   `section` 版は `赤か? true`。research.md の crate 別内訳表
   （snotra-settings: 現行 3/3 → 案A 6/0）の機序をピンポイントで裏づけた。

### `sectionOf` 失敗経路（節切り出し失敗）による偽陽性の有無

`checkModuleIndex`（`G-module-index.mjs:73-76`）は `sec.body == null` のとき
`sec.findings` を積んで **`continue`** する。この分岐は逆方向ループ（97-99行目）の**手前**にあり、
`text.includes` → `section.includes` の変更点はこの分岐の**内側にない**。したがって：

- 見出しが消えた／表記ゆれで一致しない（`anchors.length === 0`）
- 見出しが複数ある（`anchors.length > 1`）
- 「モジュール構成」が最終節になった（`ending: "heading"` で終端見出しが見つからない）

のいずれも、**変更前後で挙動が完全に同一**（1 件の finding を出して該当 crate の順方向・逆方向照合を
丸ごとスキップする）。**「節切り出しの失敗で大量の偽陽性が出る」という経路は無い**——
失敗はむしろ逆方向照合そのものを止める方向（「その crate は何も照合しない」）に倒れ、
既存の性質（body:null → continue）がそのまま新設のリスクを吸収している。

`section` が空文字列 `""` になる経路（見出し直後に別見出しが来る）は、`"".includes(x)` が
常に false なので**逆方向対象の全ファイルが赤**になる。これは plan.md の Phase 1 チェックリスト
「本文が空の節は有効…『全件』ではない、を書き換える」で計画済みの意図的な fail-closed 挙動であり、
`sec.body == null` で continue する経路（沈黙側）とは明確に別の分岐（`sec.body != null` だが `""`）
なので、境界の混同は無い。

### 「触らない」宣言の追随（`edit-findings.mjs`）

`edit-findings.mjs:31,134` は `checkModuleIndex` を実物のまま呼ぶため、逆方向の
述語変更（`text` → `section`）は編集時 reminder にも自動的に伝播する。これは
plan.md「触らない」の意図どおり（同じ関数を呼ぶので自動追随）。

**追随の意味**: 変更前は「`<crate>/CLAUDE.md` の節の外側の散文を編集して、たまたま
その散文にあった唯一のファイル名言及を消してしまった」場合でも `edit-findings.mjs` の
逆方向 reminder が発火し得た（text 全体照合のため）。変更後はこの経路が消え、
節の外側の編集で逆方向 reminder が発火することはなくなる。これは望ましくない後退ではなく
**ノイズが減る側の変化**（節と無関係な編集で無関係な reminder が鳴らなくなる）と判断した——
`moduleIndexCrateOf`（`edit-findings.mjs:94-96`）が `crate.whole = true` にするのは
`<crate>/CLAUDE.md` 自体を編集した場合のみで、その場合は元々「その crate の索引と実ファイルの
双方向の不整合」を無差別に返す設計（`docs/hooks.md:108`）なので、対象範囲が節に絞られること自体が
この行の記述（「双方向の不整合」）と矛盾しない。

---

## 観点2 — 変更で偽になる散文・メッセージ・テストの数え上げ

主エージェントが既に見つけている `G-module-index.mjs:98` の finding メッセージ以外を、
「G-module-index」という識別子を含む全 16 ファイルと、「索引」を含む `.claude/skills/**` 全 5 ファイル、
および概念ラベル（本文・basename・逆方向・文書のどこか等）の横断 grep（87 ファイル）から
関連度でスクリーニングして確認した。`.claude/worktrees/` は除外、`docs/adr/**` は凍結歴史として
実害（生きた層からの手順委譲）が無い限り対象外とした。

### 要対処（1件）

**`.claude/skills/implement/SKILL.md:77`** — 以下の記述が、`AGENTS.md:73` や `docs/hooks.md:107-108`
と**全く同じ型の無条件claim**を持つが、plan.md の「変更ファイルと対象シンボル」表に載っていない:

> 「索引漏れは `governance:check` が捕捉する——ただし `MODULE_INDEX_CRATES` に載っている crate に
> 限る。crate ごと足し忘れる形は `npm test` の母集団カナリアが捕まえる〔#701〕・#1008——が PR まで
> 漏らさない。**`.rs` を編集した直後にも reminder が鳴る**〔#1139〕」

「PR まで漏らさない」「reminder が鳴る」は、**同じ crate の「モジュール構成」節の中に同名の言及が
他にあれば鳴らない**という新しい死角の対象そのものである。AGENTS.md:73 は plan.md Phase 6 で
「同じ crate の節の中に同名の言及があれば鳴らない」という条件つきへ弱める予定だが、
**同じ主張を持つこのファイルは Phase 6 の対象に入っていない**。

- 対処案: Phase 6 の対象へ `.claude/skills/implement/SKILL.md:77` を追加し、
  AGENTS.md / docs/hooks.md と同じ表現（ADR / ヘッダを指すだけ）へ揃える。
  もしくは、この 3 箇所が同じ主張の写しであることを踏まえ、`.claude/rules/governance-docs.md`
  の「かぶりなく」原則に従い、SKILL.md 側は `AGENTS.md`「ファイル（`.rs`）を追加/削除」を
  指すだけに縮めることも検討に値する（現状は 3 箇所が独立に同じ主張を持っており、
  今回のように 1 箇所を直しても写しが残るリスクを構造的に持っている）。

### 軽微（2件）

1. **`docs/development-principles.md:194`**（検証の層の表、「文書の整合（編集時）」行）——
   「死角」列は「削除・`governanceDocs` の外・`mod` 宣言・他文書からその文書を指す参照」を挙げるが、
   今回追加される死角（同じ節の中に同名の言及があれば鳴らない）は載っていない。既存の文が
   偽になるわけではない（この行は「編集した1ファイルに帰属する索引漏れ・参照の不在を知らせる」と
   一般的に述べているだけで、無条件性を主張していない）が、この表がこの種の性質の**正本**を
   自称してはいないため必須ではない。plan.md が ADR / ヘッダへ性質を集約する方針を取るなら、
   この表の「死角」列を更新する必要はないが、更新するならここも候補になる。

2. **`snotra-settings/CLAUDE.md:21` 削除後に `` `src/style.rs` `` の言及がスタイルシステム節に残る点**——
   直接のバグではないが、案A適用後にこの crate の索引整合を目視で直す際、
   「節の外側にある言及は逆方向照合に数えられなくなった」ことを実装者が把握していないと、
   「なぜ急に赤くなったファイルがあるのか」で戸惑う可能性がある。ADR に crate 別の実例
   （snotra-settings が 3/3→6/0 と最も影響が大きい）を載せる計画（Phase 4）で対処範囲内。

### 未検証（2件）

1. ⚠️ **本文・basename 等の一般語での横断 grep（87ファイル）の全件を 1 行ずつは読んでいない。**
   `G-module-index` という識別子または「索引」+ skills という強い絞り込みで見つかった候補は
   全て確認したが、識別子を使わずに散文だけでこの検査の挙動を語っている文書
   （例: crate 側の `CLAUDE.md` 内の「モジュール構成」節の前書き）が他にゼロとは断定できない。
   `snotra-core` / `src-tauri` / `snotra-settings` / `snotra-egui-runtime` の各 `CLAUDE.md` の
   「モジュール構成」節そのものは grep で前書きを確認し、走査対象の記述は持っていなかった。

2. ⚠️ **「掃除の費用 0 件」は今日の値であり、将来の下界ではない**（plan.md 自身が明記済み）。
   自分の再測でも同じ 0 件を確認したが、これは検証時点のスナップショットに対する事実であり、
   本 PR のマージ後に別の変更が節の外へ basename 言及を追加/削除すれば値は変わりうる。
   ADR へ日付つきで書く計画（Phase 4）は妥当。

---

## まとめ

- **観点1（逆向きの監査）**: コード変更（`text.includes` → `section.includes` の 2 箇所）自体に
  新規の偽陽性爆発・沈黙経路は見つからなかった。`sectionOf` の失敗系は変更の前後で対称。
  plan.md / research.md の中核主張（掃除費用0件・残る死角・+12件の強化）は
  3 件とも独立に再測して一致した。
- **観点2（数え上げ）**: 主エージェントが見つけた `G-module-index.mjs:98` 以外に、
  **`.claude/skills/implement/SKILL.md:77`** が同型の無条件claimを持ち、plan.md の
  変更ファイル一覧から漏れている（要対処）。
