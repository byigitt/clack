# clack

A native macOS menu-bar + Dock utility that plays mechanical keyboard sounds on
every keypress. A blazing-fast Rust rewrite of
[thock](https://github.com/kamillobinski/thock).

- Global key capture via `CGEventTap` (needs Accessibility permission)
- Ultra-low-latency audio: a single persistent `cpal` CoreAudio stream with an
  app-owned additive mixer (preloaded PCM, lock-free trigger ring)
- Reuses the existing thock soundpacks at
  `~/Library/Application Support/Thock/Soundpacks/<UUID>/`
- Menu bar controls + visible in the Dock

## Build

```sh
cargo run --release        # run directly
./scripts/bundle.sh        # build dist/clack.app (Dock app, ad-hoc signed)
```

Grant Accessibility permission on first run (System Settings → Privacy &
Security → Accessibility) so the global key tap can see your keystrokes.

## Stack

`cpal` · `objc2` / `objc2-app-kit` / `objc2-core-graphics` / `objc2-service-management` ·
`tray-icon` + `muda` · `hound` · `rtrb` · `arc-swap` · `fastrand` · `dirs` ·
`macos-accessibility-client`

MIT.
