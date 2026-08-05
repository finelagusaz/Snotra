# ADR-egui-trace-hatch-empty-only: egui の trace ハッチは「空文字だけ」を未設定として扱い、許可リストへ統一しない

## 文脈

`snotra-egui-runtime` の診断用 trace ハッチ 5 名前（`SNOTRA_EGUI_{PAINT,REPAINT,WAKE,INPUT,IME}_TRACE`・読み手は 7 箇所）は `std::env::var_os(..).is_some()` で判定していた。この形は `Some("")` を真と読むため、**空文字で計器が点く**。

PowerShell の `[Environment]::SetEnvironmentVariable($name, $null, 'Process')` は変数を消さず**空文字で作る**（実測）。#872 の測定ハーネス（`scripts/repro-pester-flake.ps1`）が反復ごとの env 復元でこれを作り、**2 反復目以降の全反復が黙って計器つきで走っていた**（実測 26/27）。この計器は 1 事象あたり 2 行の stderr を挟むため、失敗率だけでなく故障の現れ方まで変える。

同じリポジトリには受理値の意味論が 3 つある:

| 意味論 | 位置 | 空文字 |
|---|---|---|
| 許可リスト（`1｜true｜yes｜on`・trim + 小文字化） | `src-tauri/src/trace.rs` の `env_flag`（`SNOTRA_TRACE`） | 偽 |
| `var_os` + `!is_empty()` | `snotra-core/src/config.rs` の `config_dir_from` | 偽（未設定として扱う） |
| 素の `is_some()` | `snotra-egui-runtime` の 7 箇所 | **真**（これが実バグ） |

## 決定

**空文字だけを未設定として扱う**（`snotra-egui-runtime/src/env.rs` の `trace_hatch_enabled`）。判定を 1 箇所へ集約し、7 箇所をそこへ寄せる。**`env_flag` の許可リストへは寄せない。**

## 検討した代替案と却下理由

- **`src-tauri/src/trace.rs` の `env_flag`（許可リスト）へ統一する**: 却下。**このハッチには PowerShell 側にも読み手が居る**（`scripts/lib/SnotraSmoke.psm1` の `Send-SnotraKey` が `if ($env:SNOTRA_EGUI_INPUT_TRACE)` で注入時刻を出す）。PowerShell の文字列真偽は空文字だけが偽なので、許可リストにすると `=0` や `=verbose` のような値で **Rust が偽・PowerShell が真**という新しい食い違いが生まれる——変更前は両者とも真で一貫していた箇所である。実バグは空文字ちょうどであり、**新しい分類を 1 つも作らずに塞げる**方を採った。加えて `renderer.rs` の読み手は `paint()` の中＝毎フレームで、`env_flag` は `var` + `trim().to_ascii_lowercase()` ゆえ ON 時の割り当てが 1 → 2 に増える（その直後のコメントが「計器が測定対象を汚さない」ことを設計意図として明記している）。
- **`env_flag` を crate 間で共有する（`snotra-core` へ寄せる／`snotra-egui-runtime` を SSOT にして `src-tauri` が委譲する）**: 上の却下により論点自体が消えた。意味論を統一しないので共有する対象が無い。
- **PowerShell 側の読み手にも許可リストを与えて 3 言語で揃える**: 却下。受理値の正本が Rust 2 か所 + PowerShell 1 か所へ散り、**言語をまたぐ写しには compile-fail も intra-doc link も届かない**。片方に `"y"` を足しても何も赤くならない。
- **読み手を直さず、`repro-pester-flake.ps1` の env 復元だけを直す**: 却下。症状の除去であって源を断たない。空文字は人が手で作ることもでき（`$env:X = ''`）、次に別の env で同じ形が再発する。

## 帰結

- **受理値は「空でなければ何でもよい」で確定し、`PERFORMANCE.md` の計器一覧がそれを明記する**（同ファイルが計器の自称正本である）。
- **`SNOTRA_TRACE` だけが別の意味論であることは残る受容残余**である。両者を揃える動機は将来も生じうるが、揃える方向は**この ADR が却下した向き**（ハッチを厳しくする）ではなく、`SNOTRA_TRACE` を緩める向きでなければならない——PowerShell 側の読み手が居るのはハッチの方だからである。
- 判定核 `is_enabled(Option<&OsStr>)` は env を読まないので並列テストから網羅的に測れる（`snotra-core` の `config_dir_from` と同じ流儀。edition 2024 では `set_var` が `unsafe` であり、env を触るテストは並列実行とも噛み合わない）。
