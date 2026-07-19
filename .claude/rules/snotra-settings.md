---
paths:
  - "snotra-settings/**/*.rs"
---

# snotra-settings ルール（ルーター）

事実の正準は `snotra-settings/CLAUDE.md`。本 rule は「どこを読むか」だけを示す（要約を置かない）。位置はファイル名で断定せず**見出し名・シンボル名で grep** して辿る（#588）。

## 読む正準（`snotra-settings/CLAUDE.md` の該当節）

- `saved` / `draft` の更新規律（save 成功・完全ロード時のみ `saved` を進める・部分保存禁止）: 「draft / saved 二重状態モデル」+「保存フロー」
- egui API の型（`Key::ALL` は `&[Key]`・`color_edit_button_srgba` は永続 `&mut Color32`・`Stroke::new` の版差）: 「API の型に注意」
- フレームごとの重い処理（`list_system_fonts()` 等は `new()` で 1 度キャッシュ）: 「フレームごとの重い処理を避ける」
- `PickerState.active = false` の戻し（キャンセル・成功の両パス・`poll()` に集約）: 「非同期ファイルピッカーパターン」
- opener ターゲット変更は remove → add（`OpenerRule.target` を直接書き換えない）: 「開発ルール」
