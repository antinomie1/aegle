//! The tray icon: a `StatusNotifierItem` at `/StatusNotifierItem` and its
//! `com.canonical.dbusmenu` menu at `/MenuBar`, served from the reading
//! thread and registered with the `org.kde.StatusNotifierWatcher`.

use std::io;

use aegle_dbus::{Message, Value};

use super::{Call, Shared, State as Desktop};
use crate::{Event, MenuItem, Tray};

const ITEM: &str = "org.kde.StatusNotifierItem";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";
const MENU_PATH: &str = "/MenuBar";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

const INTROSPECTION: &str = r#"<node>
<interface name="org.kde.StatusNotifierItem">
<method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
<signal name="NewIcon"/><signal name="NewToolTip"/><signal name="NewTitle"/>
<signal name="NewStatus"><arg type="s"/></signal>
</interface>
<interface name="com.canonical.dbusmenu">
<method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method>
<method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/></method>
<method name="GetProperty"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
<method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
<method name="EventGroup"><arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/></method>
<method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
<method name="AboutToShowGroup"><arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/></method>
<signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
</interface>
<interface name="org.freedesktop.DBus.Properties">
<method name="Get"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
<method name="GetAll"><arg type="s" direction="in"/><arg type="a{sv}" direction="out"/></method>
</interface>
</node>"#;

/// One menu entry; entry 0 is the root.
struct Entry {
    properties: Vec<(&'static str, Value)>,
    children: Vec<i32>,
    /// The id reported when chosen.
    item: Option<u32>,
}

#[derive(Default)]
pub(super) struct State {
    shown: bool,
    tooltip: String,
    /// `(width, height, ARGB32 bytes in network order)`.
    pixmap: Option<(u32, u32, Vec<u8>)>,
    entries: Vec<Entry>,
    revision: u32,
}

impl State {
    pub(super) fn shown(&self) -> bool {
        self.shown
    }

