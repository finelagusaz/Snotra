//! キーボードの Win32 メッセージを egui の入力へ写す純粋な変換核（#1289）。
//!
//! `windows_ime.rs` の subclass がメッセージを読み、ここで [`NativeInput`] の列へ写して `mpsc` へ
//! 送る。**キーボードの持ち主は subclass だけである**——tao の `KeyboardInput` /
//! `ReceivedImeText` は egui へ渡さない（`input.rs`）。Win32 の呼び出しを含まないので、
//! メッセージ番号と VK はこのファイルの定数で持ち、テストで順序と条件を固定する。
//!
//! 修飾キーはここでは扱わない。egui へ載せる修飾と Ctrl+C/X/V の判定は、drain の時点で
//! `InputState` が tao の `ModifiersChanged` の保持値から決める（AltGr の扱いを tao と揃えるため）。

use std::collections::{HashMap, HashSet};

/// subclass から `InputState` へ渡す 1 件。IME もキーも同じ列に載り、全順序になる。
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NativeInput {
    Ime(egui::ImeEvent),
    /// 修飾を持たないキーイベント。修飾は drain の時点で `InputState` が載せる。
    Key {
        key: egui::Key,
        physical_key: Option<egui::Key>,
        pressed: bool,
        repeat: bool,
    },
    /// 印字可能な文字（空・制御文字は弾いた後）。
    Text(String),
}

/// 物理キーの同一性（スキャンコード。拡張キーは `0xE000` を立てる）。
///
/// **#927 の抑止の鍵にする**——VK を鍵にすると、変換中に `VK_PROCESSKEY` で届く押しっぱなしの
/// 英字を、focus 獲得時に取り込んだ VK と照合できない。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PhysicalScan(pub(crate) u16);

pub(crate) const WM_KEYDOWN: u32 = 0x0100;
pub(crate) const WM_KEYUP: u32 = 0x0101;
pub(crate) const WM_CHAR: u32 = 0x0102;
pub(crate) const WM_SYSKEYDOWN: u32 = 0x0104;
pub(crate) const WM_SYSKEYUP: u32 = 0x0105;
pub(crate) const WM_SYSCHAR: u32 = 0x0106;

/// subclass が変換核へ渡すキーボードのメッセージか。
pub(crate) fn is_keyboard_message(message: u32) -> bool {
    matches!(
        message,
        WM_KEYDOWN | WM_KEYUP | WM_CHAR | WM_SYSKEYDOWN | WM_SYSKEYUP | WM_SYSCHAR
    )
}

/// 1 通の変換結果。`suppressed` は「focus 獲得時に押されていたキーとして落とした」（計器用）。
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Translation {
    pub(crate) events: Vec<NativeInput>,
    pub(crate) suppressed: bool,
}

/// キーボードのメッセージ列を [`NativeInput`] へ写す状態機械。ウィンドウごとに 1 つ。
#[derive(Default)]
pub(crate) struct KeyboardTranslator {
    /// focus を獲得した瞬間に押されていたキー（#927）。down を up まで落とす。
    held: HashSet<PhysicalScan>,
    /// egui へ down を送ったキー。focus を失ったら release を送る（tao の合成 release の代わり）。
    down: HashMap<PhysicalScan, (egui::Key, Option<egui::Key>)>,
    /// 抑止した down に続く `WM_CHAR` を落とす（#1274）。次の down で解除する。
    drop_chars: bool,
    /// サロゲートペアの high（キーのメッセージを跨いで保つ）。
    pending_high: Option<u16>,
}

impl KeyboardTranslator {
    /// `WM_SETFOCUS`: その瞬間に押されているキーを取り込む（集合を置き換える）。
    pub(crate) fn on_focus_gained(&mut self, held: impl IntoIterator<Item = PhysicalScan>) {
        self.held = held.into_iter().collect();
        self.down.clear();
        self.drop_chars = false;
        self.pending_high = None;
    }

