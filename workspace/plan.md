# 計画: issue #1429 — manual-smoke.ps1 の -Only に複数の番号を渡せるようにする

## 目的

`npm run smoke:manual '--' -Only 2,5,10`（使い方コメントと `docs/build-commands.md` に書いた形）で、Bash からでも PowerShell からでも複数の項目を指定できるようにする。数として読めない値は黙って化けさせず、throw する。

判定: バグ（文書に書いた使い方へコードを合わせる）。SPEC.md は変えない。

## 受け入れ条件

1. 次の 4 経路で、`-Only` が id 列 `2, 5, 10` になる（`research.md`「実測」）
   - Bash → npm → `pwsh -File`: argv `'2,5,10'`
   - PowerShell → npm.ps1 → `pwsh -File`: argv `'2 5 10'`
   - PowerShell 直接 `-Only 2,5,10`（`@('2','5','10')`）
   - PowerShell 直接 `-Only '2,5,10'`
2. 数として読めないトークンは throw する。少なくとも `2.5` / `0x10` / `1e1` / `-1` / `abc` / 全角 `１` / 桁あふれ `99999999999`。`[int]` キャストは `2.5`→2・`0x10`→16・`1e1`→10・`-1`→-1 を黙って通す（2026-11-15 実測）ので、キャストに検証を任せない
3. 区切りだけで番号が 1 つも無い値（`''`・`','`）は throw する
4. 空トークン（`2,,5`・末尾カンマ）は読み飛ばす。重複は 1 つにまとめる。先頭ゼロ（`02`）は 2
5. `-Only` を省略したときは、従来どおり全 13 項目を実施する
6. `[CmdletBinding(PositionalBinding = $false)]` を保ち、#1296 の Describe が緑のまま
7. 有効な id に無い番号が 1 つでも含まれれば throw する（例 `-Only 2,99`。従来は 2 だけを黙って実施していた）。メッセージに一致しない番号と有効な id の一覧を含める
8. 1・2・6 を Pester（`npm run test:powershell`）で固定する。param を `[int[]]` に戻したら赤になる
9. `-Only 1,3` が `13`（実在 id）に化けて別の項目が黙って走る現象（3b で発見・主が再測定）が起きない。束縛の It に argv `'1,3'` → `1,3` を入れて固定する

## 変更するファイルと対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `scripts/lib/SnotraSmoke.psm1` | 新関数 `ConvertTo-SnotraSmokeItemIds`、`Export-ModuleMember` | `[string[]]` を受け、各要素を `[,\s]+` で分割し、空トークンを捨て、各トークンを `^[0-9]+$` で検証してから `[int]::TryParse` で変換する。重複を除いた `int[]` を返す。必須引数 `-ValidIds [int[]]` に無い番号が 1 つでもあれば throw。0 件・不正トークン・桁あふれ・不一致で throw（メッセージに不正な値と有効な id の一覧を含める） |
| `scripts/manual-smoke.ps1` | param `-Only`（9–10 行）、使い方コメント（43 行）、絞り込み（184–187 行） | `[int[]]` → `[string[]]`。コメントに理由を 1 行（`pwsh -File` は配列を 1 文字列で渡し、npm.ps1 は空白区切りで渡す・#1429）。絞り込みは `$PSBoundParameters.ContainsKey('Only')` のときだけ `ConvertTo-SnotraSmokeItemIds -Value $Only -ValidIds $allIds` の結果（int 列）で行い、`$Only` の文字列配列を直接 `-contains` に渡さない（`'02'` が当たらない）。全件不一致の throw（186 行）は関数の不一致判定に含まれるので消す |
| `scripts/lib/SnotraSmoke.Tests.ps1` | 新 `Describe 'ConvertTo-SnotraSmokeItemIds（#1429）'`、#1296 の Describe の `New-SnotraParamOnlyCopy` | 純関数の表駆動テスト（受け入れ 1〜4）。`New-SnotraParamOnlyCopy` に省略可能な `-Body` を足し（既定は現行の `'BODY-RAN'`）、manual-smoke.ps1 の param() の複製を `pwsh -NoProfile -File` で起動し（本体は `SnotraSmoke.psm1` を絶対パスで import して変換結果を `-join ','` で出す）、argv `'2,5,10'`→`2,5,10`・`'2 5 10'`→`2,5,10`・`'1,3'`→`1,3` を見る（束縛から id 列まで） |
| `docs/build-commands.md` | 95 行の `-Only 2,5` の例 | 直した後に真であることを読んで確かめる（書いた形が Bash・PowerShell の両方で動くようになるので、文言の変更は不要の見込み） |

触らない: `visual-input-metrics.ps1` の `-Fonts`（`;` 区切り）。共通化しない理由: フォント名は空白を含むので空白で分割できず、`-Only` は空白でも分割しなければならない。片方だけ変わる将来が既にある（区切り文字の集合が違う）別の概念である。`package.json`・SPEC.md も触らない。

## 実装の順序

1. Red: `ConvertTo-SnotraSmokeItemIds` の Describe と、束縛の It を書き、赤を確かめる（関数が無い／param が `[int[]]` で `2510`・変換エラー）
2. Green: 関数を `SnotraSmoke.psm1` に足し、export する
3. Green: `manual-smoke.ps1` の param と絞り込みを直す
4. コメントと文書を確かめる
5. 検証（下記）

## 不変条件と異常系

