const tabs = document.querySelectorAll(".install-tab");
const commandCode = document.querySelector(".install-command code");
const commandWrap = document.querySelector(".install-command");
const copyButton = document.querySelector(".install-copy");
const storedTabKey = "zerobrew-install-tab";

const benchCard = document.querySelector(".bench-card");
const benchLists = document.querySelectorAll(".bench-list[data-bench-index]");
const benchTitles = document.querySelectorAll(".bench-title-item");
const benchArrows = document.querySelectorAll(".bench-arrow");

const setActiveTab = (activeTab) => {
  tabs.forEach((tab) => {
    tab.classList.toggle("is-active", tab === activeTab);
  });
};

if (tabs.length && commandCode) {
  const storedLabel = localStorage.getItem(storedTabKey);
  const initialTab =
    Array.from(tabs).find((tab) => tab.textContent.trim() === storedLabel) || tabs[0];
  commandCode.textContent = initialTab.dataset.command || "";
  setActiveTab(initialTab);
  tabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      commandCode.textContent = tab.dataset.command || "";
      setActiveTab(tab);
      localStorage.setItem(storedTabKey, tab.textContent.trim());
    });
  });
}

const setActiveBench = (index) => {
  if (!benchLists.length) return;
  benchLists.forEach((list) => {
    list.classList.toggle(
      "is-active",
      Number(list.dataset.benchIndex) === index
    );
  });
  benchTitles.forEach((title) => {
    title.classList.toggle(
      "is-active",
      Number(title.dataset.benchIndex) === index
    );
  });
  if (benchCard) {
    benchCard.dataset.activeBench = String(index);
  }
};

if (benchLists.length) {
  setActiveBench(0);
  benchArrows.forEach((button) => {
    button.addEventListener("click", () => {
      const current = Number(benchCard?.dataset.activeBench || 0);
      const count = Number(benchCard?.dataset.benchCount || benchLists.length);
      const direction = button.dataset.direction === "prev" ? -1 : 1;
      const next = (current + direction + count) % count;
      setActiveBench(next);
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
      copyButton.classList.add("is-copied");
      clearTimeout(copiedTimeout);
      copiedTimeout = setTimeout(() => {
        copyButton.classList.remove("is-copied");
      }, 1400);
    }
  } catch (err) {
    return;
  }
};

if (commandWrap) {
  commandWrap.addEventListener("click", copyCommand);
}

if (copyButton) {
  copyButton.addEventListener("click", copyCommand);
}
