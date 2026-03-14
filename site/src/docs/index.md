---
layout: "layouts/docs-page.njk"
title: "Documentation"
description: "Everything you need to install, configure, and use zerobrew."
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Start here

If you're new, read the introduction, then follow Quickstart. Installation covers deeper setup and shell configuration.

## How to navigate

{% call card_group(2) %}
  {% for group in docsNav %}
    {% if group.href %}
      {% call card(group.title, group.href, "") %}
        {{ group.description or "Browse this section." }}
      {% endcall %}
    {% endif %}
  {% endfor %}
{% endcall %}

{% call note() %}
If you are coming from Homebrew, check the migration guide after Quickstart.
{% endcall %}
