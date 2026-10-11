# Accessibility foundations

Aim for WCAG 2.2 AA. The package uses native controls, theme-specific semantic colors, visible keyboard focus, and reduced-motion support. Application-specific accessibility still needs verification when real app flows are connected.

## Color and contrast

Normal text should reach 4.5:1 against its surface; large text and meaningful UI boundaries should reach 3:1. The integrity check evaluates text, muted text, action labels, links, focus, and semantic status pairs in both themes. Mint and cyan are bright brand accents and are not intended as small text on white. Use navy lettering on bright fills.

Every app status has a readable label. Icons and colors reinforce the label. Unsupported apps use neutral styling; a warning is reserved for something requiring review.

## Keyboard and focus

Keep logical DOM order. Never remove the focus outline without an equivalent. Use real buttons and links, and make all actions usable without a pointer. Standard controls have a 42 px default height; compact controls are 34 px and should be given extra spacing when used with a touch pointer.

Tabs implement a roving tab stop and ArrowLeft/ArrowRight/Home/End navigation. Native dialogs constrain keyboard focus while open and close with Escape. The demo returns focus to the trigger. Placeholders do not replace labels.

## Screen readers

Use landmarks, an ordered heading structure, visible form labels, table captions and header scopes, and named dialogs. Decorative icons use `aria-hidden="true"`; meaningful image assets have descriptive alt text. Icon-only controls require `aria-label` or visible hidden text.

Connect field help/errors with `aria-describedby`. Mark invalid fields after validation. Toasts use polite status announcements for ordinary success and alert announcements for urgent errors. Avoid repeated live-region progress updates.

## Motion and scaling

Respect `prefers-reduced-motion`. Motion only clarifies changes and never carries essential information. Keep content usable at 200% zoom. The reference adapts to narrow widths; tables keep their structure with horizontal scrolling. Allow text to wrap and avoid fixed control widths for translated labels.

## Desktop integration

When implementing the native shell, check OS keyboard conventions, accessible window titles, system contrast preferences, screen-reader behavior through the webview, and tray-icon legibility. Do not assume browser verification covers every desktop platform.
