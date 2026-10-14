//! The macOS application menu. winit builds it with an "About" item that
//! opens the system's standard panel; this points that item at the IDE's
//! own About dialog instead. Elsewhere there is no native menu and both
//! entry points do nothing.

use crate::app::Message;
use iced::{Subscription, Task};

/// Task run once at startup (`App::boot`): retargets the menu's About item
/// on the main thread, through the window's run callback.
pub fn install() -> Task<Message> {
    #[cfg(target_os = "macos")]
    {
        iced::window::oldest().then(|id| match id {
            Some(id) => iced::window::run(id, |_| macos::retarget_about_item()).discard(),
            None => Task::none(),
        })
    }
    #[cfg(not(target_os = "macos"))]
    Task::none()
}

/// Clicks on the native About item, as `Cmd::About` commands. Part of
/// `App::subscription`.
pub fn about_requests() -> Subscription<Message> {
    #[cfg(target_os = "macos")]
    {
        Subscription::run(macos::about_clicks)
    }
    #[cfg(not(target_os = "macos"))]
    Subscription::none()
}

/// AppKit side: the menu item target and the click channel.
#[cfg(target_os = "macos")]
mod macos {
    use crate::app::Message;
    use crate::keymap::Cmd;
    use iced::futures::channel::mpsc::{self, UnboundedSender};
    use iced::futures::{Stream, StreamExt};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{NSObject, NSString};
    use parking_lot::Mutex;

    /// Where menu clicks go: the sender of the live `about_clicks` stream.
    static CLICKS: Mutex<Option<UnboundedSender<()>>> = Mutex::new(None);

    define_class!(
        /// Target object of the About item; forwards its action to `CLICKS`.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "PhpThunderAboutTarget"]
        struct AboutTarget;

        impl AboutTarget {
            /// The About item's action. A closed stream (app shutting down)
            /// just drops the click.
            #[unsafe(method(showAbout:))]
            fn show_about(&self, _sender: Option<&AnyObject>) {
                if let Some(clicks) = CLICKS.lock().as_ref() {
                    let _ = clicks.unbounded_send(());
                }
            }
        }
    );

    impl AboutTarget {
        /// A new target; must be created on the main thread like the menu.
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(());
            // SAFETY: NSObject's designated initializer on a fresh allocation.
            unsafe { msg_send![super(this), init] }
        }
    }

    /// Finds the app menu item whose action is the standard About panel and
    /// makes it open the IDE's dialog, titled in the user's language. Runs
    /// inside `window::run`, which iced calls on the main thread; does
    /// nothing if it is somehow elsewhere or the menu is missing.
    pub fn retarget_about_item() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(menu) = NSApplication::sharedApplication(mtm).mainMenu() else {
            return;
        };
        let Some(app_menu) = menu.itemAtIndex(0).and_then(|item| item.submenu()) else {
            return;
        };
        let standard = sel!(orderFrontStandardAboutPanel:);
        let items = app_menu.itemArray();
        let Some(about) = items.iter().find(|item| item.action() == Some(standard)) else {
            return;
        };
        // A menu item holds its target weakly, so the target lives for the
        // rest of the process.
        let target = Retained::into_raw(AboutTarget::new(mtm));
        // SAFETY: `target` is a valid, never-freed object implementing the
        // `showAbout:` action set right after.
        unsafe {
            about.setTarget(Some(&*target));
            about.setAction(Some(sel!(showAbout:)));
        }
        about.setTitle(&NSString::from_str(&crate::tr!("cmd-about")));
    }

    /// Stream of About clicks for `Subscription::run`: installs a fresh
    /// channel in `CLICKS` and maps each click to `Cmd::About`.
    pub fn about_clicks() -> impl Stream<Item = Message> {
        let (sender, receiver) = mpsc::unbounded();
        *CLICKS.lock() = Some(sender);
        receiver.map(|()| Message::Command(Cmd::About))
    }
}
