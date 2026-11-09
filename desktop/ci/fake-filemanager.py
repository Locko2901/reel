#!/usr/bin/env python3
# A file manager's FileManager1 on the session bus: prints each ShowItems it is asked.
import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
IFACE = "org.freedesktop.FileManager1"


class Manager(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, "/org/freedesktop/FileManager1")

    @dbus.service.method(IFACE, in_signature="ass")
    def ShowItems(self, uris, startup):
        for u in uris:
            print(f"show {u}", flush=True)

    @dbus.service.method(IFACE, in_signature="ass")
    def ShowFolders(self, uris, startup):
        for u in uris:
            print(f"folder {u}", flush=True)


bus = dbus.SessionBus()
name = dbus.service.BusName(IFACE, bus)
Manager(bus)
print("file manager up", flush=True)
GLib.MainLoop().run()
