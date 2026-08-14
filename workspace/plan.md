# plan — #1085: Claude Code の RA 診断をどう扱うか、実効設定を実測してから決める

## 目的

issue が「まず測ること」として挙げた 4 点を実測し、その結果を根拠に**2 層の抑制を採るか採らないかを
決める**。決定と、決定を支える実測を、次に同じ問いが来たときに再測定しなくて済む形で残す。

さらに、実測の副産物として見つかった**検知の穴（足 2）を機構で塞ぐ**——ユーザー判断により
本 issue の範囲に取り込んだ。

**製品コードは 1 行も変えない。** 変えるのは開発環境の記録と、`governance:check` の検査 1 枚である。

## 決定（実施済み・2026-08-14）

- **抑制はどちらの層も採らない。** `.lsp.json` に `diagnostics` キーを足さず、
  RA 側の `diagnostics.enable` も設定しない。根拠は下の実測 3 点。
- **足 2（索引には載るが `mod` 宣言が無い `.rs`）を `governance:check` の検査 1 枚で塞ぐ。**

## 実測の結果（`workspace/research.md` が正本・要約のみ再掲）

| issue の「まず測ること」 | 結果 |
|---|---|
| 挙動プローブ 1（`checkOnSave=false`） | **合格**。A/B（我々の設定 / RA 既定）を反復して対照 |
| 挙動プローブ 2（入れ子 `initializationOptions`） | **合格**（`limit=512` は独立には未証明） |
| 実セッションでの `<new-diagnostics>` | **構文エラーしか届かない**。正常な編集では 0 件。底値 96 件は LSP に存在しない |
| `diagnostics: false` で navigation が生きるか | **測らない**。抑制を採らない決定により不要になった |

決定の根拠は 3 点。(1) 減らせる量が無い。(2) 抑制すると cargo が見ないファイルの構文エラーを失う
（実測 2 件）。(3) 「`unlinked-file` を失う」という先送りの根拠は空だった（採否に関わらず届いていない）。

## 受け入れ条件

- [x] issue の「まず測ること」4 点それぞれに、実測値か「測らない理由」が記録に残っている
      （ADR「決定を支える実測」が正本。4 点目は「抑制を採らない決定により不要」と記録）
- [x] issue の「決めること」2 点に結論が付き、根拠が実測を指している
- [x] 決定と否定の知識が `docs/adr/` に残る（`ADR-ra-diagnostics-suppression`・却下 8 件）
- [x] 新しい検査が**足 2 の変異で赤くなる**（カナリア + 実ファイルでの e2e）
- [x] 新しい検査が**現状のリポジトリで緑**（誤検出 0。`#[path]` と `mod.rs` を含む。
      本物のリポジトリで 95/95 到達、フィクスチャでも両者を個別に固定）
- [x] 新しい検査が**「検査を殺す変異」で赤くなる**——`checkModuleLinkage` を `return []` へ
      無力化すると**赤いフィクスチャ 4 本が落ちた**（緑 4 本は no-op でも通る＝縛らない。構造上正しい）
- [x] 新しい検査が**実際に配線されている**——実ファイルへ足 2 の変異を 1 回当て、
      `npm run governance:check` の出力に findings が現れることを確認（検査 19 → 20 件）
- [x] `npm run governance:check` と `npm test`（`governance-check.test.mjs` を含む）が緑
      （20 検査 passed / 8 ファイル 663 件 passed。訂正の反映後に再実行）
- [x] メモリ `ra-diagnostics-noise-is-baseline-not-edits` が LSP 側の実態へ更新されている

## 変更ファイル一覧と対象シンボル

