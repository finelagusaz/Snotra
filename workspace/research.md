# research: #953 `check:colors` の main 窓判定が構造的に赤である

## issue の要約

`npm run check:colors` の **main 窓の判定は、どんな背景色を指定しても赤になる**。判定は窓全体の
**最頻色**が期待色と一致するかで決まるが、その前提（「背景は窓で最も広い色」）が main 窓では偽で
ある——入力欄が過半を占める。results 窓の判定は正しく動く。

**永久に赤いゲートはゲートが無いのと機能的に同じ**であり、正しく動いている results 側の判定まで
道連れになる。

射程は**背景 + 入力欄の 2 色を期待する**形（ユーザー裁定・2026-08-06）。選択色・hint 色の計器化は
含めない。

## 測定（#949 の前後 2 回・同一）

```
main 窓 752x56（総画素 42112）
  #383838（入力欄）      18159 px  43.1%   ← 最頻色
  #4A2B5C（指定した背景）14044 px  33.3%
  #373737（入力欄の縁）   4776 px  11.3%
  #5A6169                 1388 px   3.3%
  #C0DEFF                 1359 px   3.2%

results 窓 752x369
  #4A2B5C  76.1%  ← 最頻色（期待どおり）
```

`#383838` は `snotra-core/src/config.rs:377` の `default_input_background_color()` の値。
**入力欄の色は正しく届いている**（色が届いていないのではない）。

## 関連ファイル・モジュール・関数（すべて grep / Read で実在確認済み）

| 対象 | 位置 | 役割 |
|---|---|---|
| `Measure-WindowBackground` | `scripts/visual-check-colors.ps1:145-185` | **判定の実体**。Bitmap 全画素 → 最頻色 → `Ok = (mode -eq ExpectedKey)` |
| main の判定呼び出し | 同 `:239-240` | `-Label 'main'` |
| results の判定呼び出し | 同 `:255-256` | `-Label 'results'`・**同じ期待色**を渡す |
| 総合判定 | 同 `:283` | `$succeeded = $seedHealthy -and $profileWritten -and $mainResult.Ok -and $resultsResult.Ok` |
| exit code | 同 `:305` | `if (-not $succeeded) { exit 1 }` |
| seed する `[visual]` | 同 `:105-108` | **`background_color` のみ**。入力欄色は既定 `#383838` のまま |
| 期待色の変換 | 同 `:62-66` | `#RGB` / `#RRGGBB` を受理して `$expected`（R/G/B）へ |
| `New-SnotraVerificationProfile` | `scripts/lib/SnotraSmoke.psm1:178` | `-AdditionalSections` で `[visual]` を注入できる |
| `Get-SnotraWindowCapture` | 同 `:852` | 窓矩形のキャプチャ（`.Bitmap` / `.Width` / `.Height`） |
| 記述の正本 | `docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」 | 「自動判定するのは main と results の**定常背景**である」と書く |

## 再利用できる既存パターン

- **判定述語をモジュールへ出して Pester で測る**（`scripts/lib/SnotraTraceInvariants.psm1` 671 行 +
  `SnotraTraceInvariants.Tests.ps1` 533 行）。H1/H4/H5 の不変条件判定がこの形で、**判定が
  「起きてはならないことが起きていないか」を名乗れる唯一の場所**になっている
- **母集団取得・判定をスクリプトへ出す先例**: `ADR-race-check-population-tooling`
- `New-SnotraVerificationProfile` の `-AdditionalSections` に `[visual]` を渡す形は
  `visual-check-colors.ps1:105-108` が既に使っている（入力欄色を足すのは 1 行の追加）

## 技術的制約

- **`Measure-WindowBackground` は素のスクリプト内関数ゆえ Pester から呼べない**。`scripts/lib/` の
  2 モジュール（`SnotraSmoke` / `SnotraTraceInvariants`）だけが `*.Tests.ps1` を持つ
- 判定に Bitmap を要求すると合成データでテストしにくい。**「色→画素数の辞書」を入力とする純関数**へ
  切れば Bitmap 不要でテストできる
- **results の判定を壊してはならない**（背景 76.1% が最頻＝正しく動いている）。main と results で
  同じ関数・同じ期待色を共有しているため、分岐の入れ方に注意が要る
- `check:colors` は **CI で走らない**（GUI を要する）。`ci.yml` にも `e2e.yml` にも無い
- 画面ロック中は実行不能（`Assert-SnotraSessionUnlocked` が名指しで止める・#866）
- `$Color` は `#RGB` / `#RRGGBB` の両方を受理する（3 桁展開は `:63-64`・#680 の 1 の回帰検査を兼ねる）

## 未解決の疑問（`plan.md` の未確定欄へ引き継ぐ）

1. **判定述語の形**——「上位 2 色の集合 == {背景, 入力欄}」か「両色がそれぞれ閾値以上」か。
   前者は閾値というマジックナンバーを持たない代わりに、3 位の色（`#373737` 11.3%）が伸びると崩れる。
   **`font_size` を変えたときの分布を実測して決める**。
2. **判定が赤のとき理由を区別できるか**——「色が届いていない」と「レイアウトが変わって前提が崩れた」は
   別の欠陥である。現行は両方が同じ「赤」に潰れる。
3. **`docs/build-commands.md` の「main と results の定常背景」をどう書き換えるか**（main は
   背景 + 入力欄になる）。
4. **判定述語を置くモジュール名**と、`SnotraSmoke.psm1` へ相乗りするか新設するか。
