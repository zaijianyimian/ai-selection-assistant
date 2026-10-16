# Spec Delta

## Purpose

Keep a reusable compact translation window close to the user's reading location where the desktop permits positioning, while accurately representing the limitations of compositor-controlled Wayland window placement.

## ADDED Requirements

### Requirement: Popup placement uses a pre-focus anchor
On X11 and supported Windows desktops, shortcut-triggered translation SHALL place the popup near the pointer location captured before the application takes focus, using that position as an approximation of the selection endpoint. Placement MUST preserve source capture behavior, account for monitor scale and keep the window within the chosen monitor work area by flipping or clamping at edges.

#### Scenario: Selection is in the middle of the display
- **WHEN** a user triggers translation with the pointer near selected text on X11
- **THEN** the popup opens beside the captured pointer location after source capture, instead of opening at an unrelated fixed position

#### Scenario: Selection is near a screen edge
- **WHEN** the popup would extend past the monitor work area
- **THEN** it is flipped or clamped into that work area, including on a monitor with a negative origin or non-default scale

#### Scenario: Position cannot be determined
- **WHEN** pointer or monitor information is unavailable
- **THEN** translation still proceeds using the existing popup position with a clear placement limitation rather than failing the translation

### Requirement: Wayland retains compositor-managed placement
On Wayland, the application MUST NOT claim access to another application's exact selection rectangle or force unsupported global positioning. Hiding and reopening the existing popup within the same run MUST preserve its compositor-managed manual position. The UI MUST explain that automatic near-selection placement is unavailable and MUST NOT promise cross-restart coordinate restoration.

#### Scenario: User moves and reopens the popup on Wayland
- **WHEN** a user drags the popup, hides it, and reopens it in the same application run
- **THEN** the app reuses the same window without resetting its placement and shows the Wayland placement hint

### Requirement: Compact popup remains usable
The popup SHALL start at a compact size with independently scrollable content and retain the input, result, copy, error and term controls. Closing it MUST still hide the reusable window without stopping the application.

#### Scenario: Long translation contains several terms
- **WHEN** a long translation is displayed in the compact popup
- **THEN** the user can scroll through its result and term controls without requiring a larger fixed window
