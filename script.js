(() => {
  // 1. Page Theme Toggle (Dark Slate / Clean Light)
  const pageThemeBtn = document.getElementById('page-theme-toggle');
  const rootEl = document.documentElement;

  const sunSvg = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>`;
  const moonSvg = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>`;

  if (pageThemeBtn) {
    pageThemeBtn.addEventListener('click', () => {
      const isLight = rootEl.getAttribute('data-theme') === 'light';
      if (isLight) {
        rootEl.removeAttribute('data-theme');
        pageThemeBtn.innerHTML = sunSvg;
      } else {
        rootEl.setAttribute('data-theme', 'light');
        pageThemeBtn.innerHTML = moonSvg;
      }
    });
  }

  // 1b. Mobile Navigation Menu Toggle
  const mobileMenuBtn = document.getElementById('mobile-menu-toggle');
  const mobileNav = document.getElementById('mobile-nav');
  const barsSvg = `<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg>`;
  const closeSvg = `<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`;

  function closeMobileNav() {
    if (!mobileNav || !mobileMenuBtn) return;
    mobileNav.hidden = true;
    mobileMenuBtn.setAttribute('aria-expanded', 'false');
    mobileMenuBtn.innerHTML = barsSvg;
  }

  if (mobileMenuBtn && mobileNav) {
    mobileMenuBtn.addEventListener('click', () => {
      const willOpen = mobileNav.hidden;
      mobileNav.hidden = !willOpen;
      mobileMenuBtn.setAttribute('aria-expanded', willOpen ? 'true' : 'false');
      mobileMenuBtn.innerHTML = willOpen ? closeSvg : barsSvg;
    });

    mobileNav.querySelectorAll('a').forEach((link) => {
      link.addEventListener('click', closeMobileNav);
    });

    window.addEventListener('resize', () => {
      if (window.innerWidth > 860 && !mobileNav.hidden) {
        closeMobileNav();
      }
    });
  }

  // 2. Interactive EyesLive App Preview Tabs & Controls
  const appTabs = document.querySelectorAll('.app-tab');
  const tabHome = document.getElementById('tab-home');
  const tabSession = document.getElementById('tab-session');
  const demoConnectBtn = document.getElementById('demo-connect-btn');

  function activateAppTab(tabName) {
    appTabs.forEach((btn) => {
      const isTarget = btn.getAttribute('data-tab') === tabName;
      btn.classList.toggle('active', isTarget);
      btn.setAttribute('aria-selected', isTarget ? 'true' : 'false');
    });

    if (tabName === 'session') {
      tabHome?.classList.add('hidden');
      tabSession?.classList.add('active');
    } else {
      tabHome?.classList.remove('hidden');
      tabSession?.classList.remove('active');
    }
  }

  appTabs.forEach((btn) => {
    btn.addEventListener('click', () => {
      const tab = btn.getAttribute('data-tab');
      if (tab) activateAppTab(tab);
    });
  });

  if (demoConnectBtn) {
    demoConnectBtn.addEventListener('click', () => {
      activateAppTab('session');
    });
  }

  // App Window Theme Switcher (DarkSlate vs CleanLight inside the mockup)
  const toggleAppThemeBtn = document.getElementById('toggle-app-theme');
  const appThemeLabel = document.getElementById('app-theme-label');
  const eyesliveWindow = document.getElementById('eyeslive-window');

  if (toggleAppThemeBtn && eyesliveWindow && appThemeLabel) {
    toggleAppThemeBtn.addEventListener('click', () => {
      const isAppLight = eyesliveWindow.classList.toggle('app-light');
      appThemeLabel.textContent = isAppLight
        ? 'Tema do App: Modo Claro'
        : 'Tema do App: Escuro Suave';
    });
  }

  // Interactive Consent Modal inside the App Preview
  const triggerConsentBtn = document.getElementById('trigger-consent-demo');
  const consentModal = document.getElementById('consent-modal');
  const closeConsentBtns = document.querySelectorAll('[data-close-consent]');

  if (triggerConsentBtn && consentModal) {
    triggerConsentBtn.addEventListener('click', () => {
      consentModal.classList.toggle('active');
    });
  }

  closeConsentBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      consentModal?.classList.remove('active');
    });
  });

  // Copy Demo Desk ID
  const copyDemoIdBtn = document.getElementById('copy-demo-id');
  const copyDemoIdText = document.getElementById('copy-demo-id-text');
  const demoDeskId = document.getElementById('demo-desk-id');

  if (copyDemoIdBtn && copyDemoIdText && demoDeskId) {
    copyDemoIdBtn.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(demoDeskId.textContent.trim());
        copyDemoIdText.textContent = 'Copiado';
        setTimeout(() => {
          copyDemoIdText.textContent = 'Copiar ID';
        }, 1800);
      } catch (_) {
        // Fallback ignored
      }
    });
  }

  // 3. Copy CLI Commands & SHA-256 Buttons
  const copyIconSvg = `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
  const checkIconSvg = `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#34d399" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>`;

  document.querySelectorAll('.copy-cmd-btn').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const text = btn.getAttribute('data-copy');
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text);
        btn.innerHTML = checkIconSvg;
        setTimeout(() => {
          btn.innerHTML = copyIconSvg;
        }, 1800);
      } catch (_) {
        // Ignore clipboard errors on restricted contexts
      }
    });
  });

  // 4. Fetch Live Release Metadata from GitHub API
  async function syncLatestRelease() {
    try {
      const response = await fetch('https://api.github.com/repos/gabrielyandev/eyeslive/releases/latest');
      if (!response.ok) return;
      const data = await response.json();

      const tag = data.tag_name || 'v1.1.1';
      const navVersion = document.getElementById('nav-version');
      const specVersion = document.getElementById('spec-version');

      if (navVersion) navVersion.textContent = tag;
      if (specVersion) specVersion.textContent = `EyesLive ${tag}`;

      if (Array.isArray(data.assets) && data.assets.length > 0) {
        const exeAsset = data.assets.find((a) => a.name && a.name.endsWith('.exe')) || data.assets[0];
        if (exeAsset.size) {
          const mb = (exeAsset.size / (1024 * 1024)).toFixed(2);
          const specSize = document.getElementById('spec-size');
          const heroMeta = document.getElementById('hero-release-meta');
          if (specSize) specSize.textContent = `${mb} MB (Executável Único)`;
          if (heroMeta) {
            heroMeta.innerHTML = `
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10"/>
                <polyline points="12 6 12 12 16 14"/>
              </svg>
              Versão Estável ${tag} (${mb} MB)
            `;
          }
        }

        if (exeAsset.digest && exeAsset.digest.startsWith('sha256:')) {
          const sha = exeAsset.digest.replace('sha256:', '');
          const shaEl = document.getElementById('release-sha');
          const copyShaBtn = document.getElementById('copy-sha-btn');
          if (shaEl) shaEl.textContent = sha;
          if (copyShaBtn) copyShaBtn.setAttribute('data-copy', sha);
        }
      }
    } catch (_) {
      // Uses static v1.1.1 fallback gracefully
    }
  }

  syncLatestRelease();
})();
