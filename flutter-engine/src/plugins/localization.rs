//! Plugin to work with locales.
//! It handles flutter/localization type message.

use super::prelude::*;

use log::{debug, error, info, warn};

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/localization";

pub struct LocalizationPlugin {
    channel: Weak<JsonMethodChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl Default for LocalizationPlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler)),
        }
    }
}

impl Plugin for LocalizationPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        self.channel =
            registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

impl LocalizationPlugin {
    pub fn send_locale(&self, locale: locale_config::Locale) {
        debug!("Sending locales to flutter");
        if let Some(channel) = self.channel.upgrade() {
            let mut languages = Vec::<String>::new();
            for (tag, language) in locale.tags() {
                if tag.is_some() {
                    continue;
                }
                // This is kind of a hack. `locale_config` doesn't provide a way to get the components of a locale,
                // but `unic-locale` doesn't support getting the system locales. So we use the former crate to get
                // the current locale and then use `unic-locale` to parse it.
                if let Ok(loc) = unic_locale::parser::parse_locale(language.as_ref()) {
                    info!("Available locale: {}", loc);
                    languages.push(loc.get_language().to_owned());
                    languages.push(loc.get_region().unwrap_or_default().to_owned());
                    languages.push(loc.get_script().unwrap_or_default().to_owned());
                    languages.push(loc.get_variants().first().map_or("", |v| *v).to_owned());
                } else {
                    warn!("Failed to parse language range: {}", language);
                }
            }
            channel.invoke_method(MethodCall {
                method: "setLocale".into(),
                args: json_value!(languages),
            })
        } else {
            error!("Failed to upgrade channel to send message");
        }
    }
}

struct Handler;

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        _runtime_data: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        debug!("got method call {} with args {:?}", call.method, call.args);
        Err(MethodCallError::NotImplemented)
    }
}
