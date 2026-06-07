# Aria macOS Smoke and Verification

Run all commands from `aria/macos/`.

## Build

This shell is entered through Nix and sets `DEVELOPER_DIR` / `SDKROOT` to a macOS 14.4 SDK that
does not match the installed Xcode 26.4.1 Swift 6.3.1 toolchain. Use a clean Apple toolchain
environment for package verification:

```bash
env -u DEVELOPER_DIR -u SDKROOT PATH="/usr/bin:/bin:/usr/sbin:/sbin:/Applications/Xcode.app/Contents/Developer/usr/bin:/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin" swift build
```

Result on 2026-06-07: build succeeded.

## Stub Server Smoke Path

The first implementation ships with `PreviewFixtures` and a stubbed `HarmonyActor.connect(...)`
path that emits:

- `projects:list` equivalent project state
- `runtimes:snapshot` provider and role state
- ticket snapshots containing ready, building, awaiting-input, reviewing, done, blocked, and archived states
- run progress events, run-finished inline reports, and an infeasible report

Smoke flow:

1. Launch the package target from Xcode or with the built `Aria` executable.
2. Confirm the board renders seven primary columns at 1440 by 900.
3. Select `Implement native macOS UI` and confirm the inspector opens with live log rows.
4. Select `Verify live log tailing behavior` and confirm the inline report sections render.
5. Open Providers/Roles and confirm unavailable Google roles are greyed and locked.
6. Toggle Blocked and Archived and confirm the trailing Other column appears.

## Accessibility Inspector Sweep

Open `/Applications/Xcode.app/Contents/Applications/Accessibility Inspector.app` and inspect the
running Aria window:

- Tab order should move through sidebar, board toolbar, board cards, inspector, sheets, and actions.
- Icon-only controls should announce labels for sidebar collapse, daemon status, gear, close, and dismiss.
- Enable reduced motion and verify building pulse/shimmer use static or short-fade behavior.
- Enable reduced transparency and verify material surfaces fall back to solid semantic colors.
- Check light and dark status badges, mode badges, banners, cards, and toasts for at least 4.5:1 contrast.
