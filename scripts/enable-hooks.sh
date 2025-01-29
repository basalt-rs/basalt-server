echo "~/.cargo/bin/cargo sqlx prepare -- --lib 2>&1 >/dev/null; git add sqlx-data.json" >.git/hooks/pre-commit
