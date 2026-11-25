//! The tray as a StatusNotifierItem (ksni, over D-Bus, without GTK): the
//! core's rings drawn at the sizes trays ask for, its tooltip, and its menu,
//! which a left click opens too (there's no popover window yet). Where no
//! StatusNotifierWatcher answers (GNOME without the AppIndicator
//! extension), the core is told once (`Event::NoTray`), and the item waits
//! for one to come.

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{Icon, MenuItem, OfflineReason, ToolTip};
use u4ai_core::icon::Rings;
use u4ai_core::tray::{Entry, TrayView};
use u4ai_core::{Event, Waker, APP_ID};

/// The sizes the icon is drawn at; the host picks the nearest.
const SIZES: [u32; 4] = [22, 32, 48, 64];

struct Item {
    view: TrayView,
    waker: Waker,
}

/// The core's straight RGBA as SNI's ARGB32, in network byte order.
fn argb(rings: &Rings, size: u32) -> Icon {
    let mut data = rings.draw(size);
    for px in data.as_chunks_mut::<4>().0 {
        px.rotate_right(1);
    }
    Icon { width: size as i32, height: size as i32, data }
}

/// A label as dbusmenu reads it: a lone underscore marks the access key,
/// so one in the words (an email's) is written twice.
fn label(words: &str) -> String {
    words.replace('_', "__")
}

/// The tooltip's text: the spec allows a little HTML there, so `&`, `<` and
/// `>` are written as entities.
fn escaped(words: &str) -> String {
    words.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl ksni::Tray for Item {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        APP_ID.into()
    }

    fn title(&self) -> String {
        self.view.title.clone()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        SIZES.iter().map(|&s| argb(&self.view.rings, s)).collect()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip { title: self.view.title.clone(), description: escaped(&self.view.tooltip.join("\n")), ..Default::default() }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.view
            .menu
            .iter()
            .map(|entry| match entry {
                Entry::Separator => MenuItem::Separator,
                Entry::Text(words) => StandardItem { label: label(words), enabled: false, ..Default::default() }.into(),
                Entry::Item { label: words, action, enabled } => {
                    let action = *action;
                    StandardItem { label: label(words), enabled: *enabled, activate: Box::new(move |this: &mut Self| this.waker.send(Event::Act(action))), ..Default::default() }.into()
                }
                Entry::Check { label: words, action, checked } => {
                    let action = *action;
                    CheckmarkItem { label: label(words), checked: *checked, activate: Box::new(move |this: &mut Self| this.waker.send(Event::Act(action))), ..Default::default() }.into()
                }
            })
            .collect()
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        log::warn!("no StatusNotifierWatcher: {reason:?}");
        self.waker.send(Event::NoTray);
        // Keep the item, for a watcher that comes later (the extension
        // turned on, the shell restarted).
        true
    }
}

pub struct Tray {
    waker: Waker,
    handle: Option<Handle<Item>>,
    failed: bool,
}

impl Tray {
    pub fn new(waker: Waker) -> Tray {
        Tray { waker, handle: None, failed: false }
    }

    /// Shows `view`: the item made the first time, changed after.
    pub fn show(&mut self, view: &TrayView) {
        if let Some(handle) = &self.handle {
            let view = view.clone();
            handle.update(move |item| item.view = view);
            return;
        }
        if self.failed {
            return;
        }
        let item = Item { view: view.clone(), waker: self.waker.clone() };
        match item.assume_sni_available(true).spawn() {
            Ok(handle) => self.handle = Some(handle),
            Err(err) => {
                // No session bus at all: there's nowhere to show.
                log::error!("the tray couldn't start: {err}");
                self.failed = true;
                self.waker.send(Event::NoTray);
            }
        }
    }

    pub fn shut_down(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown().wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use u4ai_core::icon::{Rgb, Ring};

    #[test]
    fn the_icon_goes_as_argb_and_words_keep_their_underscores() {
        let rings = Rings { outer: Some(Ring { fill: 1.0, color: Rgb(10, 20, 30) }), inner: None };
        let straight = rings.draw(32);
        let icon = argb(&rings, 32);
        assert_eq!((icon.width, icon.height, icon.data.len()), (32, 32, 32 * 32 * 4));
        let i = straight.chunks(4).position(|p| p[3] == 255).expect("an opaque pixel") * 4;
        assert_eq!(&icon.data[i..i + 4], &[255, 10, 20, 30]);
        assert_eq!(label("first_last@example.com"), "first__last@example.com");
        assert_eq!(escaped("a<b> & c"), "a&lt;b&gt; &amp; c");
    }
}
