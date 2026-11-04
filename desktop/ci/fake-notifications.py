#!/usr/bin/env python3
# A stand-in notification server on the session bus. With FAKE_NOTIFY_ACTIONS=1 it has actions
# and presses one when "<number> <action>" lands in the file FAKE_NOTIFY_PRESS names.
import os

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
IFACE = "org.freedesktop.Notifications"


class Server(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, "/org/freedesktop/Notifications")
        self.n = 0

    @dbus.service.method(IFACE, in_signature="susssasa{sv}i", out_signature="u")
    def Notify(self, app, replaces, icon, summary, body, actions, hints, timeout):
        self.n += 1
        pairs = [f"{actions[i]}={actions[i + 1]}" for i in range(0, len(actions) - 1, 2)]
        more = f" [{', '.join(pairs)}] #{self.n}" if ACTIONS else ""
        print(f"notification {app}: {summary}: {body}{more}", flush=True)
        return self.n

    @dbus.service.method(IFACE, out_signature="as")
    def GetCapabilities(self):
        return ["body", "actions"] if ACTIONS else ["body"]

    @dbus.service.signal(IFACE, signature="us")
    def ActionInvoked(self, n, action):
        pass

    @dbus.service.signal(IFACE, signature="uu")
    def NotificationClosed(self, n, reason):
        pass

    def press(self):
        if PRESS and os.path.exists(PRESS):
            with open(PRESS) as f:
                lines = f.read().split()
            os.remove(PRESS)
            for i in range(0, len(lines) - 1, 2):
                print(f"pressed {lines[i + 1]} on #{lines[i]}", flush=True)
                self.ActionInvoked(dbus.UInt32(int(lines[i])), lines[i + 1])
                self.NotificationClosed(dbus.UInt32(int(lines[i])), dbus.UInt32(2))
        return True

    @dbus.service.method(IFACE, out_signature="ssss")
    def GetServerInformation(self):
        return ("stand-in", "reel", "1", "1.2")

    @dbus.service.method(IFACE, in_signature="u")
    def CloseNotification(self, n):
        pass


ACTIONS = os.environ.get("FAKE_NOTIFY_ACTIONS") == "1"
PRESS = os.environ.get("FAKE_NOTIFY_PRESS", "")
bus = dbus.SessionBus()
name = dbus.service.BusName(IFACE, bus)
server = Server(bus)
GLib.timeout_add(200, server.press)
print("notifications up", flush=True)
GLib.MainLoop().run()
