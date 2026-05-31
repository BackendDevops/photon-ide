/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // ── Layered surface system (Material 3 / Fleet inspired) ──
        // Never pure black; neutral, slightly cool dark with real elevation.
        surface: {
          0: "#0b0d11", // editor canvas
          1: "#111419", // sidebars
          2: "#171b22", // tool panels
          3: "#1d222b", // floating panels
          4: "#222834", // dialogs
          5: "#28303d", // command palette
        },
        // Back-compat aliases used across components.
        bg: {
          DEFAULT: "#0b0d11",
          panel: "#111419",
          elevated: "#1d222b",
          hover: "#1c212a",
        },
        line: {
          DEFAULT: "#252b35",
          soft: "#1b2027",
          strong: "#323a47",
        },
        border: {
          DEFAULT: "#252b35",
          subtle: "#1b2027",
        },
        fg: {
          DEFAULT: "#e4e8ef",
          muted: "#9aa4b2",
          faint: "#6b7585",
        },
        accent: {
          DEFAULT: "#5b8cff",
          hover: "#6f9bff",
          soft: "#5b8cff22",
        },
        // ── Semantic states ──
        success: "#3fd07e",
        warn: "#f0b429",
        danger: "#ff5d5d",
        info: "#48b6ff",
        running: "#b07bff",
        ai: "#36d6c3",
      },
      fontFamily: {
        ui: ["Inter", "-apple-system", "BlinkMacSystemFont", "Segoe UI", "sans-serif"],
        mono: ["JetBrains Mono", "SFMono-Regular", "Menlo", "Consolas", "monospace"],
      },
      fontSize: {
        "2xs": ["11px", "15px"],
        xs: ["12.5px", "17px"],
        sm: ["13.5px", "19px"],
        base: ["15px", "22px"],
        md: ["16.5px", "24px"],
        lg: ["19px", "27px"],
        xl: ["22px", "30px"],
      },
      borderRadius: {
        sm: "5px",
        DEFAULT: "7px",
        md: "9px",
        lg: "12px",
        xl: "16px",
      },
      boxShadow: {
        e1: "0 1px 2px rgba(0,0,0,0.30)",
        e2: "0 2px 8px rgba(0,0,0,0.35)",
        e3: "0 8px 24px rgba(0,0,0,0.40)",
        e4: "0 16px 48px rgba(0,0,0,0.50)",
        glow: "0 0 0 1px rgba(91,140,255,0.4), 0 8px 30px rgba(91,140,255,0.18)",
      },
      transitionTimingFunction: {
        spring: "cubic-bezier(0.34, 1.56, 0.64, 1)",
        smooth: "cubic-bezier(0.4, 0, 0.2, 1)",
      },
      transitionDuration: {
        120: "120ms",
        180: "180ms",
        250: "250ms",
      },
      keyframes: {
        "pop-in": {
          from: { opacity: "0", transform: "translateY(-6px) scale(0.985)" },
          to: { opacity: "1", transform: "translateY(0) scale(1)" },
        },
        "fade-in": { from: { opacity: "0" }, to: { opacity: "1" } },
        "slide-up": {
          from: { opacity: "0", transform: "translateY(8px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        pulse2: {
          "0%,100%": { opacity: "1" },
          "50%": { opacity: "0.4" },
        },
      },
      animation: {
        "pop-in": "pop-in 180ms cubic-bezier(0.34,1.56,0.64,1)",
        "fade-in": "fade-in 150ms ease-out",
        "slide-up": "slide-up 200ms cubic-bezier(0.4,0,0.2,1)",
        pulse2: "pulse2 1.6s ease-in-out infinite",
      },
    },
  },
  plugins: [],
};
