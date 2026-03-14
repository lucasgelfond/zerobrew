const installPanel = document.querySelector('.install-panel');
const storedTabKey = 'zerobrew-install-tab';

if (installPanel) {
  const tabs = Array.from(installPanel.querySelectorAll('.install-tab'));
  const commandCode = installPanel.querySelector('.install-command code');
  const copyButton = installPanel.querySelector('.install-copy');

  const readStoredTab = () => {
    try {
      return localStorage.getItem(storedTabKey);
    } catch {
      return null;
    }
  };

  const writeStoredTab = (label) => {
    try {
      localStorage.setItem(storedTabKey, label);
    } catch {
      // Ignore storage failures so tab switching still works in restricted contexts.
    }
  };

  const setActiveTab = (activeTab) => {
    tabs.forEach((tab, index) => {
      const isActive = tab === activeTab;
      tab.classList.toggle('is-active', isActive);
      tab.setAttribute('aria-selected', isActive ? 'true' : 'false');
      tab.tabIndex = isActive ? 0 : -1;
      tab.dataset.tabIndex = String(index);
    });
  };

  const activateTab = (tab, focus = false) => {
    if (!commandCode || !tab) return;

    commandCode.textContent = tab.dataset.command || '';
    setActiveTab(tab);
    writeStoredTab(tab.textContent.trim());

    if (focus) {
      tab.focus();
    }
  };

  if (tabs.length && commandCode) {
    const storedLabel = readStoredTab();
    const initialTab =
      tabs.find((tab) => tab.textContent.trim() === storedLabel) || tabs[0];

    activateTab(initialTab);

    tabs.forEach((tab) => {
      tab.addEventListener('click', () => activateTab(tab));

      tab.addEventListener('keydown', (event) => {
        if (!['ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(event.key)) {
          return;
        }

        event.preventDefault();

        if (event.key === 'Home') {
          activateTab(tabs[0], true);
          return;
        }

        if (event.key === 'End') {
          activateTab(tabs[tabs.length - 1], true);
          return;
        }

        const currentIndex = Number(tab.dataset.tabIndex || '0');
        const direction = event.key === 'ArrowRight' ? 1 : -1;
        const nextIndex = (currentIndex + direction + tabs.length) % tabs.length;
        activateTab(tabs[nextIndex], true);
      });
    });
  }

  let copiedTimeout;
  const copyCommand = async () => {
    if (!commandCode) return;

    const text = commandCode.textContent.trim();
    if (!text) return;

    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
      } else {
        throw new Error('Clipboard API unavailable');
      }

      if (copyButton) {
        copyButton.classList.add('is-copied');
        clearTimeout(copiedTimeout);
        copiedTimeout = setTimeout(() => copyButton.classList.remove('is-copied'), 1400);
      }
    } catch {
      copyButton?.classList.remove('is-copied');
    }
  };

  copyButton?.addEventListener('click', copyCommand);
}
