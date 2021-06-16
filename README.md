% QVM-CONVERT(1) | User Commands

IMPORTANT
==========
In development, do not use it.

Goals before reaching any 'usable' state:
- Improve my skills in rust
- Provide all the functionnalities of "qubes-app-linux-pdf-converter"
- Provide all the functionnalities of my -- currently -- pull request https://github.com/QubesOS/qubes-app-linux-pdf-converter/pull/9
- Provide most important functionnalities of "qubes-app-linux-img-converter"
- Add a basic GTK gui 
- Eventually try to raise my UI skills from 0 to non 0
- Archlinux, debian, fedora packaging for Qubes OS
- Split the software in 2 packages: client and server. The server will require
a lot of dependencies. So the goals is to be able to install the client (who
does not require a lot of dependencies) in a different Qubes Template VM than
the server.

Extended goals:
- Sound converter 

The password is "toor" for the encrypted tests file


NAME
===============
qvm-convert - converts a potentially untrusted file to a safe-to-view file

SYNOPSIS
===============
**qvm-convert** [_OPTION_]... [_FILE_]...

DESCRIPTION
==============
Qubes converter is a [Qubes](https://qubes-os.org) Application, which utilizes Qubes flexible qrexec
(inter-VM communication) infrastructure and Disposable VMs to perform conversion
of potentially untrusted (e.g. maliciously malformed) files into
safe-to-view PDF files.

This is done by having the Disposable VM perform the complex (and potentially
buggy) rendering of the PDF in question) and sending the resulting RGBA bitmap
(simple representation) to the client AppVM. The client AppVM can _trivially_
verify the received data are indeed the simple representation, and then
construct a new file out of the received bitmap. Of course the price we pay for
this conversion is loosing any structural information and text-based search in
the converted file.

More discussion and introduction of the concept has been described in the original article [here](https://blog.invisiblethings.org/2013/02/21/converting-untrusted-pdfs-into-trusted.html).

OPTIONS
=============
TODO
**-a** PATH, **`--`archive**=PATH
----------------------------------
Directory for storing archived files

CONFIGURATION
===============
To use a custom DisposableVM instead of the default one:

Let’s assume that this custom DisposableVM is called "web".
In dom0, add new line in "/etc/qubes-rpc/policy/qubes.Convert":

**YOUR_CLIENT_VM_NAME @dispvm allow,target=@dispvm:web**

AUTHOR
============
The original idea and implementation has been provided by Joanna Rutkowska. The
project has been subsequently incorporated into [Qubes OS](https://qubes-os.org)
and multiple other developers have contributed various fixes and improvements
(see the commit log for details).
