---
paths:
  - "snotra-settings/**/*.rs"
---

# snotra-settings ルール

詳細は `snotra-settings/CLAUDE.md` を参照。

- **`saved` は Save 成功時のみ更新**: `draft` は自由に変更可、`saved` を他のタイミングで変えると部分保存バグになる
- **egui API 型注意**: `Key::ALL` は `&[Key]`（`for &key in`）、`color_edit_button_srgba` は永続変数の `&mut Color32` を渡す（一時変数だと変更が消える）
- **フレームごとに重い処理を呼ばない**: `list_system_fonts()` 等は `new()` で一度だけ取得しキャッシュ
- **PickerState の `active = false` リセット忘れ**: キャンセル・成功の両パスで必ず。忘れるとボタン永久無効化
- **Opener ターゲット変更は remove → add**: ルールの `target` を直接書き換えると同じルール内の他ツールも巻き込まれる
