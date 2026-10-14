use core_graphics::event::{CGEventFlags, CGEventType, EventField};
use openlogi_core::binding::Shortcut;

use super::{
    DesktopDirection, combo, desktop_key_events, held_key_event, hid_usage_to_macos, key_event,
};
use crate::inject::{HeldKey, HeldModifiers, KeyPhase};

#[test]
fn hid_usages_map_to_macos_virtual_keys() {
    assert_eq!(hid_usage_to_macos(0x04), Some(0x00));
    assert_eq!(hid_usage_to_macos(0x13), Some(0x23));
    assert_eq!(hid_usage_to_macos(0x50), Some(0x7b));
    assert_eq!(hid_usage_to_macos(0x3a), Some(0x7a));
    assert_eq!(hid_usage_to_macos(0x6f), Some(0x5a));
    assert_eq!(hid_usage_to_macos(0xff), None);
}

/// Pin a handful of representative `Shortcut -> KeyCombo` rows so an
/// edit to the table can't silently change what ⌘C sends. macOS and
/// Linux only overlap on the letter-key chords: both `BrowserBack` and
/// `Redo` differ across the three backends by design (see the module
/// doc on `combo`), so each backend pins its own rows independently.
#[test]
fn combo_table_pins_representative_shortcuts() {
    assert_eq!(combo(Shortcut::Copy).rendered_label(), "Cmd+C");
    assert_eq!(combo(Shortcut::Redo).rendered_label(), "Cmd+Shift+Z");
    assert_eq!(combo(Shortcut::BrowserBack).rendered_label(), "Cmd+[");
    assert_eq!(combo(Shortcut::NextTab).rendered_label(), "Ctrl+Tab");
    // hid_usage_to_macos must actually resolve every table entry, or a
    // `Shortcut` silently no-ops instead of pressing anything (see
    // `press_combo`'s warn-and-drop path). Iterates `Shortcut::ALL`
    // rather than a hand-copied list, so a newly added `Shortcut`
    // variant is checked here automatically instead of depending on
    // someone remembering to extend a second, independent list.
    for &shortcut in Shortcut::ALL {
        let key = combo(shortcut).key().code();
        assert!(
            hid_usage_to_macos(key).is_some(),
            "{shortcut:?} table entry has no macOS virtual-key mapping"
        );
    }
}

#[test]
fn held_edges_carry_the_aggregate_modifier_state() {
    let mut modifiers = HeldModifiers::default();
    let (_, flags) = held_key_event(HeldKey::Command, KeyPhase::Down, &mut modifiers)
        .expect("Command has a macOS virtual-key mapping");
    assert!(flags.contains(CGEventFlags::CGEventFlagCommand));

    let (_, flags) = held_key_event(HeldKey::Control, KeyPhase::Down, &mut modifiers)
        .expect("Control has a macOS virtual-key mapping");
    assert!(flags.contains(CGEventFlags::CGEventFlagCommand));
    assert!(flags.contains(CGEventFlags::CGEventFlagControl));

    let key = combo(Shortcut::Copy).key();
    let (_, flags) = held_key_event(HeldKey::Key(key), KeyPhase::Up, &mut modifiers)
        .expect("Copy's key has a macOS virtual-key mapping");
    assert!(flags.contains(CGEventFlags::CGEventFlagCommand));
    assert!(flags.contains(CGEventFlags::CGEventFlagControl));

    let (_, flags) = held_key_event(HeldKey::Command, KeyPhase::Up, &mut modifiers)
        .expect("Command has a macOS virtual-key mapping");
    assert!(!flags.contains(CGEventFlags::CGEventFlagCommand));
    assert!(flags.contains(CGEventFlags::CGEventFlagControl));
}

/// The hook skips a key edge only by this tag, so every edge the backend
/// posts must carry it.
#[test]
fn posted_key_edges_carry_the_synthetic_tag() {
    for phase in [KeyPhase::Down, KeyPhase::Up] {
        let event =
            key_event(0x0c, CGEventFlags::CGEventFlagCommand, phase).expect("a keyboard event");
        assert_eq!(
            event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA),
            crate::SYNTHETIC_EVENT_USER_DATA
        );
    }
}

#[test]
fn desktop_action_uses_logitech_control_arrow_sequence() {
    for (direction, arrow) in [
        (DesktopDirection::Previous, 0x7b),
        (DesktopDirection::Next, 0x7c),
    ] {
        let events = desktop_key_events(direction, CGEventFlags::CGEventFlagNull)
            .expect("desktop events allocate");
        assert_eq!(events.len(), 3);
        for (event, (kind, key, flags)) in events.iter().zip([
            (
                CGEventType::KeyDown,
                arrow,
                CGEventFlags::CGEventFlagControl,
            ),
            (CGEventType::KeyUp, arrow, CGEventFlags::CGEventFlagControl),
            (
                CGEventType::FlagsChanged,
                0x3b,
                CGEventFlags::CGEventFlagNull,
            ),
        ]) {
            assert_eq!(event.get_type() as u32, kind as u32);
            assert_eq!(
                event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE),
                key,
            );
            assert_eq!(event.get_flags(), flags);
            assert_eq!(
                event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT),
                0,
            );
            assert_eq!(
                event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA),
                crate::SYNTHETIC_EVENT_USER_DATA,
            );
        }
    }
}

#[test]
fn desktop_action_preserves_an_already_held_control() {
    let held = CGEventFlags::CGEventFlagControl | CGEventFlags::CGEventFlagShift;
    let events = desktop_key_events(DesktopDirection::Next, held).expect("desktop events allocate");
    assert_eq!(events.len(), 2, "do not release the user's held Control");
    assert_eq!(events[0].get_flags(), CGEventFlags::CGEventFlagControl);
    assert!(matches!(events[1].get_type(), CGEventType::KeyUp));
    assert_eq!(events[1].get_flags(), held);
}

#[test]
fn desktop_action_restores_other_held_modifiers_after_its_chord() {
    let held = CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagAlternate;
    let events =
        desktop_key_events(DesktopDirection::Previous, held).expect("desktop events allocate");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].get_flags(), CGEventFlags::CGEventFlagControl);
    assert_eq!(events[1].get_flags(), CGEventFlags::CGEventFlagControl);
    assert_eq!(events[2].get_flags(), held);
}
