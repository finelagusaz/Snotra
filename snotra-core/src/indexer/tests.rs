//! [`IndexMaterial`] の不変条件のテスト——木と派生データの列長が揃うこと。

use super::*;
use crate::str_arena::LowerFileSlot;

/// 2 件の木を建てる（下の検証テスト群の材料）。
fn two_entry_tree() -> IndexTree {
    IndexTree::build(vec![
        AppEntry {
            name: "Firefox".to_string(),
            target_path: "C:\\apps\\firefox.lnk".to_string(),
            is_folder: false,
        },
        AppEntry {
            name: "Projects".to_string(),
            target_path: "C:\\Projects".to_string(),
            is_folder: true,
        },
    ])
}

/// **`from_untrusted` の拒否経路そのものを走らせる。**
///
/// これが無いと、`index.bin` から来た組を検証する**唯一の**機構に検知器が 1 本も無い状態になる——条件の向きを書き換えても（`!=` を `<` へ、`lower_ok` の腕を 1 本落とす等）既存テストは全数緑のまま通り、壊れた `index.bin` は「木より短いマスク」として起動経路へ入る。`assemble` の長さ検証は `debug_assert` ゆえ release では消えるので、帰結は起動後の初回検索での添字外アクセス → `panic = "abort"` である（全走査による自動復旧も起きない）。
///
/// **長い側も弾くことまで見る。** 等値（`!=`）で書いてあるので短い列と長い列の両方が落ちるが、`<` へ書き換えると長い側だけが素通りする——列が余分に長い `index.bin` は添字外にはならないが、木と対応しないマスクで検索することになり**スコアが静かにずれる**。
#[test]
fn from_untrusted_rejects_masks_whose_len_disagrees_with_the_tree() {
    let full = |n: usize| CachedMasks {
        char_masks: vec![0; n],
        file_name_char_masks: vec![0; n],
        lower: None,
    };

    // 揃っている組は受け取る（受理側を測らないと、下の拒否が「常に None」でも緑になる）。
    assert!(
        IndexMaterial::from_untrusted(two_entry_tree(), full(2)).is_some(),
        "長さの揃った組は受理されなければならない"
    );

    for n in [1usize, 3] {
        assert!(
            IndexMaterial::from_untrusted(two_entry_tree(), full(n)).is_none(),
            "木が 2 件なのにマスクが {n} 件の組を受理している"
        );
    }

    // 列ごとに独立して見ていること（片方だけずれた組も弾く）。
    let mut only_file_short = full(2);
    only_file_short.file_name_char_masks.pop();
    assert!(
        IndexMaterial::from_untrusted(two_entry_tree(), only_file_short).is_none(),
        "file_name_char_masks だけがずれた組を受理している"
    );

    // `lower` の 2 variant も見ていること。
    let collapsed_short = CachedMasks {
        char_masks: vec![0; 2],
        file_name_char_masks: vec![0; 2],
        lower: Some(CachedLower::Collapsed {
            lower_names: [None].into_iter().collect(),
            lower_file_names: [LowerFileSlot::Absent].into_iter().collect(),
        }),
    };
    assert!(
        IndexMaterial::from_untrusted(two_entry_tree(), collapsed_short).is_none(),
        "Collapsed の列がずれた組を受理している"
    );
    let raw_short = CachedMasks {
        char_masks: vec![0; 2],
        file_name_char_masks: vec![0; 2],
        lower: Some(CachedLower::Raw {
            lower_names: vec!["firefox".to_string()],
            lower_file_names: vec![None],
        }),
    };
    assert!(
        IndexMaterial::from_untrusted(two_entry_tree(), raw_short).is_none(),
        "Raw の列がずれた組を受理している"
    );
}
