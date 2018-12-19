build_mac:
	cd flutter-runner && cargo build && ./scripts/build.py mac

.PHONY: build_mac
