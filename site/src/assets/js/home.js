const tabs = document.querySelectorAll('.install-tab');
const commandCode = document.querySelector('.install-command code');
const commandWrap = document.querySelector('.install-command');
const copyButton = document.querySelector('.install-copy');
const storedTabKey = 'zerobrew-install-tab';

const setActiveTab = (activeTab) => {
  tabs.forEach((tab, index) => {
    const isActive = tab === activeTab;
    tab.classList.toggle('is-active', isActive);
    tab.setAttribute('aria-selected', isActive ? 'true' : 'false');
    tab.tabIndex = isActive ? 0 : -1;
    tab.dataset.tabIndex = String(index);
  });
};

if (tabs.length && commandCode) {
  const storedLabel = localStorage.getItem(storedTabKey);
  const initialTab =
    Array.from(tabs).find((tab) => tab.textContent.trim() === storedLabel) || tabs[0];

  commandCode.textContent = initialTab.dataset.command || '';
  setActiveTab(initialTab);

  tabs.forEach((tab) => {
    tab.addEventListener('click', () => {
      commandCode.textContent = tab.dataset.command || '';
      setActiveTab(tab);
      localStorage.setItem(storedTabKey, tab.textContent.trim());
    });

    tab.addEventListener('keydown', (event) => {
      if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') {
        return;
      }

      event.preventDefault();
      const currentIndex = Number(tab.dataset.tabIndex || '0');
      const direction = event.key === 'ArrowRight' ? 1 : -1;
      const nextIndex = (currentIndex + direction + tabs.length) % tabs.length;
      const nextTab = tabs[nextIndex];

      if (!nextTab) return;

      nextTab.focus();
      commandCode.textContent = nextTab.dataset.command || '';
      setActiveTab(nextTab);
      localStorage.setItem(storedTabKey, nextTab.textContent.trim());
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
  } catch (_err) {
    if (copyButton) {
      copyButton.classList.remove('is-copied');
    }
  }
};

if (commandWrap) {
  commandWrap.addEventListener('click', copyCommand);
}

if (copyButton) {
  copyButton.addEventListener('click', copyCommand);
}
