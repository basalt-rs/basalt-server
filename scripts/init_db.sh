#!/bin/sh

mkdir -p ./.basalt
touch .basalt/.data.db
cargo sqlx prepare --database-url sqlite:$(pwd)/.basalt/data.db
