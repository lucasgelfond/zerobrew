import fs from "node:fs";
import path from "node:path";

export default function () {
  const repoRootContributing = path.resolve(process.cwd(), "..", "CONTRIBUTING.md");
  const raw = fs.readFileSync(repoRootContributing, "utf8");

  // Replace the top-level title with an H2 section heading for docs layout.
  return raw.replace(/^#\s+.*$/m, "## Overview");
}
