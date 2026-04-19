class Tk < Formula
  desc "Standalone Rust CLI for persistent structured task lists"
  homepage "https://github.com/mkusaka/tk"
  # Release automation replaces these placeholders after the first tagged release.
  url "__SOURCE_URL__"
  version "__VERSION__"
  sha256 "__SOURCE_SHA256__"
  license "MIT"
  head "https://github.com/mkusaka/tk.git", branch: "main"

  bottle do
    root_url "__ROOT_URL__"
    sha256 arm64_tahoe: "__ARM64_TAHOE_SHA256__"
    sha256 tahoe: "__TAHOE_SHA256__"
    sha256 arm64_sequoia: "__ARM64_SEQUOIA_SHA256__"
    sha256 sequoia: "__SEQUOIA_SHA256__"
  end

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "Structured task list CLI", shell_output("#{bin}/tk --help")
  end
end
