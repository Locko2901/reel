#!/usr/bin/env python3
"""The desktop's portal for the end to end run, as much of it as reel's Wayland hotkeys use:
org.freedesktop.portal.GlobalShortcuts and the host app registry, on the session bus
under the portal's own name. What the app binds goes to stdout, one line each;
`fake-portal.py press <id>` makes the running portal fire that shortcut, as a desktop does
when its key is pressed.
  fake-portal.py           serve until killed
  fake-portal.py press ID  press one bound shortcut
"""
import sys
import time

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

NAME = "org.freedesktop.portal.Desktop"
PATH = "/org/freedesktop/portal/desktop"
SHORTCUTS = "org.freedesktop.portal.GlobalShortcuts"
REGISTRY = "org.freedesktop.host.portal.Registry"
REQUEST = "org.freedesktop.portal.Request"
SESSION = "org.freedesktop.portal.Session"
PROPS = "org.freedesktop.DBus.Properties"
TEST = "dev.lockoo.reel.FakePortal"


def say(*words):
    print(*words, flush=True)


class Request(dbus.service.Object):
    @dbus.service.signal(REQUEST, signature="ua{sv}")
    def Response(self, code, results):
        pass

    @dbus.service.method(REQUEST)
    def Close(self):
        self.remove_from_connection()


class Session(dbus.service.Object):
    def __init__(self, bus, path, portal):
        super().__init__(bus, path)
        self.portal = portal
        self.path = path

    @dbus.service.method(SESSION)
    def Close(self):
        self.portal.sessions.pop(self.path, None)
        self.remove_from_connection()

    @dbus.service.signal(SESSION, signature="a{sv}")
    def Closed(self, details):
        pass

    @dbus.service.method(PROPS, in_signature="ss", out_signature="v")
    def Get(self, interface, prop):
        return dbus.UInt32(1)

    @dbus.service.method(PROPS, in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return {"version": dbus.UInt32(1)}


class Portal(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, PATH)
        self.bus = bus
        self.sessions = {}

    def reply(self, sender, token, results):
        path = f"{PATH}/request/{sender[1:].replace('.', '_')}/{token}"
        req = Request(self.bus, path)
        GLib.timeout_add(50, lambda: (req.Response(dbus.UInt32(0), results), False)[1])
        return dbus.ObjectPath(path)

    @dbus.service.method(PROPS, in_signature="ss", out_signature="v")
    def Get(self, interface, prop):
        if interface in (SHORTCUTS, REGISTRY) and prop == "version":
            return dbus.UInt32(1)
        raise dbus.exceptions.DBusException(f"no {prop} on {interface}", name="org.freedesktop.DBus.Error.InvalidArgs")

    @dbus.service.method(PROPS, in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return {"version": dbus.UInt32(1)}

    @dbus.service.method(REGISTRY, in_signature="sa{sv}", sender_keyword="sender")
    def Register(self, app_id, options, sender=None):
        say("registered", app_id)

    @dbus.service.method(SHORTCUTS, in_signature="a{sv}", out_signature="o", sender_keyword="sender")
    def CreateSession(self, options, sender=None):
        session = f"{PATH}/session/{sender[1:].replace('.', '_')}/{options['session_handle_token']}"
        self.sessions[session] = Session(self.bus, session, self)
        say("session", session)
        return self.reply(sender, options["handle_token"], {"session_handle": dbus.String(session)})

    @dbus.service.method(SHORTCUTS, in_signature="oa(sa{sv})sa{sv}", out_signature="o", sender_keyword="sender")
    def BindShortcuts(self, session, shortcuts, parent, options, sender=None):
        bound = []
        for sid, info in shortcuts:
            trigger = str(info.get("preferred_trigger", ""))
            say("bound", sid, trigger or "-")
            bound.append(dbus.Struct((sid, {"description": info["description"], "trigger_description": dbus.String(trigger)}), signature="sa{sv}"))
        return self.reply(sender, options["handle_token"], {"shortcuts": dbus.Array(bound, signature="(sa{sv})")})

    @dbus.service.method(SHORTCUTS, in_signature="oa{sv}", out_signature="o", sender_keyword="sender")
    def ListShortcuts(self, session, options, sender=None):
        return self.reply(sender, options["handle_token"], {"shortcuts": dbus.Array([], signature="(sa{sv})")})

    @dbus.service.signal(SHORTCUTS, signature="osta{sv}")
    def Activated(self, session, shortcut_id, timestamp, options):
        pass

    @dbus.service.signal(SHORTCUTS, signature="osta{sv}")
    def Deactivated(self, session, shortcut_id, timestamp, options):
        pass

    @dbus.service.method(TEST, in_signature="s")
    def Press(self, shortcut_id):
        now = dbus.UInt64(int(time.time() * 1000))
        for session in list(self.sessions):
            self.Activated(dbus.ObjectPath(session), shortcut_id, now, dbus.Dictionary({}, signature="sv"))
            self.Deactivated(dbus.ObjectPath(session), shortcut_id, now, dbus.Dictionary({}, signature="sv"))
        say("pressed", shortcut_id)


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "press":
        bus = dbus.SessionBus()
        bus.get_object(NAME, PATH).Press(sys.argv[2], dbus_interface=TEST)
        return
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    name = dbus.service.BusName(NAME, bus)
    Portal(bus)
    say("portal up")
    GLib.MainLoop().run()
    del name


if __name__ == "__main__":
    main()