- 位置引数の拒否（#1296）を保つ。param の属性は `[string[]]` の型以外を変えない
- 不正な入力は、項目の読み上げやアプリの起動より前に throw する（絞り込みは起動前の位置にある・184 行で現状どおり）
- throw のメッセージは、どの値が不正かと、有効な id の一覧（`$allIds`）を含める

## テスト方針と検証コマンド

- `npm run test:powershell`（全件緑）
- 変異注入は `/implement` の委譲先が行う: (a) param を `[int[]]` に戻す → 束縛の It が赤、(b) `^[0-9]+$` の検証を消してキャストだけにする → `2.5` 等の It が赤、(c) 空白を区切りから外す → `'2 5 10'` の It が赤
- `npm run governance:check`（`docs/build-commands.md` を触った場合）
- カテゴリ D（`smoke:manual`）は対象外: 表示を変えない。ただし実機で `-Only` を使った確認は人間に依頼できる（任意）

## SPEC.md・関連文書の更新の要否

- SPEC.md: 不要（`-Only` の記述が無い）
- `docs/build-commands.md`: 例の文言は直した後も真。変更不要の見込み（実装時に読み直す）

## 作業項目

### Phase 1 — Red

- [x] `SnotraSmoke.Tests.ps1` に `ConvertTo-SnotraSmokeItemIds` の表駆動テストを足す（受け入れ 1〜4・7）
- [x] `New-SnotraParamOnlyCopy` に `-Body` を足し、束縛の It（`'2,5,10'` / `'2 5 10'` / `'1,3'`）を足す
- [x] `npm run test:powershell` で、足したテストが期待どおり赤になることを確かめる

### Phase 2 — Green

- [x] `SnotraSmoke.psm1` に `ConvertTo-SnotraSmokeItemIds` を足し、`Export-ModuleMember` に載せる
- [x] `manual-smoke.ps1` の `-Only` を `[string[]]` にし、絞り込みを変換後の int 列で行う
- [x] `manual-smoke.ps1` の param コメントに理由を足す（使い方コメントは直した後も真なので変更不要と確かめた）

### Phase 2b — 委譲レビュー対応（計画外・ラウンド 1 の M1）

- [x] 絞り込みを純関数 `Select-SnotraSmokeItems`（`SnotraSmoke.psm1`）へ出し、`manual-smoke.ps1` から `$allIds` と生の `-contains` を消す。束縛の It を同関数経由にし、同関数の Describe を足す（スクリプト内の絞り込みで生の `$Only` を使う変異が緑で残ったため）

### Phase 3 — 検証

- [ ] `npm run test:powershell` が全件緑
- [x] `docs/build-commands.md` の `-Only` の例が真であることを読んで確かめる

### ラウンド記録

- R1 sha 23f3d3a9 / ツール: exit 2（未実行——linked worktree から打てず、本体の HEAD も sha でない）/ 指摘 Critical・High 0・Medium 1（M1）・Low 2 / 変異 10 本中 生存 2（(f) 生の `$Only` で絞り込み・(g) `ContainsKey` → `Count -gt 0`）。(g) の挙動差は `-Only @()` を明示したときだけで、`pwsh -File` の経路では作れない——受容する残余
- R2 sha a0d89e42 / ツール: exit 2（未実行・同上）/ 指摘 Critical・High・Medium 0・Low 1（L3: doc の経緯の括弧書き → 削除）/ 変異 9 本中 生存 2（(f2)(f3) スクリプト側の 1 行の呼び出しを書き換える／消す）。param() の複製はスクリプト本体を通らないので検知できない。論理は 1 行の呼び出しまで縮み、消せば全項目が走って目に見える——受容する残余として PR 本文に書く

## 未確定（実装前に潰す）

- [x] 一部だけ一致しない id（`-Only 2,99`）を throw にするか、現状どおり黙って一致した分だけ実施するか — 2026-11-15 ユーザー裁定で throw。判定を `ConvertTo-SnotraSmokeItemIds -ValidIds` に持たせ、Pester で固定する（受け入れ 7・変更表に反映）

## 人間レビュー

- [x] 承認済み — 2026-11-15 / 問い: "計画を承認しますか？（-Only 2,99 のように一部だけ一致しない id を throw にするかも含めて）" / 回答: "throw で承認"

## セルフレビュー

- リスク: 通常（`/plan-review`「リスク判定」の各条件に当たらない。`scripts/**` はセーフティネットの範囲だが、変えるのは手動 smoke の引数解釈と Pester の追加で、hook・CI 設定・rules・skills・ガバナンス文書・公開 API・状態遷移のいずれでもない。マージはセーフティネットの規則に従いユーザーと合意してから）
- plan-review: 未実施（通常リスク）
- エージェント数: 0（3b の 1 体は別勘定）
- 要対処: 0 件。自己照合の結果——(1) issue の 3 要件（複数指定・非数で throw・Pester で固定）に作業項目が対応 (2) 境界は受け入れ 2〜5・9 に列挙し、各々テストあり (3) 新しい状態・リソース・プロセスなし (4) より単純な既存パターン: `-Fonts` の `;` 分割と同型の「1 文字列で受けて分割」を採った。共通化はしない（理由は「触らない」） (5) 壊してはならない #1296 の位置引数拒否は既存 Describe が検知する。`/dry-check` 相当: `scripts/` に番号列を分割・解析する既存関数は無い（`-split` の出現は `-Fonts` の `;` のみ）
- 未検証: Windows PowerShell 5.1 をホストにした npm.ps1 の渡し方（未測。実装が `,` と空白の両方で分割するので、どちらで渡しても受かる）
- 散文の全件列挙: 未実施（通常リスク）
