/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      animation: {
        'fade-in-out': 'fadeInOut 3s ease-in-out forwards',
      },
      keyframes: {
        fadeInOut: {
          '0%': { opacity: '0', transform: 'translate(-50%, 1rem)' },
          '10%': { opacity: '1', transform: 'translate(-50%, 0)' },
          '80%': { opacity: '1', transform: 'translate(-50%, 0)' },
          '100%': { opacity: '0', transform: 'translate(-50%, -0.5rem)' },
        },
      },
    },
  },
  plugins: [],
}
