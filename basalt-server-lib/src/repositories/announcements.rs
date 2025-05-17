use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Sqlite};
use time::OffsetDateTime;
use utoipa::ToSchema;

use super::users::UserId;

#[derive(
    Debug,
    Clone,
    Hash,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    ToSchema,
    derive_more::From,
    derive_more::Into,
    sqlx::Type,
    PartialOrd,
    Ord,
)]
#[sqlx(transparent)]
pub struct AnnouncementId(String);

impl AnnouncementId {
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Announcement {
    pub id: AnnouncementId,
    pub sender: UserId,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = Date)]
    pub time: OffsetDateTime,
    pub message: String,
}

pub async fn create_announcement(
    db: impl Executor<'_, Database = Sqlite>,
    sender: &UserId,
    message: impl AsRef<str>,
) -> anyhow::Result<Announcement> {
    let id = AnnouncementId::new();
    let message = message.as_ref();
    sqlx::query_as!(
        Announcement,
        "INSERT INTO announcements (id, sender, message) VALUES (?, ?, ?) RETURNING id, sender, time, message",
        id,
        sender,
        message
    )
    .fetch_one(db)
    .await
    .context("Failed to create submission history")
}

pub async fn get_announcements(
    db: impl Executor<'_, Database = Sqlite>,
) -> anyhow::Result<Vec<Announcement>> {
    sqlx::query_as!(
        Announcement,
        "SELECT id, sender, time, message FROM announcements ORDER BY time ASC",
    )
    .fetch_all(db)
    .await
    .context("Failed to create submission history")
}

pub async fn delete_announcement(
    db: impl Executor<'_, Database = Sqlite>,
    id: &AnnouncementId,
) -> anyhow::Result<Option<Announcement>> {
    sqlx::query_as!(
        Announcement,
        "DELETE FROM announcements WHERE id = ? RETURNING id, sender, time, message",
        id
    )
    .fetch_optional(db)
    .await
    .context("Failed to create submission history")
}

#[cfg(test)]
mod tests {
    use crate::{
        repositories::{announcements::Announcement, users::Role},
        testing::{mock_db, users_repositories::dummy_user},
    };

    #[tokio::test]
    async fn create_announcement() {
        let (f, sql) = mock_db().await;
        let user = dummy_user(&sql.db, "dummy_user", "foobar", Role::Competitor).await;
        let announcement = super::create_announcement(&sql.db, &user.id, "hello world")
            .await
            .unwrap();

        assert_eq!(announcement.sender, user.id);
        assert_eq!(&announcement.message, "hello world");
        drop(f)
    }

    #[tokio::test]
    async fn get_announcements() {
        let (f, sql) = mock_db().await;
        let user = dummy_user(&sql.db, "dummy_user", "foobar", Role::Competitor).await;
        super::create_announcement(&sql.db, &user.id, "foo")
            .await
            .unwrap();
        super::create_announcement(&sql.db, &user.id, "bar")
            .await
            .unwrap();

        let ann = super::get_announcements(&sql.db).await.unwrap();

        assert!(ann.iter().any(|a| a.message == "foo"));
        assert!(ann.iter().any(|a| a.message == "bar"));
        drop(f)
    }

    #[tokio::test]
    async fn delete_announcement() {
        let (f, sql) = mock_db().await;
        let user = dummy_user(&sql.db, "dummy_user", "foobar", Role::Competitor).await;
        let Announcement { id, .. } = super::create_announcement(&sql.db, &user.id, "foo")
            .await
            .unwrap();

        let deleted = super::delete_announcement(&sql.db, &id)
            .await
            .unwrap()
            .expect("id is the announcement we just added");
        assert_eq!(deleted.id, id);

        let ann = super::get_announcements(&sql.db).await.unwrap();
        assert!(ann.is_empty());
        drop(f)
    }
}
