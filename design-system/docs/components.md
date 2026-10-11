# Components

All component styles use a `kl-` prefix. Place components inside `.kl-root` to inherit Kitlane fonts and colors. Import `styles/index.css`. Components use the active semantic theme; their source is `styles/components.css`. The reference page provides working markup examples.

| Component | Classes / element | States and guidance |
| --- | --- | --- |
| Button | `.kl-button`, `--secondary`, `--ghost`, `--danger`, `--small`, `--large`, `--icon` | Use a real `button` for actions and an `a` for navigation. Use native `disabled`. Every icon-only button needs an accessible name. |
| Loading action | `.kl-button[aria-busy="true"]` | Keep a readable action label, disable duplicate submissions, and announce completion with a toast or inline status. |
| Field | `.kl-field`, `.kl-label`, `.kl-input`, `.kl-help`, `.kl-error` | A visible label is required. Link help/error with `aria-describedby`; set `aria-invalid="true"` only when validation has failed. |
| Search | `.kl-input-wrap`, input `type="search"` | Include a label, even if visually hidden. Searching should not destroy an active keyboard selection. |
| Select | `.kl-select` | Prefer the native select for short single-choice lists. |
| Textarea | `.kl-textarea` | Keep resizing available and use descriptive help for format requirements. |
| Checkbox / radio | `.kl-check`, native input | Group related options in a fieldset with a legend. Use radio inputs for mutually exclusive choices. |
| Switch | `.kl-switch`, checkbox with `role="switch"` | For immediate on/off preferences. The visible text describes the setting, not its current state. |
| Badge | `.kl-badge`, `--success`, `--info`, `--warning`, `--danger` | Always include a text status. Keep unsupported states neutral; they describe availability rather than a failure. |
| Alert | `.kl-alert`, `--success`, `--info`, `--warning`, `--danger` | For persistent context or a problem requiring action. Announce newly inserted urgent errors with `role="alert"`; avoid announcing every static example. |
| Tabs | `.kl-tabs`, standard tab/tabpanel roles, `initTabs()` | Arrow keys move between enabled tabs. Home/End select first/last. Tab moves into the panel. Use `manual: true` when activation is expensive. This helper supports horizontal tabs. |
| Navigation | `.kl-nav`, links | Set `aria-current="page"` on the active destination. Keep labels stable. |
| App list | `.kl-table-wrap`, `.kl-table`, `.kl-app-name` | Use table header scopes and a caption. Preserve app, version, status, and next action. Keep the table horizontally scrollable at narrow widths. |
| Progress | native `progress.kl-progress` | Connect a visible label with `for` or `aria-labelledby`. Omit `value` for indeterminate work. Announce completion rather than every percentage. |
| Empty state | `.kl-empty` | Explain what is missing and offer one useful primary action. |
| Skeleton | `.kl-skeleton` | Use `aria-hidden="true"` on decorative placeholders and `aria-busy="true"` on the region being loaded. |
| Disclosure | native `details.kl-accordion` | Use a clear `summary`. Native keyboard and disclosure semantics are retained. |
| Dialog | native `dialog.kl-dialog` | Provide a title and description, open with `showModal()`, and return focus to the opener. Use Cancel as the safe initial focus for installer decisions. |
| Toast | `toast()` and `.kl-toast-region` | Plain-text messages only. Success/info uses `role="status"`; danger uses `role="alert"`. Auto-dismiss pauses during focus or hover. Use duration `0` for a persistent message. |
| Tooltip | native `title` for optional hints | Hints supplement a visible label or accessible name. Essential instructions stay visible in the interface. |

## Button hierarchy

Use one primary action per decision area. Secondary buttons offer a related next step; ghost buttons handle low-emphasis actions. Use danger styling for destructive actions, and describe the object being removed. Avoid an icon-only install or update action.

## Form behavior

Validation must identify the affected field and explain how to fix it. Keep entered values after failure. Avoid invalid styles before users have interacted with a field. Prefer “Enter a name for this profile.” over “Invalid input.” Native controls handle keyboard interactions and operating-system conventions.

## Status vocabulary

- **Installed:** a supported app is present on this computer.
- **Update available:** a newer eligible version is listed; review its source and version.
- **Synced:** the desired app profile or compatible settings have synchronized. Installation status is separate.
- **Unsupported on this OS:** no verified native package is listed for this device's platform.
- **Closest verified match:** an alternative has a documented reason for the match; explain differences.
- **Source not verified:** provenance needs review before installation.

Always display the reason for incompatibility and the scope of verification. A provenance check is not a guarantee that software is risk-free.

## Product patterns

App profiles represent desired software. Each computer resolves the profile against its OS version and architecture. Keep the profile visible even when a package isn't available. Alternatives require an explicit choice. Updates should show app, current/new version, source, and policy before confirmation. Avoid presenting a synchronized app list as proof that all apps are installed.
