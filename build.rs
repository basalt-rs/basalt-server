const INITIAL_DATA_PATH: &str = "./initial_data.db";

#[tokio::main]
pub async fn main() -> Result<(), String> {
    println!("cargo::rerun-if-changed=migration.sql");
    std::env::set_var("DATABASE_URL", "sqlite:initial_data.db");

    if std::fs::exists(INITIAL_DATA_PATH).expect("Failed to check existence of initial_data.db") {
        std::fs::remove_file(INITIAL_DATA_PATH).unwrap()
    }
    std::fs::File::create_new(INITIAL_DATA_PATH).expect("failed to create db");

    let db = sqlx::sqlite::SqlitePool::connect("sqlite:initial_data.db")
        .await
        .unwrap();

    sqlx::raw_sql(include_str!("./migration.sql"))
        .execute(&db)
        .await
        .unwrap();

    Ok(())
}
