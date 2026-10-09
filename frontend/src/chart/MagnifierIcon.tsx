export function MagnifierIcon({ symbol }: { symbol?: "plus" | "minus" }) {
  return (
    <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true">
      <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.6" />
      <line x1="12.4" y1="12.4" x2="17" y2="17" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      {symbol === "plus" && (
        <>
          <line x1="8" y1="5.2" x2="8" y2="10.8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          <line x1="5.2" y1="8" x2="10.8" y2="8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </>
      )}
      {symbol === "minus" && (
        <line x1="5.2" y1="8" x2="10.8" y2="8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
      )}
    </svg>
  );
}
