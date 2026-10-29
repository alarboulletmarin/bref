# Single install recipe, used by the PKGBUILD and by `sudo make install` on any distro.
PREFIX ?= /usr/local
DESTDIR ?=

all:
	cargo build --release --locked

install:
	install -Dm755 target/release/bref $(DESTDIR)$(PREFIX)/bin/bref
	install -Dm644 dev.andrea.Bref.desktop $(DESTDIR)$(PREFIX)/share/applications/dev.andrea.Bref.desktop
	install -Dm644 dev.andrea.Bref.svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/dev.andrea.Bref.svg
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/bref/LICENSE

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/bref \
	  $(DESTDIR)$(PREFIX)/share/applications/dev.andrea.Bref.desktop \
	  $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/dev.andrea.Bref.svg
	rm -rf $(DESTDIR)$(PREFIX)/share/licenses/bref

test:
	cargo test --locked

# The user guide: the shortcuts page is written from the help panel, then the book is built
# into book/book/ (needs mdbook: `cargo install mdbook`).
docs:
	BREF_WRITE_DOCS=1 cargo test --locked shortcuts_page_is_current
	mdbook build book

.PHONY: all install uninstall test docs
