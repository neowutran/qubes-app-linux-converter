IMPORTANT
==========
!!! In development, do not use it. !!!

Status: Standard cases are working. Many non "standard" cases work. Currently
testing before a first stable version.

Goals before reaching any 'usable' state:
- Improve my skills in rust: In progress
- Provide all the functionnalities of "qubes-app-linux-pdf-converter": Done
- Provide all the functionnalities of my -- currently -- pull request
  https://github.com/QubesOS/qubes-app-linux-pdf-converter/pull/9 : Done
- Provide most important functionnalities of "qubes-app-linux-img-converter":
  Done
- Add a basic GTK gui: Done (it's ugly but it work)
- Eventually try to raise my UI skills from 0 to non 0 (Uh.)
- Archlinux packaging for Qubes OS (Done)
- Dom0 packaging for Qubes OS (Mostly done, not tested.)
- Split the software in 2 packages: client and server. The server will require
a lot of dependencies. So the goals is to be able to install the client (who
does not require a lot of dependencies) in a different Qubes Template VM than
the server. (Done, but, TODO, check if it is still really usefull.)

Extended goals:
- Sound converter 

The password is "toor" for the encrypted tests file

DESCRIPTION
==============
Qubes converter is software designed for [Qubes](https://qubes-os.org), which utilizes Qubes flexible qrexec
(inter-VM communication) infrastructure and Disposable VMs to perform conversion
of potentially untrusted (e.g. maliciously malformed) files into
safe-to-view files.

This is done by having the Disposable VM perform the complex (and potentially
buggy) rendering of the PDF in question) and sending the resulting RGBA bitmap
(simple representation) to the client AppVM. The client AppVM can _trivially_
verify the received data are indeed the simple representation, and then
construct a new file out of the received bitmap. Of course the price we pay for
this conversion is loosing any structural information and text-based search in
the converted file.

More discussion and introduction of the concept has been described in the original article [here](https://blog.invisiblethings.org/2013/02/21/converting-untrusted-pdfs-into-trusted.html).

CONFIGURATION
===============
To use a custom DisposableVM instead of the default one:

Let’s assume that this custom DisposableVM is called "web".
In dom0, add new line in "/etc/qubes-rpc/policy/qubes.Convert":

**YOUR_CLIENT_VM_NAME @dispvm allow,target=@dispvm:web**
