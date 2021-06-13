#!/bin/bash

# Build
cargo update
cargo outdated
cargo fmt
cargo fix --allow-dirty --allow-staged
cargo clippy -- -D clippy::pedantic -D clippy::cargo -D clippy::all -W clippy::nursery
