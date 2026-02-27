import fs from "node:fs";
import path from "node:path";

const docsRoot = path.join(process.cwd(), "src", "docs");

function humanize(value) {
  return value
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (m) => m.toUpperCase());
}

function parseMeta(filePath) {
  const text = fs.readFileSync(filePath, "utf8");
  const fmMatch = text.match(/^---\n([\s\S]*?)\n---\n/);
  if (!fmMatch) {
    return {
      title: null,
      description: null,
      weight: Number.POSITIVE_INFINITY,
    };
  }

  const fm = fmMatch[1];
  const titleMatch = fm.match(/^title:\s*["']?(.+?)["']?\s*$/m);
  const descriptionMatch = fm.match(/^description:\s*["']?(.+?)["']?\s*$/m);
  const weightMatch = fm.match(/^weight:\s*(\d+)\s*$/m);

  return {
    title: titleMatch ? titleMatch[1] : null,
    description: descriptionMatch ? descriptionMatch[1] : null,
    weight: weightMatch ? Number(weightMatch[1]) : Number.POSITIVE_INFINITY,
  };
}

function collectMdFiles(dirPath) {
  const out = [];
  const entries = fs.readdirSync(dirPath, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name));

  for (const entry of entries) {
    const fullPath = path.join(dirPath, entry.name);
    if (entry.isDirectory()) {
      out.push(...collectMdFiles(fullPath));
      continue;
    }

    if (entry.isFile() && entry.name.endsWith(".md")) {
      out.push(fullPath);
    }
  }

  return out;
}

function toPage(filePath) {
  const rel = path.relative(docsRoot, filePath).replace(/\\/g, "/");
  const relNoExt = rel.replace(/\.md$/, "");
  const parts = relNoExt.split("/");
  const base = parts[parts.length - 1];
  const dir = parts.slice(0, -1).join("/");
  const slug = base === "index" ? dir : relNoExt;
  const href = `/docs/${slug ? `${slug}/` : ""}`;

  const meta = parseMeta(filePath);
  const fallback = base === "index"
    ? humanize(dir.split("/").pop() || "docs")
    : humanize(base);

  return {
    title: meta.title || fallback,
    description: meta.description,
    href,
    weight: meta.weight,
    _isIndex: base === "index",
  };
}

function sortPages(a, b) {
  if (a._isIndex !== b._isIndex) return a._isIndex ? -1 : 1;
  if (a.weight !== b.weight) return a.weight - b.weight;
  return a.title.localeCompare(b.title);
}

export default function () {
  if (!fs.existsSync(docsRoot)) return [];

  const rootEntries = fs.readdirSync(docsRoot, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name));

  const groups = [];
  const rootPages = [];

  for (const entry of rootEntries) {
    const fullPath = path.join(docsRoot, entry.name);

    if (entry.isDirectory()) {
      const pages = collectMdFiles(fullPath)
        .map(toPage)
        .sort(sortPages);

      const indexPage = pages.find((p) => p._isIndex);
      const childPages = pages.filter((p) => !p._isIndex);

      groups.push({
        title: indexPage?.title || humanize(entry.name),
        description: indexPage?.description || null,
        href: indexPage?.href || null,
        weight: indexPage?.weight ?? Number.POSITIVE_INFINITY,
        pages: childPages.map(({ title, href, description }) => ({ title, href, description })),
      });
      continue;
    }

    if (entry.isFile() && entry.name.endsWith(".md") && entry.name !== "index.md") {
      rootPages.push(toPage(fullPath));
    }
  }

  if (rootPages.length > 0) {
    groups.push({
      title: "General",
      description: null,
      href: null,
      weight: Number.POSITIVE_INFINITY,
      pages: rootPages.sort(sortPages).map(({ title, href, description }) => ({ title, href, description })),
    });
  }

  groups.sort((a, b) => {
    if (a.weight !== b.weight) return a.weight - b.weight;
    return a.title.localeCompare(b.title);
  });

  return groups.map(({ title, description, href, pages }) => ({
    title,
    description,
    href,
    pages,
  }));
}
