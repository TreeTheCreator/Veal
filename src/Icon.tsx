// Inline SVG icons, paths copied from the sprite in mockup/index.html.
const PATHS = {
  term: <><rect x="3" y="4" width="18" height="16" rx="3" /><path d="M7 9l3 3-3 3M13 15h4" /></>,
  folder: <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />,
  file: <><path d="M6 3h8l5 5v13H6z" /><path d="M14 3v5h5" /></>,
  gear: <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></>,
  down: <path d="M12 4v16M5 13l7 7 7-7" />,
  back: <path d="M20 12H4M11 5l-7 7 7 7" />,
};

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg className="i" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth={name === "down" || name === "back" ? 2.2 : 1.8} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {PATHS[name]}
    </svg>
  );
}

// Brand mark: a calf's head with a V blaze down the forehead. Filled, not stroked, so it
// lives outside PATHS; the eyes, muzzle, and blaze are cut out with a mask so it stays
// transparent on any ground. The eyes are the terminal prompt: `>` left, `-` right.
export function Logo({ size = 20 }: { size?: number }) {
  return (
    <svg className="i" width={size} height={size} viewBox="0 0 32 32" fill="currentColor" aria-hidden="true">
      <defs>
        <mask id="veal-logo-mask">
          <rect width="32" height="32" fill="#fff" />
          <rect x="11.5" y="20" width="9" height="6" rx="3" fill="#000" />
          <path d="M9.75 13.25 L12.35 15 L9.75 16.75" fill="none" stroke="#000" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
          <rect x="19.3" y="14.35" width="3.4" height="1.3" rx="0.65" fill="#000" />
          <polygon points="12.3,0 16,17 19.7,0" fill="#000" />
        </mask>
      </defs>
      <path d="M9 10 C5.5 7.5 1.5 8 2 10.5 C2.5 13.5 6 14.5 9 14 Z" />
      <path d="M23 10 C26.5 7.5 30.5 8 30 10.5 C29.5 13.5 26 14.5 23 14 Z" />
      <g mask="url(#veal-logo-mask)">
        <path d="M11 6 C13 4.5 19 4.5 21 6 C24.5 8.5 25 14 24 19 C23 25 20 29 16 29 C12 29 9 25 8 19 C7 14 7.5 8.5 11 6 Z" />
        <circle cx="11" cy="5.8" r="1.5" />
        <circle cx="21" cy="5.8" r="1.5" />
      </g>
      <circle cx="14" cy="23" r="1.1" />
      <circle cx="18" cy="23" r="1.1" />
    </svg>
  );
}
