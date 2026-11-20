//! The "Activation shortcut" dialog. While it is open a low-level keyboard hook records the
//! keys, so shell shortcuts (Win + …) and our own hotkey don't fire, as in PowerToys.

use std::cell::RefCell;
use std::ptr::null_mut;

use windows::Foundation::TypedEventHandler;
use windows_core::{Interface, Result, HSTRING};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN, VK_SPACE, VK_TAB};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::bindings::Microsoft::UI::Xaml::Controls::{
    ContentDialog, ContentDialogButtonClickEventArgs, ContentDialogClosedEventArgs, InfoBar, Panel,
};
use crate::bindings::Microsoft::UI::Xaml::{FrameworkElement, XamlRoot};
use crate::shortcut::{modifier, Shortcut};
use crate::markup::{SECONDARY, XMLNS};
use crate::ui::{fill_keys, find, load};
use crate::gui::with_ctx;

struct Recorder {
    hook: HHOOK,
    /// Modifiers held right now.
    held: u32,
    /// Key pressed during the current chord, 0 if none yet.
    chord_vk: u32,
    /// What Save would apply.
    candidate: Shortcut,
    shown: Option<Shortcut>,
    dialog: ContentDialog,
    keys: Panel,
    warning: InfoBar,
}

thread_local! {
    static REC: RefCell<Option<Recorder>> = const { RefCell::new(None) };
}

fn dialog_xaml() -> String {
    format!(
        r#"<ContentDialog {XMLNS} Style="{{StaticResource DefaultContentDialogStyle}}" Title="Activation shortcut"
            PrimaryButtonText="Save" SecondaryButtonText="Reset" CloseButtonText="Cancel" DefaultButton="Primary">
            <StackPanel Spacing="12" MinWidth="320">
                <TextBlock Text="Press a combination of keys to change this shortcut." TextWrapping="Wrap"/>
                <StackPanel x:Name="Keys" Orientation="Horizontal" Spacing="4" HorizontalAlignment="Center" MinHeight="40" Margin="0,8"/>
                <InfoBar x:Name="Warning" Severity="Warning" IsClosable="False"/>
                <TextBlock Text="Start with Win, Ctrl or Alt. Reset restores Win + Ctrl + T." Style="{{StaticResource CaptionTextBlockStyle}}" {SECONDARY} TextWrapping="Wrap"/>
            </StackPanel>
        </ContentDialog>"#
    )
}

impl Recorder {
    fn warn(&self, message: Option<String>) {
        let _ = self.warning.SetMessage(&HSTRING::from(message.clone().unwrap_or_default()));
        let _ = self.warning.SetIsOpen(message.is_some());
    }

    fn render(&mut self) {
        let shown = if self.held != 0 { Shortcut { mods: self.held, vk: self.chord_vk } } else { self.candidate };
        if self.shown == Some(shown) {
            return;
        }
        self.shown = Some(shown);
        let _ = fill_keys(&self.keys, shown);
        let valid = self.candidate.is_valid();
        let _ = self.dialog.SetIsPrimaryButtonEnabled(valid);
        self.warn((!valid && self.candidate.vk != 0).then(|| "Shortcuts must start with Win, Ctrl or Alt.".into()));
    }

    /// Returns true to swallow the key.
    fn key(&mut self, vk: u32, down: bool) -> bool {
        // With no modifiers held, let the dialog keep Esc, Enter, Tab and Space.
        if self.held == 0 && [VK_ESCAPE, VK_RETURN, VK_TAB, VK_SPACE].contains(&(vk as u16)) {
            return false;
        }
        match modifier(vk) {
            Some(m) if down => {
                if self.held == 0 {
                    self.chord_vk = 0;
                }
                self.held |= m;
            }
            Some(m) => self.held &= !m,
            None if down => {
                self.chord_vk = vk;
                self.candidate = Shortcut { mods: self.held, vk };
            }
            None => {}
        }
        self.render();
        true
    }
}

unsafe extern "system" fn hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lp as *const KBDLLHOOKSTRUCT) };
        let down = matches!(wp as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let swallow = REC.with(|r| Some(r.try_borrow_mut().ok()?.as_mut()?.key(k.vkCode, down)));
        if swallow == Some(true) {
            return 1;
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wp, lp) }
}

fn stop() {
    if let Some(rec) = REC.with(|r| r.borrow_mut().take()) {
        unsafe { UnhookWindowsHookEx(rec.hook) };
    }
}

/// Applies `s`; on failure keeps the dialog open with a warning.
fn apply(s: Shortcut, args: &ContentDialogButtonClickEventArgs) -> Result<()> {
    if with_ctx(|c| c.set_shortcut(s)) != Some(true) {
        args.SetCancel(true)?;
        REC.with(|r| {
            if let Some(rec) = r.borrow().as_ref() {
                rec.warn(Some(format!("{} is already used by another app.", s.label())));
            }
        });
    }
    Ok(())
}

pub fn open(root: &XamlRoot, current: Shortcut) -> Result<()> {
    if REC.with(|r| r.borrow().is_some()) {
        return Ok(());
    }
    let element = load(&dialog_xaml())?;
    let dialog: ContentDialog = element.cast()?;
    dialog.SetXamlRoot(root)?;
    type Click = TypedEventHandler<ContentDialog, ContentDialogButtonClickEventArgs>;
    dialog.PrimaryButtonClick(&Click::new(|_, args| {
        let candidate = REC.with(|r| r.borrow().as_ref().map(|r| r.candidate));
        candidate.map_or(Ok(()), |s| apply(s, args.ok()?))
    }))?;
    dialog.SecondaryButtonClick(&Click::new(|_, args| apply(Shortcut::DEFAULT, args.ok()?)))?;
    dialog.Closed(&TypedEventHandler::<ContentDialog, ContentDialogClosedEventArgs>::new(|_, _| {
        stop();
        Ok(())
    }))?;

    let fe: &FrameworkElement = &element;
    let (keys, warning) = (find(fe, "Keys")?, find(fe, "Warning")?);
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), GetModuleHandleW(std::ptr::null()), 0) };
    let mut rec = Recorder { hook, held: 0, chord_vk: 0, candidate: current, shown: None, dialog: dialog.clone(), keys, warning };
    rec.render();
    REC.with(|r| *r.borrow_mut() = Some(rec));
    dialog.ShowAsync()?;
    Ok(())
}
