CREATE TABLE IF NOT EXISTS users (
  id VARCHAR(32) NOT NULL PRIMARY KEY,
  username VARCHAR(32) NOT NULL UNIQUE,
  display_name VARCHAR(64),
  password_hash TEXT NOT NULL,
  role INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT NOT NULL PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  expires_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS submission_history (
  id VARCHAR(32) NOT NULL PRIMARY KEY,
  submitter VARCHAR(32) NOT NULL REFERENCES users(id),
  time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  compile_fail BOOLEAN NOT NULL,
  code TEXT NOT NULL,
  question_index INTEGER NOT NULL,
  score FLOAT NOT NULL,
  success BOOLEAN NOT NULL,
  language TEXT NOT NULL
);

-- History of tests that have been run on submissions
CREATE TABLE IF NOT EXISTS submission_test_history (
  submission VARCHAR(32) NOT NULL REFERENCES submission_history(id),
  test_index INTEGER NOT NULL,
  result VARCHAR(32) NOT NULL,
  stdout TEXT,
  stderr TEXT,
  exit_status INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS announcements (
    id VARCHAR(32) NOT NULL PRIMARY KEY,
    sender VARCHAR(32) NOT NULL REFERENCES users(id),
    time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    message TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS test_runs (
    id VARCHAR(32) NOT NULL PRIMARY KEY,
    user_id VARCHAR(32) NOT NULL REFERENCES users(id),
    question_index INTEGER NOT NULL,
    time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
