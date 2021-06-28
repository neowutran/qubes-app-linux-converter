#!/bin/bash

# The integration test assume that every binary have already been built.
# Using "release" to not loose too much time, and to be close from reality.
cargo build --release

RUST_LOG=debug cargo test --release
