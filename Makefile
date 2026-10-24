.DEFAULT_GOAL := help

CARGO ?= cargo
PYTHON ?= python3
RELEASE_NAME = cubie-a7z-fanctl-$(VERSION)-linux-arm64
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml)

.PHONY: help fetch build fmt clippy test check licenses deb lintian source-archive release armbian-install armbian-test clean

help:
	@printf '%s\n' \
	  'make fetch          Download locked dependencies (network required)' \
	  'make build          Build release offline' \
	  'make fmt            Check Rust formatting' \
	  'make clippy         Run strict Clippy checks offline' \
	  'make test           Run tests offline' \
	  'make check          Run format, Clippy, tests and release build' \
	  'make licenses       Generate target/THIRD-PARTY-LICENSES.txt' \
	  'make deb            Build Debian binary package and run Lintian' \
	  'make lintian        Check the current Debian binary package' \
	  'make source-archive Prepare Debian upstream source archive' \
	  'make release        Build Debian arm64 release assets in dist/' \
	  'make armbian-install Install the A7Z overlay on Armbian (sudo)' \
	  'make armbian-test   Test the overlay installer (dtc, Python 3)' \
	  'make clean          Remove Cargo build output'

fetch:
	$(CARGO) fetch --locked

build:
	$(CARGO) build --release --locked --offline

fmt:
	$(CARGO) fmt --check

clippy:
	$(CARGO) clippy --locked --offline --all-targets --all-features -- -D warnings

test:
	$(CARGO) test --locked --offline

check: fmt clippy test build

licenses:
	CARGO="$(CARGO)" $(PYTHON) scripts/dependency-licenses.py

deb:
	dpkg-buildpackage -us -uc -b
	$(MAKE) lintian

lintian:
	lintian ../cubie-a7z-fanctl_$$(dpkg-parsechangelog -S Version)_$$(dpkg-architecture -qDEB_HOST_ARCH).deb

source-archive:
	LC_ALL=C COPYFILE_DISABLE=1 tar --no-xattrs --no-acls \
	  --exclude='./.git' --exclude='./target' --exclude='./debian' \
	  --exclude='./dist' --exclude='__pycache__' --exclude='*.dtbo' \
	  --exclude='*.deb' --exclude='*.ddeb' --exclude='*.buildinfo' --exclude='*.changes' \
	  -czf ../cubie-a7z-fanctl_$(VERSION).orig.tar.gz .

# Build both assets from the same native Debian arm64 release build.
release: deb
	test "$$(uname -s)" = Linux
	test "$$(dpkg-architecture -qDEB_HOST_ARCH)" = arm64
	mkdir -p dist
	rm -rf target/$(RELEASE_NAME) target/$(RELEASE_NAME)-deb
	mkdir -p target/$(RELEASE_NAME)/config
	dpkg-deb --extract ../cubie-a7z-fanctl_$$(dpkg-parsechangelog -S Version)_arm64.deb target/$(RELEASE_NAME)-deb
	install -m 0755 target/$(RELEASE_NAME)-deb/usr/sbin/cubie-a7z-fanctl target/$(RELEASE_NAME)/
	install -m 0644 config/config.toml target/$(RELEASE_NAME)/config/config.toml
	install -m 0644 debian/cubie-a7z-fanctl.service debian/cubie-a7z-fanctl.8 LICENSE README.md target/THIRD-PARTY-LICENSES.txt target/$(RELEASE_NAME)/
	tar -C target -czf dist/$(RELEASE_NAME).tar.gz $(RELEASE_NAME)
	cp ../cubie-a7z-fanctl_$$(dpkg-parsechangelog -S Version)_arm64.deb dist/
	deb_version=$$(dpkg-parsechangelog -S Version); cd dist && sha256sum cubie-a7z-fanctl_$${deb_version}_arm64.deb $(RELEASE_NAME).tar.gz > SHA256SUMS

armbian-install:
	. ./contrib/armbian/install.sh; _armbian_install "$(CURDIR)/contrib/armbian" / /usr/sbin/armbian-add-overlay

armbian-test:
	$(PYTHON) -m unittest discover -s tests -p 'test_armbian_install.py'

clean:
	$(CARGO) clean
