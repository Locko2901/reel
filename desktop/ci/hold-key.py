# Holds Ctrl+Alt+<key> on the X display as another app would, but only with NumLock on, so
# the key still reaches the focused window while the app cannot register the combination.
import ctypes
import sys
import time

x = ctypes.CDLL("libX11.so.6")
x.XOpenDisplay.restype = ctypes.c_void_p
x.XOpenDisplay.argtypes = [ctypes.c_char_p]
x.XDefaultRootWindow.restype = ctypes.c_ulong
x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x.XStringToKeysym.restype = ctypes.c_ulong
x.XStringToKeysym.argtypes = [ctypes.c_char_p]
x.XKeysymToKeycode.restype = ctypes.c_ubyte
x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x.XGrabKey.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_uint, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_int]
x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]

display = x.XOpenDisplay(None)
if not display:
    sys.exit("no X display")
keycode = x.XKeysymToKeycode(display, x.XStringToKeysym(sys.argv[1].encode()))
CONTROL, MOD1_ALT, MOD2_NUMLOCK, GRAB_ASYNC = 4, 8, 16, 1
x.XGrabKey(display, keycode, CONTROL | MOD1_ALT | MOD2_NUMLOCK, x.XDefaultRootWindow(display), 1, GRAB_ASYNC, GRAB_ASYNC)
x.XSync(display, 0)
print(f"holding Ctrl+Alt+{sys.argv[1].upper()} (with NumLock)", flush=True)
while True:
    time.sleep(1)
