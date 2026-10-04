# Single install recipe, used by the PKGBUILD and by `sudo make install` on any distro.
PREFIX ?= /usr/local
DESTDIR ?=

all:
	cargo build --release --locked

install:
	install -Dm755 target/release/encre $(DESTDIR)$(PREFIX)/bin/encre
	install -Dm644 dev.andrea.Encre.desktop $(DESTDIR)$(PREFIX)/share/applications/dev.andrea.Encre.desktop
	install -Dm644 dev.andrea.Encre.svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/dev.andrea.Encre.svg
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/encre/LICENSE

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/encre \
	  $(DESTDIR)$(PREFIX)/share/applications/dev.andrea.Encre.desktop \
	  $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/dev.andrea.Encre.svg
	rm -rf $(DESTDIR)$(PREFIX)/share/licenses/encre

test:
	cargo test --locked

.PHONY: all install uninstall test