    /// `WM_KILLFOCUS`: 状態をすべて初期化し、down を送ったキーの release を返す。
    ///
    /// **抑止を focus セッションの外へ持ち越さない**——抑止したキーの up が別ウィンドウへ行くと
    /// 解除の機会が無く、以後そのキー（Escape）が永久に効かなくなる（fail-closed・#927）。
    pub(crate) fn on_focus_lost(&mut self) -> Vec<NativeInput> {
        self.held.clear();
        self.drop_chars = false;
        self.pending_high = None;
        self.down
            .drain()
            .map(|(_, (key, physical_key))| NativeInput::Key {
                key,
                physical_key,
                pressed: false,
                repeat: false,
            })
            .collect()
    }

    /// キーボードのメッセージ 1 通を写す。`scan` は lParam から（0 なら VK から）導いた物理キー、
    /// `shift` は数字キーの写像にだけ使う。
    pub(crate) fn on_message(
        &mut self,
        message: u32,
        wparam: usize,
        lparam: u32,
        scan: PhysicalScan,
        shift: bool,
    ) -> Translation {
        match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => self.on_key(wparam as u16, lparam, scan, shift, true),
            WM_KEYUP | WM_SYSKEYUP => self.on_key(wparam as u16, lparam, scan, shift, false),
            WM_CHAR | WM_SYSCHAR => self.on_char(wparam as u32),
            _ => Translation::default(),
        }
    }

    fn on_key(
        &mut self,
        vk: u16,
        lparam: u32,
        scan: PhysicalScan,
        shift: bool,
        pressed: bool,
    ) -> Translation {
        if pressed {
            // **抑止した down に続く `WM_CHAR` も落とす**（#1274）。旗は次の down が上書きする。
            self.drop_chars = self.held.contains(&scan);
            if self.drop_chars {
                return Translation {
                    events: Vec::new(),
                    suppressed: true,
                };
            }
        } else {
            // release は常に渡す——落とすと egui の `keys_down` に押しっぱなしが残る。
            self.held.remove(&scan);
            self.down.remove(&scan);
        }

        let physical_key = physical_key_from_scan(scan);
        let Some(key) = key_from_vk(vk, shift).or(physical_key) else {
            return Translation::default();
        };
        if pressed {
            self.down.insert(scan, (key, physical_key));
        }
        Translation {
            events: vec![NativeInput::Key {
                key,
                physical_key,
                pressed,
                repeat: pressed && lparam & (1 << 30) != 0,
            }],
            suppressed: false,
        }
    }

    fn on_char(&mut self, unit: u32) -> Translation {
        if self.drop_chars {
            return Translation {
                events: Vec::new(),
                suppressed: true,
            };
        }
        let text = match unit {
            0xD800..=0xDBFF => {
                self.pending_high = Some(unit as u16);
                return Translation::default();
            }
            0xDC00..=0xDFFF => {
                let Some(high) = self.pending_high.take() else {
                    return Translation::default();
                };
                String::from_utf16(&[high, unit as u16]).ok()
            }
            _ => {
                self.pending_high = None;
                char::from_u32(unit).map(String::from)
            }
        };
        Translation {
            events: text
                .as_deref()
                .and_then(printable_text)
                .map(NativeInput::Text)
                .into_iter()
                .collect(),
            suppressed: false,
        }
    }
}

/// VK から egui のキー（論理）を導く。現行（tao の `logical_key` → `key_from_tao`）と同じ集合。
///
/// 数字の VK は Shift が無いときだけ写す——tao の logical は Shift+1 で `"!"` になり写らなかった。
/// `VK_PROCESSKEY`（変換中）と `VK_PACKET`（`SendInput` の Unicode・絵文字パネル）は論理キーを
/// 持たない。
pub(crate) fn key_from_vk(vk: u16, shift: bool) -> Option<egui::Key> {
    use egui::Key;
    Some(match vk {
        0x08 => Key::Backspace,
        0x09 => Key::Tab,
        0x0D => Key::Enter,
        0x1B => Key::Escape,
        0x20 => Key::Space,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x23 => Key::End,
        0x24 => Key::Home,
        0x25 => Key::ArrowLeft,
        0x26 => Key::ArrowUp,
        0x27 => Key::ArrowRight,
        0x28 => Key::ArrowDown,
        0x2D => Key::Insert,
        0x2E => Key::Delete,
        0x30..=0x39 if !shift => DIGITS[(vk - 0x30) as usize],
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0x60..=0x69 => DIGITS[(vk - 0x60) as usize],
        0x70..=0x83 => FUNCTION_KEYS[(vk - 0x70) as usize],
        _ => return None,
    })
}

