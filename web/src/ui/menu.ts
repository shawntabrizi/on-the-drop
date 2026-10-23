// The section label menu: the words of the map's vocabulary; the computer's
// word is marked "auto", the word in use is checked; "Reset to auto" undoes
// the user's change. Keyboard: arrows move, Enter picks, Escape closes.

export interface MenuOptions {
  title: string;
  words: string[];
  auto: string;
  current: string;
  user: boolean;
  pick(word: string): void;
  reset(): void;
  closed(): void;
}

let open: HTMLElement | null = null;

export function closeMenu(): void {
  const m = open;
  open = null;
  if (m) {
    m.remove();
    m.dispatchEvent(new Event('closed'));
  }
}

export function menuOpen(): boolean {
  return open !== null;
}

export function openMenu(anchor: HTMLElement, o: MenuOptions): void {
  closeMenu();
  const m = document.createElement('div');
  m.className = 'menu';
  m.setAttribute('role', 'menu');
  m.setAttribute('aria-label', o.title);
  const h = document.createElement('div');
  h.className = 'menu-title';
  h.textContent = o.title;
  m.append(h);
  const items: HTMLButtonElement[] = [];
  for (const w of o.words) {
    const b = document.createElement('button');
    b.setAttribute('role', 'menuitemradio');
    b.setAttribute('aria-checked', String(w === o.current));
    b.className = 'menu-item' + (w === o.current ? ' checked' : '');
    b.innerHTML = `<span class="check" aria-hidden="true">${w === o.current ? '●' : ''}</span><span class="word"></span>${w === o.auto ? '<span class="auto">auto</span>' : ''}`;
    b.querySelector('.word')!.textContent = w;
    b.addEventListener('click', () => {
      closeMenu();
      o.pick(w);
    });
    items.push(b);
    m.append(b);
  }
  const reset = document.createElement('button');
  reset.setAttribute('role', 'menuitem');
  reset.className = 'menu-item reset';
  reset.textContent = 'Reset to auto';
  reset.disabled = !o.user;
  reset.addEventListener('click', () => {
    closeMenu();
    o.reset();
  });
  items.push(reset);
  m.append(reset);
  m.addEventListener('closed', () => o.closed());
  m.addEventListener('keydown', (e) => {
    const live = items.filter((b) => !b.disabled);
    const i = live.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'Escape') {
      e.preventDefault();
      closeMenu();
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const n = live.length;
      live[(i + (e.key === 'ArrowDown' ? 1 : n - 1) + n) % n]?.focus();
    } else if (e.key === 'Tab') {
      e.preventDefault();
    }
    e.stopPropagation();
  });
  document.body.append(m);
  // place it above the anchor, inside the viewport (16 px gutters)
  const r = anchor.getBoundingClientRect();
  const mw = m.offsetWidth;
  const mh = m.offsetHeight;
  const left = Math.max(16, Math.min(window.innerWidth - mw - 16, r.left + r.width / 2 - mw / 2));
  const top = r.top - mh - 8 >= 8 ? r.top - mh - 8 : Math.min(window.innerHeight - mh - 8, r.bottom + 8);
  m.style.left = `${left}px`;
  m.style.top = `${Math.max(8, top)}px`;
  open = m;
  (items.find((b) => b.getAttribute('aria-checked') === 'true') ?? items[0]).focus();
  setTimeout(() => {
    const away = (e: PointerEvent) => {
      if (open === m && !m.contains(e.target as Node)) closeMenu();
      if (open !== m) document.removeEventListener('pointerdown', away, true);
    };
    document.addEventListener('pointerdown', away, true);
  });
}
