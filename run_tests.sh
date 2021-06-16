#!/bin/bash

# The integration test assume that every binary have already been built
cargo build

RUST_LOG=debug cargo test
