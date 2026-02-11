require "fileutils"
require "pathname"
require "json"

ZEROBREW_PREFIX = ENV.fetch("ZEROBREW_PREFIX")
ZEROBREW_CELLAR = ENV.fetch("ZEROBREW_CELLAR")
FORMULA_NAME = ENV.fetch("ZEROBREW_FORMULA_NAME")
FORMULA_VERSION = ENV.fetch("ZEROBREW_FORMULA_VERSION")
FORMULA_FILE = ENV.fetch("ZEROBREW_FORMULA_FILE")
INSTALLED_DEPS = JSON.parse(ENV.fetch("ZEROBREW_INSTALLED_DEPS", "{}"))

module OS
  def self.mac?
    RUBY_PLATFORM.include?("darwin")
  end

  def self.linux?
    RUBY_PLATFORM.include?("linux")
  end

  module Mac
    def self.version
      return MacOSVersion.new("15.0") if OS.mac?
      MacOSVersion.new("0")
    end
  end
end

class MacOSVersion
  include Comparable

  def initialize(version)
    @version = version
    @major = version.split(".").first.to_i
  end

  def <=>(other)
    other = MacOSVersion.new(other.to_s) unless other.is_a?(MacOSVersion)
    @version <=> other.to_s
  end

  def to_s = @version
  def to_i = @major
end

module Hardware
  module CPU
    def self.arm?
      RUBY_PLATFORM.include?("arm") || RUBY_PLATFORM.include?("aarch64")
    end

    def self.intel?
      RUBY_PLATFORM.include?("x86_64")
    end

    def self.is_64_bit?
      true
    end
  end
end

module DevelopmentTools
  def self.ld64_version
    return MacOSVersion.new("0") unless OS.mac?
    raw = `xcrun ld -version_details 2>/dev/null`.strip rescue ""
    m = raw.match(/"version"\s*:\s*"([^"]+)"/)
    MacOSVersion.new(m ? m[1] : "0")
  end
end

module Kernel
  def odie(message)
    $stderr.puts "Error: #{message}"
    exit 1
  end
end

class BuildOptions
  def head?  = false
  def stable? = true
  def with?(name) = false
  def without?(name) = true
end

class Pathname
  def install(*sources)
    sources.flatten.each do |src|
      src = Pathname.new(src) unless src.is_a?(Pathname)
      if src.directory?
        dst = self + src.basename
        FileUtils.mkdir_p(dst)
        Dir.children(src.to_s).each { |child| dst.install(src + child) }
      else
        FileUtils.mkdir_p(self)
        FileUtils.cp(src.to_s, self.to_s)
      end
    end
  end

  def write(content)
    FileUtils.mkdir_p(dirname)
    File.write(to_s, content)
  end

  def unlink
    FileUtils.rm_f(to_s)
  end
end

