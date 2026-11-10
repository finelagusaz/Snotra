# 台帳: src-tauri/src/egui_shell/launcher_controller/activation.rs（基準 0265463c・候補 22 行・17 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! 起動の入口と、そこから走る dispatch・in-flight 回収・slash の即実行（#631 / #666 段 3）。 | 縮める | `（#631 / #666）` | | |
| B2 | 5-6 | //! どの子モジュールに在っても検査は生き続ける**（移設して測った——移すだけなら 3 本とも緑、 ⏎ //! 移した先でゲートを落とすと赤。同じ移設を変更前の版へ当てると赤だった）。**… | 残す | | 主張（入口がどこに在っても検査が生きる）を支える過去の実測 | |
| B3 | 41-41 | /// worker 側で行う（spec 決定 5: WebView2 が backend 側で UI 可視性と無関係に記録する parity。 | 縮める | `（#532: WebView2 が…` | `git log -S 'spec 決定 5'` の最古 → 70a34910（#647・SU5＝#532） | 同 doc の「spec C 節」は設計 spec の節を指すだけの参照 |
| B4 | 124-125 | /// …clearResults parity・spec 決定 7）—— ⏎ /// …snapshot.show=false・#646 PR2 決定 6）・ | 縮める | `・#532）`・`・#646）` | spec 決定 7: 最古 70a34910（#647・#532）。PR2 決定 6: 最古 a9cb6ef4（#669・#646 PR2） | |
| B5 | 185-185 | // 履歴記録は worker 側（spec 決定 5）。 | 縮める | `（#532）` | B3 と同じ | |
| B6 | 196-200 | // 状態を変えたら起こす（#648 A）。旧実装は「results クリア → 高さ collapse → set_size → Resized → repaint」の暗黙連鎖…#646 PR2 決定 6 で…（SU5・e746826）… | 書き直す | 「状態を変えたら起こす（#648）。main の高さは bar(+toast) 固定で、results をクリアしても main は伸縮しない（#646）——初回 overlay と timeout 予約を成立させるフレームを、ほかの何も起こさない（results 側の repaint は snapshot 差分 wake が担う）。toast dismiss の同型バグ（#532）と同じ規範で、この行単体で自己完結させた明示 repaint にする」 | 元の主張の現在形部分（高さ固定・snapshot 差分 wake・同型バグ・自己完結）。旧実装の暗黙連鎖の描写を消した。「ほかの何も起こさない」は元の「暗黙連鎖は消滅」の言い換え。SU5 → #532（commit e746826 の名指しは落とした） | |
| B7 | 205-205 | /// drain が回収した結果の UI 後処理（成功列は M1/M3 同期版と同じ末尾へ合流）。 | 消す（括弧のみ） | 「drain が回収した結果の UI 後処理。」 | M1/M3 の同期版は今は無い実装（起動はすべて `start_launch` の worker）。旧実装との対比 | |
| B8 | 228-228 | // 旧 Timeout ステータスは IPC の run_launch_blocking 専用で #532 SU7 PR3 で消滅 | 書き直す | 「`LaunchStatus` は timeout を持たない（drain 側の 4 秒 timeout は Empty 経路で扱う・#532）」 | `commands/launch.rs:LaunchStatus` は `Ok` / `Failed` だけ（grep で確認） | |
| B9 | 260-260 | // 4 秒経過＝「結果不明」（spec 決定 8）。 | 縮める | `（#532）` | `git log -S 'spec 決定 8'` 最古 → 70a34910（#647） | |
| B10 | 291-291 | /// slash コマンドを実行する（§15.3 即実行・#532 SU3 M3）。 | 縮める | `・#532）` | | 同 doc の「SolidJS handleCommandQueryInput と同順」は旧フロントエンドとの parity（経緯ではなく仕様の出自として残した） |
| B11 | 343-344 | /// **抽出が engine lock を経ないのは #1076 の移行による**（それ以前はここで `engine.lock()` を取っていた）—— | 書き直す | 「**抽出は engine lock を経ない**（#1076）——」以降は元のまま | 元の主張の現在形化 | |
| B12 | 390-390 | /// Enter/クリックの単一 dispatch（§19.6/§4.8・#532 SU3 M3）。 | 縮める | `・#532）` | | |
| B13 | 441-441 | // #1072 が塞いだのは同じ族の unsettled 側の切片だけだった。 | 書き直す | 「同じ族の unsettled 側は `on_enter` の flush 分岐が塞ぐ（#1072）」 | 直前の文が「`is_unsettled` が偽の Enter は `on_enter` の flush 分岐を通らず」と述べており、その分岐（`activation.rs:on_enter` の `should_flush_on_enter`）が unsettled 側を扱う。#1072（28a6342a）は `search_dispatch.rs` に `is_unsettled` を新設し、`should_flush_on_enter` の第 3 引数へ渡す形にした（`git show` で確認） | |
| B14 | 567-567 | /// [`crate::egui_shell::read_config`] へ移した）。 | 書き直す | 「**解決は config の読み（[`crate::egui_shell::read_config`]）の中で行う**（#1076）」 | 移設の記録（`engine.lock()` から移した）を落とした。intra-doc link は残した | |
| B15 | 581-581 | /// フレームの IME 確定・paste が旧 state で起動されるのを防ぐ（不変条件 3）。 | 残す | | 誤検出（同じフレームの前の state） | 「不変条件 3」は設計 spec の番号を指す（範囲外） |
| B16 | 597-597 | // …正本である**（#1038。`armed` だけを渡していた頃に開いていた隙もそこが記す。#1039 で `search_dispatch.rs` の自由関数から移設）。 | 縮める | `（#1038）` | 移設の記録と、正本側の経緯への案内を落とした | |
| B17 | 617-617 | // …（`run_search_with` の Plain 早期 return が旧実装で担っていた処置である）。 | 消す（括弧のみ） | | 旧実装との対比で、今の制約は前半の文が運ぶ | |
| X1 | 3 | //! **起動の入口をこのファイルへ集める規範は要らなくなった**（#1201）。 | 書き直す | 「…規範は要らない（#1201）」 | 変化の語りを現在形へ | |
| X2 | 66 | /// 起動成功時に drain が行う UI 後処理の種別（M1/M3 の同期版と同じ末尾へ合流させる）。 | 消す（括弧のみ） | | B7 と同じ | |
| X6 | 69 | Normal,  // emit_hide のみ（M1 activate parity・クエリは次 show の reset で消える） | 消す（ラベルのみ） | 「emit_hide のみ（クエリは次 show の reset で消える）」 | M1 の同期 activate は今は無い（Phase 1 追補「消えた型・関数を指すなら消す」）。Tool / Instant の行の `execute_tool_selected` / `execute_instant_selected` は現存するので残した（2 つ目のコミット） | |
| X7 | 77 | /// …per-launch なら不要——並行性レビューで確定）。 | 残す | | 判断の出自の一言（作業単位の番号ではない）。`frame_stages.rs` の「並行性レビュー High」は同じ括弧に #532 を足したので消したが、こちらは番号を持たず消すと出自が無くなる | ⚠️ 扱いが `frame_stages.rs` と揃っていない |
| X3 | 89,93,471 | `#636 レビュー Finding A`・`#636 Finding A` | 縮める | `#636` | 作業単位の副ラベル | |
| X4 | 137 | （#1039 で `SearchState` の内側へ入った） | 書き直す | 「（失効は `SearchState` の内側が行う・#1039）」 | `search_state.rs:SearchState::set_results` の doc が「in-flight を失効させる（`put_rows` が行う）」 | |
| X5 | 310 | // indexing 中の Err（ERR_INDEXING_IN_PROGRESS）は trace のみ（spec M3 実装確定・ | 縮める | `（#532・` | `git log -S 'spec M3 実装確定'` 最古 → ebc59178（#637・SU3 M3＝#532）。`search_flow.rs` の同じラベルと揃えた（2 つ目のコミットで直した） | |

## 範囲外メモ

- ⚠️ `/code-review #637 finding 0`・`finding 1`・`finding 6` は副ラベルだが、別の行が「finding 0 の規律」「finding 6 と同じ債務」とラベルで引いているので変えなかった（群 E の他ファイル・`search_flow.rs` にも同じ引用がある）
- `spec C 節`・`spec C 節 不変条件 1/2` は設計 spec を指すだけの参照
