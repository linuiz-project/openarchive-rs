use clap::Parser;

#[derive(Parser)]
pub enum Arguments {}

pub fn main() {
    let args = Arguments::parse();
}
