/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        brand: {
          50: "#f0f7ff",
          500: "#3b82f6",
          900: "#1e3a5f",
        },
        // Noviq green research-station palette (Feature 009 UI, per UI Design.jpeg).
        noviq: {
          50: "#f3f8f1",
          100: "#e3efde",
          200: "#c6dfbd",
          300: "#9cc78d",
          400: "#6faa5b",
          500: "#4d8c3a",   // lime/forest accent
          600: "#3a6f2c",
          700: "#2e5724",   // forest (primary buttons)
          800: "#264620",
          900: "#1f3a1d",
          // Body text. Was #1b2a1b — described as "near-black" but in fact a dark
          // GREEN (rgb 27,42,27), and it read as washed out against the paper
          // canvas; there was no darker token in the palette to escape to.
          // Now an actual near-black with only a whisper of the brand hue
          // (architect, 2026-08-25: "make it near black").
          ink: "#0e120e",   // near-black text on light surfaces
          paper: "#fbfdfb", // canvas background
          panel: "#f3f6f2", // sidebar / rail background
          line: "#e2e8df",  // hairline borders
        },
      },
    },
  },
  plugins: [require("@tailwindcss/typography")],
};
