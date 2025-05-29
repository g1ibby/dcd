class Dcd < Formula
  desc "Docker Compose Deployment tool for remote servers"
  homepage "https://github.com/g1ibby/dcd"
  license "MIT"

  on_macos do
    on_intel do
      url "https://github.com/g1ibby/dcd/releases/download/v0.2.2/dcd-x86_64-apple-darwin.tar.gz"
      sha256 "3d01bc1779154f3955368c7d228074b0328d9d834c93bcd364859d37a36f65e6"
    end

    on_arm do
      url "https://github.com/g1ibby/dcd/releases/download/v0.2.2/dcd-aarch64-apple-darwin.tar.gz"
      sha256 "2e1262c7977a4cc3b1637d6391e180a22fece626600893b7f51a7c3209db7a5c"
    end
  end

  def install
    bin.install "dcd"
  end

  test do
    assert_match "dcd", shell_output("#{bin}/dcd --version")
  end
end