class Kedu < Formula
  desc "Application-level resource monitor for macOS terminals"
  homepage "https://github.com/0x30/Kedu"
  version "0.1.1"
  license "MIT"
  depends_on macos: :sonoma

  # Updated by scripts/update-kedu-formula.rb after each GitHub Release.
  if Hardware::CPU.arm?
    url "https://github.com/0x30/Kedu/releases/download/v#{version}/kedu-#{version}-aarch64-apple-darwin.tar.gz"
    sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  else
    url "https://github.com/0x30/Kedu/releases/download/v#{version}/kedu-#{version}-x86_64-apple-darwin.tar.gz"
    sha256 "1111111111111111111111111111111111111111111111111111111111111111"
  end

  def install
    bin.install "kedu"
  end

  def caveats
    <<~EOS
      Start the background monitor with:
        kedu start

      Open the terminal interface with:
        kedu
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/kedu --version")
  end
end
