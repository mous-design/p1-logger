import { useEffect, useRef, useState } from "react";

/** Open/closed state for a click-to-open popup menu (the dashboard's cog
 * menu, the period-type picker). Closes on a click anywhere outside
 * `menuRef`, not just a second click on the trigger -- the usual dropdown
 * expectation. Only listens while open, so a closed menu doesn't keep a
 * document-wide handler around. */
export function usePopupMenu() {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  return { open, setOpen, menuRef };
}