    fn add(&mut self, items: &[MenuItem<'_>]) -> Vec<i32> {
        items
            .iter()
            .map(|item| {
                let id = self.entries.len() as i32;
                let (mut properties, mut chosen, mut children) = (Vec::new(), None, &[][..]);
                match *item {
                    MenuItem::Item {
                        id,
                        label,
                        enabled,
                        checked,
                    } => {
                        properties.push(("label", Value::str(label)));
                        properties.push(("enabled", Value::Bool(enabled)));
                        if let Some(checked) = checked {
                            properties.push(("toggle-type", Value::str("checkmark")));
                            properties.push(("toggle-state", Value::I32(checked.into())));
                        }
                        chosen = Some(id);
                    }
                    MenuItem::Separator => properties.push(("type", Value::str("separator"))),
                    MenuItem::Submenu { label, items } => {
                        properties.push(("label", Value::str(label)));
                        properties.push(("children-display", Value::str("submenu")));
                        children = items;
                    }
                }
                self.entries.push(Entry {
                    properties,
                    children: Vec::new(),
                    item: chosen,
                });
                let nested = self.add(children);
                self.entries[id as usize].children = nested;
                id
            })
            .collect()
    }

    fn properties(&self, id: usize) -> Value {
        let properties = self.entries[id]
            .properties
            .iter()
            .map(|(k, v)| (*k, v.clone()));
        Value::dict(properties)
    }

    /// `(ia{sv}av)` for entry `id` and `depth` levels below it (all for -1).
    fn layout(&self, id: usize, depth: i64) -> Value {
        let children = match depth {
            0 => Vec::new(),
            _ => (self.entries[id].children.iter())
                .map(|&child| Value::variant(self.layout(child as usize, depth - 1)))
                .collect(),
        };
        Value::Struct(vec![
            Value::I32(id as i32),
            self.properties(id),
            Value::Array("v".into(), children),
        ])
    }

    fn item_properties(&self, app_id: &str) -> Vec<(&'static str, Value)> {
        let pixmaps = |pixmap: Option<&(u32, u32, Vec<u8>)>| {
            let icons = pixmap.map(|(width, height, argb)| {
                let bytes = argb.iter().copied().map(Value::Byte).collect();
                Value::Struct(vec![
                    Value::I32(*width as i32),
                    Value::I32(*height as i32),
                    Value::Array("y".into(), bytes),
                ])
            });
            Value::Array("(iiay)".into(), icons.into_iter().collect())
        };
        let tooltip = Value::Struct(vec![
            Value::str(""),
            pixmaps(None),
            Value::str(&self.tooltip),
            Value::str(""),
        ]);
        vec![
            ("Category", Value::str("ApplicationStatus")),
            ("Id", Value::str(app_id)),
            ("Title", Value::str(&self.tooltip)),
            (
                "Status",
                Value::str(if self.shown { "Active" } else { "Passive" }),
            ),
            ("WindowId", Value::I32(0)),
            ("IconName", Value::str("")),
            ("IconThemePath", Value::str("")),
            ("IconPixmap", pixmaps(self.pixmap.as_ref())),
            ("OverlayIconName", Value::str("")),
            ("OverlayIconPixmap", pixmaps(None)),
            ("AttentionIconName", Value::str("")),
            ("AttentionIconPixmap", pixmaps(None)),
            ("AttentionMovieName", Value::str("")),
            ("ToolTip", tooltip),
            ("ItemIsMenu", Value::Bool(false)),
            ("Menu", Value::Path(MENU_PATH.into())),
        ]
    }

    fn menu_properties(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("Version", Value::U32(3)),
            ("TextDirection", Value::str("ltr")),
            ("Status", Value::str("normal")),
            ("IconThemePath", Value::strings([])),
        ]
    }
}

pub(super) fn set(shared: &Shared, tray: Option<&Tray<'_>>) -> io::Result<()> {
    let mut state = shared.lock();
    let first = !state.tray.shown && tray.is_some();
    let tray_state = &mut state.tray;
    tray_state.shown = tray.is_some();
    if let Some(tray) = tray {
        let argb = tray
            .icon
            .rgba
            .chunks(4)
            .flat_map(|p| [p[3], p[0], p[1], p[2]]);
        tray_state.pixmap = Some((tray.icon.width, tray.icon.height, argb.collect()));
        tray_state.tooltip = tray.tooltip.into();
        tray_state.entries = vec![Entry {
            properties: vec![("children-display", Value::str("submenu"))],
            children: Vec::new(),
            item: None,
        }];
        tray_state.entries[0].children = tray_state.add(tray.menu);
        tray_state.revision += 1;
    }
    let revision = tray_state.revision;
    if first {
        register(shared, &mut state)?;
    }
    let status = if tray.is_some() { "Active" } else { "Passive" };
    for (member, body) in [
        ("NewIcon", vec![]),
        ("NewToolTip", vec![]),
        ("NewTitle", vec![]),
        ("NewStatus", vec![Value::str(status)]),
    ] {
        shared
            .bus
            .send(&Message::signal(ITEM_PATH, ITEM, member, body))?;
    }
    let updated = vec![Value::U32(revision), Value::I32(0)];
    shared
        .bus
        .send(&Message::signal(MENU_PATH, MENU, "LayoutUpdated", updated))?;
    Ok(())
}

/// Registers the item with the watcher under this connection's name.
pub(super) fn register(shared: &Shared, state: &mut Desktop) -> io::Result<()> {
    let call = Message::call(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
        "RegisterStatusNotifierItem",
        vec![Value::str(shared.bus.name())],
    );
    shared.call(state, &call, Call::Register)
}

/// Answers a method call on the item or menu objects.
pub(super) fn serve(shared: &Shared, state: &mut Desktop, call: &Message, events: &mut Vec<Event>) {
    let tray = &state.tray;
    let args = &call.body[..];
    let int = |at: usize| args.get(at).and_then(Value::as_i64).unwrap_or(-1);
    let entry = |id: i64| {
        usize::try_from(id)
            .ok()
            .filter(|&id| id < tray.entries.len())
    };
    let reply = match (
        call.path.as_str(),
        call.interface.as_str(),
        call.member.as_str(),
    ) {
        (ITEM_PATH | MENU_PATH, "org.freedesktop.DBus.Introspectable", "Introspect") => {
            call.reply(vec![Value::str(INTROSPECTION)])
        }
        (path @ (ITEM_PATH | MENU_PATH), PROPERTIES, member @ ("Get" | "GetAll")) => {
            let properties = match path {
                ITEM_PATH => tray.item_properties(&shared.app_id),
                _ => tray.menu_properties(),
            };
            if member == "GetAll" {
                call.reply(vec![Value::dict(properties)])
            } else {
                let name = args.get(1).and_then(Value::as_str);
                match properties.into_iter().find(|(key, _)| Some(*key) == name) {
                    Some((_, value)) => call.reply(vec![Value::variant(value)]),
                    None => call.error(
                        "org.freedesktop.DBus.Error.UnknownProperty",
                        "no such property",
                    ),
                }
            }
        }
        (ITEM_PATH, ITEM, "Activate" | "SecondaryActivate") => {
            events.push(Event::TrayActivated);
            call.reply(vec![])
        }
        (ITEM_PATH, ITEM, "ContextMenu" | "Scroll") => call.reply(vec![]),
        (MENU_PATH, MENU, "GetLayout") => match entry(int(0)) {
            Some(id) => call.reply(vec![Value::U32(tray.revision), tray.layout(id, int(1))]),
            None => call.error("org.freedesktop.DBus.Error.InvalidArgs", "no such item"),
        },
        (MENU_PATH, MENU, "GetGroupProperties") => {
            let ids = args.first().map_or(&[][..], Value::items);
            let all = (0..tray.entries.len() as i64)
                .map(Value::I64)
                .collect::<Vec<_>>();
            let ids = if ids.is_empty() { &all[..] } else { ids };
            let items = ids
                .iter()
                .filter_map(|id| entry(id.as_i64()?))
                .map(|id| Value::Struct(vec![Value::I32(id as i32), tray.properties(id)]))
                .collect();
            call.reply(vec![Value::Array("(ia{sv})".into(), items)])
        }
        (MENU_PATH, MENU, "GetProperty") => {
            let name = args.get(1).and_then(Value::as_str);
            let value = entry(int(0)).and_then(|id| {
                tray.entries[id]
                    .properties
                    .iter()
                    .find(|(k, _)| Some(*k) == name)
            });
            match value {
                Some((_, value)) => call.reply(vec![Value::variant(value.clone())]),
                None => call.error("org.freedesktop.DBus.Error.InvalidArgs", "no such property"),
            }
        }
        (MENU_PATH, MENU, "Event") => {
            clicked(tray, int(0), args.get(1).and_then(Value::as_str), events);
            call.reply(vec![])
        }
        (MENU_PATH, MENU, "EventGroup") => {
            for event in args.first().map_or(&[][..], Value::items) {
                let fields = event.items();
                let id = fields.first().and_then(Value::as_i64).unwrap_or(-1);
                clicked(tray, id, fields.get(1).and_then(Value::as_str), events);
            }
            call.reply(vec![Value::Array("i".into(), vec![])])
        }
        (MENU_PATH, MENU, "AboutToShow") => call.reply(vec![Value::Bool(false)]),
        (MENU_PATH, MENU, "AboutToShowGroup") => {
            let none = || Value::Array("i".into(), vec![]);
            call.reply(vec![none(), none()])
        }
        _ if call.no_reply => return,
        _ => call.error("org.freedesktop.DBus.Error.UnknownMethod", "unknown method"),
    };
    let _ = shared.bus.send(&reply);
}

fn clicked(tray: &State, id: i64, event: Option<&str>, events: &mut Vec<Event>) {
    let item = usize::try_from(id)
        .ok()
        .and_then(|id| tray.entries.get(id)?.item);
    if let (Some(item), Some("clicked")) = (item, event) {
        events.push(Event::TrayMenu { item });
    }
}
