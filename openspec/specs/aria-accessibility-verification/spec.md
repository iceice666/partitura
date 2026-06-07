# aria-accessibility-verification Specification

## Purpose
TBD - created by archiving change implement-aria-macos-ui. Update Purpose after archive.
## Requirements
### Requirement: Clickable accessibility
Every clickable surface and icon-only control SHALL expose pointer affordance where appropriate and VoiceOver labels for controls such as close, gear, mode dots, daemon status, and dispatch controls.

#### Scenario: Icon-only button is announced
- **WHEN** VoiceOver focuses an icon-only control
- **THEN** Aria announces a meaningful action label rather than only the symbol

### Requirement: Appearance and contrast
All text-on-color pairs SHALL meet at least 4.5:1 contrast in light and dark appearances for status badges, mode badges, banners, cards, and toasts.

#### Scenario: Dark appearance contrast passes
- **WHEN** the app is inspected in dark appearance
- **THEN** visible text on colored backgrounds meets the required contrast threshold

### Requirement: Reduced motion and transparency
Aria SHALL replace pulses and spring animations with static fills or short opacity fades when reduced motion is enabled, and SHALL replace material backgrounds with solid window background colors when reduced transparency is enabled.

#### Scenario: Reduced motion disables pulse
- **WHEN** reduced motion is enabled and a run is building
- **THEN** Aria shows a static building indicator instead of a pulsing animation

### Requirement: Keyboard and localization readiness
Interactive controls SHALL follow visual tab order and all user-facing strings SHALL use localization-ready string construction.

#### Scenario: Tab order follows layout
- **WHEN** the user tabs through the window
- **THEN** focus moves through sidebar, board, inspector, sheets, and actions in visual order

### Requirement: Verification previews and smoke path
The implementation SHALL include verification paths for static color previews, 7-column by 6-card density at 1440 by 900, accessibility inspector sweep, reduced-motion and reduced-transparency previews, light/dark switching, and an end-to-end smoke with a Harmony stub server covering dispatch, live log, `run:finished`, and inline report.

#### Scenario: Stub server smoke completes
- **WHEN** the Harmony stub emits dispatch, progress, and finished-run events
- **THEN** Aria shows the live log during the run and the inline report after completion

