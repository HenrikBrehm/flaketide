//! Build script — currently no-op. Manpage and shell-completion generation is
//! driven by the `flaketide man` / `flaketide completions` subcommands at runtime
//! to avoid pulling clap_mangen into the build-graph for plain `cargo install`.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
