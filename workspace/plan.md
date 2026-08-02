# SPEC.md の as-built 化と腐り検出器の射程拡大 実装計画

> **エージェント実行者へ:** タスク単位で実行する。各ステップは `- [ ]` で追跡する。
> **`gh pr create` は未チェック項目が残っていると拒否される**（`.claude/hooks/pre-bash.mjs`）。
> やらないと決めた項目は削除して理由を記録すること。

**目的:** `SPEC.md`（第 1 層＝意図の正本）に残る WebView2 期の死んだ記述を as-built へ畳み、
**その結果として腐り検出器 `G-stale-identifiers` の射程を SPEC.md へ広げられる状態にする**。

**なぜこの順序か:** `ADR-stale-identifier-detector-scope` 却下 4 が、SPEC.md を検出器の
**語彙源**に置いた（SPEC の語は定義により「現行語彙」＝腐りを指摘されない）。これは
**暫定措置**であり、同 ADR が「SPEC 自身の stale は #735 の射程」と明記している。
**#735 を実行しないかぎり射程は広げられない**——先に広げれば、SPEC の既存の腐りが
一斉に finding になって作業が破綻する。

## 出典（この計画の根拠）

- **issue #735**（OPEN）— 13 箇所の stale 記述と二層構造 3 節を表で特定済み。**消しすぎの境界 3 点**を持つ
- `.superpowers/sdd/plan/spec-inventory-identifiers.md` — 441 スパンの全数分類・腐敗 23 件
- `.superpowers/sdd/plan/spec-inventory-duplication.md` — 重複 28 件・**既に食い違い 5 件**
- `docs/adr/ADR-stale-identifier-detector-scope.md` — 却下 4（SPEC を語彙源に置いた暫定措置）
- `docs/superpowers/specs/2026-07-19-doc-governance-design.md` §1 責務分担表

## Global Constraints（全タスクに掛かる）

- **`main` へ直接コミット・プッシュしない**
- **全称表現は前提条件とセットで書く。書けないなら書かない**（`AGENTS.md`「検証の作法」）
- **正本を 1 か所に定め他は参照へ**（写しを増やさない）
- **序数で他を指してはならない**（`.claude/rules/governance-docs.md`）
- **`scripts/governance-check.mjs` はセーフティネットである。** 変更時は `.claude/rules/safety-nets.md` に従い
  **フォールトインジェクションで一度は実測する**（稼働中のガードを弱めず、複製に変異を当てる）
- **`.claude/rules/` の恒久規範は文字数予算を持つ**（`rules N/9200`）。**足すなら削る原資が要る**
- **コマンド本体の正本は `docs/build-commands.md`。** この計画に写しを増やさない

## 射程についての正直な見積もり（着手前に確認した事実）

`G-stale-identifiers` の述語は **`STALE_IDENT = /^([a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)(\(\))?$/`**
——**バッククォート内の camelCase（こぶ 1 つ以上・先頭小文字）だけ**である。ゆえに:

- **捕まる**: `createEffect` / `noResults` / `handleInput` / `activateSelected` /
  `activateSelectedByIndex` / `executeInstantCommand` / `resetForShow` / `shouldShowResults` /
  `toolSelectionState` / `folderState` / `alwaysOnTop` — 棚卸しの腐敗 23 件のうち **11 件程度**
- **捕まらない**: snake_case（`launch_item` / `show_main_and_emit` / `get_icons_batch` /
  `spawn_blocking` / `hint_instant_program`）・PascalCase（`LruIconCache`）・
  ドット区切り（`notice.launch.timeout`）・式（`results().length > 0`）

**「射程を広げれば M1〜M3 が機構で捕まる」とは書けない。** 書けるのは
**「camelCase で書かれた再発は捕まる」**までである。残りは受容する残余として明記する。

---

## タスク 1: 既に食い違っている 5 件を直す（A 群）

**最も安く実害が消える。** 5 件のうち 3 件が `docs/architecture.md` の 2 行に集中している。

**Files:**
- Modify: `docs/architecture.md`（`:82` 付近と `:110` 付近）
- Modify: `SPEC.md`（§17.1 の履歴キー正規化）

**注意:** A-3（`LaunchResult` の `timeout` ステータス）は **#735 の表にも在る**ため、
**タスク 2 で §14.2 ごと畳む**。ここでは触らない。

- [ ] **Step 1: `docs/architecture.md` の「LRU」を削る（A-1）**

現在「`icons.bin` に **LRU** キャッシュ・遅延ロード」と書かれているが、実装は **FIFO** である。
一次証拠: `src-tauri/src/icon.rs` の `//!`（「挿入順保持により FIFO 退避（最古から pop）」）と
テスト `insert_evicts_oldest_when_over_cap`（`cap=2` に `a,b,c` → **`a` が落ちる**）。`get` は順序を書かない。

