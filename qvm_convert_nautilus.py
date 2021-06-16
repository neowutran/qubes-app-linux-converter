import os
import subprocess

from gi.repository import Nautilus, GObject


class ConvertPdfItemExtension(GObject.GObject, Nautilus.MenuProvider):
    '''Send file to disposable virtual machine to convert to a safe format.

    Uses the nautilus-python api to provide a context menu within Nautilus which
    will enable the user to select file(s) to send to a disposable virtual
    machine for safe processing
    '''

    def get_file_items(self, window, files):
        '''Attaches context menu in Nautilus to local file objects only.
        '''
        if not files:
            return

        for file_obj in files:

            # Do not attach context menu to a directory
            if file_obj.is_directory():
                return

            # Do not attach context menu  to anything other that a file
            # local files only; not remote
            if file_obj.get_uri_scheme() != 'file':
                return

        menu_item = Nautilus.MenuItem(name='QubesMenuProvider::Convert',
                                      label='Convert To Trusted',
                                      tip='',
                                      icon='')

        menu_item.connect('activate', self.on_menu_item_clicked, files)
        return menu_item,

    def on_menu_item_clicked(self, menu, files):
        '''Called when user chooses files though Nautilus context menu.
        '''
        files_path = []
        for file_obj in files:

            # Check if file still exists
            if file_obj.is_gone():
                return

            files_path.append(file_obj.get_location().get_path())

        subprocess.call(['/usr/lib/qubes/qvm-convert.gnome'] + files_path)