| ファイル | 変更内容 |
|---|---|
| `scripts/governance-check.mjs` | `G-module-linkage` を新設。`checkModuleLinkage(snapshot)` を export し、検査一覧（1857 行〜の配列）へ登録する。crate の母集団は既存の `workspaceMembers(snapshot)`（307 行・**導出する唯一の口**）から取る |
| `scripts/governance-check.test.mjs` | `checkModuleLinkage` のカナリア（足 2 の変異・誤検出 0・検査を殺す変異の 3 方向） |
| `docs/adr/ADR-ra-diagnostics-suppression.md`（新規） | 決定と却下理由。前段の `ADR-claude-code-ra-lsp-plugin-delivery` を短縮引用する（**旧 ADR は編集しない**） |
| `docs/hooks.md`「Claude Code の RA インスタンスと hook の分担」 | 「何が届くか」を 1 段落。ADR を正準形で参照する |
| `AGENTS.md`「条件別チェック（トリガー → 参照先）」 | 「ファイル（`.rs`）を追加/削除」の行へ、`mod` 宣言も機構が見るようになったことを反映する |
| `C:/Users/Eoh/.claude/projects/C--workspace-Snotra/memory/ra-diagnostics-noise-is-baseline-not-edits.md` | LSP 側の実測で更新（リポジトリ外・git 管理外） |
| `workspace/research.md` / `workspace/plan.md` | 本サイクルの成果物 |

**新規 ADR の形は `G-adr-file-names` が機械で縛る**: ファイル名 `ADR-<slug>.md`、本文 1 行目
`# ADR-<slug>: <題>` が stem と一致。連番を振らない（`.claude/rules/governance-docs.md`・#812）。
引用されない ADR は落ちない（`G-adr-citations` は引用側しか見ない）。

### 写しを作らない分担（`AGENTS.md`「文書に事実の写しを増やす変更」）

**同じ事実を 3 か所へ書こうとしている**ので、先に正本を決める。

| 置き場所 | 持つもの | 正本か |
|---|---|---|
| `docs/adr/ADR-ra-diagnostics-suppression.md` | 決定・実測の要約・却下理由・受容する残余 | **正本**（恒久） |
| `docs/hooks.md` の分担節 | 「何が届くか」という**帰結**の 1 段落 + ADR への正準形参照 | 参照 |
| `AGENTS.md` の条件別チェック表 | トリガー → 参照先の 1 行だけ | 参照 |
| メモリ | **数値を写さず**、CLI だけで測った当時の記述を正して正本を指す | 参照 |
| `workspace/research.md` | 生の実測（測り方・治具・敵対枠の採否） | **正本にしない**（次サイクルで上書きされる） |

**メモリへ実測値を再掲しない。** リポジトリが記録するようになった事実をメモリへ写すと、
`docs/adr/` を直したときに腐る側が 1 枚増える。

## 実装順序

1. `G-module-linkage` を `governance-check.mjs` に実装する（判定は試作で検証済み・下記）
2. `governance-check.test.mjs` にカナリアを書き、3 方向の変異で測る
3. ADR を書く（決定・実測の要約・却下理由・受容する残余）
4. `docs/hooks.md` の分担節と `AGENTS.md` の条件別チェック表を更新する
5. メモリを更新する
6. `npm run governance:check` と `npm test` を実行する

**1 → 2 の順は入れ替えない**——検査を書いてから変異で測る（`.claude/rules/safety-nets.md`
「効いていることは、フォールトインジェクションで一度は実測する」）。

## 判定式（試作で検証済み・`research.md` M5）

crate ルート（`src/lib.rs` / `src/main.rs`）から `mod` 宣言を辿り、`<crate>/src/**/*.rs` のうち
到達しないものを findings にする。

- `mod x;` の解決先は、宣言元が `lib.rs` / `main.rs` / `mod.rs` なら同じディレクトリの
  `x.rs` または `x/mod.rs`、そうでなければ `<宣言元の stem>/x.rs` または `<stem>/x/mod.rs`
- `#[path = "..."] mod n;` は宣言元のディレクトリからの相対で解決する（**実在する**——
  `snotra-egui-runtime/src/ime.rs` の `#[cfg(windows)] #[path = "windows_ime.rs"] mod platform;`）
- `#[cfg(...)]` は**無視して和を取る**（どの cfg であれ宣言されていれば「所有されている」）
- インライン `mod x { ... }` の中の `mod y;` は追わない（**現状 0 件**であることを実装時に
  grep で確かめ、在れば判定へ足す）

