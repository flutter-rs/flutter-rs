#[macro_export]
macro_rules! json_value {
    ($($tt:tt)*) => {
        {
            use std::convert::TryInto;
            serde_json::json!($($tt)*).try_into().unwrap_or($crate::codec::Value::Null)
        }
    };
}

#[macro_export]
macro_rules! method_call_args {
    (
    $(#$attr:tt)*
    $(@$pub:tt)? struct $args_struct:ident {
        $($(@$field_pub:tt)? $field:ident : $field_type:ty = match map_value($map_name:expr) {
            $($map_pattern:pat => $map_value:expr,)*
        },)*
    }) => {
        $(#$attr)*
        $($pub)? struct $args_struct {
            $($($field_pub)? $field: $field_type),*
        }

        impl std::convert::TryFrom<$crate::codec::Value> for $args_struct {
            type Error = $crate::error::MethodArgsError;
            fn try_from(value: Value) -> Result<Self, Self::Error> {
                use $crate::error::MethodArgsError;
                // first check that we have a map, needed to parse a struct
                let mut map = match value {
                    Value::Map(map) => map,
                    value => return Err(MethodArgsError::WrongType("Map".into(), value)),
                };
                // get fields
                $(let $field: $field_type = match map.remove($map_name) {
                    Some(value) => {
                        match value {
                            $($map_pattern => $map_value,)*
                            value => {
                                return Err(MethodArgsError::WrongType(stringify!($field_type).into(), value));
                            }
                        }
                    },
                    None => {
                        return Err(MethodArgsError::MissingField(stringify!($map_name).into()));
                    }
                };)*
                // return struct
                Ok(Self {
                    $($field,)*
                })
            }
        }
    };
}
