build_mac:
	cd flutter-app && flutter build bundle
	cd flutter-app/rust && cargo build && ./scripts/build.py mac

.PHONY: build_mac
