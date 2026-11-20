//! The WinUI 3 settings window, loaded from XAML markup.

use std::ffi::c_void;

use windows::Foundation::TypedEventHandler;
use windows::Graphics::SizeInt32;
use windows_core::{h, Interface, Result, HRESULT, HSTRING};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, SetForegroundWindow, ICON_BIG, ICON_SMALL, WM_SETICON};

use crate::bindings::Microsoft::UI::Windowing::{AppWindow, AppWindowClosingEventArgs, OverlappedPresenter};
use crate::bindings::Microsoft::UI::Xaml::Controls::Primitives::ButtonBase;
use crate::bindings::Microsoft::UI::Xaml::Controls::{InfoBar, Panel, TextBlock, ToggleSwitch};
use crate::bindings::Microsoft::UI::Xaml::Markup::XamlReader;
use crate::bindings::Microsoft::UI::Xaml::Media::MicaBackdrop;
use crate::bindings::Microsoft::UI::Xaml::{FrameworkElement, RoutedEventHandler, UIElement, Visibility, Window};
use crate::markup::{keycap, page, row};
use crate::shortcut::Shortcut;
use crate::bindings::Microsoft::UI::Xaml::Application;
use crate::gui::with_ctx;
use crate::{autostart, elevation, pin, recorder, settings, tray};

const WIDTH: i32 = 480;
const HEIGHT: i32 = 856;

#[windows_core::interface("EECDBF0E-BAE9-4CB6-A68E-9598E1CB57BB")]
unsafe trait IWindowNative: windows_core::IUnknown {
    fn window_handle(&self, hwnd: *mut *mut c_void) -> HRESULT;
}

/// Shows `s` as a row of keycaps in `panel`.
pub fn fill_keys(panel: &Panel, s: Shortcut) -> Result<()> {
    let children = panel.Children()?;
    children.Clear()?;
    for key in s.keys() {
        children.Append(&load(&keycap(&key))?)?;
    }
    Ok(())
}

pub fn load(xaml: &str) -> Result<FrameworkElement> {
    XamlReader::Load(&HSTRING::from(xaml))?.cast()
}

pub fn find<T: Interface>(root: &FrameworkElement, name: &str) -> Result<T> {
    root.FindName(&HSTRING::from(name))?.cast()
}

fn on_click(root: &FrameworkElement, name: &str, f: impl Fn() + Send + 'static) -> Result<()> {
    find::<ButtonBase>(root, name)?.Click(&RoutedEventHandler::new(move |_, _| {
        f();
        Ok(())
    }))?;
    Ok(())
}

fn on_toggle(root: &FrameworkElement, name: &str, on: bool, f: impl Fn(bool) + Send + 'static) -> Result<()> {
    let switch: ToggleSwitch = find(root, name)?;
    switch.SetIsOn(on)?;
    switch.Toggled(&RoutedEventHandler::new(move |sender, _| {
        f(sender.ok()?.cast::<ToggleSwitch>()?.IsOn()?);
        Ok(())
    }))?;
    Ok(())
}

fn visibility(visible: bool) -> Visibility {
    if visible {
        Visibility::Visible
    } else {
        Visibility::Collapsed
    }
}

/// Unpins one window (0 = all) in the tray process, then redraws the list.
fn unpin(hwnd: usize) {
    tray::send(tray::WM_UNPIN, hwnd, 0);
    with_ctx(|c| c.refresh());
}

pub struct Ui {
    window: Window,
    hwnd: HWND,
    list: Panel,
    header: TextBlock,
    empty: TextBlock,
    unpin_all: UIElement,
    shortcut_keys: Panel,
    hotkey_error: InfoBar,
    /// What the list shows now, to skip rebuilding it when nothing changed.
    shown: Option<Vec<(usize, String)>>,
}

