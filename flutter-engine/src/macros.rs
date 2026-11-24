/// Build a [`Value`](crate::codec::Value) from `serde_json`-style literal
/// syntax.
///
/// The calling crate does not need to depend on `serde_json` or `log` itself.
/// A value that cannot be represented as a `Value` is logged and becomes
/// [`Value::Null`](crate::codec::Value::Null).
#[macro_export]
macro_rules! json_value {
    ($($tt:tt)*) => {{
        match <$crate::codec::Value as ::core::convert::TryFrom<_>>::try_from(
            $crate::__private::serde_json::json!($($tt)*),
        ) {
            ::core::result::Result::Ok(value) => value,
            ::core::result::Result::Err(error) => {
                $crate::__private::log::error!(
                    "json_value! could not be converted to a Value: {}",
                    error
                );
                $crate::codec::Value::Null
            }
        }
    }};
}