**実測**: 現状 95/95 到達・誤検出 0。変異 2 種（新規ファイルの `mod` 忘れ / 既存 `mod` の削除）で赤。

## 不変条件と異常系

- **fail-closed に倒す。** crate 一覧が空・`src/` が読めない・ルート（`lib.rs` / `main.rs`）が
  1 つも見つからない場合は**緑を返さず finding を積む**。「検査を殺す変異」は変異を足す側から
  原理的に見えないため、設計の側で塞ぐ（`ADR-claude-code-ra-lsp-plugin-delivery` で 14 中 3 件の実例）。
- **`G-module-index` の保証を薄めない。** 新しい検査は足 2 だけを足すものであり、
  足 1（索引の欠落）は引き続き `G-module-index` が持つ。**責務を移さない。**
- **crate の母集団は `workspaceMembers(snapshot)` から取る**（`governance-check.mjs:307`・
  ルート `Cargo.toml` の `[workspace] members` を導出する**唯一の口**で、返り値の `error` が
  fail-closed を既に担っている——読めない・節が無い・0 件・glob 要素はすべて母集団の欠落として扱う）。
  **`MODULE_INDEX_CRATES` を再利用しない**（当初案から変更）。あれは同じ members の写しであり、
  母集団カナリア（`governance-check.test.mjs:92-115`）が縛るのは
  **`CLAUDE.md` を持つ member だけ**である（実読して確認）。`CLAUDE.md` を持たない crate を
  新設すると、写しを使う検査はその crate を黙って飛ばす。**リンク性の検査は `CLAUDE.md` の
  有無と無関係**なので、写しではなく口を使う。
- **検査の一覧を散文へ写さない。** 一覧の SSOT は `governance-check.mjs` のコメント見出しであり、
  `docs/build-commands.md` が「ここに範囲で写すと黙って腐る」と明言している（実際
  「G-module-index〜G-config-reachability」と書いたまま増えて腐った・#812）。
  **`docs/build-commands.md` と `AGENTS.md` の既存の列挙に新しい検査名を足さない。**
- **全称表現を作らない。** 「構文エラーしか届かない」は測った 4 種の変異の範囲での観測である。
  記録には測った範囲を添える。
- **`unlinked-file` が出ない機序を書かない。** 分かっているのは「出ていない」ことと
  「我々の設定が原因ではない」ことの 2 つだけである。

## テスト方針と検証コマンド

- `npm run governance:check`（`docs/build-commands.md` カテゴリ F）
- `npm test`（`governance-check.test.mjs` のカナリアを含む）
カナリアは `governance-check.mjs` 冒頭の契約が定める形に揃える（逐語:
「フィクスチャでフォールトインジェクション red / 正常 green / **判定対象外の不混入**を検証する」）。
**複製に変異を当てる**——`snapshot` を差し替えてメモリ上で行い、稼働中のガードは弱めない
（`.claude/rules/safety-nets.md`）。

| # | 変異 | 期待 | 何を守るか |
|---|---|---|---|
| 1 | 足 2 の姿: 索引には載るが `mod` 宣言が無い `.rs` | **赤** | 実際の回帰の姿 |
| 2 | 現状のスナップショット | **緑** | 誤検出なし（`#[path]` / `mod.rs` を含む） |
| 3 | `tests/*.rs`・`build.rs`・`src/` 外の `.rs` | **緑** | **判定対象外の不混入**（cargo が自動発見する target は `mod` 宣言を要しない） |
| 4 | `workspaceMembers` が `error` を返す形（`Cargo.toml` 不読・members 0 件） | **緑を返さない** | 検査を殺す変異（fail-closed） |
| 5 | crate の `src/` にルート（`lib.rs`・`main.rs`）が 1 つも無い | **緑を返さない** | 同上（母集団は在るのに探索が始まらない形） |

**3 は誤爆の側の検算である。** `snotra-core/tests/*.rs` 4 枚と `build.rs` 2 枚が現に存在し、
これらは `mod` 宣言を持たないまま正当である。母集団を `<crate>/src/` に閉じることで外れるが、
**外れていることを測る**（`.claude/rules/safety-nets.md`「検査の入力集合を、具体対象で検算する」の
両方向）。

