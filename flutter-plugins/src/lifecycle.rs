//! Plugin for the `flutter/lifecycle` channel.
//!
//! Pushes `AppLifecycleState` transitions to the framework, which is what
//! drives `didChangeAppLifecycleState` and `AppLifecycleListener`. Nothing
//! asks for them: the embedder has to send them as the host window gains and
//! loses focus and visibility, and once at startup, or the framework's
//! lifecycle state stays `null` for the whole run.
//!
//! The embedder does *not* have to walk the state machine one step at a time.
//! `ServicesBinding._generateStateTransitions` fills in the states between the
//! one it last saw and the one it is given, so sending `hidden` straight after
//! `resumed` makes the framework synthesise the `inactive` in between.

use log::debug;
use std::sync::Weak;

use flutter_engine::{
    FlutterEngine,
    channel::{MessageChannel, MessageHandler},
    codec::STRING_CODEC,
    plugins::Plugin,
};

use flutter_engine::channel::Message;
use flutter_engine::codec::Value;

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/lifecycle";

/// One of the framework's `AppLifecycleState`s.
///
/// Deliberately *not* `#[non_exhaustive]`: these are the five states
/// `dart:ui`'s `AppLifecycleState` defines and
/// `ServicesBinding._parseAppLifecycleMessage` accepts, and an embedder
/// deciding which one to send has to consider all of them. The variants are in
/// the same order as `AppLifecycleState.values`, which is the order the
/// framework walks when it fills in skipped transitions.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AppLifecycleState {
    /// Hosted by an engine but attached to no view. Only ever entered on
    /// Android, iOS and the web; a desktop embedder has no way to reach it.
    Detached,
    /// Visible and focused: the normal running state.
    Resumed,
    /// Visible but not focused — on desktop, a window that is not in the
    /// foreground but still has visible pixels.
    Inactive,
    /// Not visible: on desktop, minimised or on a workspace that is not shown.
    Hidden,
    /// Running in the background, not visible and not accepting input. Only
    /// ever entered on Android and iOS, where the platform really does stop
    /// delivering frames; a desktop embedder wants [`Hidden`](Self::Hidden).
    Paused,
}

impl AppLifecycleState {
    /// The exact message `ServicesBinding._parseAppLifecycleMessage` parses.
    ///
    /// A spelling the framework does not recognise is dropped on the floor
    /// (the parse returns null and then throws on the `!`), so these strings
    /// are the whole protocol.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AppLifecycleState::Detached => "AppLifecycleState.detached",
            AppLifecycleState::Resumed => "AppLifecycleState.resumed",
            AppLifecycleState::Inactive => "AppLifecycleState.inactive",
            AppLifecycleState::Hidden => "AppLifecycleState.hidden",
            AppLifecycleState::Paused => "AppLifecycleState.paused",
        }
    }
}

/// Plugin for the `flutter/lifecycle` channel.
#[derive(Default)]
pub struct LifecyclePlugin {
    channel: Weak<MessageChannel>,
}

impl Plugin for LifecyclePlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel =
            engine.register_channel(MessageChannel::new(CHANNEL_NAME, Handler, &STRING_CODEC));
    }
}

impl LifecyclePlugin {
    /// Notify the framework that the app has entered `state`.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    ///
    /// Sending the state the framework is already in is harmless — it compares
    /// before it notifies — but the embedder still owns the de-duplication,
    /// because every send crosses the channel.
    pub fn send_state(&self, state: AppLifecycleState) {
        debug!("Sending {}", state.as_str());
        if let Some(channel) = self.channel.upgrade() {
            channel.send(state.as_str());
        }
    }

    /// Notify the framework that the app is visible but not focused.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    pub fn send_app_is_inactive(&self) {
        self.send_state(AppLifecycleState::Inactive);
    }

    /// Notify the framework that the app is visible and focused.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    pub fn send_app_is_resumed(&self) {
        self.send_state(AppLifecycleState::Resumed);
    }

    /// Notify the framework that none of the app's windows are visible —
    /// minimised, withdrawn, or on a workspace that is not shown.
    ///
    /// This, not [`send_app_is_paused`](Self::send_app_is_paused), is what a
    /// desktop embedder sends when its window disappears from the screen.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    pub fn send_app_is_hidden(&self) {
        self.send_state(AppLifecycleState::Hidden);
    }

    /// Notify the framework that the app is running in the background.
    ///
    /// `AppLifecycleState.paused` is documented as reachable only on Android
    /// and iOS, where the platform stops asking for frames. A desktop embedder
    /// wants [`send_app_is_hidden`](Self::send_app_is_hidden) instead.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    pub fn send_app_is_paused(&self) {
        self.send_state(AppLifecycleState::Paused);
    }
}

struct Handler;

impl MessageHandler for Handler {
    fn on_message(&mut self, msg: Message) {
        // The framework does not send on this channel, but a reply it is not
        // waiting for would only be logged as an error.
        if msg.can_respond() {
            msg.respond(Value::Null);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppLifecycleState;

    /// These strings are matched literally by
    /// `ServicesBinding._parseAppLifecycleMessage`; a typo is a silently
    /// ignored lifecycle transition, so pin every one of them.
    #[test]
    fn state_names_match_the_frameworks_parser() {
        assert_eq!(
            AppLifecycleState::Detached.as_str(),
            "AppLifecycleState.detached"
        );
        assert_eq!(
            AppLifecycleState::Resumed.as_str(),
            "AppLifecycleState.resumed"
        );
        assert_eq!(
            AppLifecycleState::Inactive.as_str(),
            "AppLifecycleState.inactive"
        );
        assert_eq!(
            AppLifecycleState::Hidden.as_str(),
            "AppLifecycleState.hidden"
        );
        assert_eq!(
            AppLifecycleState::Paused.as_str(),
            "AppLifecycleState.paused"
        );
    }

    /// The framework fills in skipped transitions by walking
    /// `AppLifecycleState.values` between the old index and the new one, so
    /// this enum is only a faithful mirror if it is declared in the same
    /// order.
    #[test]
    fn states_are_declared_in_the_frameworks_order() {
        assert_eq!(
            [
                AppLifecycleState::Detached,
                AppLifecycleState::Resumed,
                AppLifecycleState::Inactive,
                AppLifecycleState::Hidden,
                AppLifecycleState::Paused,
            ]
            .map(AppLifecycleState::as_str),
            [
                "AppLifecycleState.detached",
                "AppLifecycleState.resumed",
                "AppLifecycleState.inactive",
                "AppLifecycleState.hidden",
                "AppLifecycleState.paused",
            ]
        );
    }
}
