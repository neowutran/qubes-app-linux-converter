#!/bin/bash

# Download one GIEC report (3900+ pages)
test -f ./tests/files/IPCC_AR6_WGI_Full_Report.pdf
file_exist=$?
if [ file_exist -eq 0 ]; then
	wget https://www.ipcc.ch/report/ar6/wg1/downloads/report/IPCC_AR6_WGI_Full_Report.pdf -O ./tests/files/IPCC_AR6_WGI_Full_Report.pdf
fi

# The integration test assume that every binary have already been built.
# Using "release" to not loose too much time, and to be close from reality.
cargo build --release

RUST_LOG=debug cargo test --release
