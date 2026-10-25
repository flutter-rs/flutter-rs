//! The platform-thread registry that maps a channel name to its [`Channel`].

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Weak},
};

use log::warn;

use crate::FlutterEngineWeakRef;

use super::Channel;
use crate::channel::platform_message::PlatformMessage;

/// Every [`Channel`] registered with an engine, keyed by channel name.
///
/// The registry is only ever touched from the platform thread: the engine's
/// platform message callback looks a channel up here for every message Dart
/// sends.
#[derive(Default)]
pub struct ChannelRegistry {
    channels: HashMap<String, Arc<dyn Channel>>,
    engine: FlutterEngineWeakRef,
}

impl fmt::Debug for ChannelRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChannelRegistry")
            .field("channels", &self.channels.len())
            .finish_non_exhaustive()
    }
}

impl ChannelRegistry {
    /// An empty registry, not yet attached to an engine.
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }

    /// Attach the registry to `engine`, which it needs to answer messages for
    /// channels nobody registered.
    pub fn init(&mut self, engine: FlutterEngineWeakRef) {
        self.engine = engine;
    }

    /// Register `channel` and hand back a weak reference to it.
    ///
    /// Registering a second channel under a name that is already taken replaces
    /// the first one and logs a warning: the earlier registration's `Weak`
    /// immediately goes dangling, so whatever held it silently stops working.
    pub fn register_channel<C>(&mut self, mut channel: C) -> Weak<C>
    where
        C: Channel + 'static,
    {
        channel.init(self.engine.clone());
        let name = channel.name().to_owned();
        let arc = Arc::new(channel);
        let weak = Arc::downgrade(&arc);
        if self.channels.insert(name.clone(), arc).is_some() {
            warn!("channel {name} was already registered; the previous registration is now dead");
        }
        weak
    }

    /// Remove the channel registered under `channel_name`, returning it.
    pub fn remove_channel(&mut self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.channels.remove(channel_name)
    }

    /// The channel registered under `channel_name`, if any.
    ///
    /// The `Arc` is cloned out of the registry so that callers can release the
    /// lock guarding the registry *before* handing control to the channel's
    /// handler: handlers are user code and may register or remove channels.
    #[must_use]
    pub fn channel(&self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.channels.get(channel_name).cloned()
    }

    /// Answer a message no channel claimed with an empty reply.
    ///
    /// An empty reply is what the framework reads as "no plugin implements this
    /// channel", and answering is also what frees the engine-side response
    /// handle. If the engine is already gone there is nothing to answer with,
    /// so the handle is dropped (and complains on the way out).
    pub fn reject(&self, mut message: PlatformMessage) {
        warn!(
            "No plugin registered to handle messages from channel: {}",
            message.channel
        );
        if let Some(handle) = message.response_handle.take() {
            match self.engine.upgrade() {
                Some(engine) => engine.send_platform_message_response(handle, &[]),
                // Reached during shutdown, while Dart is still flushing
                // messages. Dropping the handle leaks a little engine memory,
                // which beats panicking inside an `extern "C"` callback.
                None => warn!(
                    "the engine is gone; dropping the response for channel {}",
                    message.channel
                ),
            }
        }
    }
}
