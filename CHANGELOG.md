# Changelog

## Unreleased

- Added `docs/roadmap.md`, the full gap-closure plan.
- Pad actions pause while the Settings window is focused; Preview and the tray
  legend still work.

## 0.1.0 - 2026-10-26

First public prototype.

- 3-key + 1-knob CH57x macropad (`1189:8890`) flashed once to `F13`–`F18`.
- Windows Raw Input capture filtered to the pad, with tap / double / hold
  gestures and knob rotation acceleration.
- Context → layer model: contexts own an ordered layer list and a default layer;
  layers are a shared library.
- Actions: chord, sequence, text, media, wheel, layer switch/one-shot/momentary/
  next/previous/back, peek, app, URL, script, window, nothing.
- SQLite persistence at `%APPDATA%\macrokey\macrokey.db`, live reload, export.
- Physical-pad HUD with layer rail, key gestures, knob press gestures, action
  toast and press feedback.
- Settings deck mirrors the pad: keycaps, CCW/knob/CW cluster, TAP / 2× / HOLD
  cards, and a Matches modal.
- Tray icon colour and tooltip reflect the active context and layer.
- Single-instance guard; first launch opens Settings; append-only `agent.log`.
- CI, unit tests, MIT license, security policy and publishing checklist.
