# SPDX-License-Identifier: GPL-3.0-only
#!/usr/bin/env python3
"""FileChooser fixture for the private QA bus; responses come from portal-response.json."""
import json
import sys
from pathlib import Path

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

RUN = Path(sys.argv[1])
DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
name = dbus.service.BusName("org.freedesktop.portal.Desktop", bus)
requests = []


class Request(dbus.service.Object):
    @dbus.service.signal("org.freedesktop.portal.Request", signature="ua{sv}")
    def Response(self, response, results):
        pass

    @dbus.service.method("org.freedesktop.portal.Request", in_signature="", out_signature="")
    def Close(self):
        pass


class Portal(dbus.service.Object):
    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, interface, property):
        if property == "version":
            return dbus.UInt32(4)
        raise dbus.exceptions.DBusException("Unknown property")

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return {"version": dbus.UInt32(4)}

    @dbus.service.method("org.freedesktop.portal.FileChooser", in_signature="ssa{sv}",
                         out_signature="o", sender_keyword="sender")
    def OpenFile(self, parent, title, options, sender=None):
        token = options.get("handle_token", "lockscreen_qa")
        path = "/org/freedesktop/portal/desktop/request/" + sender.lstrip(":").replace(".", "_") + "/" + token
        request = Request(bus, path)
        requests.append(request)
        response = json.loads((RUN / "portal-response.json").read_text())

        def reply():
            if response.get("cancel"):
                request.Response(dbus.UInt32(1), {})
            else:
                request.Response(dbus.UInt32(0), {"uris": dbus.Array([Path(response["file"]).as_uri()], signature="s")})
            return False

        GLib.timeout_add(200, reply)
        return dbus.ObjectPath(path)


portal = Portal(bus, "/org/freedesktop/portal/desktop")
print("QA file portal ready", flush=True)
GLib.MainLoop().run()
