import type { Metadata } from "next";
import { DM_Sans, JetBrains_Mono, Instrument_Serif } from "next/font/google";
import { AuthProvider } from "@/components/AuthProvider";
import ThemeToggle from "@/components/ThemeToggle";
import "./globals.css";

const dmSans = DM_Sans({
  subsets: ["latin"],
  variable: "--font-dm-sans",
  display: "swap",
});

const jetbrainsMono = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-jetbrains",
  display: "swap",
});

const instrumentSerif = Instrument_Serif({
  subsets: ["latin"],
  weight: "400",
  style: ["normal", "italic"],
  variable: "--font-instrument",
  display: "swap",
});

export const metadata: Metadata = {
  title: "Hanuman",
  description: "Autonomous AI agent platform",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  // Inline script to set theme before hydration (prevents flash of wrong theme).
  // Content is a static string literal — no user input, safe from XSS.
  const themeScript = `
    (function() {
      var t = localStorage.getItem('hanuman-theme');
      if (t === 'dark' || (!t && window.matchMedia('(prefers-color-scheme: dark)').matches)) {
        document.documentElement.setAttribute('data-theme', 'dark');
      }
    })();
  `;

  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body
        className={`${dmSans.variable} ${jetbrainsMono.variable} ${instrumentSerif.variable}`}
      >
        <AuthProvider>
          <div className="fixed top-4 left-4 z-50">
            <ThemeToggle />
          </div>
          <div className="relative z-10">{children}</div>
        </AuthProvider>
      </body>
    </html>
  );
}
