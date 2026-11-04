#!/usr/bin/env python3
"""A StatusNotifier host on the session bus, as the bars of Hyprland and KDE's panel are, for the
save actions' check: with it the app's tray is a StatusNotifierItem whose menu is a DBusMenu.
  fake-sni.py                serve until killed; each item that registers goes to stdout
  fake-sni.py click LABEL    click the last registered item's menu entry of that label, as a bar does
  fake-sni.py menu           print the last registered item's menu, an entry a line, "-" before one
                             that cannot be clicked
"""
import sys
import time

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

WATCHER = "org.kde.StatusNotifierWatcher"
ITEM = "org.kde.StatusNotifierItem"
MENU = "com.canonical.dbusmenu"
PROPS = "org.freedesktop.DBus.Properties"


def say(*words):
    print(*words, flush=True)


class Watcher(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, "/StatusNotifierWatcher")
        self.items = []

    @dbus.service.method(WATCHER, in_signature="s", sender_keyword="sender")
    def RegisterStatusNotifierItem(self, service, sender=None):
        item = service if service.startswith(":") or "." in service.split("/")[0] else sender
        path = service if service.startswith("/") else "/StatusNotifierItem"
        self.items.append(f"{sender}{path}")
        say("item", sender, path)
        self.StatusNotifierItemRegistered(f"{sender}{path}")

    @dbus.service.method(WATCHER, in_signature="s")
    def RegisterStatusNotifierHost(self, service):
        pass

    @dbus.service.signal(WATCHER, signature="s")
    def StatusNotifierItemRegistered(self, item):
        pass

    @dbus.service.method(PROPS, in_signature="ss", out_signature="v")
    def Get(self, iface, prop):
        return self.GetAll(iface)[prop]

    @dbus.service.method(PROPS, in_signature="s", out_signature="a{sv}")
    def GetAll(self, iface):
        return {
            "RegisteredStatusNotifierItems": dbus.Array(self.items, signature="s"),
            "IsStatusNotifierHostRegistered": dbus.Boolean(True),
            "ProtocolVersion": dbus.Int32(0),
        }


def entries(layout):
    """Each entry of a DBusMenu layout, its id and label, depth first."""
    ident, props, children = layout
    yield int(ident), str(props.get("label", "")), bool(props.get("enabled", True))
    for child in children:
        yield from entries(child)


def layout_of():
    bus = dbus.SessionBus()
    watcher = bus.get_object(WATCHER, "/StatusNotifierWatcher")
    items = watcher.Get(WATCHER, "RegisteredStatusNotifierItems", dbus_interface=PROPS)
    if not items:
        say("no item registered")
        sys.exit(1)
    name, path = str(items[-1]).split("/", 1)
    item = bus.get_object(name, "/" + path)
    menu_path = item.Get(ITEM, "Menu", dbus_interface=PROPS)
    menu = bus.get_object(name, menu_path)
    menu.AboutToShow(0, dbus_interface=MENU)
    _, layout = menu.GetLayout(0, -1, dbus.Array([], signature="s"), dbus_interface=MENU)
    return layout


def show_menu():
    for _, label, enabled in entries(layout_of()):
        if label:
            say(("" if enabled else "- ") + label.replace("_", ""))


def click(label):
    bus = dbus.SessionBus()
    watcher = bus.get_object(WATCHER, "/StatusNotifierWatcher")
    items = watcher.Get(WATCHER, "RegisteredStatusNotifierItems", dbus_interface=PROPS)
    if not items:
        say("no item registered")
        sys.exit(1)
    name, path = str(items[-1]).split("/", 1)
    item = bus.get_object(name, "/" + path)
    menu_path = item.Get(ITEM, "Menu", dbus_interface=PROPS)
    menu = bus.get_object(name, menu_path)
    _, layout = menu.GetLayout(0, -1, dbus.Array([], signature="s"), dbus_interface=MENU)
    found = [(i, l, e) for i, l, e in entries(layout) if l.replace("_", "") == label]
    if not found:
        say("no entry", label, "in", [l for _, l, _ in entries(layout)])
        sys.exit(1)
    ident, _, enabled = found[0]
    menu.AboutToShow(0, dbus_interface=MENU)
    menu.Event(ident, "clicked", dbus.String("", variant_level=1), dbus.UInt32(int(time.time())), dbus_interface=MENU)
    say("clicked", label, "enabled" if enabled else "disabled")


if __name__ == "__main__":
    if len(sys.argv) > 2 and sys.argv[1] == "click":
        click(sys.argv[2])
        sys.exit(0)
    if len(sys.argv) > 1 and sys.argv[1] == "menu":
        show_menu()
        sys.exit(0)
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    name = dbus.service.BusName(WATCHER, bus)
    Watcher(bus)
    say("watcher up")
    GLib.MainLoop().run()
