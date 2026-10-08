//! File dialogs through `org.freedesktop.portal.FileChooser`.

use std::{io, path::PathBuf};

use aegle_dbus::{Message, Value};

use super::{Call, Request, Shared};
use crate::FileDialog;

pub(super) fn request(shared: &Shared, dialog: &FileDialog<'_>, save: bool) -> io::Result<u32> {
    let mut state = shared.lock();
    let id = state.id();
    let token = format!("aegle_files_{id}");
    let filters = dialog.filters.iter().map(|&(name, patterns)| {
        let globs = patterns
            .iter()
            .map(|&glob| Value::Struct(vec![Value::U32(0), Value::str(glob)]))
            .collect();
        Value::Struct(vec![Value::str(name), Value::Array("(us)".into(), globs)])
    });
    let mut options = vec![
        ("handle_token", Value::str(&token)),
        (
            "filters",
            Value::Array("(sa(us))".into(), filters.collect()),
        ),
    ];
    if let Some(folder) = dialog.folder {
        let mut bytes = folder.as_os_str().as_encoded_bytes().to_vec();
        bytes.push(0);
        let bytes = bytes.into_iter().map(Value::Byte).collect();
        options.push(("current_folder", Value::Array("y".into(), bytes)));
    }
    if save {
        options.push(("current_name", Value::str(dialog.name)));
    } else {
        options.push(("multiple", Value::Bool(dialog.multiple)));
        options.push(("directory", Value::Bool(dialog.directory)));
    }
    let call = Message::call(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.FileChooser",
        if save { "SaveFile" } else { "OpenFile" },
        vec![
            Value::str(""),
            Value::str(dialog.title),
            Value::dict(options),
        ],
    );
    let path = shared.request_path(&token);
    state.requests.insert(path.clone(), Request::Files(id));
    shared.call(&mut state, &call, Call::Request(path))?;
    Ok(id)
}

/// The local paths of a Response's `uris`; other schemes are dropped.
pub(super) fn paths(results: &Value) -> Vec<PathBuf> {
    let uris = results.get("uris").map_or(&[][..], Value::items);
    uris.iter()
        .filter_map(|uri| aegle_types::path_from_file_uri(uri.as_str()?))
        .collect()
}
