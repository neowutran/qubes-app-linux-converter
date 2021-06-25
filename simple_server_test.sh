#!/bin/bash
RUST_LOG=debug echo -ne "toor\n1\n44\n$(wc -c tests/files/arch-spec-0.3.pdf)\n$(cat tests/files/arch-spec-0.3.pdf)" \
	| cargo flamegraph --dev --bin server
