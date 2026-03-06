const tabs = document.querySelectorAll('.install-tab');
const commandCode = document.querySelector('.install-command code');
const commandWrap = document.querySelector('.install-command');
const copyButton = document.querySelector('.install-copy');
const storedTabKey = 'zerobrew-install-tab';

const setActiveTab = (activeTab) => {
  tabs.forEach((tab) => {
    const isActive = tab === activeTab;
    tab.classList.toggle('is-active', isActive);
    tab.setAttribute('aria-selected', isActive ? 'true' : 'false');
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
  });
}

let copiedTimeout;
const copyCommand = async () => {
  if (!commandCode) return;
  const text = commandCode.textContent.trim();
  if (!text) return;

  try {
    await navigator.clipboard.writeText(text);
    if (copyButton) {
      copyButton.classList.add('is-copied');
      clearTimeout(copiedTimeout);
      copiedTimeout = setTimeout(() => copyButton.classList.remove('is-copied'), 1400);
    }
  } catch (_err) {
    // no-op
  }
};

if (commandWrap) {
  commandWrap.addEventListener('click', copyCommand);
}

if (copyButton) {
  copyButton.addEventListener('click', copyCommand);
}
