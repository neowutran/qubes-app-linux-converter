PANDOC=pandoc -s -f markdown -t man
NAME := convert

qvm-$(NAME).1: README.md
	$(PANDOC) $< > $@

qvm-$(NAME).1.gz: qvm-$(NAME).1
	gzip -f $<

manpage: qvm-$(NAME).1.gz

install-vm-server:
	install -d $(DESTDIR)/etc/qubes-rpc
	ln -s /usr/bin/qubes-app-linux-converter-server $(DESTDIR)/etc/qubes-rpc/qubes.Convert

install-vm-client: manpage
	install -d $(DESTDIR)/usr/share/man/man1
	install -D qvm-$(NAME).1.gz $(DESTDIR)/usr/share/man/man1/
	install -D qvm-convert.gnome $(DESTDIR)/usr/lib/qubes/qvm-convert.gnome
	install -d $(DESTDIR)/usr/share/nautilus-python/extensions
	install -m 0644 qvm_convert_nautilus.py $(DESTDIR)/usr/share/nautilus-python/extensions
	install -d $(DESTDIR)/usr/share/kde4/services
	install -m 0644 qvm-convert.desktop $(DESTDIR)/usr/share/kde4/services

install-dom0:
	install -D -m 0664 policy /etc/qubes-rpc/policy/qubes.Convert

clean:
	rm -rf debian/changelog.*
	rm -rf pkgs
	rm -f qvm-$(NAME).1.gz
