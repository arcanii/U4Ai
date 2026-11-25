//! The tray icon in the notification area, from Flow's `windows/src/tray.rs`:
//! a window of its own, never shown, on a `u4ai-tray` thread that runs
//! its messages (not message-only, so it hears `TaskbarCreated` and adds the
//! icon again when Explorer starts again). The core's rings are drawn at the
//! notification area's size into an icon each time they change; a click, a
//! right-click or Enter opens the menu, made afresh from the core's
//! entries, and the item chosen goes to the core as its action. Balloons
//! (`NIF_INFO`), which Windows 10 and 11 show as notifications, tell of
//! limits.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Mutex};

use u4ai_core::icon::Rings;
use u4ai_core::tray::{Entry, TrayView};
use u4ai_core::{Event, Waker};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NINF_KEY, NIN_SELECT,
    NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, ChangeWindowMessageFilterEx, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, GetSystemMetrics, PostMessageW, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenuEx, TranslateMessage,
    HICON, ICONINFO, MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, MSGFLT_ALLOW, SM_CXSMICON, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
    WM_CONTEXTMENU, WM_NULL, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::wide;

/// The message the notification area sends the icon's clicks as.
const MESSAGE: u32 = WM_APP + 1;
/// The icon's number for its window: there's the one.
const ID: u32 = 1;
/// Enter or Space on the icon from the keyboard.
const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;

static WAKER: Mutex<Option<Waker>> = Mutex::new(None);
static ICON: Mutex<Option<Icon>> = Mutex::new(None);
/// The menu's entries, as the core last gave them.
static MENU: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct Icon {
    /// Handles as numbers, since a pointer isn't `Send`.
    window: usize,
    picture: usize,
    tip: [u16; 128],
    shown: bool,
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn prepare(waker: Waker) {
    *locked(&WAKER) = Some(waker);
}

/// Shows `view`: the icon made the first time, its picture, words and menu
/// changed after.
pub fn show(view: &TrayView) {
    *locked(&MENU) = view.menu.clone();
    let mut icon = locked(&ICON);
    if icon.is_none() {
        *icon = make();
    }
    let Some(icon) = icon.as_mut() else { return };
    let old = icon.picture;
    icon.picture = picture(&view.rings).unwrap_or(old);
    let mut words = vec![view.title.clone()];
    words.extend(view.tooltip.iter().cloned());
    icon.tip = fixed(&words.join("\n"));
    if icon.shown {
        call(NIM_MODIFY, icon);
    } else {
        add(icon);
    }
    if old != 0 && old != icon.picture {
        // SAFETY: an icon this made, no longer in the notification area.
        unsafe { DestroyIcon(old as HICON) };
    }
}

/// Takes the icon away as U4Ai quits.
pub fn remove() {
    if let Some(icon) = locked(&ICON).as_mut().filter(|i| i.shown) {
        call(NIM_DELETE, icon);
        icon.shown = false;
    }
}

/// A balloon by the icon, which Windows shows as a notification.
pub fn balloon(title: &str, body: &str) {
    let icon = locked(&ICON);
    let Some(icon) = icon.as_ref().filter(|i| i.shown) else {
        log::info!("no icon for a balloon: {title}: {body}");
        return;
    };
    let mut data = data(icon);
    data.uFlags = NIF_INFO;
    data.szInfoTitle = fixed(title);
    data.szInfo = fixed(body);
    data.dwInfoFlags = NIIF_INFO;
    // SAFETY: the data is filled in as the call reads it.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
}

/// `text` in a fixed buffer of `N` UTF-16 units, the last always nothing,
/// cut short with … where it's longer.
fn fixed<const N: usize>(text: &str) -> [u16; N] {
    let mut words: Vec<u16> = text.encode_utf16().collect();
    if words.len() > N - 1 {
        words.truncate(N - 2);
        // A cut through a surrogate pair would leave half a letter.
        if words.last().is_some_and(|w| (0xd800..0xdc00).contains(w)) {
            words.pop();
        }
        words.push('…' as u16);
    }
    let mut out = [0u16; N];
    out[..words.len()].copy_from_slice(&words);
    out
}

fn data(icon: &Icon) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: icon.window as HWND,
        uID: ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: MESSAGE,
        hIcon: icon.picture as HICON,
        szTip: icon.tip,
        ..Default::default()
    };
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    data
}

fn call(message: u32, icon: &Icon) -> bool {
    // SAFETY: the data is filled in as the call reads it, for a window of
    // U4Ai's own.
    unsafe { Shell_NotifyIconW(message, &data(icon)) != 0 }
}

fn add(icon: &mut Icon) {
    icon.shown = call(NIM_ADD, icon);
    if icon.shown {
        // Version 4's clicks: NIN_SELECT, NIN_KEYSELECT and WM_CONTEXTMENU.
        call(NIM_SETVERSION, icon);
    } else {
        log::warn!("the notification area took no icon");
    }
}

/// The rings as an icon at the notification area's size.
fn picture(rings: &Rings) -> Option<usize> {
    // SAFETY: a size from the system.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64) as u32;
    let pixels = rings.draw(size);
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size as i32,
        // Rows top to bottom, as drawn.
        biHeight: -(size as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..Default::default()
    };
    let info = BITMAPINFO { bmiHeader: header, ..Default::default() };
    // SAFETY: a DIB of `size` × `size` 32-bit pixels, filled within its
    // length; both bitmaps are let go once the icon holds its copies.
    unsafe {
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(std::ptr::null_mut(), &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if color.is_null() || bits.is_null() {
            return None;
        }
        let out = std::slice::from_raw_parts_mut(bits as *mut u8, pixels.len());
        // Windows' order is BGRA.
        for (to, from) in out.as_chunks_mut::<4>().0.iter_mut().zip(pixels.as_chunks::<4>().0) {
            *to = [from[2], from[1], from[0], from[3]];
        }
        // The alpha says what shows; the mask is all there.
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, std::ptr::null());
        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        (!icon.is_null()).then_some(icon as usize)
    }
}

