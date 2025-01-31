use std::path::PathBuf;

use anyhow::Context;

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    println!("cargo::rerun-if-changed=migration.sql");
    let cargo_target_dir =
        std::env::var("OUT_DIR").context("Failed to get cargo target directory")?;

    let path = PathBuf::from(cargo_target_dir)
        .join("initial_data")
        .with_extension("db");

    println!(
        "cargo::rustc-env=INITIAL_DATA_PATH={}",
        path.to_str().unwrap()
    );

    let sqlite_uri = format!("sqlite:{}", path.to_str().unwrap());

    std::env::set_var("DATABASE_URL", &sqlite_uri);

    tokio::fs::File::create(path)
        .await
        .context("Failed to create db")?;

    let db = sqlx::sqlite::SqlitePool::connect(&sqlite_uri)
        .await
        .context("Failed to create database layer")?;

    sqlx::raw_sql(include_str!("./migration.sql"))
        .execute(&db)
        .await
        .context("Failed to execute migration")?;

    Ok(())
}
