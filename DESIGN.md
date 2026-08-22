# Sensitivity design system

## Product character

Sensitivity is a safety-critical Windows utility, not a generic flashing
dashboard. The experience should feel calm, native, precise, and deliberate:
the next safe action is obvious, risky actions are explicit, and technical
detail is available without dominating the screen.

The Windows app is designed in WinUI 3 with the Windows App SDK, Fluent 2
controls, and Fluent Design principles. Use native platform behaviour first.
Do not imitate web controls, custom window chrome, Mica or Acrylic effects.

## Shell and layout

- Use `NavigationView` for durable top-level areas. Keep the active recovery
  flow connected across Overview, ROM selection, Flash, Recovery, and
  Diagnostics rather than making each page an isolated dead end.
- The title bar holds the Sensitivity mark, product name, short tagline, and
  compact refresh action. It must reserve the right inset required by Windows
  caption buttons.
- The navigation pane adapts automatically: expanded at 1180 px and above,
  compact below 1180 px, with the compact threshold at 820 px.
- Content uses a readable left-aligned column, not a narrow centred island.
  Overview may use up to 1120 px; focused task pages use up to 900 px.
- Use 24 px page rhythm, 16 to 24 px card padding, and 10 to 20 px internal
  control gaps. Do not create large empty left margins just to imitate a
  marketing layout.

## Theme, colour, and material

- Respect the Windows system light or dark theme by default. Use theme
  resources such as `TextFillColorSecondaryBrush`, `CardBackgroundFillColorSecondaryBrush`,
  and `AccentFillColorDefaultBrush` instead of hard-coded blue or white.
- Mica BaseAlt is the Windows backdrop where the OS supports it. It is an
  enhancement, never a dependency for contrast or hierarchy.
- The system accent is the default primary action colour. The optional Xiaomi
  brand override is Xiaomi Orange `#FF6900`; it must change accent resources
  consistently and retain native hover, pressed, disabled, and focus states.
- Do not force white icons or number glyphs. Use `TextOnAccentFillColorPrimaryBrush`
  for accent fills so contrast follows the selected theme.
- Critical and destructive states use system critical resources, not a custom
  brand colour.

## Type, icons, and controls

- Use the system typography scale. Page titles are 32 px semibold, section
  titles are 20 px semibold, and supporting text uses the secondary text
  resource with wrapping enabled.
- Use Segoe Fluent `FontIcon` glyphs for interface actions. The Sensitivity
  asset is for product identity, not as a substitute for every action icon.
- Primary actions use the system accent button style. Secondary actions use
  native default buttons. Avoid custom hover colours, fake buttons, emoji, and
  unexplained icon-only controls. Every icon-only action needs an accessible
  name and a familiar glyph.
- Cards group related choices or status. They use a 12 px radius and native
  card resources. Do not add a border around ordinary empty states merely to
  make them look important.

## Recovery flow and safety

1. Detect and select the Mi Assistant USB interface.
2. Read identity or test the connection before offering package actions.
3. Select an existing Recovery ROM ZIP or download an approved package into
   the configured download location.
4. Display validation, wipe, progress, cancellation, completion, and error
   state in one continuous task flow.

Disable operations that require a device or a valid ZIP and explain why in
plain language. Keep server-requested data wipes visually distinct, explicit,
and confirmed. Never signal a completed flash before the backend reports it.

## Accessibility and localization

- Preserve keyboard navigation, focus visuals, native control semantics, and
  high-contrast fallback.
- Set `AutomationProperties.Name` for controls that do not have visible text.
- Use semantic localization keys. UI strings must be concise and natural in
  context, not literal translations of implementation language.
- Allow text to wrap and controls to reflow at narrow widths. Test at 100,
  125, 150, 175, and 200 percent DPI for clipping, crowded title bars, and
  oversized hit targets.

## Design review checklist

Before merging a Windows UI change, verify system theme and accent behaviour,
keyboard and screen-reader names, narrow-width navigation, high DPI layout,
localized text wrapping, disabled states without a USB device, and the full
error and confirmation paths.