/// 物理キー（スキャンコード）から egui のキーを導く。現行（`key_from_key_code`）と同じく A–Z だけ。
/// スキャンコードは US 配列の set 1（拡張キーは対象外）。
pub(crate) fn physical_key_from_scan(scan: PhysicalScan) -> Option<egui::Key> {
    use egui::Key;
    Some(match scan.0 {
        0x10 => Key::Q,
        0x11 => Key::W,
        0x12 => Key::E,
        0x13 => Key::R,
        0x14 => Key::T,
        0x15 => Key::Y,
        0x16 => Key::U,
        0x17 => Key::I,
        0x18 => Key::O,
        0x19 => Key::P,
        0x1E => Key::A,
        0x1F => Key::S,
        0x20 => Key::D,
        0x21 => Key::F,
        0x22 => Key::G,
        0x23 => Key::H,
        0x24 => Key::J,
        0x25 => Key::K,
        0x26 => Key::L,
        0x2C => Key::Z,
        0x2D => Key::X,
        0x2E => Key::C,
        0x2F => Key::V,
        0x30 => Key::B,
        0x31 => Key::N,
        0x32 => Key::M,
        _ => return None,
    })
}

/// 文字列が egui へ入れてよい文字か（空と制御文字を弾く）。
pub(crate) fn printable_text(text: &str) -> Option<String> {
    (!text.is_empty() && text.chars().all(|c| !c.is_ascii_control() && c != '\u{7f}'))
        .then(|| text.to_owned())
}

const DIGITS: [egui::Key; 10] = {
    use egui::Key::*;
    [Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9]
};

const LETTERS: [egui::Key; 26] = {
    use egui::Key::*;
    [
        A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    ]
};

