# Brioche Frontend Style Guide

> Companion to `docs/PHILOSOPHY.md`. Governs Tailwind CSS v4, React component styling, and desktop-app UI conventions in `crates/apps/brioche-desktop/frontend`.
>
> This guide is **prescriptive**, not descriptive. When the existing code disagrees with this guide, update the code.

---

## 1. Philosophy

The Brioche desktop UI is a **mechanism**, not a design system playground. It should be:

- **Consistent**: one palette, one spacing scale, one component vocabulary.
- **Maintainable**: the next maintainer should recognize every pattern.
- **Accessible**: focus states, disabled states, and reduced-motion support are mandatory.
- **Fast**: avoid unnecessary CSS, avoid runtime style computation, and let Tailwind tree-shake unused utilities.

---

## 2. Tailwind v4 Architecture

### 2.1 Single source of truth: `@theme`

All design tokens live in `src/styles/global.css` inside the `@theme` block.

```css
@theme {
  /* Base palette (from https://brio.build/) */
  --color-brio-bg: #1a1614;
  --color-brio-foam: #f5e6d3;
  --color-brio-clay: #bd6532;
  --color-brio-sand: #ffddb3;

  /* Semantic tokens */
  --color-bg-base: #0a0808;
  --color-bg-surface: #14100e;
  --color-bg-elevated: #1c1715;
  --color-fg-primary: #f5e6d3;
  --color-fg-secondary: #d8cdc0;
  --color-fg-muted: #6b6258;
  --color-accent: #bd6532;
  --color-accent-text: #ffddb3;

  /* Spacing */
  --spacing-1: 4px;
  --spacing-2: 8px;
  --spacing-3: 12px;
  --spacing-4: 16px;
  --spacing-5: 20px;

  /* etc. */
}
```

Tailwind v4 exposes every `@theme` key as both a CSS custom property (`--color-bg-base`) and a utility class (`bg-bg-base`). Prefer the utility class.

### 2.2 Semantic tokens over literal colors

Always use a semantic token. Never hardcode a hex or RGB value in a component.

| Bad | Good |
|-----|------|
| `text-[#e8a0a0]` | `text-error-text` |
| `bg-green-800/20` | `bg-status-success-bg text-status-success-text` |
| `text-amber-500` | `text-status-warning-text` |
| `bg-[#bd6532]` | `bg-accent` |

If a token does not exist, add it to `@theme` and `:root` aliases rather than hardcoding.

### 2.3 Use Tailwind utilities, not CSS variables in `className`

Do not reach into `var(--*)` inside class strings. Tailwind generates the variable reference for you.

| Bad | Good |
|-----|------|
| `px-[var(--space-3)]` | `px-3` |
| `gap-[var(--space-2)]` | `gap-2` |
| `text-[var(--text-primary)]` | `text-fg-primary` |
| `bg-[var(--bg-2)]` | `bg-bg-elevated` |

The only exceptions are values that Tailwind cannot express, such as dynamic runtime positions or complex gradients defined in CSS.

### 2.4 Avoid arbitrary values

Arbitrary values (`w-[280px]`, `text-[13px]`, `h-[52px]`) are a code smell. They usually mean a token or utility is missing.

| Bad | Good |
|-----|------|
| `w-[280px]` | Extend the theme with `--width-sidebar: 280px` and use `w-sidebar`, or use a standard Tailwind width |
| `text-[13px]` | Use `text-xs` (12px) or `text-sm` (14px), or add `--text-size-xs2: 13px` to `@theme` |
| `h-[52px]` | Use a standard size or add `--height-header: 52px` |

If you must use an arbitrary value, leave a comment explaining why it cannot be a token.

### 2.5 Use `@utility` for repeated style patterns

Tailwind v4 replaces v3's `@apply` with `@utility`. Use it for cross-cutting style patterns that are not full React components.

```css
@utility btn-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius);
  color: var(--fg-muted);
  transition: all 200ms ease;

  &:hover {
    color: var(--fg-secondary);
    background: var(--bg-highlight);
  }

  &:focus-visible {
    outline: none;
    box-shadow: 0 0 0 1px var(--accent-glow);
  }
}
```

Then consume it as a utility:

```tsx
<button className="btn-icon w-8 h-8" />
```

Rules:
- `@utility` should be **pure style**; no structure.
- Prefer explicit CSS properties inside `@utility`; do **not** use `@apply`.
- If a pattern has structure (header, footer, list item), make it a React component instead.

### 2.6 Vanilla CSS for global/base rules

Use plain CSS for:
- Global resets and base element styles (`body`, `button`, `input`).
- Complex selectors that utilities cannot express (`:not()`, `:has()` where supported).
- Markdown content styling (`message-content h1`, `.code-block-header`).
- Scrollbars, animations, and keyframes.

Do not use `@apply` to consume utilities inside these rules. Tailwind v4 discourages it, and explicit CSS is clearer.

### 2.7 React components for structural reuse

If a UI element repeats with the same structure and props, extract a React component. Do not copy-paste class strings.

```tsx
// Good: component encapsulates structure + style
<PanelOverlay title="Settings" onClose={onClose}>
  ...
</PanelOverlay>

// Bad: same backdrop/header/body classes copied into every panel
```

---

## 3. Component-Level Conventions

### 3.1 Class ordering

Group classes in this order:

1. **Layout**: `flex`, `grid`, `block`, `hidden`, `relative`, `absolute`, `inset-0`, `z-10`
2. **Sizing**: `w-full`, `h-screen`, `min-w-0`, `max-w-md`, `flex-1`
3. **Spacing**: `p-4`, `px-3`, `gap-2`, `m-2`
4. **Typography**: `text-sm`, `font-medium`, `text-fg-primary`, `font-mono`
5. **Visual**: `bg-bg-surface`, `border`, `rounded`, `shadow-md`
6. **Interactive states**: `hover:bg-bg-highlight`, `focus-visible:ring-1`, `disabled:opacity-50`
7. **Animation**: `transition-all`, `duration-200`, `animate-fadeIn`
8. **Responsive**: `max-[900px]:absolute`
9. **Arbitrary/conditional**: template literals and `cn()` last

### 3.2 Conditional classes

Use `cn()` from `src/components/ui/lib.ts` (or `clsx` + `tailwind-merge`) for conditional classes. Never concatenate strings manually.

```tsx
// Good
className={cn(
  "flex items-center gap-2 px-3 py-2 rounded transition-colors",
  active && "bg-bg-highlight border-accent-dim/40",
  !active && "hover:bg-bg-elevated/30"
)}

// Bad
className={`flex items-center gap-2 px-3 py-2 rounded transition-colors ${active ? "bg-bg-highlight" : "hover:bg-bg-elevated/30"}`}
```

### 3.3 Buttons

Every button must:
- Have `type="button"` unless it is a submit.
- Have a clear interaction state (`hover`, `active`, `focus-visible`, `disabled`).
- Use a semantic color token; destructive actions use `text-error-text hover:bg-error-bg`.
- Prefer the existing `@utility` classes: `btn-icon`, `btn-toolbar`, `btn-primary`, `btn-secondary`, `btn-ghost`.

### 3.4 Forms and inputs

- Use the shared `Input`, `Textarea`, `Label`, `Checkbox`, and `Select` primitives.
- Never remove focus rings. Use `focus-visible:` variants.
- Disabled inputs must have `disabled:opacity-50 disabled:cursor-not-allowed`.
- Error messages use `text-error-text`.

### 3.5 Status colors

Status indicators (success, warning, error, info) must use semantic tokens, not arbitrary Tailwind colors.

```css
@theme {
  --color-status-success-bg: rgba(22, 163, 74, 0.15);
  --color-status-success-text: #4ade80;
  --color-status-warning-bg: rgba(202, 138, 4, 0.15);
  --color-status-warning-text: #facc15;
  --color-status-error-bg: rgba(220, 38, 38, 0.15);
  --color-status-error-text: #f87171;
  --color-status-info-bg: rgba(59, 130, 246, 0.15);
  --color-status-info-text: #60a5fa;
}
```

### 3.6 No inline `style={}` for layout

Inline `style` props are forbidden except for genuinely dynamic values (e.g., `left: contextMenu.x`). Even then, prefer CSS custom properties if the value is themable.