impl Ui {
    pub fn build() -> Result<Self> {
        let window = Window::new()?;
        window.SetTitle(h!("Always on Top"))?;
        window.SetExtendsContentIntoTitleBar(true)?;
        window.SetSystemBackdrop(&MicaBackdrop::new()?)?;

        let root = load(&page())?;
        window.SetContent(&root)?;
        // The tray process reads these from the registry on WM_RELOAD.
        for (toggle, name) in [("BorderToggle", "Border"), ("SoundToggle", "Sound"), ("TrayIconToggle", "TrayIcon")] {
            on_toggle(&root, toggle, settings::get(name, true), move |on| {
                settings::set(name, on);
                tray::send(tray::WM_RELOAD, 0, 0);
            })?;
        }
        // The tray does it, since only an elevated process can make the admin-mode task.
        on_toggle(&root, "AutostartToggle", autostart::enabled(), |on| {
            if tray::send(tray::WM_AUTOSTART, on as usize, 0) == 0 {
                autostart::set(on);
            }
        })?;
        let admin: ToggleSwitch = find(&root, "AdminToggle")?;
        admin.SetIsOn(elevation::wanted())?;
        admin.Toggled(&RoutedEventHandler::new(|sender, _| {
            let switch = sender.ok()?.cast::<ToggleSwitch>()?;
            let on = switch.IsOn()?;
            // Equal when this is the revert below firing again.
            if on != elevation::wanted() && !elevation::switch(on) {
                switch.SetIsOn(!on)?;
            }
            Ok(())
        }))?;
        on_click(&root, "UnpinAll", || unpin(0))?;
        find::<ButtonBase>(&root, "EditShortcut")?.Click(&RoutedEventHandler::new(|sender, _| {
            let xaml_root = sender.ok()?.cast::<UIElement>()?.XamlRoot()?;
            recorder::open(&xaml_root, Shortcut::load())
        }))?;

        let mut hwnd = std::ptr::null_mut();
        unsafe { window.cast::<IWindowNative>()?.window_handle(&mut hwnd).ok()? };
        let scale = unsafe { GetDpiForWindow(hwnd) } as i32;
        // Taskbar and Alt+Tab icon.
        unsafe {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, tray::app_icon(32 * scale / 96) as isize);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, tray::app_icon(16 * scale / 96) as isize);
        }
        let app_window = window.AppWindow()?;
        app_window.Resize(SizeInt32 { Width: WIDTH * scale / 96, Height: HEIGHT * scale / 96 })?;
        let presenter: OverlappedPresenter = app_window.Presenter()?.cast()?;
        presenter.SetIsMaximizable(false)?;
        presenter.SetIsResizable(false)?;
        // Closing ends this process; the tray process keeps running.
        app_window.Closing(&TypedEventHandler::<AppWindow, AppWindowClosingEventArgs>::new(|_, _| {
            Application::Current()?.Exit()
        }))?;

        Ok(Self {
            window,
            hwnd,
            list: find(&root, "PinnedList")?,
            header: find(&root, "PinnedHeader")?,
            empty: find(&root, "EmptyText")?,
            unpin_all: find(&root, "UnpinAll")?,
            shortcut_keys: find(&root, "ShortcutKeys")?,
            hotkey_error: find(&root, "HotkeyError")?,
            shown: None,
        })
    }

    /// Shows the current shortcut everywhere it is named; `ok` is whether it registered.
    pub fn set_shortcut(&self, s: Shortcut, ok: bool) -> Result<()> {
        let label = s.label();
        fill_keys(&self.shortcut_keys, s)?;
        self.empty.SetText(&HSTRING::from(format!("Nothing pinned yet. Focus a window and press {label}.")))?;
        self.hotkey_error.SetMessage(&HSTRING::from(format!(
            "Another app already uses {label} (PowerToys Always on Top?). Click the shortcut below to pick another."
        )))?;
        self.hotkey_error.SetIsOpen(!ok)
    }

    pub fn show(&self) -> Result<()> {
        self.window.Activate()?;
        unsafe { SetForegroundWindow(self.hwnd) };
        Ok(())
    }

    /// Re-reads the pinned windows from the tray process.
    pub fn refresh(&mut self) -> Result<()> {
        let now: Vec<(usize, String)> = tray::pinned().into_iter().map(|h| (h, pin::title(h as HWND))).collect();
        if self.shown.as_ref() == Some(&now) {
            return Ok(());
        }
        self.header.SetText(&HSTRING::from(format!("Pinned windows ({})", now.len())))?;
        self.empty.SetVisibility(visibility(now.is_empty()))?;
        self.unpin_all.SetVisibility(visibility(!now.is_empty()))?;
        let children = self.list.Children()?;
        children.Clear()?;
        for (hwnd, title) in &now {
            let item = load(&row())?;
            find::<TextBlock>(&item, "Title")?.SetText(&HSTRING::from(title))?;
            let hwnd = *hwnd;
            on_click(&item, "Unpin", move || unpin(hwnd))?;
            children.Append(&item)?;
        }
        self.shown = Some(now);
        Ok(())
    }
}
