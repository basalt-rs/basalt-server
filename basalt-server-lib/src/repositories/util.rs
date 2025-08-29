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
            Debug,
            Clone,
            Serialize,
            Deserialize,
            derive_more::From,
            derive_more::Into,
            sqlx::Type,
            ToSchema,
            Eq,
            PartialEq,
            Hash,
        )]
        #[sqlx(transparent)]
        // TODO: replace inner type with [u8; 20] for memory efficiency
        //       We would also then be able to do Copy
        pub struct $name(String);

        impl $name {
            fn new() -> Self {
                use rand::{distributions::Alphanumeric, Rng};
                let id = rand::thread_rng()
                    .sample_iter(Alphanumeric)
                    .take(20)
                    .map(char::from)
                    .collect::<String>();
                Self(id)
            }
        }
    };
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
