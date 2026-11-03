---
paths:
  - "snotra-settings/**/*.rs"
---

# snotra-settings ルール（ルーター）

事実の正本は `snotra-settings/CLAUDE.md` である。この rule は「どこを読むか」だけを示す（要約は置かない）。位置はファイル名で決めつけず、見出し名やシンボル名で grep して探す（#588）。

## 読む正本（`snotra-settings/CLAUDE.md` の該当節）

- `saved` / `draft` の更新の決まり（`saved` を進めるのは、保存に成功したときと完全にロードしたときだけ。部分的な保存はしない）: 「draft / saved 二重状態モデル」と「保存フロー」
- egui API の型（`Key::ALL` は `&[Key]`、`color_edit_button_srgba` は永続する `&mut Color32` を取る、`Stroke::new` は版で違う）: 「API の型に注意」
- フレームごとの重い処理（`list_system_fonts()` などは `new()` で 1 度だけ呼んでキャッシュする）: 「フレームごとの重い処理を避ける」
- `PickerState.active = false` に戻す処理（キャンセルと成功の両方の経路で行い、`poll()` にまとめる）: 「非同期ファイルピッカーパターン」
- opener のターゲットを変えるときは、remove してから add する（`OpenerRule.target` を直接書き換えない）: 「開発ルール」

## トリガー → 検査

- 並行境界（worker・共有スロット（`PickerState` など）・live-read など）を追加/変更したら: `/race-check`（述語の全文の SSOT は、常時ロードされる `AGENTS.md`「条件別チェック」表である。ここに写しは置かない）
