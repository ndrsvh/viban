//! Forward-only migration runner backed by a `migrations` table that records
//! every applied version.

use tokio_rusqlite::rusqlite::Connection;

use super::schema::MIGRATIONS;

/// Applies every migration newer than the highest recorded version. Each
/// migration runs in its own transaction, so a failure leaves the database at
/// the last good version.
pub fn run(conn: &mut Connection) -> tokio_rusqlite::rusqlite::Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS migrations (version INTEGER PRIMARY KEY);")?;

    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM migrations",
        [],
        |row| row.get(0),
    )?;

    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = index as i64 + 1;
        if version > current {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute("INSERT INTO migrations (version) VALUES (?1)", [version])?;
            tx.commit()?;
        }
    }
    Ok(())
}
