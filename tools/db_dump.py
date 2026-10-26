"""Print the agent SQLite state, for development and tests."""

import json
import os
import sqlite3


def main() -> int:
    path = os.path.join(os.environ["APPDATA"], "macrokey", "macrokey.db")
    connection = sqlite3.connect(path)
    connection.row_factory = sqlite3.Row

    print("contexts:")
    for row in connection.execute(
        "SELECT id, label, default_layer, position FROM contexts ORDER BY position"
    ):
        layers = [
            f"{entry['layer_id']}@{entry['position']}"
            for entry in connection.execute(
                "SELECT layer_id, position FROM context_layers "
                "WHERE context_id = ? ORDER BY position",
                (row["id"],),
            )
        ]
        print(
            f"  {row['id']:<10} {row['label'] or '':<14} pos={row['position']} "
            f"default={row['default_layer']:<9} layers={layers}"
        )

    print("layers:")
    for row in connection.execute(
        "SELECT id, label, color, position FROM layers ORDER BY position"
    ):
        count = connection.execute(
            "SELECT COUNT(*) FROM bindings WHERE layer_id = ?", (row["id"],)
        ).fetchone()[0]
        print(
            f"  {row['id']:<10} {row['label'] or '':<14} {row['color'] or '':<9} "
            f"pos={row['position']} bindings={count}"
        )

    print("rules:")
    for row in connection.execute(
        "SELECT process, title, context_id FROM rules ORDER BY position"
    ):
        print(f"  {row['process'] or ''!r:<20} {row['title'] or ''!r:<24} -> {row['context_id']}")

    print("bindings (general/media):")
    for row in connection.execute(
        "SELECT input, gesture, action FROM bindings "
        "WHERE layer_id = 'media' ORDER BY input, gesture"
    ):
        print(f"  {row['input']:<11} {row['gesture']:<7} {row['action']}")

    revision = connection.execute(
        "SELECT value FROM meta WHERE key = 'revision'"
    ).fetchone()[0]
    print(f"revision: {revision}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