Rust 側の検証は不要（製品コードを触らない）。`.md` の編集に PostToolUse hook は検査を
割り当てないため、`governance:check` は**手で実行する**。
`scripts/*.mjs` の編集では `hook-selftest` が自動発火する（沈黙＝合格）。

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**。製品の挙動・フロー・状態遷移を変えない。
- `AGENTS.md`: **要**（上表）。条件別チェック表の該当行を、機構が増えた事実へ合わせる。
- ルート `CLAUDE.md`: **不要**（見込み）。診断の扱いは `docs/hooks.md` が正本のままである。

## フェーズごとの作業項目

### Phase 1 — 検知器（足 2 を塞ぐ）

- [x] `scripts/governance-check.mjs` に `checkModuleLinkage` を実装し、`G-module-linkage` として登録する
- [x] インライン `mod x { ... }` の中に `mod y;` が無いことを grep で確かめる（**0 件**——
      インデントされた `mod \w+;` が 4 crate の `src/` に 1 件も無い。判定へ足さない）
- [x] `scripts/governance-check.test.mjs` にカナリアを書き、5 方向の変異で赤/緑を実測する
      （8 本すべて緑。**検査を無力化する変異では赤 4 本だけが落ちた**＝赤いフィクスチャが
      検査を縛っている。緑の 4 本は no-op でも通るが、役割は誤検出の防止であり正しい構造）
- [x] **配線を end-to-end で確認**（vitest は純関数を見るが登録は見ない）——実ファイルへ足 2 の
      変異を当て、`npm run governance:check` の出力に findings が現れることを確認。検査 19 → 20 件

### Phase 2 — 記録

- [x] `docs/adr/ADR-ra-diagnostics-suppression.md` を書く（決定・実測の要約・却下理由・受容する残余）
- [x] `docs/hooks.md`「Claude Code の RA インスタンスと hook の分担」へ「何が届くか」を足し、
      ADR を正準形で参照する
- [x] `AGENTS.md`「条件別チェック（トリガー → 参照先）」の `.rs` 追加/削除の行を更新する。
      **書くのは帰結だけ**（「`mod` 宣言の有無も機構が見る」）で、機構の実装の詳細
      （母集団・述語・件数）を写さない（`.claude/rules/governance-docs.md`）。
      **全称表現にしない**——「全 `.rs` の `mod` 漏れを検知」とは書けない
      （母集団はルート `Cargo.toml` の members に載る crate の `src/` に限る）
- [x] メモリ `ra-diagnostics-noise-is-baseline-not-edits` を更新する（数値を写さず ADR を指す形へ。
      あわせて **stdio クライアントという再測定の手段**を残した——リポジトリに残らないため）

### Phase 3 — 検証

- [x] `npm run governance:check` を実行して緑を確認する（検査 20 件 / ADR 50 本 / 見出し参照 209 件）
- [x] `npm test` を実行して緑を確認する（8 ファイル・663 件）
- [x] 書いた記述に実装より強い主張（全称表現・未測定の機序）が無いか `research.md` と突き合わせる
      —— **1 件見つかって直した**。「未リンクの `.rs` は rust-analyzer からも見えない」と 3 か所へ
      書いていたが、実測は逆で **RA は当該ファイルの構文エラーを届けている**。見えないのは
      `mod` 忘れという事実のほうなので「cargo も rust-analyzer も**報せない**」へ改めた
      （`AGENTS.md` / `governance-check.mjs` のコメントと finding 文言）。
      あわせて `docs/hooks.md` の「構文エラーだけ」に**測った 4 種の変異**という前提を添えた

## 未確定（実装前に潰す）

（なし）

## plan-review 結果

- リスク: **高**（セーフティネット＝`governance:check` の検査を新設する／網羅性が要件）
- レビュー方式: 計画準拠レビュー 1 体（Step 2）。Step 2b は採らなかった——
  **issue の WHAT に検知器が含まれない**ため、独立導出は原理的にこの範囲へ到達しない
  （検知器はユーザー判断で本 issue へ取り込んだもの）
