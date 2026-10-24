import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';

const INTERACTIVE_SELECTOR =
  'button, a, input, select, textarea, [role="button"], .window-controls, .titlebar-actions, .mobile-menu-button';

function inTitlebarDragZone(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element || typeof element.closest !== 'function') return false;
  const titlebar = element.closest('.titlebar');
  if (!titlebar) return false;
  return !element.closest(INTERACTIVE_SELECTOR);
}

export function installDesktopShell(): void {
  const current = getCurrentWindow();

  document.addEventListener(
    'mousedown',
    (event) => {
      if (event.button !== 0 || event.detail > 1) return;
      if (!inTitlebarDragZone(event.target)) return;
      void current.startDragging();
    },
    true,
  );

  document.addEventListener(
    'dblclick',
    (event) => {
      if (!inTitlebarDragZone(event.target)) return;
      void invoke('ysc_window_toggle_maximize');
    },
    true,
  );
}