**方式名ごと落とす**——分担表は `architecture.md` に「関数名・バイト形式・現在の状態式を持たない」
ことを求めており、そもそも方式名を書く根拠が無い。「件数上限で頭打ち（方式は `src-tauri/src/icon.rs`）」
程度の参照へ倒すこと。

- [ ] **Step 2: `docs/architecture.md` の main 窓高さの式を落とす（A-2）**

算入項が 1 つ欠けている。**式ごと落として**正本（`src-tauri/src/egui_shell/layout.rs` の
`main_window_height`）への参照へ倒す。Step 1 と同じ理由である。

- [ ] **Step 3: `docs/architecture.md` の results 窓高さの式を落とす（A-5）**

#675 の作業領域クランプが落ちている。**式ごと落として** `layout.rs` の
`results_window_height` / `clamp_results_height` への参照へ倒す。

- [ ] **Step 4: `SPEC.md` §17.1 の履歴キー正規化を参照へ倒す（A-4）**

SPEC が関数名を誤り、正規化ステップを 1 つ落としている。**正しい記述は
`snotra-core/CLAUDE.md`「history.rs のキー正規化に関するチェックリスト」が持つ**ので、
そこへの参照に倒すだけでよい（`` `<path>.md`「<見出し>」 `` の正準形で書くこと——
`governance:check` の G-heading-refs が実在を照合する）。

- [ ] **Step 5: 検証**

`npm run governance:check`（`*.md` のみ触るので必須）。`.rs` を触らないなら カテゴリ A は不要。

- [ ] **Step 6: コミット**

```
docs: 文書間で既に食い違っている 4 件を正本への参照へ倒す
```

---

## タスク 2: #735 を実行する（SPEC の as-built 化）

**issue #735 が正本である。** その表の 13 箇所と二層構造 3 節（§19.6 / §19.7 / §14.2）を扱う。

**Files:** `SPEC.md`

- [ ] **Step 1: #735 の「消しすぎの境界」を先に読む**

`gh issue view 735` の当該節を読むこと。**3 点ある**:
1. **`SPEC.md` の 4000ms は半分生きている**——`launch_item_core` 自体に timeout は無いが、
   4 秒は drain 側の `LAUNCH_TIMEOUT`（§19.6 の egui 節が正しく記述）。**丸ごと消すと逆向きの嘘になる**
2. **`alwaysOnTop`（camelCase）は概念が生存**している。Tauri の JSON/JS 側表記であり
   **嘘ではなく表記の揺れ**。優先度最低
3. **Rust コメント内の `resetForShow` は「〜相当 / parity」という由来注記**であり、
   現行 API 名として書かれていない。**SPEC 側の 2 行とは別クラスで、消してはならない**

- [ ] **Step 2: 二層構造 3 節を畳む**

#726 が §20.3 に採った形をそのまま当てる——**古い層を削除し、生存部分は egui 層へ一本化**する。
両層が同じ対象を語っているため、スコープ宣言ではなく削除で解ける。

- [ ] **Step 3: 単独の stale 記述 13 箇所を直す**

#735 の表を 1 行ずつ潰す。**表の「反証」欄が正本の位置を持っている**ので、そこへ参照を倒すこと。

- [ ] **Step 4: 棚卸しが追加で見つけた分を合流させる**

`.superpowers/sdd/plan/spec-inventory-identifiers.md` §3 の 23 件のうち、#735 の表に無いものを拾う
（`about` / `settings` のウィンドウラベル・`LruIconCache` / `get_icons_batch`・
`LaunchResult::succeeded`・`show_main_and_emit`・`notice.launch.timeout`・`hint_instant_program`）。
**各件に「提案」欄がある**ので、それに従うこと。

- [ ] **Step 5: 検証**

`npm run governance:check`。**加えて `git grep` で、直した識別子がコードに実在するか
（または参照へ倒れたか）を全件確認すること**——直しながら別の腐りを作らないため。

- [ ] **Step 6: コミット**

```
docs: SPEC の WebView2 期 stale 記述を as-built へ畳む (#735)
```

---

## タスク 3: 射程を広げたときの finding を実測する

**変更の前に測る。** これは `AGENTS.md`「検証の作法」——判定の中核は自分で測る——の適用である。

**Files:** 変更なし（測定のみ）

- [ ] **Step 1: 語彙から SPEC.md を外したときの finding を測る**

`scripts/governance-check.mjs` の `VOCAB_DOCS = ["SPEC.md"]` を**一時的に**空にして実行し、
finding の件数と内訳を記録する。**稼働中のガードを弱めないこと**——
`.claude/rules/safety-nets.md` に従い、**複製に変異を当てる**（作業ツリーを汚さない方法で測る）。

- [ ] **Step 2: SPEC.md を検査対象に加えたときの finding を測る**

`staleIdentifierDocs` の母集団に `SPEC.md` を加えた場合を、同じ要領で測る。

- [ ] **Step 3: 真の腐り / 偽陽性を分類する**

