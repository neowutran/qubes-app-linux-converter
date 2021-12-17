#!/bin/bash

# archlinux
#makepkg

# debian
( cd ./cli && cargo deb )
( cd ./gtk && cargo deb )
( cd ./server && cargo deb )

# fedora
#( cd ./cli && cargo rpm build )
#( cd ./gtk && cargo rpm build )
#( cd ./server && cargo rpm build )
