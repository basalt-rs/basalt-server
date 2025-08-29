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
  code TEXT NOT NULL,
  question_index INTEGER NOT NULL,
  language TEXT NOT NULL,
  compile_result INTEGER NOT NULL, -- CompileResultState enum
  compile_stdout TEXT NOT NULL,
  compile_stderr TEXT NOT NULL,
  compile_exit_status INTEGER NOT NULL,
  -- The remaining data will be updated after the tests have finished running
  state INTEGER NOT NULL DEFAULT 0, -- SubmissionState
  score FLOAT NOT NULL DEFAULT 0.0,
  success BOOLEAN NOT NULL DEFAULT false,
  time_taken INTEGER NOT NULL DEFAULT 0 -- NOTE: This is stored as a `u64` cast as an `i64`.  Keep that in mind while doing operations on this data in queries.
);

-- Output of each test
CREATE TABLE IF NOT EXISTS test_results (
  submission VARCHAR(32) NOT NULL REFERENCES submission_history(id),
  test_index INTEGER NOT NULL,
  result INTEGER NOT NULL, -- TestResultState enum
  stdout TEXT NOT NULL,
  stderr TEXT NOT NULL,
  exit_status INTEGER NOT NULL,
  time_taken INTEGER NOT NULL, -- NOTE: This is stored as a `u64` cast as an `i64`.  Keep that in mind while doing operations on this data in queries.

  PRIMARY KEY (submission, test_index)
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
