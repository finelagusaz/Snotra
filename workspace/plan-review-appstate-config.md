# plan-review — #1129 AppState.config の private 化

対象 issue: #1129

## 要対処

なし。2 つのリスク観点（ガバナンス文書の射程記述／偽になる散文の網羅）のいずれについても、計画を差し戻すべき欠陥は見つからなかった。根拠は以下「軽微」「未検証」に記す。

## 軽微

- **Phase 3 の作業項目（`plan.md:100`）の文言が新設すべき残余を明示していない** — 「`src-tauri/CLAUDE.md` 57 行目の条項の『ただし機構ではない』を実態へ更新する（**残る残余 2 つを消さない**）」は、消してはならない**既存**の 2 残余（`engine.lock()` 越しの読み・guard 内 I/O）だけを名指し、private 化で**新たに生じる**残余（`state.rs` の内側〔`#[cfg(test)] mod tests` を含む〕ではフィールドを直に綴れること）を追加する指示になっていない。実害は無い——受け入れ条件 3（`plan.md:17-18`）が「`state.rs` の内側では綴れること」を明記し、「直読みは書けなくなった」と無条件に書くことも既に禁じているため、実装はそちらに拘束される。ただし Phase 3 のチェックリスト単体を読んだ実装者が AC3 を参照し損ねる余地はあるので、文言を「残る残余 2 つを消さず、**新たな残余（state.rs 内部での直読み可能性）を追加する**」まで具体化すると取りこぼしが構造的に減る。
- **新設 ADR の作業項目（`plan.md:101`）が構成要素を「決定・却下した代替案・旧 2 ADR との関係」までしか列挙していない** — 参照した既存 ADR（`ADR-config-read-without-exception.md`・`ADR-config-read-exception-discriminator.md`・`ADR-row-replacement-choke-point.md` 等）はいずれも「文脈」「決定」「却下した代替案」「（旧 ADR との関係）」「帰結」の型を持つ。`.claude/rules/governance-docs.md`「書く約束」の「必要なことだけ」に従えば省略も許容されるが、少なくとも「なぜ private 化を選び検知器を評価しないのか」という**文脈**と、「新たに生じた残余をどこが正本として持つか」という**帰結**は、読者が旧 2 ADR の要旨を持たずに読めるために書いておく方が安全（既存 2 ADR の踏襲パターンに合わせる程度の指摘であり、必須要件ではない）。
- **`main.rs:241` の説明コメント（「engine が持つのと同じ `Arc` を渡す（写しではない・`Engine::config_handle` の doc）」）が `AppState::new` への移行で失われる** — 計画の作業項目（`plan.md:82`）はこのコメントの移設に触れていない。実害は無い——同じ事実は `state.rs` の `config` フィールド doc（`state.rs:16`）と `Engine::config_handle` 自身の doc（`engine.rs:267` 付近）に既に書かれており、写しを 1 か所失うだけで正本は健在。

## 未検証

なし。

---

### 検証の詳細（根拠）

## 観点 1 — ガバナンス文書の射程記述

**残余 3 点の記述義務**: `research.md`「変更後に残る残余」が挙げる (a) `engine.lock().unwrap().config_handle().read()` が今も通る、(b) `state.rs` の内側では綴れる、(c) guard 内の錠/I/O は構造で止まらない、の 3 点はいずれも `plan.md` の受け入れ条件 3（`plan.md:17-18`）に**正確に**対応している。(a)(c) は既存の `src-tauri/CLAUDE.md:57` 条項に既に書かれており（`grep` で確認・引用は上の「軽微」節）、private 化はこれらを変えない。(b) は新規に追加すべき記述で、AC3 がこれを要求しているため取りこぼしはない（「軽微」節の指摘は文言の冗長性の問題であって欠落ではない）。

**凍結 ADR の扱い**: `docs/adr/ADR-adr-frozen-history.md`「決定」は「歴史は消えることに対してだけ守り、変わることに対しては守らない」「G-adr-citations・見出し正準参照以外の辺は黙って腐ってよい」と定める。`ADR-config-read-exception-discriminator.md` と `ADR-config-read-without-exception.md` を編集せず新 ADR を書く計画の判断は、`ADR-config-read-without-exception.md`「旧 ADR の案 G との関係」節が実際に取った作法（凍結された `ADR-config-read-exception-discriminator` の却下理由 2 件が #1123 で偽になったが、あちらは直さず「生きた層」である自分自身に事実を書いた）と同型であり、整合している。`plan.md:115-118`／`research.md:187-189` はこの先例を明示的に引いて新 ADR を書く判断をしている。

