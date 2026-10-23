//! Shows the native file dialogs controls ask for and delivers the chosen
//! paths to the asking window.

use std::rc::Rc;

use aegle_ui::{Result, Ui};

use crate::native::App;

#[cfg(feature = "desktop-services")]
mod service {
    use std::{
        cell::RefCell,
        collections::HashMap,
        rc::{Rc, Weak},
    };

    use aegle_desktop::{Desktop, Event};
    use aegle_ui::{FileRequest, Result, Ui};

    /// Requests waiting for the desktop, by desktop request id.
    type Pending = Rc<RefCell<HashMap<u32, (Weak<Ui>, FileRequest)>>>;

    /// The application's own desktop connection for file dialogs.
    pub(crate) struct Service {
        pub desktop: Desktop,
        pub pending: Pending,
    }

    impl crate::native::App {
        /// The file dialog service, connected on first use.
        pub(super) fn file_service(&self) -> Result<std::cell::RefMut<'_, Service>> {
            let mut files = self.files.borrow_mut();
            if files.is_none() {
                let pending: Pending = Default::default();
                let waiting = pending.clone();
                let app_id = self.runtime.borrow().options.app_id.clone();
                let desktop = self.desktop(&app_id, move |event| -> Result {
                    match event {
                        Event::Files { request, paths } => {
                            let found = waiting.borrow_mut().remove(&request);
                            match found.and_then(|(ui, request)| Some((ui.upgrade()?, request))) {
                                Some((ui, request)) => ui.files_chosen(request, paths),
                                None => Ok(()),
                            }
                        }
                        Event::Unavailable(reason) => Err(reason.into()),
                        _ => Ok(()),
                    }
                })?;
                *files = Some(Service { desktop, pending });
            }
            Ok(std::cell::RefMut::map(files, |files| {
                files.as_mut().unwrap()
            }))
        }
    }
}
#[cfg(feature = "desktop-services")]
pub(crate) use service::Service;

impl App {
    /// Shows the file dialogs `ui`'s controls asked for. A dialog that cannot
    /// be shown is reported to [`App::on_error`] and answered as cancelled.
    pub(crate) fn file_dialogs(&self, ui: &Rc<Ui>) -> Result<()> {
        for (request, dialog) in ui.take_file_dialogs() {
            if let Err(error) = self.show(ui, request, &dialog) {
                self.report(Err(error))?;
                self.report(ui.files_chosen(request, None))?;
            }
        }
        Ok(())
    }

    #[cfg(feature = "desktop-services")]
    fn show(
        &self,
        ui: &Rc<Ui>,
        request: aegle_ui::FileRequest,
        dialog: &aegle_ui::FileDialog,
    ) -> Result<()> {
        use aegle_ui::FileMode;
        let patterns: Vec<Vec<&str>> = (dialog.filters.iter())
            .map(|(_, patterns)| patterns.iter().map(String::as_str).collect())
            .collect();
        let filters: Vec<(&str, &[&str])> = (dialog.filters.iter().zip(&patterns))
            .map(|((name, _), patterns)| (name.as_str(), patterns.as_slice()))
            .collect();
        let native = aegle_desktop::FileDialog {
            title: &dialog.title,
            filters: &filters,
            multiple: dialog.mode == FileMode::OpenMany,
            directory: dialog.mode == FileMode::Folder,
            name: &dialog.name,
            folder: dialog.folder.as_deref(),
        };
        let service = self.file_service()?;
        let id = if dialog.mode == FileMode::Save {
            service.desktop.save_file(&native)?
        } else {
            service.desktop.open_file(&native)?
        };
        service
            .pending
            .borrow_mut()
            .insert(id, (Rc::downgrade(ui), request));
        Ok(())
    }

    #[cfg(not(feature = "desktop-services"))]
    fn show(&self, _: &Rc<Ui>, _: aegle_ui::FileRequest, _: &aegle_ui::FileDialog) -> Result<()> {
        Err("file dialogs need aegle-app's desktop-services feature".into())
    }
}