const FUNCTION_KEYS: [egui::Key; 20] = {
    use egui::Key::*;
    [
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20,
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    const VK_ESCAPE: u16 = 0x1B;
    const VK_PROCESSKEY: u16 = 0xE5;
    const VK_PACKET: u16 = 0xE7;
    const VK_A: u16 = 0x41;
    const VK_Z: u16 = 0x5A;
    const VK_1: u16 = 0x31;

    const SCAN_ESCAPE: PhysicalScan = PhysicalScan(0x01);
    const SCAN_A: PhysicalScan = PhysicalScan(0x1E);
    const SCAN_Z: PhysicalScan = PhysicalScan(0x2C);
    const SCAN_ENTER: PhysicalScan = PhysicalScan(0x1C);
    const SCAN_1: PhysicalScan = PhysicalScan(0x02);
    const REPEAT: u32 = 1 << 30;

    fn key(key: egui::Key, physical_key: Option<egui::Key>, pressed: bool) -> NativeInput {
        NativeInput::Key {
            key,
            physical_key,
            pressed,
            repeat: false,
        }
    }

    fn down(t: &mut KeyboardTranslator, vk: u16, scan: PhysicalScan) -> Translation {
        t.on_message(WM_KEYDOWN, vk as usize, 0, scan, false)
    }

    fn up(t: &mut KeyboardTranslator, vk: u16, scan: PhysicalScan) -> Translation {
        t.on_message(WM_KEYUP, vk as usize, 0, scan, false)
    }

    fn chr(t: &mut KeyboardTranslator, unit: u32) -> Translation {
        t.on_message(WM_CHAR, unit as usize, 0, PhysicalScan(0), false)
    }

    // --- 写像（D6）---

    #[test]
    fn navigation_and_editing_keys_are_mapped() {
        use egui::Key;
        for (vk, expected) in [
            (0x25, Key::ArrowLeft),
            (0x26, Key::ArrowUp),
            (0x27, Key::ArrowRight),
            (0x28, Key::ArrowDown),
            (0x1B, Key::Escape),
            (0x09, Key::Tab),
            (0x08, Key::Backspace),
            (0x0D, Key::Enter),
            (0x2D, Key::Insert),
            (0x2E, Key::Delete),
            (0x24, Key::Home),
            (0x23, Key::End),
            (0x21, Key::PageUp),
            (0x22, Key::PageDown),
            (0x20, Key::Space),
            (0x70, Key::F1),
            (0x83, Key::F20),
        ] {
            assert_eq!(key_from_vk(vk, false), Some(expected), "vk={vk:#x}");
        }
    }

    #[test]
    fn letters_digits_and_numpad_are_mapped() {
        use egui::Key;
        assert_eq!(key_from_vk(VK_A, false), Some(Key::A));
        assert_eq!(
            key_from_vk(VK_Z, true),
            Some(Key::Z),
            "Shift でも英字は写る"
        );
        assert_eq!(key_from_vk(VK_1, false), Some(Key::Num1));
        assert_eq!(key_from_vk(0x60, false), Some(Key::Num0), "VK_NUMPAD0");
        assert_eq!(key_from_vk(0x69, false), Some(Key::Num9), "VK_NUMPAD9");
    }

    /// tao の logical は Shift+1 で `"!"` になり写らない。現行と同じく Shift つきの数字は写さない。
    #[test]
    fn shifted_digit_is_not_mapped() {
        assert_eq!(key_from_vk(VK_1, true), None);
    }

    #[test]
    fn process_and_packet_keys_have_no_logical_key() {
        assert_eq!(key_from_vk(VK_PROCESSKEY, false), None);
        assert_eq!(key_from_vk(VK_PACKET, false), None);
    }

    #[test]
    fn physical_key_is_mapped_for_letters_only() {
        assert_eq!(physical_key_from_scan(SCAN_A), Some(egui::Key::A));
        assert_eq!(
            physical_key_from_scan(PhysicalScan(0x10)),
            Some(egui::Key::Q)
        );
        assert_eq!(
            physical_key_from_scan(PhysicalScan(0x32)),
            Some(egui::Key::M)
        );
        assert_eq!(physical_key_from_scan(SCAN_ENTER), None);
        assert_eq!(physical_key_from_scan(SCAN_1), None);
        // 拡張キーは英字にならない（0xE01E は A ではない）。
        assert_eq!(physical_key_from_scan(PhysicalScan(0xE01E)), None);
    }

    // --- キーイベント ---

    #[test]
    fn keydown_and_keyup_produce_key_events_with_physical_letter() {
        let mut t = KeyboardTranslator::default();
        assert_eq!(
            down(&mut t, VK_A, SCAN_A).events,
            vec![key(egui::Key::A, Some(egui::Key::A), true)]
        );
        assert_eq!(
            up(&mut t, VK_A, SCAN_A).events,
            vec![key(egui::Key::A, Some(egui::Key::A), false)]
        );
    }

    #[test]
    fn repeat_comes_from_lparam_bit_30() {
        let mut t = KeyboardTranslator::default();
        let events = t
            .on_message(WM_KEYDOWN, VK_A as usize, REPEAT, SCAN_A, false)
            .events;
        assert_eq!(
            events,
            vec![NativeInput::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: true,
            }]
        );
    }

    /// D1: 変換中（`VK_PROCESSKEY`）は論理キーを作らず、物理が A–Z のときだけ Key を作る。
    #[test]
    fn process_key_falls_back_to_physical_letter_and_drops_enter() {
        let mut t = KeyboardTranslator::default();
        assert_eq!(
            down(&mut t, VK_PROCESSKEY, SCAN_A).events,
            vec![key(egui::Key::A, Some(egui::Key::A), true)]
        );
        assert!(down(&mut t, VK_PROCESSKEY, SCAN_ENTER).events.is_empty());
    }

    #[test]
    fn sys_keys_are_translated_like_plain_keys() {
        let mut t = KeyboardTranslator::default();
        let events = t
            .on_message(WM_SYSKEYDOWN, VK_A as usize, 0, SCAN_A, false)
            .events;
        assert_eq!(events, vec![key(egui::Key::A, Some(egui::Key::A), true)]);
    }

    // --- 文字 ---

    #[test]
    fn char_commits_printable_text() {
        let mut t = KeyboardTranslator::default();
        assert_eq!(
            chr(&mut t, 'a' as u32).events,
            vec![NativeInput::Text("a".to_owned())]
        );
        assert_eq!(
            chr(&mut t, 'あ' as u32).events,
            vec![NativeInput::Text("あ".to_owned())]
        );
    }

    /// D2: `WM_SYSCHAR`（Alt+文字）も文字にする（現行の tao と同じ）。
    #[test]
    fn syschar_commits_text() {
        let mut t = KeyboardTranslator::default();
        let events = t
            .on_message(WM_SYSCHAR, 'q' as usize, 0, PhysicalScan(0), false)
            .events;
        assert_eq!(events, vec![NativeInput::Text("q".to_owned())]);
    }

    #[test]
    fn control_characters_are_dropped() {
        let mut t = KeyboardTranslator::default();
        for unit in [0x01, 0x0D, 0x08, 0x09, 0x1B, 0x7F] {
            assert!(chr(&mut t, unit).events.is_empty(), "unit={unit:#x}");
        }
        assert_eq!(printable_text(""), None);
        assert_eq!(printable_text("日本語"), Some("日本語".to_owned()));
    }

    /// サロゲートペアは high を保留して low で連結する。`KEYEVENTF_UNICODE` では間にキーの
    /// メッセージが挟まるので、保留はそれを跨いで保つ。
    #[test]
    fn surrogate_pair_is_joined_across_key_messages() {
        let mut t = KeyboardTranslator::default();
        let [high, low] = surrogates('😀');
        assert!(chr(&mut t, high).events.is_empty());
        assert!(up(&mut t, VK_PACKET, PhysicalScan(0)).events.is_empty());
        assert!(down(&mut t, VK_PACKET, PhysicalScan(0)).events.is_empty());
        assert_eq!(
            chr(&mut t, low).events,
            vec![NativeInput::Text("😀".to_owned())]
        );
    }

    #[test]
    fn unpaired_surrogates_are_dropped() {
        let mut t = KeyboardTranslator::default();
        let [high, low] = surrogates('😀');
        // low だけ。
        assert!(chr(&mut t, low).events.is_empty());
        // high の後に通常の文字: high は捨て、通常の文字は入る。
        assert!(chr(&mut t, high).events.is_empty());
        assert_eq!(
            chr(&mut t, 'a' as u32).events,
            vec![NativeInput::Text("a".to_owned())]
        );
        // その後の low は対にならない。
        assert!(chr(&mut t, low).events.is_empty());
    }

    fn surrogates(c: char) -> [u32; 2] {
        let mut buf = [0u16; 2];
        c.encode_utf16(&mut buf);
        [buf[0] as u32, buf[1] as u32]
    }

    // --- focus 獲得時の抑止（#927・D4）---

    /// focus 獲得時に押されていたキーの down は落とす。**この判定が「1 回の押下で設定ウィンドウと
    /// 本体の 2 つが閉じる」を止めている。**
    #[test]
    fn key_held_at_focus_gain_is_suppressed() {
        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_ESCAPE]);
        let translation = down(&mut t, VK_ESCAPE, SCAN_ESCAPE);
        assert!(translation.events.is_empty());
        assert!(translation.suppressed);
    }

    /// 抑止中の autorepeat（物理キーが focus 移行を跨いで届く経路）も落とす。
    #[test]
    fn autorepeat_while_held_is_suppressed() {
        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_ESCAPE]);
        let translation = t.on_message(WM_KEYDOWN, VK_ESCAPE as usize, REPEAT, SCAN_ESCAPE, false);
        assert!(translation.events.is_empty());
    }

    /// release は渡し、抑止を解く。
    #[test]
    fn release_is_passed_and_lifts_suppression() {
        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_ESCAPE]);
        assert_eq!(
            up(&mut t, VK_ESCAPE, SCAN_ESCAPE).events,
            vec![key(egui::Key::Escape, None, false)]
        );
        assert_eq!(
            down(&mut t, VK_ESCAPE, SCAN_ESCAPE).events,
            vec![key(egui::Key::Escape, None, true)],
            "抑止は 1 回の押下で終わる（fail-closed の防止）"
        );
    }

    #[test]
    fn other_keys_are_unaffected_by_suppression() {
        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_ESCAPE]);
        assert_eq!(
            down(&mut t, VK_A, SCAN_A).events,
            vec![key(egui::Key::A, Some(egui::Key::A), true)]
        );
    }

    /// 抑止は focus セッションを越えない。release が届かない経路（release 時に focus を失っている）で
    /// 持ち越されると、以後 Escape が効かなくなる。
    #[test]
    fn losing_focus_clears_suppression() {
        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_ESCAPE]);
        t.on_focus_lost();
        assert_eq!(
            down(&mut t, VK_ESCAPE, SCAN_ESCAPE).events,
            vec![key(egui::Key::Escape, None, true)]
        );
    }

    /// 取り込みは集合を置き換える（前の focus セッションの取り込みを残さない）。
    #[test]
    fn focus_gain_replaces_the_held_set() {
        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_ESCAPE]);
        t.on_focus_gained([SCAN_Z]);
        assert_eq!(
            down(&mut t, VK_ESCAPE, SCAN_ESCAPE).events,
            vec![key(egui::Key::Escape, None, true)]
        );
        assert!(down(&mut t, VK_Z, SCAN_Z).events.is_empty());
    }

    /// 鍵は物理キーである: 変換中に `VK_PROCESSKEY` で届く押しっぱなしの英字も照合される。
    #[test]
    fn suppression_matches_process_key_by_physical_scan() {
        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_A]);
        assert!(down(&mut t, VK_PROCESSKEY, SCAN_A).events.is_empty());
    }

    /// #1274: 抑止した down に続く `WM_CHAR` は入れない。次の（抑止されない）down で解除する。
    #[test]
    fn char_after_suppressed_down_is_dropped_until_next_down() {
        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_Z]);
        assert!(down(&mut t, VK_Z, SCAN_Z).events.is_empty());
        let translation = chr(&mut t, 'z' as u32);
        assert!(translation.events.is_empty());
        assert!(translation.suppressed);
        up(&mut t, VK_Z, SCAN_Z);
        down(&mut t, VK_A, SCAN_A);
        assert_eq!(
            chr(&mut t, 'a' as u32).events,
            vec![NativeInput::Text("a".to_owned())]
        );
    }

    /// focus の出入りで旗と保留中の high を初期化する。旗が残ると次の `PostMessage(WM_CHAR)` を
    /// 落とし、high が残ると無関係な low と対になる。
    #[test]
    fn focus_transitions_reset_char_flag_and_pending_surrogate() {
        let [high, low] = surrogates('😀');

        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_Z]);
        down(&mut t, VK_Z, SCAN_Z);
        t.on_focus_lost();
        assert_eq!(
            chr(&mut t, 'x' as u32).events,
            vec![NativeInput::Text("x".to_owned())],
            "focus 喪失で旗が消える"
        );

        let mut t = KeyboardTranslator::default();
        chr(&mut t, high);
        t.on_focus_gained([]);
        assert!(
            chr(&mut t, low).events.is_empty(),
            "focus 獲得で high が消える"
        );

        let mut t = KeyboardTranslator::default();
        t.on_focus_gained([SCAN_Z]);
        down(&mut t, VK_Z, SCAN_Z);
        t.on_focus_gained([]);
        assert_eq!(
            chr(&mut t, 'x' as u32).events,
            vec![NativeInput::Text("x".to_owned())],
            "focus 獲得で旗が消える"
        );
    }

    /// D10: focus を失ったら、down を送ったキーの release を返す（tao の合成 release の代わり）。
    #[test]
    fn losing_focus_releases_keys_that_were_sent_down() {
        let mut t = KeyboardTranslator::default();
        down(&mut t, VK_A, SCAN_A);
        down(&mut t, VK_ESCAPE, SCAN_ESCAPE);
        up(&mut t, VK_ESCAPE, SCAN_ESCAPE);
        assert_eq!(
            t.on_focus_lost(),
            vec![key(egui::Key::A, Some(egui::Key::A), false)]
        );
        assert!(t.on_focus_lost().is_empty(), "2 回目は空");
    }
}
