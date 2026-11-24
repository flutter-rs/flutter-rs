//! Plugin for the `flutter/localization` channel.
//!
//! The framework calls `Localization.getStringResource` here for
//! platform-provided strings, which this embedder does not have. The locale
//! list itself does *not* travel over this channel: it goes through
//! [`FlutterEngine::update_locales`](flutter_engine::FlutterEngine::update_locales).

use log::{debug, error, info, warn};
use std::sync::Weak;

use flutter_engine::channel::MethodCall;
use flutter_engine::{
    FlutterEngine, FlutterEngineWeakRef,
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
};

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/localization";

/// Plugin for the `flutter/localization` channel.
#[derive(Default)]
pub struct LocalizationPlugin {
    channel: Weak<MethodChannel>,
    engine: FlutterEngineWeakRef,
}

impl Plugin for LocalizationPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        // `send_locales` goes to the engine, not to the channel, so the handle
        // is kept alongside it.
        self.engine = engine.downgrade();
        self.channel =
            engine.register_channel(MethodChannel::new(CHANNEL_NAME, Handler, &JSON_CODEC));
    }
}

impl LocalizationPlugin {
    /// Sends the system locales to the framework, most preferred first.
    ///
    /// The caller supplies the locales as BCP-47 strings (for example from
    /// `sys_locale::get_locales()`); each is parsed into the
    /// language/region/script/variant quadruple the embedder API expects and
    /// handed to
    /// [`FlutterEngine::update_locales`](flutter_engine::FlutterEngine::update_locales).
    /// Unparseable entries are skipped, and if nothing parses the framework is
    /// left on its default locale rather than being told the list is empty.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped; can be called from any thread, the update hops onto the
    /// platform thread by itself.
    pub fn send_locales<I>(&self, locales: I)
    where
        I: IntoIterator<Item = String>,
    {
        let Some(engine) = self.engine.upgrade() else {
            error!("cannot send locales: the engine is gone");
            return;
        };

        let mut parsed = Vec::new();
        for language in locales {
            if let Ok(loc) = unic_locale::parser::parse_locale(language.as_str()) {
                info!("Available locale: {}", loc);
                parsed.push((
                    loc.id.language.as_str().to_owned(),
                    loc.id
                        .region
                        .map_or(String::new(), |r| r.as_str().to_owned()),
                    loc.id
                        .script
                        .map_or(String::new(), |s| s.as_str().to_owned()),
                    loc.id
                        .variants()
                        .next()
                        .map_or(String::new(), |v| v.as_str().to_owned()),
                ));
            } else {
                warn!("Failed to parse language range: {}", language);
            }
        }

        if parsed.is_empty() {
            warn!("no locale could be parsed; leaving the framework's default in place");
            return;
        }

        debug!("Sending {} locales to flutter", parsed.len());
        // `update_locales` asserts the platform thread; this runs inline when
        // the caller is already on it.
        engine.run_on_platform_thread(move |engine| engine.update_locales(&parsed));
    }
}

struct Handler;

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        debug!(
            "got method call {} with args {:?}",
            call.method(),
            call.raw_args()
        );
        call.not_implemented()
    }
}
