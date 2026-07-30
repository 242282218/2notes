import { defineConfig, presetIcons, presetUno } from "unocss";

export default defineConfig({
  presets: [
    presetUno(),
    presetIcons({
      scale: 1.2,
      warn: true,
    }),
  ],
  theme: {
    colors: {
      brand: {
        DEFAULT: "var(--color-brand)",
        hover: "var(--color-brand-hover)",
        active: "var(--color-brand-active)",
        subtle: "var(--color-brand-subtle)",
      },
      danger: {
        DEFAULT: "var(--color-danger)",
        hover: "var(--color-danger-hover)",
      },
      success: {
        DEFAULT: "var(--color-success)",
        subtle: "var(--color-success-subtle)",
      },
      warning: "var(--color-warning)",
      selected: "var(--color-bg-selected)",
      backdrop: "var(--color-backdrop)",
      bg: {
        base: "var(--color-bg-base)",
        elevated: "var(--color-bg-elevated)",
        secondary: "var(--color-bg-secondary)",
        hover: "var(--color-bg-hover)",
        active: "var(--color-bg-active)",
        inset: "var(--color-bg-inset)",
      },
      border: {
        DEFAULT: "var(--color-border)",
        strong: "var(--color-border-strong)",
        hover: "var(--color-border-hover)",
        subtle: "var(--color-border-subtle)",
        highlight: "var(--color-border-highlight)",
      },
      text: {
        primary: "var(--color-text-primary)",
        secondary: "var(--color-text-secondary)",
        tertiary: "var(--color-text-tertiary)",
        placeholder: "var(--color-text-placeholder)",
        disabled: "var(--color-text-disabled)",
      },
      "on-brand": "var(--color-on-brand)",
      "on-danger": "var(--color-on-danger)",
    },
    fontSize: {
      display: [
        "var(--text-display)",
        {
          "line-height": "var(--text-display-line-height)",
          "font-weight": "var(--text-display-weight)",
          "letter-spacing": "var(--text-display-tracking)",
        },
      ],
      title: [
        "var(--text-title)",
        {
          "line-height": "var(--text-title-line-height)",
          "font-weight": "var(--text-title-weight)",
          "letter-spacing": "var(--text-title-tracking)",
        },
      ],
      heading: [
        "var(--text-heading)",
        {
          "line-height": "var(--text-heading-line-height)",
          "font-weight": "var(--text-heading-weight)",
          "letter-spacing": "var(--text-heading-tracking)",
        },
      ],
      body: [
        "var(--text-body)",
        {
          "line-height": "var(--text-body-line-height)",
          "font-weight": "var(--text-body-weight)",
          "letter-spacing": "var(--text-body-tracking)",
        },
      ],
      ui: [
        "var(--text-ui)",
        {
          "line-height": "var(--text-ui-line-height)",
          "font-weight": "var(--text-ui-weight)",
          "letter-spacing": "var(--text-ui-tracking)",
        },
      ],
      caption: [
        "var(--text-caption)",
        {
          "line-height": "var(--text-caption-line-height)",
          "font-weight": "var(--text-caption-weight)",
          "letter-spacing": "var(--text-caption-tracking)",
        },
      ],
      micro: [
        "var(--text-micro)",
        {
          "line-height": "var(--text-micro-line-height)",
          "font-weight": "var(--text-micro-weight)",
          "letter-spacing": "var(--text-micro-tracking)",
        },
      ],
    },
    boxShadow: {
      sm: "var(--shadow-sm)",
      md: "var(--shadow-md)",
      lg: "var(--shadow-lg)",
      xl: "var(--shadow-xl)",
      glow: "var(--color-brand-glow)",
    },
    duration: {
      fast: "var(--duration-fast)",
      base: "var(--duration-base)",
      slow: "var(--duration-slow)",
    },
  },
  rules: [
    [
      "animate-fade-in",
      { animation: "fade-in var(--duration-base) var(--ease-out)" },
    ],
    [
      "skeleton-pulse",
      {
        animation:
          "skeleton-pulse var(--duration-slow) var(--ease-out) infinite alternate",
      },
    ],
    ["ease-token", { "transition-timing-function": "var(--ease-out)" }],
  ],
  shortcuts: {
    // Layout
    "flex-center": "flex items-center justify-center",
    "flex-between": "flex items-center justify-between",

    // Interactions
    "ring-focus":
      "focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-[var(--color-focus-ring-bg)]",
    // Elevation
    "elevation-0": "shadow-none",
    "elevation-panel": "shadow-none",
    "elevation-1": "border border-border shadow-none",
    "elevation-2": "border border-border shadow-md",
    "elevation-3": "border border-border shadow-xl",

    // UI Elements
    "btn-base":
      "inline-flex items-center justify-center gap-2 rounded-md font-medium transition-[color,background-color,border-color,box-shadow] duration-fast ease-token ring-focus disabled:opacity-55 disabled:cursor-not-allowed",
    "btn-primary":
      "btn-base min-w-[88px] h-[36px] px-3 bg-brand text-on-brand border border-brand hover:bg-brand-hover hover:border-brand-hover active:bg-brand-active shadow-[0_1px_2px_var(--color-brand-shadow)]",
    "btn-primary-danger":
      "btn-primary bg-danger text-on-danger border-danger hover:bg-danger-hover hover:border-danger-hover active:bg-danger-hover shadow-[0_1px_2px_var(--color-danger-shadow)]",
    "btn-secondary":
      "btn-base min-w-[88px] h-[36px] px-3 bg-bg-elevated text-text-primary border border-border-strong hover:bg-bg-hover hover:border-border-hover active:bg-bg-active",
    // Legacy aliases kept for readability; resolve to the canonical btn-icon shape
    // so legacy class names that predate this shortcut still generate CSS.
    "icon-button":
      "btn-base size-[34px] bg-transparent text-text-secondary border border-transparent hover:bg-bg-hover hover:text-text-primary active:bg-bg-active",
    "btn-icon":
      "btn-base size-[34px] bg-transparent text-text-secondary border border-transparent hover:bg-bg-hover hover:text-text-primary active:bg-bg-active",
    "btn-icon-danger":
      "btn-icon text-danger hover:bg-danger/10 hover:text-danger",
    // Drop-in menu entry used by the tree move menu and similar inline menus.
    "menu-item":
      "flex items-center gap-2 rounded-sm px-3 py-2 text-left text-ui text-text-primary transition-[background-color] duration-fast hover:bg-bg-hover",

    // Form Inputs
    "input-base":
      "w-full border border-border-strong rounded-md bg-bg-elevated text-text-primary placeholder:text-text-placeholder transition-[color,background-color,border-color,box-shadow] duration-fast ease-token ring-focus focus:border-brand disabled:opacity-55 disabled:cursor-not-allowed",
    "select-base":
      "h-[36px] px-3 border border-border-strong rounded-md bg-bg-elevated text-text-primary transition-[color,background-color,border-color,box-shadow] duration-fast ease-token ring-focus focus:border-brand disabled:opacity-55 disabled:cursor-not-allowed",

    // Translucent chrome is reserved for persistent navigation surfaces.
    "glass-panel": "bg-[var(--color-bg-translucent)] backdrop-blur-xl",
  },
});
