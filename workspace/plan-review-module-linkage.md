# 独立レビュー — #1085 plan.md（G-module-linkage 新設）

対象: `workspace/plan.md`（決定・実装順序・変更ファイル一覧）、`workspace/research.md`（M5・Q3 の実測）。
検証観点は依頼どおり 2 つ（A: 新設検査の母集団と誤爆／B: G-module-index との責務・記録の写し）。
リポジトリへの変更は行っていない。

## 検証方法（実測）

- `scripts/governance-check.mjs` を全読み（1-806, 1800-1915 行）。`checkModuleIndex`（87-150 行）・
  `MODULE_INDEX_CRATES`（103-108 行）・検査登録表 `buildChecks`（1836-1877 行）を確認。
- 4 crate の `src/` 配下を bash で列挙し、`mod.rs`（5 枚）・`#[path]`（1 箇所）・`tests/*.rs`（crate 直下 4 枚）・
  `build.rs`（2 枚）・`src/bin/`（0 件）・proc-macro crate（0 件）・`include!`（0 件）・
  インライン `mod x { ... }` 内の `mod y;`（0 件）を実測。
- `.rs` の総数（95 件 = 31+12+37+15）が research.md M5 の「95/95 到達」と一致することを確認。
- `scripts/governance-check.test.mjs` の `#701` 母集団カナリア（92-115 行）を実読し、
  `MODULE_INDEX_CRATES` を実 `Cargo.toml` の `[workspace] members` に対して検算するテストの実在を確認。
- `docs/build-commands.md:166` と `.claude/rules/governance-docs.md` を実読し、
  計画の「検査一覧を散文へ写さない」記述が引用元と逐語一致することを確認。

## 要対処

（なし）

## 軽微

- **AGENTS.md 行の更新文言が未確定** / 根拠: plan.md:167「`AGENTS.md`... `.rs` 追加/削除の行を更新する」は
  作業項目としてのみ存在し、実際の文言案が plan.md 中に無い。`.claude/rules/governance-docs.md:18`
  「機構の実装の詳細（述語の種類・件数・分岐の列挙）を散文へ写さない…書くのは帰結だけ」と、
  `AGENTS.md`「全称表現は前提条件とセットで書く」に照らすと、実装時の文言が
  「`mod` 宣言も機構が見るようになった」のような**帰結のみ**に留まっているかを Phase 2 完了時点で
  確認する必要がある。「全 `.rs` の `mod` 漏れを検知する」のような前提無しの全称にならないよう
  （実際の母集団は `MODULE_INDEX_CRATES` の 4 crate の `src/` 配下に限られ、`tests/*.rs` や `build.rs` は
  対象外）注意が要る。 / 提案: Phase 2 の当該作業項目に「帰結のみ・母集団の限定を明示」と一言添えるか、
  実装後のセルフレビューで governance-docs.md の当該条項と突き合わせる。
- **ADR の短縮引用先の綴りが計画内でのみ確認可能** / 根拠: `ADR_CITATION` 正規表現
  （`scripts/governance-check.mjs:1788`）は `ADR-[a-z][a-z0-9]*(?:-[a-z0-9]+)*` で、
  plan.md が使う slug `ra-diagnostics-suppression` はこの形に適合する（実測: 正規表現へ手動で当てて確認）。
  問題ではないが、実装後に `G-adr-citations` / `G-adr-file-names` が緑を返すことを
  `npm run governance:check` の実行結果で確かめる、という計画の既存項目（受け入れ条件・Phase 3）が
  この点も自動的に検算する形になっており、二重の確認は不要という所見を記録しておく。

## 未検証

- **判定式の実装コード自体は未検証**（当然）。plan.md M5 の実測は試作
  （`scratchpad` の `unlinked-proto.mjs`、リポジトリ外）による結果の転記であり、本レビューは
  「試作が対象にした母集団・エッジケースが今のリポジトリの実態と一致するか」までを実ファイル列挙で
  検算した。試作コード自体の読解は行っていない（存在しないため）。
- **`docs/hooks.md`「Claude Code の RA インスタンスと hook の分担」への追記文言**も同様に plan.md には
  未確定。現行節（`docs/hooks.md:67-87`）は表 2 枚の短い構成で、「何が届くか」の 1 段落を足す計画は
  節の分量・様式と整合するように見えるが、実際の文言は実装時にしか確認できない。

## 詳細（観点 A: 母集団と誤爆）

**両方向とも実測で裏付いた。**

1. **足 2 は母集団に入る。** 判定対象は `<crate>/src/**/*.rs` で、`mod` を辿れないファイルはそのまま
   findings になる設計（plan.md:91-93）。現状は 95/95 到達（誤検出 0）だが、これは「今は誰も違反していない」
   だけであり、判定ロジック自体は新規 `.rs` を作って `mod` を忘れれば拾う（研究側で変異実測済み・
   plan.md 受け入れ条件にも同じ変異がカナリアとして要求されている）。
