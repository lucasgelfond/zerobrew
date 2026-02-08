const initTabs = () => {
  document.querySelectorAll("[data-tabs]").forEach((tabs) => {
    const list = tabs.querySelector(".docs-tab-list");
    const panels = Array.from(tabs.querySelectorAll(".docs-tab-panel"));
    if (!list || panels.length === 0) return;

    list.innerHTML = "";
    const buttons = [];
    panels.forEach((panel, index) => {
      const title = panel.dataset.tabTitle || `Tab ${index + 1}`;
      const button = document.createElement("button");
      button.type = "button";
      button.className = "docs-tab";
      button.textContent = title;
      button.setAttribute("role", "tab");
      button.setAttribute("aria-selected", "false");
      list.appendChild(button);
      buttons.push(button);

      button.addEventListener("click", () => {
        panels.forEach((item, itemIndex) => {
          const active = itemIndex === index;
          item.classList.toggle("is-active", active);
          buttons[itemIndex]?.setAttribute("aria-selected", active ? "true" : "false");
          buttons[itemIndex]?.classList.toggle("is-active", active);
        });
      });
    });

    panels.forEach((panel, index) => {
      panel.classList.toggle("is-active", index === 0);
    });
    if (buttons[0]) {
      buttons[0].classList.add("is-active");
      buttons[0].setAttribute("aria-selected", "true");
    }
  });
};

const initAccordion = () => {
  document.querySelectorAll(".docs-accordion-trigger").forEach((trigger) => {
    trigger.addEventListener("click", () => {
      const item = trigger.closest(".docs-accordion-item");
      if (!item) return;
      const isOpen = item.classList.toggle("is-open");
      trigger.setAttribute("aria-expanded", isOpen ? "true" : "false");
    });
  });
};

const initNavToggle = () => {
  document.querySelectorAll(".docs-nav-toggle").forEach((toggle) => {
    const nav = toggle.closest(".docs-nav");
    if (!nav) return;

    const setExpanded = (isOpen) => {
      nav.classList.toggle("is-open", isOpen);
      toggle.setAttribute("aria-expanded", isOpen ? "true" : "false");
    };

    const closeNav = () => {
      setExpanded(false);
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKeyDown);
    };

    const onDocClick = (event) => {
      if (!nav.contains(event.target)) {
        closeNav();
      }
    };

    const onKeyDown = (event) => {
      if (event.key === "Escape") {
        closeNav();
      }
    };

    nav.querySelectorAll(".docs-nav-groups a").forEach((link) => {
      link.addEventListener("click", closeNav);
    });

    toggle.addEventListener("click", () => {
      const isOpen = nav.classList.contains("is-open");
      if (isOpen) {
        closeNav();
        return;
      }
      setExpanded(true);
      setTimeout(() => {
        document.addEventListener("click", onDocClick);
        document.addEventListener("keydown", onKeyDown);
      }, 0);
    });
  });
};

