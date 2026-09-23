# ADR-startup-terminal-single-derivation: 起動計器の終端の名前と ok を 1 か所で導き、ハーネスの整合検査は残す

## 文脈

#1026。起動計器の終端では、イベント名（`startup:ready` / `startup:failed`）を `finish` が、`data.ok` を `to_json` が、それぞれ別々に `outcome` から導いていた。食い違いは #1009 が足したハーネスの整合検査（`scripts/lib/SnotraStartupContract.psm1`）が外から監視していた。ただしその検査を実バイナリへ当てる `bench-startup.ps1` は `smoke.yml` で `continue-on-error` として走り、PR を止めない。

## 決定

1. 名前と `ok` を `startup.rs` の `terminal` の 1 つの `match` から組で導き、`finish` は `Timeline::terminal_line` が返す組を出すだけにする。
2. `Ok` と `StartupFailure` の全 variant について、名前と `ok` / `reason` の組を単体テストで固定する。
3. **ハーネスの整合検査は残し、説明だけを現行の事実へ直す**（人間の裁定・2026-09-23）。

## 検討した代替案と却下理由

- **ハーネスの整合検査を外す**（検査ブロック・Pester の対応 3 ケース・`bench-startup.ps1` の説明を撤去する）: 却下。
  - 導出を 1 か所にすると、この検査は単体テストと大部分が重なる。
  - それでも実バイナリで出た 1 行を観測できるのはこの検査だけである。
  - `finish` が組の名前を `_` で明示的に捨てて名前を作り直す退行は、単体テストにも clippy にも届かない。届くのはこの検査だけである（#1026 の検証で注入して実測）。
  - 残す費用は 1 ブロックと Pester 3 ケースで、`bench-startup.ps1` は既に観測として走っている。
- **網羅 `match` の証人を置き、`StartupFailure` の variant を足し忘れたら compile-fail にする**: 却下。
  - 証人が縛るのは証人自身の `match` だけで、テストが回す手書きの列挙（`ALL_FAILURES`）への足し忘れは止まらない。
  - 列挙の足し忘れは、`failure_reasons_are_stable_and_unique` の doc が既に受容している残余である。証人を足しても、その残余は閉じない。