/// Makes the icon's window on a thread of its own, which then runs its
/// messages for good.
fn make() -> Option<Icon> {
    let (said, heard) = mpsc::channel();
    let started = std::thread::Builder::new().name("u4ai-tray".into()).spawn(move || {
        // SAFETY: the class and window are this thread's own; the strings
        // outlive the calls, and `msg` is written by GetMessageW.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = wide("U4AiTray");
            let mut kind: WNDCLASSW = std::mem::zeroed();
            kind.lpfnWndProc = Some(window_proc);
            kind.hInstance = instance;
            kind.lpszClassName = class.as_ptr();
            RegisterClassW(&kind);
            let title = wide("U4Ai");
            let window = CreateWindowExW(WS_EX_TOOLWINDOW, class.as_ptr(), title.as_ptr(), WS_POPUP, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
            if window.is_null() {
                let _ = said.send(None);
                return;
            }
            let created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
            TASKBAR_CREATED.store(created, Ordering::Relaxed);
            ChangeWindowMessageFilterEx(window, created, MSGFLT_ALLOW, std::ptr::null_mut());
            let _ = said.send(Some(window as usize));
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
    if started.is_err() {
        return None;
    }
    let window = heard.recv().ok().flatten()?;
    Some(Icon { window, picture: 0, tip: [0; 128], shown: false })
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == MESSAGE {
        if matches!(lparam as u32 & 0xffff, NIN_SELECT | NIN_KEYSELECT | WM_CONTEXTMENU) {
            // Version 4 gives where the menu goes in wparam; the cursor's
            // place where it's none.
            let (mut x, mut y) = ((wparam & 0xffff) as u16 as i16 as i32, ((wparam >> 16) & 0xffff) as u16 as i16 as i32);
            if x == 0 && y == 0 {
                let mut at = POINT { x: 0, y: 0 };
                // SAFETY: writes the cursor's place into `at`.
                unsafe { GetCursorPos(&mut at) };
                (x, y) = (at.x, at.y);
            }
            menu(window, x, y);
        }
        return 0;
    }
    let created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if created != 0 && message == created {
        // Explorer started again with an empty notification area.
        if let Some(icon) = locked(&ICON).as_mut().filter(|i| i.shown) {
            add(icon);
        }
        return 0;
    }
    // SAFETY: Windows' own handling of everything else.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// A menu item's words as Win32 reads them: `&` marks the access key, so
/// one in the words is written twice.
fn label(words: &str) -> Vec<u16> {
    wide(&words.replace('&', "&&"))
}

/// The menu at (x, y), on the icon's own thread, for its window: each
/// entry's place in the list is its item's number, from 1.
fn menu(window: HWND, x: i32, y: i32) {
    let entries = locked(&MENU).clone();
    let labels: Vec<Vec<u16>> = entries
        .iter()
        .map(|e| match e {
            Entry::Text(words) | Entry::Item { label: words, .. } | Entry::Check { label: words, .. } => label(words),
            Entry::Separator => Vec::new(),
        })
        .collect();
    // SAFETY: a menu made and destroyed here, its strings outliving it; the
    // window takes the keys first, as a notification icon's menu must or it
    // won't close when clicked away from, and the empty message after is
    // Windows' advice for the same.
    let chosen = unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        for (i, (entry, words)) in entries.iter().zip(&labels).enumerate() {
            let flags = match entry {
                Entry::Separator => MF_SEPARATOR,
                Entry::Text(_) => MF_STRING | MF_GRAYED,
                Entry::Item { enabled, .. } => MF_STRING | if *enabled { 0 } else { MF_GRAYED },
                Entry::Check { checked, .. } => MF_STRING | if *checked { MF_CHECKED } else { 0 },
            };
            AppendMenuW(menu, flags, i + 1, if words.is_empty() { std::ptr::null() } else { words.as_ptr() });
        }
        SetForegroundWindow(window);
        let chosen = TrackPopupMenuEx(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, x, y, window, std::ptr::null());
        PostMessageW(window, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen
    };
    let action = (chosen as usize).checked_sub(1).and_then(|i| entries.get(i)).and_then(|e| match e {
        Entry::Item { action, .. } | Entry::Check { action, .. } => Some(*action),
        _ => None,
    });
    if let (Some(action), Some(waker)) = (action, locked(&WAKER).as_ref()) {
        waker.send(Event::Act(action));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_fit_their_buffers_and_keep_their_ampersands() {
        let short: [u16; 128] = fixed("U4Ai");
        assert_eq!(String::from_utf16_lossy(&short[..8]), "U4Ai");
        assert_eq!(short[8], 0);
        let long: [u16; 64] = fixed(&"x".repeat(300));
        assert_eq!((long[62], long[63]), ('…' as u16, 0));
        let pairs: [u16; 128] = fixed(&format!("{}{}", "x".repeat(125), "😀😀"));
        assert_eq!((pairs[125], pairs[126]), ('…' as u16, 0));
        assert_eq!(String::from_utf16_lossy(&label("Fish & Chips")[..12]), "Fish && Chip");
    }
}