| Bad | Good |
|-----|------|
| `style={{ flex: 1, background: 'transparent' }}` | `className="flex-1 bg-transparent"` |
| `style={{ padding: 24, textAlign: 'center' }}` | `className="p-6 text-center"` |

---

## 4. Accessibility

### 4.1 Focus management

- All interactive elements must have a visible focus indicator.
- Use `focus-visible:` so focus rings do not appear on mouse click.
- Modals must trap focus and be dismissible via `Escape`.

### 4.2 Reduced motion

Respect `prefers-reduced-motion`:

```css
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
```

The global reset already covers this; do not override it with per-component transitions.

### 4.3 Color contrast

Text on interactive elements must meet WCAG 2.1 AA. Use `text-fg-primary` on dark surfaces and `text-accent-text` on accent backgrounds.

### 4.4 Disabled states

Disabled controls must be visually and semantically disabled:

```tsx
<button disabled={isLoading} className="disabled:opacity-50 disabled:cursor-not-allowed">
```

---

## 5. Anti-Patterns

These are forbidden:

| Anti-pattern | Why | Fix |
|--------------|-----|-----|
| `@apply` inside component CSS | Tailwind v4 discourages it; utilities should be components or `@utility` | Move to `@utility` or a React component |
| `!important` in class strings | Breaks theming and accessibility | Remove or use stronger specificity |
| Hardcoded hex/RGB colors | Breaks dark mode and palette consistency | Add/use semantic token |
| `var(--space-*)` in `className` | Defeats Tailwind's utility model | Use `p-3`, `gap-2`, etc. |
| Arbitrary pixel values | Usually signals a missing token | Add token or use standard utility |
| Copy-pasted class strings | Violates DRY, drifts over time | Extract component or `@utility` |
| Inline `style` for layout | Hard to override, non-themable | Use Tailwind utilities |
| `text-text-*` / `bg-bg-[0-6]` aliases | Deprecated legacy tokens | Use `fg-*` / `bg-bg-*` semantic tokens |

---

## 6. Migration Notes

### Legacy tokens

The following token families are deprecated but kept as `:root` aliases for backward compatibility. Do not use them in new code:

- `--bg-1`, `--bg-2`, `--bg-3`, `--bg-4`, `--bg-5`, `--bg-6` → use `--bg-surface`, `--bg-elevated`, `--bg-highlight`, `--bg-subtle`, etc.
- `--text-primary`, `--text-secondary`, `--text-tertiary`, `--text-muted`, `--text-dim` → use `--fg-primary`, `--fg-secondary`, `--fg-tertiary`, `--fg-muted`, `--fg-dim`.

### Known debt: arbitrary font sizes

Several components still use arbitrary font sizes such as `text-[9px]`, `text-[10px]`, `text-[11px]`, and `text-[13px]`. These should eventually be folded into a standardized type scale (`text-xs`, `text-sm`, `text-base`, etc.) or custom `--text-*` theme keys. When touching a component that uses them, prefer replacing the arbitrary value with the nearest standard Tailwind text size or adding a named theme token if the size is reused.

### When touching a file

If you edit a component, bring its styling up to this guide within reason. Do not do a pure style-only refactor unless the file is already under change.

---

## 7. Verification

Before submitting frontend changes, run:

```bash
cd crates/apps/brioche-desktop/frontend
pnpm test
pnpm build
```

And manually verify in the browser that:
- The component renders with the Brio palette.
- Focus, hover, active, and disabled states are visible.
- No console errors about invalid Tailwind classes.

---

## 8. References

- [Tailwind CSS v4 Best Practices](https://tailgrids.com/blog/tailwind-css-best-practices)
- [Defining Reusable Styles in Tailwind CSS v4](https://levelup.gitconnected.com/how-to-define-reusable-styles-in-tailwind-css-v4-cfbe723b9ab8)
- [Tailwind `@theme` inline discussion](https://github.com/tailwindlabs/tailwindcss/discussions/15600#discussioncomment-11805993)
- `docs/PHILOSOPHY.md` — Rust and architecture standards
- `crates/apps/brioche-desktop/frontend/src/styles/global.css` — canonical token definitions
