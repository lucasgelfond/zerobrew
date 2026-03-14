export default function (eleventyConfig) {
  eleventyConfig.addPassthroughCopy("src/_redirects");
  eleventyConfig.addPassthroughCopy("src/assets");

  return {
    dir: {
      input: "src",
      output: "_site",
    },
  };
}