const initSearch = () => {
  const root = document.querySelector("[data-docs-search]");
  if (!root) return;

  const input = root.querySelector("[data-docs-search-input]");
  const results = root.querySelector("[data-docs-search-results]");
  const meta = root.querySelector("[data-docs-search-meta]");
  const closeButtons = root.querySelectorAll("[data-docs-search-close]");

  if (!input || !results || !meta) return;

  let indexItems = [];
  let isLoaded = false;
  let isOpen = false;
  let activeIndex = -1;

  const loadIndex = async () => {
    if (isLoaded) return indexItems;

    const loadScript = (src) =>
      new Promise((resolve, reject) => {
        const script = document.createElement("script");
        script.src = src;
        script.async = true;
        script.onload = () => resolve(true);
        script.onerror = () => reject(new Error(`Failed to load ${src}`));
        document.head.appendChild(script);
      });

    let data = window.searchIndex;
    const sources = ["/search_index.en.js", "/search_index.js"];

    if (!data) {
      for (const source of sources) {
        try {
          await loadScript(source);
          if (window.searchIndex) {
            data = window.searchIndex;
            break;
          }
        } catch (err) {
          continue;
        }
      }
    }

    if (!data) {
      meta.textContent = "Search index not available.";
      isLoaded = true;
      indexItems = [];
      return indexItems;
    }

    if (Array.isArray(data)) {
      indexItems = data;
    } else if (Array.isArray(data.items)) {
      indexItems = data.items;
    } else if (Array.isArray(data.documents)) {
      indexItems = data.documents;
    } else if (data.documentStore && data.documentStore.docs) {
      indexItems = Object.values(data.documentStore.docs);
    } else {
      indexItems = [];
    }

    isLoaded = true;
    return indexItems;
  };

  const getItemText = (item) => {
    const title = item.title || "";
    const description = item.description || "";
    const content = item.content || item.body || "";
    return { title, description, content };
  };

  const toPath = (item) => {
    return item.path || item.url || item.permalink || item.id || "";
  };

  const normalizePath = (path) => {
    if (!path) return "";
    if (path.startsWith("http")) {
      try {
        return new URL(path).pathname;
      } catch (err) {
        return path;
      }
    }
    if (!path.startsWith("/")) return `/${path}`;
    return path;
  };

  const navMap = (() => {
    const node = document.querySelector("[data-docs-nav-map]");
    if (!node) return {};
    try {
      const groups = JSON.parse(node.textContent);
      const map = {};

      const toDocsPath = (pagePath) => {
        if (!pagePath) return "";
        const trimmed = pagePath.replace(/\.md$/, "");
        return normalizePath(`${trimmed}/`);
      };

      groups.forEach((group) => {
        const title = group.title || "Docs";
        (group.pages || []).forEach((pagePath) => {
          map[toDocsPath(pagePath)] = title;
        });
      });

      return map;
    } catch (err) {
      return {};
    }
  })();

  const filterDocs = (items) => {
    return items.filter((item) => {
      const path = normalizePath(toPath(item));
      return path.startsWith("/docs/");
    });
  };

  const renderResults = (items, query) => {
    results.innerHTML = "";
    activeIndex = -1;

    if (!query) {
      meta.textContent = "Start typing to search the docs.";
      return;
    }

    if (items.length === 0) {
      meta.textContent = "No results.";
      return;
    }

    meta.textContent = `${items.length} result${items.length === 1 ? "" : "s"}.`;

    items.forEach((item, index) => {
      const { title, description, content } = getItemText(item);
      const path = normalizePath(toPath(item));
      const group = navMap[path] || "Docs";
      const link = document.createElement("a");
      link.href = path;
      link.className = "docs-search-result";
      link.setAttribute("role", "option");
      link.dataset.index = String(index);

      const titleEl = document.createElement("div");
      titleEl.className = "docs-search-result-title";

      const titleText = document.createElement("span");
      titleText.className = "docs-search-result-title-text";
      titleText.textContent = title || path;
      titleEl.appendChild(titleText);

      if (group) {
        const groupText = document.createElement("span");
        groupText.className = "docs-search-result-group";
        groupText.textContent = group;
        titleEl.appendChild(groupText);
      }

      link.appendChild(titleEl);

      const snippetSource = description || content || "";
      const snippet = createSnippet(snippetSource, query);
      if (snippet) {
        const snippetEl = document.createElement("div");
        snippetEl.className = "docs-search-result-snippet";
        snippetEl.textContent = snippet;
        link.appendChild(snippetEl);
      }

      results.appendChild(link);
    });
  };

  const createSnippet = (text, query) => {
    const clean = text.replace(/\s+/g, " ").trim();
    if (!clean) return "";
    const lower = clean.toLowerCase();
    const needle = query.toLowerCase();
    const index = lower.indexOf(needle);
    const length = 140;
    if (index === -1) return clean.slice(0, length) + (clean.length > length ? "…" : "");
    const start = Math.max(0, index - 40);
    const end = Math.min(clean.length, index + needle.length + 80);
    const prefix = start > 0 ? "…" : "";
    const suffix = end < clean.length ? "…" : "";
    return prefix + clean.slice(start, end) + suffix;
  };

  const scoreItem = (item, query) => {
    const { title, description, content } = getItemText(item);
    const lowerQuery = query.toLowerCase();
    const titleIndex = title.toLowerCase().indexOf(lowerQuery);
    const descIndex = description.toLowerCase().indexOf(lowerQuery);
    const contentIndex = content.toLowerCase().indexOf(lowerQuery);

    if (titleIndex !== -1) return titleIndex;
    if (descIndex !== -1) return descIndex + 200;
    if (contentIndex !== -1) return contentIndex + 400;
    return Number.POSITIVE_INFINITY;
  };

  const handleSearch = async () => {
    const query = input.value.trim();
    if (query.length < 2) {
      renderResults([], "");
      return;
    }

    const items = filterDocs(await loadIndex());
    const matches = items
      .map((item) => ({ item, score: scoreItem(item, query) }))
      .filter((entry) => Number.isFinite(entry.score))
      .sort((a, b) => a.score - b.score)
      .slice(0, 10)
      .map((entry) => entry.item);

    renderResults(matches, query);
  };

  const setActiveResult = (nextIndex) => {
    const items = Array.from(results.querySelectorAll(".docs-search-result"));
    if (items.length === 0) return;

    const clamped = (nextIndex + items.length) % items.length;
    activeIndex = clamped;

    items.forEach((item, index) => {
      item.classList.toggle("is-active", index === activeIndex);
    });

    const activeItem = items[activeIndex];
    if (activeItem) {
      activeItem.scrollIntoView({ block: "nearest" });
    }
  };

  const openSearch = () => {
    if (isOpen) return;
    isOpen = true;
    root.classList.add("is-open");
    document.body.classList.add("is-search-open");
    input.focus();
    input.select();
  };

  const closeSearch = () => {
    if (!isOpen) return;
    isOpen = false;
    root.classList.remove("is-open");
    document.body.classList.remove("is-search-open");
    input.value = "";
    renderResults([], "");
  };

  const onKeyDown = (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      openSearch();
      return;
    }

    if (!isOpen) return;

    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveResult(activeIndex + 1);
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveResult(activeIndex - 1);
    }

    if (event.key === "Enter") {
      const active = results.querySelector(".docs-search-result.is-active");
      if (active) {
        active.click();
      }
    }
  };

  input.addEventListener("input", handleSearch);
  closeButtons.forEach((button) => {
    button.addEventListener("click", closeSearch);
  });

  results.addEventListener("mousemove", (event) => {
    const target = event.target.closest(".docs-search-result");
    if (!target) return;
    const index = Number(target.dataset.index || -1);
    if (Number.isFinite(index)) {
      setActiveResult(index);
    }
  });

  document.addEventListener("keydown", onKeyDown);
};

const initSidebarFade = () => {
  const sidebar = document.querySelector(".docs-sidebar");
  if (!sidebar) return;

  const updateFade = () => {
    const maxScroll = sidebar.scrollHeight - sidebar.clientHeight;
    const atTop = sidebar.scrollTop <= 1;
    const atBottom = sidebar.scrollTop >= maxScroll - 1;
    sidebar.classList.toggle("is-scroll-top", !atTop);
    sidebar.classList.toggle("is-scroll-bottom", !atBottom);
  };

  updateFade();
  sidebar.addEventListener("scroll", updateFade, { passive: true });
  window.addEventListener("resize", updateFade);
};

document.addEventListener("DOMContentLoaded", () => {
  initTabs();
  initAccordion();
  initNavToggle();
  initSearch();
  initSidebarFade();
});
