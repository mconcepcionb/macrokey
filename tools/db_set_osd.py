"""Tune OSD settings in the live agent database.

    python tools/db_set_osd.py                 # primary monitor, no focus pop
    python tools/db_set_osd.py focused --focus
"""

import json
import os
import sqlite3
import sys

monitor = sys.argv[1] if len(sys.argv) > 1 else "primary"
show_on_focus = "--focus" in sys.argv

path = os.path.join(os.environ["APPDATA"], "macrokey", "macrokey.db")
if not os.path.exists(path):
    print(f"database not found: {path}")
    raise SystemExit(1)

connection = sqlite3.connect(path)
row = connection.execute("SELECT json FROM settings WHERE id = 1").fetchone()
data = json.loads(row[0]) if row else {}
osd = data.setdefault("settings", {}).setdefault("osd", {})
osd["monitor"] = monitor
osd["show_on_focus"] = show_on_focus

connection.execute("UPDATE settings SET json = ? WHERE id = 1", (json.dumps(data),))
connection.execute(
    "UPDATE meta SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) "
    "WHERE key = 'revision'"
)
connection.commit()
revision = connection.execute(
    "SELECT value FROM meta WHERE key = 'revision'"
).fetchone()[0]
print(
    f"osd.monitor = {monitor}, osd.show_on_focus = {show_on_focus}, "
    f"revision = {revision}"
)
