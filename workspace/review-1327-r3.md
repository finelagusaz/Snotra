# review-1327-r3（sha f531cd6cd4952e64293afd72282f88d670050b29）
align exit 0 / snotra-core/CLAUDE.md の blob fd090a6c = アンカー / governance:check 17 件 passed / 差分は CLAUDE.md の 2 文だけ

## 直した 2 文の検証
- 「compute_wave2 をどの枝でも通らず（マスクはキャッシュから来る）」: 真。compute_wave2 の呼び出しは build.rs:387（new_with_migemo）と :423（new_from_tree）だけで、new_with_cached_masks（:460-570）には無い。マスクは CachedMasks から分解して得る（:470-474）
- 「compute_wave1 も派生文字列を持たない枝でしか通らない」: 真。match cached_lower の None 枝（:546「派生文字列を持たない版からのフォールバック」）で wave1_from_tree（:552、内部で compute_wave1 :107）を呼ぶ。cached_lower は Option<CachedLower> なので、派生文字列を持たない枝は None 枝の 1 つだけ
- 「載らない列は Collapsed / Raw 枝で別に建てる（kana_for_cached の形）」: 構造と合う。kana_for_cached は new_with_cached_masks 内のクロージャ（:476）で、Collapsed（:516）と Raw（:532）の両枝で呼ばれる。None 枝では wave1_from_tree が kana も返す
- ⚠3 の「search/path_store.rs と」: 解消

## 新しい ⚠（Low）
- キャッシュに載らない列でも、マスク型の列（kana_char_masks の形）は枝の中ではなく match の後に全枝共通で建てている（compute_kana_char_masks・:563）。「Collapsed/Raw 枝で別に建てる」を字義どおりに読むとこの形が漏れる。実害は小さい（kana の行が両方を例示している）
