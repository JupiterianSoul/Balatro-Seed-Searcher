import { useEffect, useId, useMemo, useRef, useState } from 'react';

// Searchable dropdown. Replaces native <select> for long lists (jokers, tags,
// bosses, vouchers, cards) so the user can type to filter instead of scrolling
// through 150+ rows on mobile.
//
// Keyboard:
//   ArrowDown / ArrowUp — move highlight
//   Enter               — pick highlighted option
//   Escape              — close
//   Tab                 — close without changing selection

export type ComboboxProps = {
  value: string;
  options: readonly string[];
  onChange: (next: string) => void;
  ariaLabel?: string;
  placeholder?: string;
  /** CSS class applied to the wrapper. Defaults to `combobox--wide`. */
  widthClass?: string;
};

export function Combobox({
  value,
  options,
  onChange,
  ariaLabel,
  placeholder = 'Type to search…',
  widthClass = 'combobox--wide',
}: ComboboxProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [highlight, setHighlight] = useState(0);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const listboxId = useId();

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return options;
    return options.filter(o => o.toLowerCase().includes(q));
  }, [query, options]);

  // Reset highlight when filter changes or list opens
  useEffect(() => {
    setHighlight(0);
  }, [query, open]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!wrapperRef.current) return;
      if (!wrapperRef.current.contains(e.target as Node)) {
        setOpen(false);
        setQuery('');
      }
    };
    window.addEventListener('mousedown', handler);
    return () => window.removeEventListener('mousedown', handler);
  }, [open]);

  // Keep the highlighted row scrolled into view
  useEffect(() => {
    if (!open || !listRef.current) return;
    const el = listRef.current.children[highlight] as HTMLElement | undefined;
    el?.scrollIntoView({ block: 'nearest' });
  }, [highlight, open]);

  function pick(opt: string) {
    onChange(opt);
    setOpen(false);
    setQuery('');
  }

  function onKey(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHighlight(h => Math.min(h + 1, Math.max(filtered.length - 1, 0)));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlight(h => Math.max(h - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (filtered[highlight]) pick(filtered[highlight]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      setOpen(false);
      setQuery('');
    } else if (e.key === 'Tab') {
      setOpen(false);
      setQuery('');
    }
  }

  return (
    <div className={`combobox ${widthClass}`} ref={wrapperRef}>
      {!open && (
        <button
          type="button"
          className="combobox-trigger"
          aria-label={ariaLabel}
          aria-haspopup="listbox"
          aria-expanded={false}
          onClick={() => {
            setOpen(true);
            // Defer focus so the input is mounted
            setTimeout(() => inputRef.current?.focus(), 0);
          }}
        >
          <span className="combobox-trigger-value">{value || placeholder}</span>
          <span className="combobox-trigger-caret" aria-hidden="true">▾</span>
        </button>
      )}
      {open && (
        <>
          <input
            ref={inputRef}
            type="text"
            className="combobox-input"
            value={query}
            placeholder={placeholder}
            aria-label={ariaLabel}
            aria-autocomplete="list"
            aria-controls={listboxId}
            aria-expanded={true}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={onKey}
          />
          <ul
            id={listboxId}
            ref={listRef}
            role="listbox"
            className="combobox-list"
          >
            {filtered.length === 0 && (
              <li className="combobox-empty">No matches</li>
            )}
            {filtered.map((opt, i) => (
              <li
                key={opt}
                role="option"
                aria-selected={opt === value}
                className={
                  'combobox-option' +
                  (i === highlight ? ' combobox-option--active' : '') +
                  (opt === value ? ' combobox-option--selected' : '')
                }
                onMouseDown={e => {
                  // mousedown so we beat the outside-click handler
                  e.preventDefault();
                  pick(opt);
                }}
                onMouseEnter={() => setHighlight(i)}
              >
                {opt}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
