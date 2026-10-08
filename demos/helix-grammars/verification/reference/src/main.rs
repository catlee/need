use anyhow::{bail, Context, Result};

fn parse(library: &str, name: &str, input: &str) -> Result<()> {
    let library = unsafe { libloading::Library::new(library) }?;
    let function: libloading::Symbol<unsafe extern "C" fn() -> tree_sitter::Language> =
        unsafe { library.get(format!("tree_sitter_{name}").as_bytes()) }?;
    let language = unsafe { function() };
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language)?;
    let tree = parser.parse(input, None).context("no parse tree")?;
    if tree.root_node().has_error() {
        bail!("parse contains errors: {}", tree.root_node().to_sexp());
    }
    println!("{}", tree.root_node().to_sexp());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 4 && args[0] == "parse" {
        return parse(&args[1], &args[2], &args[3]);
    }
    helix_loader::grammar::build_grammars(std::env::var("TARGET").ok())
}
