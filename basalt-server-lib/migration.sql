CREATE TABLE IF NOT EXISTS users (
  username VARCHAR(32) NOT NULL PRIMARY KEY,
  password_hash TEXT NOT NULL,
  role INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT NOT NULL PRIMARY KEY,
  username TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  FOREIGN KEY (username) REFERENCES users(username)
)
