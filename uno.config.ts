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
    },
    boxShadow: {
      sm: "var(--shadow-sm)",
      md: "var(--shadow-md)",
      lg: "var(--shadow-lg)",
      xl: "var(--shadow-xl)",
      glow: "var(--color-brand-glow)",
      focus: "0 0 0 3px var(--color-focus-ring-bg)",
      "focus-glow": "0 0 0 3px var(--color-focus-ring-bg), var(--color-brand-glow)",
    },
    animation: {
      "scale-spring": "scale-spring 200ms cubic-bezier(0.34, 1.56, 0.64, 1)",
      "fade-in": "fade-in 200ms cubic-bezier(0.16, 1, 0.3, 1)",
      "pulse-saved": "pulse-saved 1.5s cubic-bezier(0.16, 1, 0.3, 1)",
    },
    keyframes: {
      "scale-spring": {
        "0%": { opacity: "0", transform: "scale(0.95)" },
        "100%": { opacity: "1", transform: "scale(1)" },
      },
      "fade-in": {
        "0%": { opacity: "0" },
        "100%": { opacity: "1" },
      },
      "pulse-saved": {
        "0%": { boxShadow: "0 0 0 0 rgba(22, 114, 70, 0.4), var(--shadow-md)" },
        "70%": { boxShadow: "0 0 0 6px rgba(22, 114, 70, 0), var(--shadow-md)" },
        "100%": { boxShadow: "var(--shadow-md)" },
      },
    },
  },
  shortcuts: {
    // Layout
    "flex-center": "flex items-center justify-center",
    "flex-between": "flex items-center justify-between",
    
    // Interactions
    "ring-focus": "focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-brand/20",
    "ring-focus-danger": "focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-danger/20",
    
    // UI Elements
    "btn-base": "inline-flex items-center justify-center gap-2 rounded-md font-medium transition-all duration-150 ring-focus disabled:opacity-55 disabled:cursor-not-allowed",
    "btn-primary": "btn-base min-w-[88px] h-[38px] px-3 bg-brand text-white border border-brand hover:bg-brand-hover hover:border-brand-hover active:bg-brand-active active:scale-97 shadow-[inset_0_1px_0_rgba(255,255,255,0.1),0_1px_2px_var(--color-brand-shadow)] hover:shadow-[inset_0_1px_0_rgba(255,255,255,0.12),0_2px_6px_var(--color-brand-shadow)]",
    "btn-primary-danger": "btn-primary bg-danger border-danger hover:bg-danger-hover hover:border-danger-hover active:bg-danger-hover shadow-[inset_0_1px_0_rgba(255,255,255,0.1),0_1px_2px_var(--color-danger-shadow)] hover:shadow-[inset_0_1px_0_rgba(255,255,255,0.12),0_2px_6px_var(--color-danger-shadow)]",
    "btn-secondary": "btn-base min-w-[88px] h-[38px] px-3 bg-bg-elevated text-text-primary border border-border-strong hover:bg-bg-hover hover:border-border-hover active:bg-bg-active active:scale-97",
    "btn-icon": "btn-base size-[34px] bg-bg-elevated text-text-secondary border border-border-strong hover:bg-bg-hover hover:border-border-hover hover:text-text-primary active:scale-95",
    "btn-icon-danger": "btn-icon text-danger border-danger/30 hover:bg-danger/10 hover:border-danger/40 hover:text-danger",
    
    // Form Inputs
    "input-base": "w-full border border-border-strong rounded-md bg-bg-base text-text-primary placeholder:text-text-placeholder transition-colors duration-150 ring-focus focus:border-brand focus:bg-bg-elevated disabled:opacity-55 disabled:cursor-not-allowed",
    "select-base": "h-[38px] px-3 border border-border-strong rounded-md bg-bg-elevated text-text-primary transition-colors duration-150 ring-focus focus:border-brand disabled:opacity-55 disabled:cursor-not-allowed",
    
    // Glassmorphism
    "glass-panel": "bg-white/70 dark:bg-[#181c1e]/70 backdrop-blur-xl",
  },
});