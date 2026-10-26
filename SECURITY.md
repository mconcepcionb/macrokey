# Security policy

macrokey is a local Windows desktop agent. It does not open network sockets,
upload telemetry, or send data anywhere. Its state is a local SQLite database
(`%APPDATA%\macrokey\macrokey.db`) and a local log
(`%APPDATA%\macrokey\agent.log`).

## What the agent can do

- Read Raw Input events from the configured pad only (VID/PID filter).
- Read the foreground process name, window title and window class to resolve a
  context.
- Synthesize keyboard, mouse-wheel and text input through `SendInput`.
- Open apps/files/URLs, run commands, and minimize/maximize/close the foreground
  window if a binding is configured to do so.
- Read/write the local SQLite database and export it to YAML.

Bindings are user-authored local data. A `script` action runs `cmd /C <cmd>` as
the current user; treat an exported config as executable content.

## Reporting

Please report a vulnerability with GitHub's **Private vulnerability reporting**
on the repository's Security tab, or by contacting the maintainer directly.
Include the version, Windows version, and a minimal reproduction. Do not open a
public issue for a security problem.

## Known limitations

- The WebView loads only local HTML from the bundle. The CSP is currently unset
  because the UI uses inline scripts and styles; do not load remote content into
  the settings window.
- The app is not code-signed. Windows SmartScreen may warn about the release
  binary.
- The agent does not capture or log normal keyboard input; it filters Raw Input
  to the configured pad device. The log contains context labels and window titles
  used for rule matching, so review it before sharing it publicly.