2. **判定対象外の不混入。** `<crate>/src/` に閉じることで:
   - `snotra-core/tests/*.rs`（4 枚: `dir_stat_cost.rs` / `memory_footprint.rs` / `path_query_cost.rs` /
     `search_frame_cost.rs`）と `src-tauri/build.rs` / `snotra-settings/build.rs`（crate 直下、`src/` 外）は
     実測で population から**構造的に**除外されることを確認した（`find` で `src/` 配下のみ数えると
     ちょうど 95 件で、これら 6 枚は含まれない）。
   - `#[path]` は実在（`snotra-egui-runtime/src/ime.rs:83-84`: `#[cfg(windows)] #[path = "windows_ime.rs"]
     mod platform;`）。同じシンボル名 `platform` が非 Windows 側では `#[cfg(not(windows))] mod platform { ... }`
     という**インラインモジュール**として存在する（`ime.rs:86-99`）。plan.md の「`#[cfg(...)]` は無視して和を取る」
     という規則で両方とも正しく扱える形になっている（file 版は `#[path]` 解決、inline 版はそもそも
     `mod y;` を含まないファイルレスの定義なので判定対象外）——実ファイルで実測済みの想定内ケース。
   - `mod.rs` は 5 枚（`snotra-core/src/search/tests/mod.rs` / `src-tauri/src/commands/mod.rs` /
     `src-tauri/src/egui_shell/mod.rs` / `src-tauri/src/platform/mod.rs` /
     `snotra-settings/src/tabs/mod.rs`）。特に `search/tests/mod.rs` は自身が `mod common;` 等 8 個の
     `mod x;` 宣言を持ち、宣言元が `mod.rs` の場合「同じディレクトリ」で解決する規則
     （plan.md:94-95）どおりに `search/tests/` 配下のファイルへ正しく解決することを手で辿って確認した。
   - `src/bin/`・proc-macro crate・`include!` はいずれも 0 件（grep で確認）。
   - インライン `mod x { ... }` の中に `mod y;`（ファイル参照の入れ子宣言）が無いことも grep で確認
     （0 件）——plan.md の Phase 1 項目「grep で確かめる」がそのまま実行可能で、結果は計画の前提と一致する。
3. **`MODULE_INDEX_CRATES` の再利用は妥当。** `scripts/governance-check.test.mjs:92-115`
   （`describe("G-module-index/G-references 母集団カナリア — #701"`) は実 `Cargo.toml` の
   `[workspace] members` を読み、`CLAUDE.md` を持つ member が `MODULE_INDEX_CRATES` と `governanceDocs()`
   の両方に載ることを強制している。**この canary は定数 `MODULE_INDEX_CRATES` 自体を検算するので**、
   これを再利用する新検査は自動的に同じドリフト防止を受け継ぐ（plan.md の主張は実測で裏付く）。
   現在の `[workspace] members` は `snotra-core` / `snotra-egui-runtime` / `src-tauri` / `snotra-settings`
   の 4 つのみで `MODULE_INDEX_CRATES` と完全一致（Cargo.toml 実読で確認）。

## 詳細（観点 B: 責務の重なりと記録の写し）

1. **責務は重ならない。** `checkModuleIndex`（governance-check.mjs:110-150）は「CLAUDE.md の散文 ↔ 実ファイル
   basename」の双方向照合であり、mod 宣言のグラフ到達性は一切見ない。research.md の Q3
   （足 1: 索引にもmodにも書かない→ G-module-index が捕捉／足 2: 索引だけ書きmodを忘れる→ 素通り）は
   plan.md にそのまま引き継がれており、両検査が守る命題が異なることは実測（変異注入）で裏付いている。
   新検査は `G-module-index` の判定ロジックにも `MODULE_INDEX_CRATES` の定義にも触れない
   （純粋な追加）ので、既存保証を薄める変更ではない。
2. **写しの分担は AGENTS.md の規範と整合する。** plan.md 62-76 行の表
   （ADR=正本／`docs/hooks.md`=帰結+参照／`AGENTS.md`=1 行のみ／メモリ=正本を指すだけ／`research.md`=
   正本にしない）は、`AGENTS.md`「文書に事実の写しを増やす変更」が要求する「正本を 1 か所に定め他は参照へ」
   にそのまま対応する。各配置先の現状の分量・様式（`docs/hooks.md:67-87` の短い節、
   `AGENTS.md` の条件別チェック表が 1 トリガー 1 行という形式）とも整合し、そこへ全文転記を追加する
   計画にはなっていない。
3. **検査一覧を散文へ写す変更は計画に含まれない。** `docs/build-commands.md:166` は
   「検査の一覧は同ファイルのコメント見出しが SSOT——ここに範囲で写すと黙って腐る」と明言しており
   （実測: 該当行を直接確認、plan.md:114-117 の引用と逐語一致）、plan.md 自身がこれを明示的な不変条件
   として書き（「`docs/build-commands.md` と `AGENTS.md` の既存の列挙に新しい検査名を足さない」）、
   変更ファイル一覧にも `docs/build-commands.md` は含まれていない。整合している。

## 結論

観点 A・B とも、plan.md / research.md の主張は実ファイルでの検算に耐えた。新検査の母集団は
現状のリポジトリの全エッジケース（`#[path]`・`mod.rs`・cfg 分岐・`tests/`・`build.rs`）を
正しく扱える設計になっており、既存 `G-module-index` の保証を薄めない。ガバナンス文書の写し管理も
規範（`AGENTS.md`「文書に事実の写しを増やす変更」・`docs/build-commands.md:166`）に沿っている。
唯一の実務的な注意点は、AGENTS.md への追記文言がまだ確定していないため、実装時に「帰結のみ・
母集団の限定を明示」という既存規範を踏み外さないよう確認することである（軽微・要対処ではない）。
