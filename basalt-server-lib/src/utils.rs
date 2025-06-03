use chrono::{DateTime, Utc};

pub fn utc_now() -> DateTime<Utc> {
    chrono::offset::Local::now().to_utc()
}
