basepackagename=$(egrep "^name =" Cargo.toml | head -n 1 | cut -d '"' -f 2)
pkgname=($basepackagename-client $basepackagename-server)
pkgver=$(egrep "^version" Cargo.toml | head -n 1 | cut -d '"' -f 2)
pkgrel=1
arch=(x86_64)
pkgdesc=$(egrep "^description" Cargo.toml | head -n 1 | cut -d '"' -f 2)
url=$(cat ./.git/config | grep "url =" | cut -d ' ' -f 3)
license=(GPL)
makedepends=(pandoc rustup gtk4)

build() {
 RUSTUP_TOOLCHAIN=stable cargo build --release --locked --all-features --target-dir=target
}
package_qubes-converter-server() {
   depends=(libreoffice graphicsmagick zenity poppler java-commons-lang pdftk bcprov)
   mkdir -p "$pkgdir/usr/bin"
   install -m 755 target/release/$basepackagename-server "$pkgdir"/usr/bin
   make -C ../ install-vm-server DESTDIR="$pkgdir/"
}
package_qubes-converter-client() {
   depends=(python-nautilus bcprov pdftk java-commons-lang gtk4)
   optdepends=('tesseract: Text search support through OCR' 'tesseract-data: Text search support through OCR (languages)')
   mkdir -p "$pkgdir/usr/bin"
   install -m 755 target/release/$basepackagename-client-cli "$pkgdir"/usr/bin
   install -m 755 target/release/$basepackagename-client-gtk "$pkgdir"/usr/bin
   make -C ../ install-vm-client DESTDIR="$pkgdir/"
}