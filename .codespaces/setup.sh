apt-get update;
apt-get install -y curl git gcc
    
curl https://sh.rustup.rs -sSf | sh -s -- --profile minimal --default-toolchain nightly --component rustfmt,clippy -y
source "$HOME/.cargo/env"

rustup --version
cargo --version
rustc --version