**G-adr-citations への対応**: `scripts/governance/checks/G-adr-citations.mjs` を読んだ結果、この検査は「文書中に現れた `ADR-<slug>` という短縮引用が実在の ADR ファイルを指すか」だけを照合しており、新設 ADR が**引用されていないこと自体**を赤くする機構ではない（母集団は `docs/adr/` 全体 + 生きた層のガバナンス文書 + `.rs`/`.mjs` コメントで、引用が 1 件も無ければ単に検査対象が 0 件のまま緑で通る）。したがって `plan.md:102`「新 ADR への短縮引用を生きた層（条項）へ置く」は governance:check を通すための必須条件ではなく、実在の辺を張る規約上の良い実務（`ADR-adr-frozen-history.md`「残すのは実在の辺だけ」）である。計画がこの作業項目を独立に持っている点は妥当。

**既存 ADR の様式との整合**: `docs/adr/ADR-config-read-without-exception.md`・`ADR-config-read-exception-discriminator.md`・`ADR-row-replacement-choke-point.md` を読んだ結果、典型構成は「文脈 → 決定 → 検討した代替案と却下理由 →（旧 ADR との関係）→ 帰結」。計画の新 ADR 作業項目（`plan.md:101`）はこのうち「決定」「却下した代替案」「旧 ADR との関係」を明示するが「文脈」「帰結」を明示しない（「軽微」参照）。

## 観点 2 — 変更で偽になる散文の網羅

以下のラベルでフィルタ無し grep を追加実施し、`research.md`「文書の写しの母集団」が挙げた生きた層 2 件（`src-tauri/src/state.rs`・`src-tauri/CLAUDE.md:57`）以外に**新たな更新対象は見つからなかった**:

```
grep -rn "pub ゆえ" . / "呼び出し点は 0" . / "規範は機構より広い" . / "読み口" .
grep -rln "表現不能" . / "1 か所へ集約" . / "コンパイラが" . / "private 化" . / "config.*フィールド" . / "残余" .
```

- `pub ゆえ` — ヒット 0 件
- `呼び出し点は 0` — ヒット 0 件
- `規範は機構より広い` — `src-tauri/CLAUDE.md:57`（既知）と `research.md`（作業ファイル）のみ
- `読み口` — 多数ヒットしたが、いずれも他の話題（wake 経路・`indexing` の読み口・`window_width` の読み口等）であり `AppState.config` の可視性とは無関係
- `表現不能` — `src-tauri/src/state.rs`（既知・`read_config` doc の「表現不能化ではない」）以外は他トピック
- `private 化` / `私的化` — `PERFORMANCE.md:882`（`IndexTree` のフィールド private 化。別の構造体で無関係。`research.md` が「再利用できる先例」として引用しているのみで、本文自体は本変更と無関係のため更新不要）
- `config.*フィールド` — `docs/build-commands.md:168` は `snotra_core::config::Config` の値到達性の話で `AppState.config` とは別物

**`Engine::config_handle` の doc（`snotra-core/src/engine.rs:267` 付近）**: 実際に読んだ結果、本変更で 3→1 へ減る呼び出し点数について**数値的な主張を一切含んでいない**（「UI の毎フレーム live-read を Mutex の外へ出す口」「返すのは同じ Arc であって写しではない」という契約の記述のみ）。ゆえに計画の「触らない」判断（`plan.md:35`）は正しい。

**`docs/architecture.md` / `PERFORMANCE.md` / `SPEC.md`**: 実際に `config_handle|AppState.config|read_config|state\.config` で grep した結果、`architecture.md:231` は「射程と、規範を守る機構は `src-tauri/CLAUDE.md`『モジュール構成』の当該条項が正本——ここに言い換えを置かない」と明記しており、可視性の記述自体を持たない。`PERFORMANCE.md:560` は `read_config` の口を通すようになった事実のみで `pub`/private に触れない。`SPEC.md` はヒット 0 件。3 文書とも計画の「更新不要」判断は正しい。

**構築点・呼び出し点の実測との一致**: `main.rs:240` 付近・`commands/system.rs:44` 付近・`state.rs:110` 付近を実際に読み、`research.md`「構築点は 3 件」の記述（`config: engine.config_handle(), engine: Mutex::new(engine)` の同一形、`indexing` の初期値だけが違う）と完全に一致することを確認した。`AppState::new(engine: Engine, initial_indexing: bool)` という提案シグネチャは 3 件すべての差分（`indexing` のみ）を過不足なくカバーする。
