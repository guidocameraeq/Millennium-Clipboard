# Millennium Clipboard

Share text and files between devices on the same local network. Cross-platform: Windows desktop and Android mobile.

> **Status:** 🚧 Early prototype. Architecture decisions taken, code not started.

## What & Why

Moving snippets and files between your own devices — PC↔PC or PC↔phone on the same Wi-Fi — should be trivial. It isn't. The usual workarounds (emailing yourself, WhatsApp-to-self, USB cables, cloud storage) are clunky.

**Millennium Clipboard** is a small, local-only utility that discovers your other devices on the LAN and lets you send text or files to a specific one. Mark trusted devices as favorites; the rest stay out of your face.

## Aesthetic

Windows 98 base UI (beveled borders, gray panels, classic controls) combined with **typewriter typography and click-clack animations**. Vintage, deliberate, a little playful.

## Stack

- **Framework:** [Tauri 2.0](https://v2.tauri.app/) — Rust backend, web frontend, ~10 MB binaries.
- **UI:** [98.css](https://jdan.github.io/98.css/) for vintage controls, monospace fonts for typewriter feel.
- **Discovery:** mDNS (`_millennium._tcp.local`).
- **Transport:** HTTPS REST with self-signed certs and fingerprint pinning.
- **Targets:** Windows `.exe` and Android `.apk`.

## Roadmap

- [ ] Project skeleton (Tauri init, monorepo layout)
- [ ] mDNS discovery — desktop
- [ ] HTTPS transfer endpoints
- [ ] Send text MVP
- [ ] Send files MVP
- [ ] UI: device list with Win98 + typewriter styling
- [ ] Favorites / trusted devices
- [ ] Android port
- [ ] History view
- [ ] Clipboard sync (stretch)

## License

TBD.