- エージェント数: 2（3b の敵対的調査 1 体 + 本レビュー 1 体）

### 要対処

- （なし。実ファイル検算で `mod.rs` 5 枚・`#[path]` 1 件・`tests/*.rs` 4 枚・`build.rs` 2 枚・
  `src/bin/` 0・proc-macro 0・`include!` 0・インライン `mod` 内の `mod y;` 0 を確認し、
  95/95 到達と誤爆なしが独立に再現された）

### 軽微（2 件・どちらも反映済み）

- `AGENTS.md` の更新文言が未確定 → **帰結のみ／全称表現にしない**制約を作業項目へ明記した
- ADR 短縮引用 `ADR-ra-diagnostics-suppression` が `ADR_CITATION` 正規表現に適合することを確認（対処不要）

### 主エージェントの再照合で見つけた 1 件（レビューの指摘ではない）

- **母集団の取り方を変更した。** レビューは `MODULE_INDEX_CRATES` の再利用を追認したが、
  その母集団カナリアを自分で読んだところ、縛っているのは**`CLAUDE.md` を持つ member だけ**だった
  （`governance-check.test.mjs:92-115`）。`CLAUDE.md` を持たない crate は黙って飛ばされる。
  `workspaceMembers(snapshot)`（導出する唯一の口・fail-closed 済み）へ変更した。

### 未検証

- 判定式の実装コードそのもの（試作は scratchpad にあり、実装は Phase 1）
- `docs/hooks.md` への追記文言の具体案（実装時に書く）

## セルフレビュー（5a の自己照合）

1. **issue の全要件に作業項目が対応するか** — 「まず測ること」4 点は実測済み（`research.md`）。
   4 点目（navigation）は「抑制を採らない」決定により不要になり、その理由を記録する。
   「決めること」2 点はユーザー承認で決着し、2 点目（代替検知）が Phase 1 になった。
2. **境界条件と検証** — 検知器の境界は「母集団に入るもの／入らないもの」に集約され、
   カナリア 5 本が各境界に対応する（足 2・誤検出なし・不混入・母集団の欠落・ルート不在）。
3. **新しい状態・リソース・プロセス** — 新設しない。計測用の治具は scratchpad に置き、
   リポジトリへ入れない（撤去条件を持たせる必要が無い）。計測用の `.rs` は削除済みで、
   作業ツリーは `workspace/` 以外 clean であることを確認済み。
4. **より単純な既存パターンで置き換えられないか** — 既存 ADR への追記は却下（凍結された歴史）。
   新しい検査 id ではなく既存検査の拡張で済ませる案も検討したが、`G-module-index` は
   索引 ↔ ファイルの照合であり `mod` 到達性と**責務が違う**（レビューがコード読解で追認）。
5. **壊してはならない不変条件に検知手段があるか** — `G-module-index` の保証（足 1）は
   そのまま残り、カナリアが縛る。`RATOML_FORBIDDEN` の縛りは触らない。

## 人間レビュー

- [x] 承認済み — 2026-08-14 / 問い: "`workspace/plan.md` の**未確定欄は空**です。次のどちらかをお願いします。1. `workspace/plan.md` へ注釈を追加する（反映します） 2. **計画を明示的に承認する**" / 回答: "承認"

先立って、issue の「決めること」2 点も明示の回答で決着している。

- 問い: "実測を踏まえて、診断の抑制はどうしますか（issue の「決めること」1 番目）。`.claude/lsp/**` はセーフティネットなので、合意なしには変更しません。" / 回答: "どちらも採らない（推奨）"
- 問い: "実測で「`mod` 忘れ（未リンク `.rs`）を検知する手段が現在どこにも無い」ことが分かりました（cargo からも LSP からも見えず、規範のみ）。抑制の採否とは独立した穴で、本 issue の範囲外です。どう扱いますか。" / 回答: "この issue の範囲に取り込む"
