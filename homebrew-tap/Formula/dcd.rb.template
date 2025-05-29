class Dcd < Formula
  desc "Docker Compose Deployment tool for remote servers"
  homepage "https://github.com/g1ibby/dcd"
  license "MIT"

  on_macos do
    on_intel do
      url "https://github.com/g1ibby/dcd/releases/download/v__VERSION__/dcd-x86_64-apple-darwin.tar.gz"
      sha256 "__SHA256_INTEL__"
    end

    on_arm do
      url "https://github.com/g1ibby/dcd/releases/download/v__VERSION__/dcd-aarch64-apple-darwin.tar.gz"
      sha256 "__SHA256_ARM__"
    end
  end

  def install
    bin.install "dcd"
  end

  test do
    assert_match "dcd", shell_output("#{bin}/dcd --version")
  end
end