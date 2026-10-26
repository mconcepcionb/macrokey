"""Bump the agent database revision after changing data.

Useful for testing live reload from outside the app:

    python tools/db_bump.py "General (live)"
"""

import os
import sqlite3
import sys


def main() -> int:
    path = os.path.join(os.environ["APPDATA"], "macrokey", "macrokey.db")
    if not os.path.exists(path):
        print(f"database not found: {path}")
        return 1

    connection = sqlite3.connect(path)
    label = sys.argv[1] if len(sys.argv) > 1 else "General (live)"
    connection.execute("UPDATE contexts SET label = ? WHERE id = 'general'", (label,))
    connection.execute(
        "UPDATE meta SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) "
        "WHERE key = 'revision'"
    )
    connection.commit()

    revision = connection.execute(
        "SELECT value FROM meta WHERE key = 'revision'"
    ).fetchone()[0]
    print(f"general label = {label!r}, revision = {revision}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
