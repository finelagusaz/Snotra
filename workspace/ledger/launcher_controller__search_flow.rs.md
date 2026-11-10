# 台帳: src-tauri/src/egui_shell/launcher_controller/search_flow.rs（基準 0265463c・候補 8 行・6 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! 検索の発行（打鍵 → debounce → worker への要求）と、その結果の採り込み（#1004 PR 2）。 | 縮める | `（#1004）` | | |
| B2 | 40-40 | /// **この読みは #1076 で `engine.lock()` から [`crate::egui_shell::read_config`] へ移した。** | 書き直す | 「**この読みは [`crate::egui_shell::read_config`] を通し、engine lock を取らない**（#1076）」 | 移設の記録を現在形へ。`state.rs:AppState::read_config` は engine lock の外で config を読む | |
| B3 | 53-53 | /// view_kind 先の同期 dispatch（#532 SU3 M2）。…results は M1 の interp 分岐（plain 検索）。 | 縮める | `（#532）`・「results は interp 分岐（plain 検索）」 | `M1` も SU3 の副ラベル | |
| B4 | 57-57 | /// （/code-review #637 finding 9。当時は engine lock の回数だった・#1076 で読み口が移った）。 | 縮める | `（/code-review #637 finding 9・#1076）` | 「当時は〜」は経緯。`finding 9` は 265 行が「finding 9」と引くので残した | |
| B5 | 117-118 | // ここは config の読みだけを担う（#1124 で `commands/instant.rs` から移した—— ⏎ // かつて IPC コマンドと共有していた相手は #532 SU7 で消滅していた）。 | 縮める | `（#1124）` | 移設の記録 | |
| B6 | 204-207 | // "view" = Folder ビューへ遷移していた（#1039 のガード）。… // "seq"  = 追い越された、または同期で差し替えて失効していた。 | 残す | | 誤検出（trace の reason の値の意味） | |
| X1 | 114, 276 | （30ms debounce 撤廃・spec M3 実装確定） | 書き直す | 「（debounce しない・#532）」 | 「撤廃」は旧フロントエンドとの対比。Instant 分岐は `search_debounce.cancel()` のうえ `run_search_with` を同期で呼ぶ（同ファイル `on_input_changed`）。`git log -S 'spec M3 実装確定'` 最古 ebc59178（#637・#532） | |
| X2 | 263 | // SolidJS parity: 毎打鍵 selected=0（M1 gap 是正） | 縮める | `（#532）` | `git log -S 'M1 gap 是正'` 最古 ebc59178（#637・#532） | |

## 範囲外メモ

- `/code-review #637 finding 0`・`finding 6`・`finding 9` は副ラベルだが、別の行がラベルで引き合っているので変えなかった（`activation.rs` の台帳にも同じメモ）
