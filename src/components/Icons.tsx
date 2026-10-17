// Inline SVG icons from docs/design/Main.dc.html.

export function LogIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
      <path d="M3 4h10M3 8h10M3 12h6" />
    </svg>
  );
}

export function SettingsIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
      <path d="M2.5 4.5h6M11.5 4.5h2M2.5 11.5h2M7.5 11.5h6" />
      <circle cx="10" cy="4.5" r="1.5" />
      <circle cx="6" cy="11.5" r="1.5" />
    </svg>
  );
}

export function ProbeLargeIcon() {
  return (
    <svg width="56" height="56" viewBox="0 0 56 56" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" aria-hidden="true">
      <rect x="10" y="18" width="30" height="20" rx="4" />
      <path d="M40 22h6v12h-6" />
      <path d="M43 26v4" />
      <circle cx="18" cy="28" r="2" />
      <path d="M24 28h10" />
    </svg>
  );
}

export function ProbeIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" aria-hidden="true">
      <rect x="2" y="6" width="12" height="8" rx="2" />
      <path d="M14 8h3v4h-3" />
    </svg>
  );
}

export function ChipIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
      <rect x="5" y="5" width="10" height="10" rx="1.5" />
      <path d="M8 2v3M12 2v3M8 15v3M12 15v3M2 8h3M2 12h3M15 8h3M15 12h3" />
    </svg>
  );
}

export function WriteIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M10 3v9M6 8l4 4 4-4M3 14v2.5h14V14" />
    </svg>
  );
}

export function ReadIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M10 12V3M6 7l4-4 4 4M3 14v2.5h14V14" />
    </svg>
  );
}

export function WarnIcon({ size = 22, strokeWidth = 1.7 }: { size?: number; strokeWidth?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 22 22" fill="none" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M11 3 2.5 18h17z" />
      <path d="M11 9v4.5M11 16v.01" />
    </svg>
  );
}

export function CheckSmallIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="m2.5 6.2 2.3 2.3 4.7-5" />
    </svg>
  );
}

export function CheckLargeIcon() {
  return (
    <svg width="34" height="34" viewBox="0 0 34 34" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="m9 17.5 5.5 5.5L25.5 11" />
    </svg>
  );
}

export function FolderIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" aria-hidden="true">
      <path d="M2 5.5V14a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V6.5a1 1 0 0 0-1-1H8.5L7 3.5H3a1 1 0 0 0-1 1z" />
    </svg>
  );
}
