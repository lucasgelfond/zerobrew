export default function (eleventyConfig) {
  eleventyConfig.addPassthroughCopy("src/_redirects");

  return {
    dir: {
      input: "src",
      output: "_site",
    },
  };
}
