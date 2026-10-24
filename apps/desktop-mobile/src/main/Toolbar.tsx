import type { ComponentChildren } from "preact";

export type ToolIcon = "trash" | "sync" | "export" | "settings" | "browser";

export interface ToolButtonProps {
  icon: ToolIcon;
  label: string;
  onClick: () => void;
}

/**
 * A top bar button: icon and label, or only the icon when the window is narrow (main.css). The label stays its
 * accessible name and its tooltip either way.
 */
export function ToolButton({ icon, label, onClick }: ToolButtonProps) {
  return (
    <button type="button" class="secondary tool-button" title={label} aria-label={label} onClick={onClick}>
      <svg class="tool-icon" viewBox="0 0 16 16" aria-hidden="true">
        {ICONS[icon]}
      </svg>
      <span class="tool-label">{label}</span>
    </button>
  );
}

const ICONS: Record<ToolIcon, ComponentChildren> = {
  trash: <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5M7 7v4M9 7v4" />,
  sync: <path d="M12.5 6A4.6 4.6 0 0 0 4 5.2M3.5 10A4.6 4.6 0 0 0 12 10.8M4 2.8v2.5h2.5M12 13.2v-2.5H9.5" />,
  export: <path d="M8 10V2.5M5.2 5.2 8 2.5l2.8 2.7M3 9v4h10V9" />,
  settings: (
    <>
      <circle cx="8" cy="8" r="2" />
      <path d="M8 2v1.6M8 12.4V14M2 8h1.6M12.4 8H14M3.8 3.8l1.1 1.1M11.1 11.1l1.1 1.1M3.8 12.2l1.1-1.1M11.1 4.9l1.1-1.1" />
    </>
  ),
  browser: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M2.5 8h11M8 2.5c1.6 1.6 2.4 3.4 2.4 5.5S9.6 11.9 8 13.5M8 2.5C6.4 4.1 5.6 5.9 5.6 8s.8 3.9 2.4 5.5" />
    </>
  ),
};
