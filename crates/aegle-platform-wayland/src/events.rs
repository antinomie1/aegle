//! Translation of Wayland and XKB values into the platform contract.

use aegle_platform::{Key, KeyboardInteractivity, Layer, LayerOptions, Modifiers};
use smithay_client_toolkit::{
    seat::keyboard::{self, KeyEvent, Keysym},
    shell::wlr_layer,
};
use wayland_client::Proxy;

use crate::{Event, PixelSize, SeatId, WindowId, WindowInfo, WlSeat};

/// A seat's contract identity: its protocol object id on this connection.
pub(crate) fn seat_id(seat: &WlSeat) -> SeatId {
    SeatId(seat.id().protocol_id())
}

/// Sets the buffer extent from the logical size and scale, rounded half away
/// from zero; an extent the protocol cannot represent becomes zero, which
/// presentation rejects.
pub(crate) fn rescale(info: &mut WindowInfo) {
    let scaled = |extent: u32| {
        let pixels = (f64::from(extent) * f64::from(info.scale)).round();
        if pixels >= 1.0 && pixels <= f64::from(i32::MAX / 4) {
            pixels as u32
        } else {
            0
        }
    };
    info.buffer = PixelSize {
        width: scaled(info.size.width),
        height: scaled(info.size.height),
    };
}

/// A key transition with its XKB translation.
pub(crate) fn key_event(
    window: WindowId,
    seat: &WlSeat,
    event: KeyEvent,
    pressed: bool,
    repeat: bool,
    state: keyboard::Modifiers,
) -> Event {
    Event::Key {
        window,
        seat: seat_id(seat),
        key: key(event.keysym),
        text: event.utf8,
        pressed,
        repeat,
        modifiers: modifiers(state),
        time: event.time,
    }
}

pub(crate) fn modifiers(value: keyboard::Modifiers) -> Modifiers {
    Modifiers {
        shift: value.shift,
        control: value.ctrl,
        alt: value.alt,
        meta: value.logo,
    }
}

pub(crate) fn key(value: Keysym) -> Key {
    match value {
        Keysym::Return | Keysym::KP_Enter => Key::Enter,
        Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::Escape => Key::Escape,
        Keysym::BackSpace => Key::Backspace,
        Keysym::Delete => Key::Delete,
        Keysym::Left => Key::Left,
        Keysym::Right => Key::Right,
        Keysym::Up => Key::Up,
        Keysym::Down => Key::Down,
        Keysym::Home => Key::Home,
        Keysym::End => Key::End,
        Keysym::Page_Up => Key::PageUp,
        Keysym::Page_Down => Key::PageDown,
        Keysym::Menu => Key::ContextMenu,
        value if (Keysym::F1.raw()..=Keysym::F24.raw()).contains(&value.raw()) => {
            Key::Function((value.raw() - Keysym::F1.raw() + 1) as u8)
        }
        value => value
            .key_char()
            .map(Key::Character)
            .unwrap_or(Key::Unidentified),
    }
}

/// A Linux input-event code for a non-primary mouse button.
pub(crate) fn button(code: u32) -> Option<aegle_platform::PointerButton> {
    use aegle_platform::PointerButton::*;
    Some(match code {
        0x111 => Secondary,
        0x112 => Middle,
        0x113 | 0x116 => Back,
        0x114 | 0x115 => Forward,
        _ => return None,
    })
}

/// The evdev code of the primary button.
pub(crate) const PRIMARY: u32 = 0x110;

pub(crate) fn layer(options: &LayerOptions) -> (wlr_layer::Layer, wlr_layer::Anchor) {
    let layer = match options.layer {
        Layer::Background => wlr_layer::Layer::Background,
        Layer::Bottom => wlr_layer::Layer::Bottom,
        Layer::Top => wlr_layer::Layer::Top,
        Layer::Overlay => wlr_layer::Layer::Overlay,
    };
    let mut anchor = wlr_layer::Anchor::empty();
    for (edge, native) in [
        (aegle_platform::Anchor::TOP, wlr_layer::Anchor::TOP),
        (aegle_platform::Anchor::BOTTOM, wlr_layer::Anchor::BOTTOM),
        (aegle_platform::Anchor::LEFT, wlr_layer::Anchor::LEFT),
        (aegle_platform::Anchor::RIGHT, wlr_layer::Anchor::RIGHT),
    ] {
        if options.anchor.contains(edge) {
            anchor |= native;
        }
    }
    (layer, anchor)
}

pub(crate) fn interactivity(value: KeyboardInteractivity) -> wlr_layer::KeyboardInteractivity {
    match value {
        KeyboardInteractivity::None => wlr_layer::KeyboardInteractivity::None,
        KeyboardInteractivity::Exclusive => wlr_layer::KeyboardInteractivity::Exclusive,
        KeyboardInteractivity::OnDemand => wlr_layer::KeyboardInteractivity::OnDemand,
    }
}
