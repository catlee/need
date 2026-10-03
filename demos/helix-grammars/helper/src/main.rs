use anyhow::{bail, Context, Result};
use std::{fs, io::Write, path::PathBuf, process::Command, time::SystemTime};

#[allow(clippy::unnecessary_map_or, clippy::useless_borrows_in_formatting)]
mod upstream;
use upstream as grammar;

fn runtime_dirs() -> Vec<PathBuf> {
    vec![std::env::current_dir().unwrap().join("work/runtime")]
}

fn configs() -> Result<Vec<grammar::GrammarConfiguration>> {
    #[derive(serde::Deserialize)]
    struct Languages {
        grammar: Vec<grammar::GrammarConfiguration>,
    }
    let selection = fs::read_to_string("selection.txt")?;
    let languages: Languages = toml::from_str(&fs::read_to_string("upstream/languages.toml")?)?;
    let selected: Vec<_> = languages
        .grammar
        .into_iter()
        .filter(|g| selection.lines().any(|name| name == g.grammar_id))
        .collect();
    if selected.len() != selection.lines().count() {
        bail!("selection.txt: unknown or duplicate grammar; help: use pinned Helix grammar names");
    }
    Ok(selected)
}

fn target() -> Result<String> {
    let host = env!("BUILD_TARGET");
    let target = std::env::var("TARGET").unwrap_or_else(|_| host.to_owned());
    if !cfg!(target_os = "linux") || !host.ends_with("-linux-gnu") || target != host {
        bail!("native Linux GNU only: TARGET={target}; help: unset TARGET or use {host}");
    }
    Ok(target)
}

fn trace(event: &str, name: &str, destination: &std::path::Path) {
    if let Ok(path) = std::env::var("HELIX_TRACE") {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        writeln!(
            file,
            "{now} {event} {name} {} {}",
            std::process::id(),
            destination.display()
        )
        .unwrap();
    }
}

fn compile(config: grammar::GrammarConfiguration, output: PathBuf) -> Result<grammar::BuildStatus> {
    let name = config.grammar_id.clone();
    let mut source = runtime_dirs()[0].join("grammars/sources").join(&name);
    match &config.source {
        grammar::GrammarSource::Git { subpath: None, .. } => {}
        grammar::GrammarSource::Git { subpath: Some(path), .. } if path == "." => {}
        _ => bail!("upstream/languages.toml: this subset requires a Git source with root src/; help: restore the pinned grammar configuration"),
    }
    source.push("src");
    let output = std::env::current_dir()?.join(output);
    fs::create_dir_all(output.parent().unwrap())?;
    grammar::build_tree_sitter_library(&source, config, Some(&target()?), &output).context(format!(
        "grammar {name}: compilation failed; help: inspect compiler diagnostics and rerun"
    ))
}

fn identity() -> Result<()> {
    let compiler = grammar::compiler(Some(&target()?));
    println!(
        "path={:?}\nargs={:?}\nenv={:?}",
        compiler.path(),
        compiler.args(),
        compiler.env()
    );
    let resolved = Command::new("which").arg(compiler.path()).output()?;
    if !resolved.status.success() {
        bail!("compiler missing; help: install a native C++ compiler or set CXX");
    }
    let path = String::from_utf8(resolved.stdout)?;
    let hash = Command::new("sha256sum").arg(path.trim()).output()?;
    if !hash.status.success() {
        bail!(
            "compiler hash probe failed: {}; help: check sha256sum and the compiler path",
            String::from_utf8_lossy(&hash.stderr)
        );
    }
    print!("{}", String::from_utf8(hash.stdout)?);
    for flag in ["--version", "-dumpmachine"] {
        let output = compiler.to_command().arg(flag).output()?;
        if !output.status.success() {
            bail!("compiler identity probe failed; help: use GCC or Clang directly in CXX");
        }
        print!(
            "{}{}",
            String::from_utf8(output.stdout)?,
            String::from_utf8(output.stderr)?
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("compile" | "identity")
    ) && std::env::var_os("HELIX_DEMO_INTERNAL").is_none()
    {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(&args)
            .env_clear()
            .env("HELIX_DEMO_INTERNAL", "1");
        for key in [
            "PATH",
            "CC",
            "CXX",
            "CFLAGS",
            "CXXFLAGS",
            "TARGET",
            "HELIX_TRACE",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let status = command.status()?;
        std::process::exit(status.code().unwrap_or(1));
    }
    match args.first().map(String::as_str) {
        Some("fetch") if args.len() == 1 => {
            for config in configs()? {
                grammar::fetch_grammar(config)?;
            }
        }
        Some("compile") if args.len() == 3 => {
            let config = configs()?
                .into_iter()
                .find(|g| g.grammar_id == args[1])
                .context("unknown grammar; help: see selection.txt")?;
            compile(config, PathBuf::from(&args[2]))?;
        }
        Some("identity") if args.len() == 1 => identity()?,
        _ => bail!("usage: helper fetch|identity|compile NAME DEST; help: use just"),
    }
    Ok(())
}
