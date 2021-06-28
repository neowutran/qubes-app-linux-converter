#!/bin/bash

cargo update
cargo outdated
cargo fmt
cargo fix --allow-dirty --allow-staged
cargo clippy -- -W clippy::pedantic -W clippy::cargo -W clippy::all -W clippy::nursery

