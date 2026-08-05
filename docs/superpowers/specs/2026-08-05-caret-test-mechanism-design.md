# キャレット検査の機序の再設計（#872 / #936）

2026-08-05。**日付付き設計書ゆえ歴史記録である**——規範の正本はコードと各 `CLAUDE.md` に置く。

## 背景

`scripts/lib/SnotraSmoke.Tests.ps1` の実機配管 It `フォルダ復帰後の次打鍵を復元クエリの末尾へ追加する`（#840）は、7 か月にわたり CI で間欠失敗した。対処は 5 回入っている（#854 / #871 / #887 / #889 / #890）。

**5 回とも観測装置側の修正であり、断言そのものには一度も触れていない。** #938 が根本原因（`view.rs` が `TextEdit` 構築後に `request_focus()` を撃ち、そのフレームの文字を捨てていた）を特定して初めてアプリ側の欠陥が出たが、その後も残余がある。

残余の実測（同日・同一 runner イメージ・30 反復・[run 30967971392](https://github.com/finelagusaz/Snotra/actions/runs/30967971392)）は **3 種類**である。一次証拠と導出は `workspace/research.md` と `workspace/plan-review-evidence.md`。

| 種類 | 件数 | 性質 |
|---|---|---|
| 単一インスタンス衝突 | 3 | 直前の It が待たずに kill したプロセスを掴む。**打鍵に到達せず終わる** |
| 遅着 | 4 | 待った事象が予算の後に届く（`seq` を名指しできる） |
| フレーム不回転 | 1 | 24.3 秒 1 フレームも回らない。猶予 15 秒でも届かない |

**注意**: この 30 反復は測定ハーネスが `SNOTRA_EGUI_INPUT_TRACE` を空文字で漏らしていたため、打鍵の到達計器が有効な状態で走っている（26/27 反復）。**絶対率としては使えない**（計器は率だけでなく喪失の現れ方も変える）。ここで使うのは分類と機序だけである。

## 問題の定式化

この検査が実際に断言していることは 3 層に分かれ、**そのうち 2 層は既に別の場所で守られているか、守れる**。

| 層 | 断言 | 現状 |
|---|---|---|
| L1 状態遷移 | Escape で `restore_query` / `restore_results` / `restore_selected` が戻る | `search_state.rs` の単体検査が守る（`escape_folder_restores_then_hides` ほか） |
| L2 キャレット | 復元フレームで**同一フレームの文字**が末尾へ入る | #938 の単体検査は **egui の意味論**だけを固定。⚠️「本体側の呼び出しが後ろへ戻っても通る・受容する残余」と本人が明記 |
| L3 実プロセス | 起動後の**最初のフレーム**で入力欄が打鍵を受け取れる | Pester だけが見る。#938 はここでしか捕まらなかった |

**L3 のためだけに、L1 と L2 を巻き込んだ 8 打鍵・3 段の待ちを実行している。** flake の構造的前提（#872 本文の要素 1＝前面窓依存、要素 2＝実時間ポーリング）は、その巻き込みから来ている。

## 決定

**L2 を kittest で格上げし、L3 を最小の配線検査へ縮める。**

### L2 — `search_input_ui` の切り出しと kittest

`view.rs`（1,217 行）が `RuntimeFrame` を使うのは **3 箇所だけ**である（実測）。

| 位置 | 用途 |
|---|---|
| `311` | `frame.drag_window()` |
| `382` | `frame.set_clear_color(visual.background)` |
| `1046` | `frame.event_loop()`（証人） |

**検索入力欄の区画（`576`〜`672` 付近・`move_text_cursor_to_end` と focus 要求と `TextEdit` が並ぶ場所）に `frame` の依存は 1 つも無い。** ゆえに `&mut egui::Ui` だけを取るメソッドへ切り出せる。

- 手本は `snotra-settings/src/app.rs` の `ui_impl`（#440）。`Harness::new_ui_state` は `FnMut(&mut Ui, &mut State)` を取り `Frame` を渡せないため、Frame 非依存の入口を切り出す形が既にある。`settle()` ヘルパもそこにある
- `egui_kittest = "0.35"` は既に `snotra-settings` の dev 依存にあり、`egui = "=0.35.0"` とピンが一致する。`src-tauri` へは dev-dependency として足す（現在 `[dev-dependencies]` 節は無い）

**この検査が縛るもの**: `move_text_cursor_to_end` → `request_focus` → `TextEdit` 構築という**実コードの並び**。#938 が受容した残余（意味論は縛るが並びは縛らない）がここで閉じる。

### L3 — 縮めた実機配管

```
現行  Resolve → Start → WaitWindow → index.bin 待ち → SetForeground
      → A/L/P/H/A → 待ち(5s) → Right → A/A → 待ち(5s) → Escape → z → 待ち(5s)

縮小  Resolve → Start → WaitWindow → SetForeground
      → egui_input:focus_state を待つ → has_focus == true
```

消えるもの: **打鍵注入 8 回**・**待ち 2 回**・**3 段の順序依存**。

検出器は #938 が**そのために置いたもの**であり、`view.rs:683-698` のコメントが明言している。

> **この行は移設の回帰検出器になる**（偽に戻れば、起動直後の打鍵が再び捨てられている）

**前面化（`Set-SnotraForegroundWindow`）は残す。** focus 要求は `pre.focused`（窓の OS focus）に条件づけられている（`view.rs`）ため、前面が取れなければ `has_focus` は真にならない。#890 が現れ方 1 の原因（debug ビルドのコンソール窓）を除いた後、30 反復で前面化の失敗は 0 件である。

`index.bin` の待ちと `[config]` 不在の検査（seed が効いた肯定的証拠）は、直前の It（`Tests.ps1:345`）が同じ断言を持つため、縮小版からは落とす。

## 却下した案（否定の知識）

- **`RuntimeFrame` にテスト用の構築点を足す** — `EventLoopProof::new()` は `pub(crate)` で、`proof.rs` の doc が「構築点は 2 つだけである。**3 つ目を足すときは、その経路が本当にイベントループ上かを一次証拠で示すこと**」と守っている。切り出しで足りる以上、この守りを崩す理由が無い
- **`frame` コマンドを trait 化して `update()` 全体を kittest へ載せる** — `set_clear_color` の呼び忘れ（現在ビルドでも自動テストでも落ちない）を検出できる利得はあるが、crate 境界をまたぐ抽象の新設に見合わない。切り出す区画に `frame` 依存が無いことが実測で分かっている
- **L3 を完全に捨てて kittest へ一本化** — #938 が捕まえた「実プロセス起動後の最初のフレーム」は kittest が届かない層である（窓は可視・前面・focus 済みなのに widget が焦点を持たない、という状態は実プロセスの起動順序からしか生じない）
- **打鍵注入を 1 文字だけ残す** — 前面の奪取（要素 1）が戻る。配線（OS の打鍵がアプリへ届くこと）は `smoke-egui.ps1` が release ビルドで既にカバーしている（hotkey VK 列 → `egui_show:done` → 1 文字クエリ → `egui_results:show`）

## 残余（受容する）

- **フレーム不回転**（実測 1/30）は縮小版でも残り、`focus_state` の待ちが時間切れになる。**ただし断言の失敗ではなく 1 回の待ちの明確な時間切れとして現れる**ため、今より読める。専用の診断へ分離するかは #786 系（`smoke:startup` の同型 flake）と併せて判断する
- **単一インスタンス衝突**（実測 3/30）は縮小版でも残る。2 つの It が連続してプロセスを起動する構造は変わらないため、別途 `Stop-SnotraProcessAndWait` で塞ぐ（`workspace/plan.md` Phase 1）

## 既存計画（`workspace/plan.md`）との関係

- **Phase 1（単一インスタンス衝突）は生き残る。** 上記のとおり構造が変わらない
- **Phase 2 は Rust 側の空文字修正だけへ縮める**（ユーザー合意 2026-08-05）。`scripts/repro-pester-flake.ps1` は自身の `.NOTES` が「**#872 / #936 が閉じたら一式を撤去する**」と定めており、そこへの作り込み（意図ではなく証拠に基づく自己記述など）は撤去される先への投資になる
- 残す修正は `snotra-egui-runtime` の `var_os(...).is_some()` が空文字を「有効」と読む点である。**手本は同じリポジトリの `snotra-core/src/config.rs` の `config_dir_from`**（`var_os` + `!is_empty()`・rustdoc に理由あり）。**厳しい許可リスト（`src-tauri/src/trace.rs` の `env_flag`）へ寄せる案は採らない**——PowerShell 側の読み手（`SnotraSmoke.psm1:664`）が緩いまま残るため、`=0` 系で新しい食い違いが生まれる。加えて `renderer.rs:76` は毎フレーム評価されるため、割り当てが 1→2 に増える

## 検証

- `cargo clippy --workspace --all-targets -- -D warnings` / workspace 全テスト
- `npm run test:powershell`（縮小した実機配管を含む）
- **kittest 側は両方向で固定する**——focus 要求を `TextEdit` 構築の後ろへ戻したときに落ちることまで検査する（「前なら入る」だけでは戻したときの保証にならない。#938 の単体検査と同じ規律）
- 縮小した L3 は、`focus_state` の `has_focus` を偽にする変異（focus 要求の条件を落とす）で 1 度赤くなることを確かめる

## Follow-up（この設計に含めない）

- **U-A**: `Set-SnotraForegroundWindow` を外しても `pre.focused` が真になるか。アプリは show 直後に `set_focus()` を呼ぶ（`window_coordinator.rs:341`）が、**それだけで OS が前面を渡すかは未実測**（今の標本は PowerShell が先に前面化しているため区別できない）。外せれば要素 1 が完全に消える
- **U-B**: フレーム不回転の機序。`take` 行が 0 件で 24.3 秒沈黙する現象は、この設計の射程外である
- **U-C**: 計器なしでの残余の率（#872 の型 A の予算判断の前提）
