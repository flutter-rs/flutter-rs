#[macro_export]
macro_rules! json_value {
    ($($tt:tt)*) => {
        {
            use std::convert::TryInto;
            serde_json::json!($($tt)*).try_into().unwrap_or($crate::Value::Null)
        }
    };
}
