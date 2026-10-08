#![allow(unsafe_code)]
use crate::Preferences;
use windows::{
    Win32::{
        System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            Input::KeyboardAndMouse::GetDoubleClickTime,
            WindowsAndMessaging::{
                SPI_GETCLIENTAREAANIMATION, SPI_GETHIGHCONTRAST, SystemParametersInfoW,
            },
        },
    },
    core::{BOOL, w},
};

/// Reads the app color scheme, high contrast, client-area animation, text
/// scale and double-click time settings.
/// A setting the system cannot report stays `None`.
pub(crate) fn read() -> Preferences {
    let mut light = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: the data pointer and size describe a live DWORD local.
    let dark = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut light).cast()),
            Some(&mut size),
        )
    }
    .is_ok()
    .then_some(light == 0);
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // SAFETY: SPI_GETHIGHCONTRAST writes one HIGHCONTRASTW whose cbSize is set.
    let high_contrast = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&raw mut contrast).cast()),
            Default::default(),
        )
    }
    .is_ok()
    .then(|| contrast.dwFlags.contains(HCF_HIGHCONTRASTON));
    let mut animate = BOOL(1);
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL.
    let reduced_motion = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut animate).cast()),
            Default::default(),
        )
    }
    .is_ok()
    .then(|| !animate.as_bool());
    // The Settings "Text size" slider stores a percentage, 100–225.
    let mut percent = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: the data pointer and size describe a live DWORD local.
    let text_scale = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Accessibility"),
            w!("TextScaleFactor"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut percent).cast()),
            Some(&mut size),
        )
    }
    .is_ok()
    .then_some(percent)
    .filter(|p| (50..=400).contains(p))
    .map(|p| p as u16);
    // SAFETY: GetDoubleClickTime takes no arguments and cannot fail.
    let double_click = unsafe { GetDoubleClickTime() };
    Preferences {
        dark,
        high_contrast,
        reduced_motion,
        text_scale,
        double_click: Some(std::time::Duration::from_millis(double_click.into())),
    }
}
