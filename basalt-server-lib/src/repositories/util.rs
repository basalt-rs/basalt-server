use std::{error::Error, time::Duration};

use derive_more::{Deref, DerefMut, From, Into};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteTypeInfo, Decode, Encode};
use utoipa::ToSchema;

#[derive(Debug, derive_more::Display)]
#[display("Invalid id string length, expected {}, got {}", expected, actual)]
pub struct InvalidIdLength {
    pub expected: usize,
    pub actual: usize,
}

impl Error for InvalidIdLength {}

/// Define a type to be used as a randomly generated unique ID
///
/// Defines a tuple struct holding a single array of fixed length that should be used as a unqiue
/// identifier.  See macro implementation for more detail
#[macro_export]
macro_rules! define_id_type {
    ($name: ident) => {
        ident_str::ident_str! {
            #mod_name = concat!(stringify!($name), "_MODULE") =>
            // This is wrapped in a module so that we don't expose the inner type of the id
            #[allow(non_snake_case)]
            mod #mod_name {
                #[derive(
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
                        let buf: [u8; Self::LEN] =
                            std::array::from_fn(|_| it.next().expect("This is an infinite iterator"));
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

                impl std::fmt::Debug for $name {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.debug_tuple(stringify!($name))
                            .field(&self.as_str())
                            .finish()
                    }
                }

                impl std::str::FromStr for $name {
                    type Err = $crate::repositories::util::InvalidIdLength;

                    fn from_str(s: &str) -> Result<Self, Self::Err> {
                        s.as_bytes()
                            .try_into()
                            .map_err(|_| $crate::repositories::util::InvalidIdLength {
                                expected: Self::LEN,
                                actual: s.len(),
                            })
                            .map(Self)
                    }
                }

                /// Parse an ID from a string. This implementation exists to satisfy `sqlx` and must never
                /// be called manually.  If a string needs to be parsed, prefer the [`FromStr`]
                /// implementation.
                ///
                /// [`FromStr`]: std::str::FromStr
                impl From<String> for $name {
                    fn from(value: String) -> Self {
                        value.parse().expect(concat!(
                            "Invalid value pased to From<String> on ",
                            stringify!($name),
                        ))
                    }
                }

                impl<'de> serde::Deserialize<'de> for $name {
                    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                    where
                        D: serde::Deserializer<'de>,
                    {
                        let s: &str = <&str>::deserialize(deserializer)?;
                        s.parse().map_err(serde::de::Error::custom)
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
                        Ok(s.parse()?)
                    }
                }
            }
            pub use #mod_name::$name;
        }
    };
}

/// Define a new enum that works with sqlx via integer serialisation
///
/// If parentheses are provided after the name, it will be used as a mapper from the type in
/// parens, see submissions repo. (see implementation for details)
///
/// The following traits get implemented for the generated struct:
/// - [`serde::Serialize`]/[`serde::Deserialize`] (using kebab-case)
/// - [`sqlx::Type`]
/// - [`utoipa::ToSchema`]
/// - Debug, Clone, Copy, Eq, PartialEq, Hash
/// - From<i64>, Into<i64>, From<i32>, Into<i32>
/// - From<$type> if specified as `Foo($type)`
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
        #[serde(rename_all = "kebab-case")]
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

#[derive(Clone, Debug, From, Into, Deref, DerefMut, ToSchema, PartialEq, Eq, Hash)]
/// Wrapped duration type that allows us to implement sqlx/serde/utoipa traits
///
/// Note: the (de)serialisation of this type uses milliseconds, so any sub-millisecond precision is
/// lost.
pub struct WrappedDuration(Duration);

impl<'de> Deserialize<'de> for WrappedDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(Duration::from_millis(Deserialize::deserialize(
            deserializer,
        )?)))
    }
}

impl Serialize for WrappedDuration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.as_millis().serialize(serializer)
    }
}

impl From<i64> for WrappedDuration {
    fn from(value: i64) -> Self {
        WrappedDuration(Duration::from_nanos(value as u64))
    }
}

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
            .expect("Duration must not exceed 500 years (u64::max nanos)");
        // NOTE: This `cast_signed` is bad as it will do weird things like u64::MAX -> -1, but
        // that's fine, as long as we cast it back when we actually need to read the value.
        <i64 as Encode<sqlx::Sqlite>>::encode(nanos.cast_signed(), args)
    }
}

impl<'r> Decode<'r, sqlx::Sqlite> for WrappedDuration {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        // NOTE: This `cast_unsigned` is to reverse that from `encode_by_ref`
        let nanos = <i64 as Decode<sqlx::Sqlite>>::decode(value)?.cast_unsigned();
        Ok(WrappedDuration(Duration::from_nanos(nanos)))
    }
}
