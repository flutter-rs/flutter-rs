macro_rules! method_channel {
    ($channel:ty) => {
        impl $crate::channel::Channel for $channel {
            fn name(&self) -> &'static str {
                ChannelImpl::name(self)
            }

            fn init_data(&self) -> Option<Arc<InitData>> {
                ChannelImpl::init_data(self)
            }

            fn init(&mut self, runtime_data: Weak<InitData>, plugin_name: &'static str) {
                ChannelImpl::init(self, runtime_data, plugin_name)
            }

            fn plugin_name(&self) -> &'static str {
                ChannelImpl::plugin_name(self)
            }

            fn handle_platform_message(&self, msg: $crate::ffi::PlatformMessage) {
                $crate::channel::MethodChannel::handle_platform_message(self, msg)
            }

            fn try_as_method_channel(&self) -> Option<&dyn $crate::channel::MethodChannel> {
                Some(self)
            }

            fn try_as_message_channel(&self) -> Option<&dyn $crate::channel::MessageChannel> {
                None
            }
        }
    };
}

macro_rules! message_channel {
    ($channel:ty) => {
        impl $crate::channel::Channel for $channel {
            fn name(&self) -> &'static str {
                ChannelImpl::name(self)
            }

            fn init_data(&self) -> Option<Arc<InitData>> {
                ChannelImpl::init_data(self)
            }

            fn init(&mut self, runtime_data: Weak<InitData>, plugin_name: &'static str) {
                ChannelImpl::init(self, runtime_data, plugin_name)
            }

            fn plugin_name(&self) -> &'static str {
                ChannelImpl::plugin_name(self)
            }

            fn handle_platform_message(&self, msg: $crate::ffi::PlatformMessage) {
                $crate::channel::MessageChannel::handle_platform_message(self, msg)
            }

            fn try_as_method_channel(&self) -> Option<&dyn $crate::channel::MethodChannel> {
                None
            }

            fn try_as_message_channel(&self) -> Option<&dyn $crate::channel::MessageChannel> {
                Some(self)
            }
        }
    };
}
