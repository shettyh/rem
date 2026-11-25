(() => {
  // Interactive Agent Skill + CLI scenario tabs
  const agentRoot = document.querySelector('[data-agent-tabs]');
  if (agentRoot) {
    const agentTabs = Array.from(agentRoot.querySelectorAll('[data-agent-tab]'));
    const agentPanels = Array.from(agentRoot.querySelectorAll('[data-agent-panel]'));

    function activateAgentTab(index) {
      agentTabs.forEach((tab, idx) => {
        const active = idx === index;
        tab.classList.toggle('is-active', active);
        tab.setAttribute('aria-selected', String(active));
      });
      agentPanels.forEach((panel, idx) => {
        const active = idx === index;
        panel.classList.toggle('is-active', active);
        panel.hidden = !active;
      });
    }

    agentTabs.forEach((tab) => {
      tab.addEventListener('click', () => {
        const idx = Number(tab.dataset.agentTab);
        if (!Number.isNaN(idx)) activateAgentTab(idx);
      });
    });
  }

  // Copy-to-clipboard buttons for CLI & skill commands
  document.querySelectorAll('[data-copy]').forEach((btn) => {
    let resetTimer;
    btn.addEventListener('click', async () => {
      const text = btn.getAttribute('data-copy');
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text);
        btn.textContent = 'Copied';
        btn.classList.add('is-copied');
        clearTimeout(resetTimer);
        resetTimer = setTimeout(() => {
          btn.textContent = 'Copy';
          btn.classList.remove('is-copied');
        }, 1800);
      } catch {
        // Clipboard API unavailable (e.g., non-secure context)
      }
    });
  });

  // Interactive Desktop + iPhone Slide Showcase
  const deck = document.querySelector('[data-showcase]');
  if (!deck) return;

  const slides = Array.from(deck.querySelectorAll('[data-slide-panel]'));
  const tabs = Array.from(document.querySelectorAll('.slide-tab[data-slide]'));
  const dots = Array.from(deck.querySelectorAll('.slide-dot[data-slide]'));
  const prevBtn = document.querySelector('[data-slide-prev]');
  const nextBtn = document.querySelector('[data-slide-next]');
  const counterEl = document.querySelector('[data-slide-current]');
  const surfaceBtns = Array.from(deck.querySelectorAll('[data-surface]'));
  const phoneButtons = Array.from(deck.querySelectorAll('[data-phone-spotlight]'));

  let current = 0;
  const total = slides.length;

  deck.setAttribute('data-mobile-surface', 'desktop');

  function clearSpotlights() {
    deck.querySelectorAll('.slide-stage.is-phone-spotlight').forEach((stage) => {
      stage.classList.remove('is-phone-spotlight');
    });
  }

  function goTo(nextIndex, direction) {
    const target = ((nextIndex % total) + total) % total;
    if (target === current) return;

    const dir = direction ?? (target > current ? 1 : -1);
    const prevSlide = slides[current];
    const nextSlide = slides[target];

    clearSpotlights();

    // Position outgoing slide opposite to travel direction and incoming from travel direction
    prevSlide.style.setProperty('--dir', String(-dir));
    prevSlide.classList.remove('is-active');
    prevSlide.setAttribute('aria-hidden', 'true');

    nextSlide.style.setProperty('--dir', String(dir));
    // Force layout so the incoming slide animates from the new direction
    void nextSlide.offsetWidth;
    nextSlide.classList.add('is-active');
    nextSlide.setAttribute('aria-hidden', 'false');

    tabs.forEach((tab, idx) => {
      const active = idx === target;
      tab.classList.toggle('is-active', active);
      tab.setAttribute('aria-selected', String(active));
    });

    dots.forEach((dot, idx) => {
      dot.classList.toggle('is-active', idx === target);
    });

    if (counterEl) {
      counterEl.textContent = String(target + 1).padStart(2, '0');
    }

    current = target;
  }

  tabs.forEach((tab) => {
    tab.addEventListener('click', () => {
      const idx = Number(tab.dataset.slide);
      if (!Number.isNaN(idx)) goTo(idx);
    });
  });

  dots.forEach((dot) => {
    dot.addEventListener('click', () => {
      const idx = Number(dot.dataset.slide);
      if (!Number.isNaN(idx)) goTo(idx);
    });
  });

  prevBtn?.addEventListener('click', () => goTo(current - 1, -1));
  nextBtn?.addEventListener('click', () => goTo(current + 1, 1));

  // Keyboard left/right navigation
  deck.addEventListener('keydown', (event) => {
    if (event.key === 'ArrowRight') {
      event.preventDefault();
      goTo(current + 1, 1);
    } else if (event.key === 'ArrowLeft') {
      event.preventDefault();
      goTo(current - 1, -1);
    } else if (event.key === 'Escape') {
      clearSpotlights();
    }
  });

  // Click iPhone frame to toggle spotlight zoom; clicking outside clears it
  phoneButtons.forEach((btn) => {
    btn.addEventListener('click', (event) => {
      event.stopPropagation();
      const stage = btn.closest('.slide-stage');
      if (!stage) return;
      stage.classList.toggle('is-phone-spotlight');
    });
  });

  document.addEventListener('click', (event) => {
    if (!(event.target instanceof Element) || !event.target.closest('[data-phone-spotlight]')) {
      clearSpotlights();
    }
  });

  // Mobile Desktop/iPhone toggle
  surfaceBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const surface = btn.dataset.surface;
      if (!surface) return;
      deck.setAttribute('data-mobile-surface', surface);
      surfaceBtns.forEach((b) => b.classList.toggle('is-active', b === btn));
    });
  });

  // Touch swipe support on the slide viewport
  let touchStartX = 0;
  let touchStartY = 0;

  deck.addEventListener(
    'touchstart',
    (event) => {
      if (event.touches.length !== 1) return;
      touchStartX = event.touches[0].clientX;
      touchStartY = event.touches[0].clientY;
    },
    { passive: true },
  );

  deck.addEventListener(
    'touchend',
    (event) => {
      if (event.changedTouches.length !== 1) return;
      const dx = event.changedTouches[0].clientX - touchStartX;
      const dy = event.changedTouches[0].clientY - touchStartY;
      if (Math.abs(dx) > 48 && Math.abs(dx) > Math.abs(dy) * 1.4) {
        goTo(dx < 0 ? current + 1 : current - 1, dx < 0 ? 1 : -1);
      }
    },
    { passive: true },
  );
})();
