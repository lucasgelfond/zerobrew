import markdownIt from "markdown-it";

const md = markdownIt({
  html: true,
  linkify: true,
});

export default function (eleventyConfig) {
  eleventyConfig.setLibrary("md", md);
  eleventyConfig.addFilter("markdown", (value = "") => md.render(String(value)).trim());
  eleventyConfig.addFilter("capitalizeFirst", (value = "") => {
    const s = String(value);
    return s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
  });
  eleventyConfig.addPassthroughCopy("src/_redirects");
  eleventyConfig.addPassthroughCopy("src/assets");

  return {
    markdownTemplateEngine: "njk",
    htmlTemplateEngine: "njk",
    dataTemplateEngine: "njk",
    dir: {
      input: "src",
      output: "_site",
    },
  };
}