ADR が採った方法と同じである（同 ADR の表: 述語ごとに finding / 真の腐り / 偽陽性を並べた）。
**偽陽性が出るなら、ADR 却下 2 の判断（免除注記の機構は設けない・行の形で外す）に従うこと。**

- [ ] **Step 4: 測定結果を報告する**

**この時点で判断が要る。** 偽陽性が多ければ射程拡大は見送り、その事実を ADR へ追記する
（それも成果である）。**測定結果を見ずにタスク 4 へ進んではならない。**

---

## タスク 4: 検出器の射程を広げる（測定が支持した場合のみ）

**タスク 3 の測定が「偽陽性が受容可能」を示したときだけ実行する。**

**Files:**
- Modify: `scripts/governance-check.mjs`（`VOCAB_DOCS` / `staleIdentifierDocs` と、その節の doc コメント）
- Modify: `docs/adr/ADR-stale-identifier-detector-scope.md`（却下 4 の失効を追記）

- [ ] **Step 1: 射程を変える**

測定が支持した形（語彙から外す / 母集団に加える / 両方）を実装する。
**節の doc コメントの自称スコープを同時に直すこと**——`governance-check.mjs` 冒頭の契約が
「自称スコープを明記する」を求めている。

- [ ] **Step 2: 受容する残余を明記する**

**`STALE_IDENT` は camelCase しか見ない。** snake_case・PascalCase・ドット区切り・式で
書かれた腐りは対象外である。**「SPEC の腐りが機構で捕まるようになった」とは書かないこと**——
書けるのは「**camelCase で書かれた再発は捕まる**」までである。

- [ ] **Step 3: ADR へ却下 4 の失効を追記する**

**原文は残し、失効注記を append する**（ADR の標準的な supersession の形。
直前のサイクルで `ADR-window-coordinator-split-rule` に同じ形を採っている）。
書く内容: 暫定措置だったこと・#735 の完了で前提が変わったこと・新しい射程・残る残余。

- [ ] **Step 4: フォールトインジェクションで検知を実測する（省略不可）**

`.claude/rules/safety-nets.md`——「**効いていることは、フォールトインジェクションで一度は実測する**」。
**稼働中のガードを弱めず、複製に変異を当てる**こと。
`SPEC.md` の複製へ現行語彙に無い camelCase 識別子を仕込み、finding が出ることを確認する。
**逆方向も見る**——正当な識別子で偽陽性が出ないこと。

- [ ] **Step 5: 検証**

`npm run governance:check`（全検査 passed）+ `npm test`（`governance-check` のユニットテストが在れば）。
**`rules N/9200` の実数を報告に含めること。**

- [ ] **Step 6: コミット**

```
feat(governance): SPEC.md を腐り検出器の射程へ入れる
```

---

## タスク 5: 記法の設計を issue へ切る

**この計画では実装しない。** 射程が大きく、`governance:check` の検査を新設する話になる。

**Files:** 変更なし（issue 起票のみ）

- [ ] **Step 1: issue を起票する**

タイトル案: 「文書の腐りを防ぐ記法を定める（コード参照の正準形・歴史記述の印・値の正本の印）」

本文に含めること（**棚卸しの実データを根拠として引くこと**）:

- **N1. コードへの参照に正準形が無い。** 見出し参照には `` `<path>.md`「<見出し>」 `` が在り
  G-heading-refs が実在を照合している（193 件）。**コードへの参照には対応物が無く**、
  自由記法ゆえ「参照」と「散文中の語」を機械が区別できない
- **N2. 歴史記述の印が無い。** 過去形は人間には読めるが機械には読めない。
  直前のサイクルで「生きた参照 vs 歴史記述」の線引きが**毎回人手の判断**になった実績がある
- **N3. 値の正本の印が無い。** 43px が 6 か所・アイコン上限の導出式が 5 か所に在るが、
  どれが正本か書かれていない
- **機序の分類**（M1〜M6）と、**機構で捕まる範囲と捕まらない範囲の境界**
- **根拠データ**: 棚卸し 2 本（441 スパン分類・重複 28 件）。**`.superpowers/` は gitignore ゆえ
  消えうる**——issue 本文に要点を写すか、追跡下へ移すこと

---

## タスク 6: 仕上げ

- [ ] **Step 1: 全検証**

`npm run governance:check` / `npm test` / `.rs` を触ったなら `docs/build-commands.md` カテゴリ A。
**実機（カテゴリ C / D）は本計画の射程外**（文書と検査スクリプトしか触らない）。

- [ ] **Step 2: PR を作成する**

`git push -u origin HEAD` を先に打つか `&&` で繋ぐ。**鎖に `cd` を含めない**。
PR 本文に**必ず書くこと**:
- **タスク 3 の測定結果**（射程拡大を採ったか見送ったか、その数値）
- **受容する残余**——camelCase 以外の腐りは検出器の対象外であること
- #735 を close すること（`closingIssuesReferences` はマージ前に確認する・`/merge-pr` の手順）
