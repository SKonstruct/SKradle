# dmgbuild settings for SKradle's disk image (see packaging/macos.sh): the app on the left, a shortcut to
# /Applications on the right and an arrow between them, so installing is one drag.
#   dmgbuild -s packaging/macos/dmg-settings.py -D app=<path to SKradle.app> -D icon=<skradle.icns> SKradle out.dmg
import os

app = defines["app"]  # noqa: F821  (dmgbuild provides `defines`)
appname = os.path.basename(app)

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = defines.get("icon")  # noqa: F821
background = "builtin-arrow"
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
window_rect = ((200, 140), (640, 400))
icon_size = 128
text_size = 13
icon_locations = {appname: (170, 200), "Applications": (470, 200)}
