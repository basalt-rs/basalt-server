use std::time::Duration;

use derive_more::{Deref, DerefMut, From, Into};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteTypeInfo, Decode, Encode};
use utoipa::ToSchema;

/// Define a type to be used as an ID (wraps a string)
///
/// Adds a `new` method that creates a random id using
#[macro_export]
macro_rules! define_id_type {
    ($name: ident) => {
        #[derive(
            ::derive_more::Debug,
            ::derive_more::From,
            ::derive_more::Into,
            ::utoipa::ToSchema,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
        )]
        pub struct $name([u8; $name::LEN]);

        impl $name {
            const LEN: usize = 20;

            #[allow(clippy::new_without_default)] // default is kind of bad here as new generates a random string
            pub fn new() -> Self {
                use rand::{distributions::Alphanumeric, Rng};
                let mut it = rand::thread_rng().sample_iter(Alphanumeric);
                let buf: [u8; Self::LEN] = std::array::from_fn(|_| it.next().unwrap());
                Self(buf)
            }

            fn as_str(&self) -> &str {
                // SAFETY: we define this as an array of alphanumeric characters, so it's already
                // utf-8
                unsafe { str::from_utf8_unchecked(&self.0) }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.as_str())
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                assert!(value.len() == Self::LEN);
                Self(
                    value
                        .as_bytes()
                        .try_into()
                        .expect("if value.len() == Self::LEN, then this works"),
                )
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::from(value.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let s: &str = <&str>::deserialize(deserializer)?;
                if s.len() != Self::LEN {
                    return Err(serde::de::Error::custom(format!(
                        "Invalid string length, got {}, expected {}",
                        s.len(),
                        Self::LEN
                    )));
                }
                Ok(Self::from(s))
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                self.as_str().serialize(serializer)
            }
        }

        impl sqlx::Type<sqlx::Sqlite> for $name {
            fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
                <&str as sqlx::Type<sqlx::Sqlite>>::type_info()
            }

            fn compatible(ty: &sqlx::sqlite::SqliteTypeInfo) -> bool {
                <&str as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
            }
        }

        impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for $name {
            fn encode_by_ref(
                &self,
                args: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <String as sqlx::Encode<sqlx::Sqlite>>::encode(self.as_str().to_string(), args)
            }
        }

        impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for $name {
            fn decode(
                value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let s = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
                if s.len() != Self::LEN {
                    Err(format!(
                        "Invalid length of string.  Got {}, expected {}",
                        s.len(),
                        Self::LEN
                    ))?
                }

                Ok(Self(
                    s.as_bytes()
                        .try_into()
                        .expect("if value.len() == Self::LEN, then this works"),
                ))
            }
        }
    };
}

/// Define a new enum that works with sqlx via integer serialisation
///
/// If parentheses are provided after the name, it will be used as a mapper from the type in
/// parens, see submissions repo.
#[macro_export]
macro_rules! define_sqlx_enum {
    // define_sqlx_enum! {
    //     pub enum Foo(Bar) {
    //         A = Bar::A,
    //         B = Bar::B,
    //     }
    // }
    (
        $(#[$($attr: tt)+])*
        pub enum $name: ident($map_from: ty) {
            $variant0: ident = $pat0: pat,
            $($variant: ident = $pat: pat),+$(,)?
        }
    ) => {
        define_sqlx_enum! {
            $(#[$($attr)+])*
            pub enum $name {
                $variant0,
                $($variant),+
            }
        }

        impl From<$map_from> for $name {
            fn from(value: $map_from) -> Self {
                match value {
                    $pat0 => Self::$variant0,
                    $($pat => Self::$variant),+
                }
            }
        }
    };
    // define_sqlx_enum! {
    //     pub enum Foo {
    //         A,
    //         B,
    //     }
    // }
    (
        $(#[$($attr: tt)+])*
        pub enum $name: ident {
            $variant0: ident,
            $($variant: ident),+$(,)?
        }
    ) => {
        #[derive(
            ::derive_more::Debug,
            ::serde::Deserialize,
            ::serde::Serialize,
            ::sqlx::Type,
            ::utoipa::ToSchema,
            Clone,
            Copy,
            Eq,
            PartialEq,
            Hash,
        )]
        #[repr(i64)]
        $(#[$($attr)+])*
        pub enum $name {
            $variant0 = 0,
            $($variant),+
        }

        impl From<$name> for i64 {
            fn from(value: $name) -> Self {
                value as _
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                assert!(value >= 0);
                [Self::$variant0, $(Self::$variant),+][value as usize]
            }
        }

        impl From<$name> for i32 {
            fn from(value: $name) -> Self {
                value as _
            }
        }

        impl From<i32> for $name {
            fn from(value: i32) -> Self {
                assert!(value >= 0);
                [Self::$variant0, $(Self::$variant),+][value as usize]
            }
        }

    }
}

#[derive(Clone, Debug, From, Into, Deref, DerefMut, Deserialize, Serialize, ToSchema)]
#[serde(transparent)]
pub struct WrappedDuration(Duration);

impl From<i64> for WrappedDuration {
    fn from(value: i64) -> Self {
        WrappedDuration(Duration::from_nanos(value as u64))
    }
}

// This is awful
impl sqlx::Type<sqlx::Sqlite> for WrappedDuration {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <i64 as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
    fn compatible(ty: &SqliteTypeInfo) -> bool {
        <i64 as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

impl<'q> Encode<'q, sqlx::Sqlite> for WrappedDuration {
    // https://docs.rs/sqlx-sqlite/0.8.6/src/sqlx_sqlite/types/int.rs.html#103
    fn encode_by_ref(
        &self,
        args: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let nanos: u64 = self
            .0
            .as_nanos()
            .try_into()
            .map_err(|e| {
                // NOTE: we're just panicing here, since u64::MAX nanos is more than 500 years,
                // which we can just assume is a duration that will not show up.
                format!(
                    "Max duration: {:?}, got {:?}: {:?}",
                    Duration::from_nanos(u64::MAX),
                    self.0,
                    e
                )
            })
            .unwrap();
        // NOTE: This cast is bad as it will do weird things like u64::MAX -> -1, but that's a
        // fine, as long as we cast it back when we actually need to read the value.
        <i64 as Encode<sqlx::Sqlite>>::encode(nanos as i64, args)
    }
}

impl<'r> Decode<'r, sqlx::Sqlite> for WrappedDuration {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        // NOTE: This i64 -> u64 cast is the opposite of the one in `encode_by_ref`
        let nanos = <i64 as Decode<sqlx::Sqlite>>::decode(value)? as u64;
        Ok(WrappedDuration(Duration::from_nanos(nanos)))
    }
}
