#!/bin/bash
mime_office=$(cat /usr/share/applications/libreoffice*.desktop | grep MimeType | sed 's/MimeType=//g' | tr --delete '\n')
mime_gimp=$(cat /usr/share/applications/gimp.desktop | grep MimeType | sed 's/MimeType=//g' | tr --delete '\n')
readarray -t all_mime <<< $(echo "$mime_office$mime_gimp" | sed 's/;/\n/g' | sort | uniq)
IFS=';'
echo "${all_mime[*]:1};"
