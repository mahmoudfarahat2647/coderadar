/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        canvas: "#121212",
        panel: "#181818",
        elevated: "#222222",
        overlay: "#2A2A2A",
        surface: {
          DEFAULT: "#131313",
          canvas: "#121212",
          panel: "#181818",
          elevated: "#222222",
          overlay: "#2A2A2A",
          bright: "#393939",
          variant: "#353534",
          container: "#201f1f",
          "container-low": "#1c1b1b",
          "container-high": "#2a2a2a",
        },
        border: {
          subtle: "#282828",
          strong: "#383838",
        },
        primary: {
          DEFAULT: "#1E8854",
          container: "#3fa26c",
          tint: "#78da9e",
        },
        secondary: {
          DEFAULT: "#3ECF8E",
          container: "#00b173",
        },
        tertiary: {
          DEFAULT: "#00B4D8",
          bright: "#4cd6fb",
        },
        status: {
          critical: "#EF4444",
          high: "#F97316",
          medium: "#F59E0B",
          low: "#06B6D4",
          clean: "#10B981",
        },
        text: {
          primary: "#e5e2e1",
          secondary: "#becabe",
          muted: "#889489",
        },
      },
      fontFamily: {
        sans: ["Inter", "-apple-system", "BlinkMacSystemFont", "Segoe UI", "Roboto", "sans-serif"],
        headline: ["Manrope", "Inter", "sans-serif"],
        mono: ["JetBrains Mono", "SFMono-Regular", "Menlo", "Monaco", "Consolas", "monospace"],
      },
      boxShadow: {
        glow: "0 0 20px -5px rgba(62, 207, 142, 0.25)",
        "glow-critical": "0 0 20px -5px rgba(239, 68, 68, 0.35)",
      },
    },
  },
  plugins: [],
};