class Formula
  class << self
    attr_accessor :formula_name, :formula_version

    def desc(_) = nil
    def homepage(_) = nil
    def license(_) = nil
    def url(*_args, **_kwargs) = nil
    def sha256(_) = nil
    def revision(_) = nil
    def compatibility_version(_) = nil
    def mirror(_) = nil

    def depends_on(_) = nil
    def uses_from_macos(*_args, **_kwargs) = nil
    def on_macos(&block) = yield if OS.mac?
    def on_linux(&block) = yield if OS.linux?
    def on_intel(&block) = yield if Hardware::CPU.intel?
    def on_arm(&block) = yield if Hardware::CPU.arm?
    def on_system(*_args, **_kwargs, &_block) = nil

    def head(&block) = nil
    def no_autobump!(**_kwargs) = nil
    def livecheck(&block) = nil
    def bottle(&block) = nil
    def skip_clean(*_args) = nil
    def link_overwrite(*_args) = nil
    def keg_only(*_args) = nil
    def caveats = nil
    def test(&block) = nil
    def service(&block) = nil
    def resource(_name, &block) = nil
    def patch(opts = {}, &block) = nil

    def [](name)
      FormulaRef.new(name)
    end

    def inherited(subclass)
      subclass.formula_name = FORMULA_NAME
      subclass.formula_version = FORMULA_VERSION
    end
  end

  def name = self.class.formula_name
  def version = self.class.formula_version
  def build = BuildOptions.new

  def prefix
    Pathname.new(ZEROBREW_CELLAR) + name + version
  end

  def bin       = prefix + "bin"
  def sbin      = prefix + "sbin"
  def lib       = prefix + "lib"
  def libexec   = prefix + "libexec"
  def include   = prefix + "include"
  def share     = prefix + "share"
  def man       = share + "man"
  def man1      = man + "man1"
  def man2      = man + "man2"
  def man3      = man + "man3"
  def man4      = man + "man4"
  def man5      = man + "man5"
  def man6      = man + "man6"
  def man7      = man + "man7"
  def man8      = man + "man8"
  def doc       = share + "doc" + name
  def info      = share + "info"
  def pkgshare  = share + name
  def frameworks = prefix + "Frameworks"
  def kext      = prefix + "Library" + "Extensions"

  def opt_prefix = Pathname.new(ZEROBREW_PREFIX) + "opt" + name

  def etc
    Pathname.new(ZEROBREW_PREFIX) + "etc"
  end

  def var
    Pathname.new(ZEROBREW_PREFIX) + "var"
  end

  def buildpath
    Pathname.new(Dir.pwd)
  end

  def testpath
    buildpath + "test"
  end

  def inreplace(paths, before = nil, after = nil, &block)
    Array(paths).each do |path|
      content = File.read(path.to_s)
      if block_given?
        s = InreplaceString.new(content)
        block.call(s)
        content = s.to_s
      else
        content = content.gsub(before.to_s, after.to_s)
      end
      File.write(path.to_s, content)
    end
  end

  def system(*args)
    cmd = args.map(&:to_s).join(" ")
    puts "==> #{cmd}"
    result = Kernel.system(*args.map(&:to_s))
    exit 1 unless result
  end

  def std_configure_args
    ["--disable-debug", "--disable-dependency-tracking", "--prefix=#{prefix}", "--libdir=#{lib}"]
  end

  def std_cmake_args
    [
      "-DCMAKE_INSTALL_PREFIX=#{prefix}",
      "-DCMAKE_INSTALL_LIBDIR=lib",
      "-DCMAKE_BUILD_TYPE=Release",
      "-DCMAKE_FIND_FRAMEWORK=LAST",
      "-DCMAKE_VERBOSE_MAKEFILE=ON",
      "-DBUILD_TESTING=OFF",
    ]
  end

  def std_meson_args
    ["--prefix=#{prefix}", "--libdir=lib", "--buildtype=release", "--wrap-mode=nofallback"]
  end

  def std_go_args(**overrides)
    ldflags = overrides.fetch(:ldflags, "")
    output = overrides.fetch(:output, bin + name)
    args = ["build", "-trimpath", "-o=#{output}"]
    args << "-ldflags=#{ldflags}" unless ldflags.empty?
    args
  end

  def self.method_missing(method_name, *args, &block)
    nil
  end

  def self.respond_to_missing?(method_name, include_private = false)
    true
  end
end

class FormulaRef
  def initialize(name)
    @name = name.to_s
  end

  def opt_prefix
    dep_info = INSTALLED_DEPS[@name]
    return Pathname.new(ZEROBREW_PREFIX) + "opt" + @name if dep_info
    Pathname.new(ZEROBREW_PREFIX) + "opt" + @name
  end

  def opt_lib     = opt_prefix + "lib"
  def opt_include = opt_prefix + "include"
  def opt_bin     = opt_prefix + "bin"
  def lib         = opt_lib
  def include     = opt_include

  def prefix
    dep_info = INSTALLED_DEPS[@name]
    return Pathname.new(dep_info["cellar_path"]) if dep_info
    opt_prefix
  end

  def any_installed_version = self

  def to_s = @name
end

class InreplaceString
  def initialize(content)
    @content = content
  end

  def gsub!(pattern, replacement)
    @content = @content.gsub(pattern, replacement)
  end

  def sub!(pattern, replacement)
    @content = @content.sub(pattern, replacement)
  end

  def change_make_var!(name, value)
    @content = @content.gsub(/^#{Regexp.escape(name)}\s*[?:]?=.*$/, "#{name}=#{value}")
  end

  def remove_make_var!(name)
    @content = @content.gsub(/^#{Regexp.escape(name)}\s*[?:]?=.*$\n?/, "")
  end

  def to_s = @content
end

module Language
  module Python
    def self.major_minor_version(python)
      raw = `#{python} --version 2>&1`.strip.split.last
      parts = raw.split(".")
      "#{parts[0]}.#{parts[1]}"
    end
  end
end

ENV["HOMEBREW_PREFIX"] = ZEROBREW_PREFIX
ENV["HOMEBREW_CELLAR"] = ZEROBREW_CELLAR

load FORMULA_FILE

formula_class = ObjectSpace.each_object(Class).find { |c| c < Formula && c != Formula }
unless formula_class
  $stderr.puts "Error: no formula class found in #{FORMULA_FILE}"
  exit 1
end

instance = formula_class.new

puts "==> Building #{FORMULA_NAME} #{FORMULA_VERSION}"
FileUtils.mkdir_p(instance.prefix.to_s)
instance.install
puts "==> Build complete: #{FORMULA_NAME} #{FORMULA_VERSION}"
