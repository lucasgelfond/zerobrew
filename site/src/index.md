---
layout: layouts/base.njk
title: Home
templateEngineOverride: njk
home:
  hero:
    brand: "Zerobrew"
    line1Suffix: "is a drop-in replacement"
    line2Prefix: "for Homebrew, achieving"
    line2Highlight: "20x/5x"
    line3: "the speed with the same packages."
  summary:
    why:
      title: "Why zerobrew"
      body: "zerobrew is a drop-in, CLI-first replacement for Homebrew focused on speed, determinism, and a cleaner store layout."
    getStarted:
      title: "Get started"
      body: "Install zerobrew quickly, then jump into docs for commands, configuration, migration, and troubleshooting."
---

{% include "components/home/home-shell.njk" %}
