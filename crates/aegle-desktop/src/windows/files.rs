//! File dialogs through `IFileOpenDialog` and `IFileSaveDialog`.

use std::path::PathBuf;

use windows::{
    Win32::{
        Foundation::ERROR_CANCELLED,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
        UI::Shell::{
            Common::COMDLG_FILTERSPEC, FOS_ALLOWMULTISELECT, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
            FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog,
            IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
        },
    },
    core::{HRESULT, Interface, PCWSTR},
};

/// Shows a file dialog, modally on this thread; `None` when cancelled.
pub(super) fn show(
    save: bool,
    title: &[u16],
    filters: &[(Vec<u16>, Vec<u16>)],
    multiple: bool,
    directory: bool,
    name: &[u16],
    folder: Option<&[u16]>,
) -> windows::core::Result<Option<Vec<PathBuf>>> {
    // SAFETY: COM is initialized on this thread; every string outlives the
    // calls that read it, and returned strings are freed with CoTaskMemFree.
    unsafe {
        let dialog: IFileDialog = match save {
            true => {
                CoCreateInstance::<_, IFileSaveDialog>(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?
                    .into()
            }
            false => {
                CoCreateInstance::<_, IFileOpenDialog>(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?
                    .into()
            }
        };
        let mut options = dialog.GetOptions()? | FOS_FORCEFILESYSTEM;
        if multiple && !save {
            options |= FOS_ALLOWMULTISELECT;
        }
        if directory && !save {
            options |= FOS_PICKFOLDERS;
        }
        dialog.SetOptions(options)?;
        dialog.SetTitle(PCWSTR(title.as_ptr()))?;
        if !filters.is_empty() && (save || !directory) {
            let specs: Vec<_> = filters
                .iter()
                .map(|(name, spec)| COMDLG_FILTERSPEC {
                    pszName: PCWSTR(name.as_ptr()),
                    pszSpec: PCWSTR(spec.as_ptr()),
                })
                .collect();
            dialog.SetFileTypes(&specs)?;
        }
        if save && name.len() > 1 {
            dialog.SetFileName(PCWSTR(name.as_ptr()))?;
        }
        if let Some(folder) = folder
            && let Ok(item) =
                SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(folder.as_ptr()), None)
        {
            dialog.SetFolder(&item)?;
        }
        if let Err(error) = dialog.Show(None) {
            return match error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                true => Ok(None),
                false => Err(error),
            };
        }
        let path = |item: IShellItem| -> windows::core::Result<PathBuf> {
            let name = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = name
                .to_string()
                .map_err(|_| windows::core::Error::from_thread());
            CoTaskMemFree(Some(name.0 as _));
            Ok(PathBuf::from(path?))
        };
        let paths = match save {
            true => vec![path(dialog.GetResult()?)?],
            false => {
                let items = dialog.cast::<IFileOpenDialog>()?.GetResults()?;
                (0..items.GetCount()?)
                    .map(|index| path(items.GetItemAt(index)?))
                    .collect::<windows::core::Result<_>>()?
            }
        };
        Ok(Some(paths))
    }
}
