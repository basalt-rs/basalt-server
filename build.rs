use anyhow::Context;

const INITIAL_DATA_PATH: &str = "./initial_data.db";

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    println!("cargo::rerun-if-changed=migration.sql");
    std::env::set_var("DATABASE_URL", "sqlite:initial_data.db");

    tokio::fs::File::create(INITIAL_DATA_PATH)
        .await
        .context("Failed to create db")?;

    let db = sqlx::sqlite::SqlitePool::connect("sqlite:initial_data.db")
        .await
        .context("Failed to create database layer")?;

    sqlx::raw_sql(include_str!("./migration.sql"))
        .execute(&db)
        .await
        .context("Failed to execute migration")?;

    Ok(())
}
