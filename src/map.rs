use std::io::Write;

use crate::{Result, model::ParsedDependency, parser::parse_needfile_text};

#[derive(Clone, Debug)]
pub(crate) struct PercentPattern {
    prefix: String,
    suffix: String,
}

impl PercentPattern {
    pub(crate) fn new(pattern: &str) -> Option<Self> {
        let (prefix, suffix) = pattern.split_once('%')?;
        if suffix.contains('%') {
            return None;
        }
        Some(Self {
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
        })
    }

    pub(crate) fn capture<'a>(&self, input: &'a str) -> Option<&'a str> {
        input
            .strip_prefix(&self.prefix)
            .and_then(|rest| rest.strip_suffix(&self.suffix))
            .filter(|_| input.len() >= self.prefix.len() + self.suffix.len())
    }

    pub(crate) fn replace(&self, capture: &str) -> String {
        let mut result =
            String::with_capacity(self.prefix.len() + capture.len() + self.suffix.len());
        result.push_str(&self.prefix);
        result.push_str(capture);
        result.push_str(&self.suffix);
        result
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PatternMap {
    from: PercentPattern,
    to: PercentPattern,
}

impl PatternMap {
    pub(crate) fn from_rule(rule: &str) -> Result<Self> {
        let (variables, mut rules) = parse_needfile_text(std::path::Path::new("<map rule>"), rule)?;
        if !variables.is_empty() || rules.len() != 1 {
            return Err("map rule must contain exactly one rule".into());
        }
        let rule = rules.pop().unwrap();
        if rule.outputs.len() != 1 {
            return Err("map rule must contain exactly one target pattern".into());
        }
        if rule.deps.len() != 1 {
            return Err("map rule must contain exactly one input pattern".into());
        }
        if !rule.recipe.is_empty()
            || rule.options.output.is_some()
            || rule.options.outputs.is_some()
            || rule.options.depfile.is_some()
        {
            return Err("map rule must not contain a recipe or attributes".into());
        }
        let to = &rule.outputs[0];
        let ParsedDependency::File(from) = &rule.deps[0] else {
            return Err("map input must be a file pattern".into());
        };
        let from = PercentPattern::new(from)
            .ok_or("map source pattern must contain exactly one '%'".to_string())?;
        let to = PercentPattern::new(to)
            .ok_or("map destination pattern must contain exactly one '%'".to_string())?;
        Ok(Self { from, to })
    }

    pub(crate) fn apply(&self, input: &str) -> Result<String> {
        let capture = self.from.capture(input).ok_or_else(|| {
            format!(
                "input '{input}' does not match pattern '{}'",
                self.from.as_str()
            )
        })?;
        Ok(self.to.replace(capture))
    }
}

impl PercentPattern {
    fn as_str(&self) -> String {
        format!("{}%{}", self.prefix, self.suffix)
    }
}

pub(crate) fn run(args: Vec<String>) -> Result<()> {
    let mut prefix = args;
    let inputs = if let Some(index) = prefix.iter().position(|arg| arg == "--") {
        let inputs = prefix.split_off(index + 1);
        prefix.pop();
        inputs
    } else {
        Vec::new()
    };
    let nul = crate::cli::take_flag(&mut prefix, "-0");
    if prefix.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "usage: need map [-0] <RULE> <INPUT>...\n\nMap input filenames using a Need pattern rule\n\nArguments:\n  <RULE>      One target pattern and one input pattern, such as 'thumbs/%: %'\n  <INPUT>...  Input filenames to map\n\nOptions:\n  -0          Separate output filenames with NUL bytes\n  -h, --help  Print help"
        );
        return Ok(());
    }
    if prefix.is_empty() || (prefix.len() == 1 && inputs.is_empty()) {
        return Err("map requires <RULE> and at least one <INPUT>".into());
    }
    let rule = prefix.remove(0);
    prefix.extend(inputs);
    let mapped = map_inputs(&rule, &prefix)?;
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    write_mapped(&mut stdout, &mapped, nul)
}

pub(crate) fn map_inputs(rule: &str, inputs: &[String]) -> Result<Vec<String>> {
    let mapper = PatternMap::from_rule(rule)?;
    inputs.iter().map(|input| mapper.apply(input)).collect()
}

fn write_mapped<W: Write>(writer: &mut W, mapped: &[String], nul: bool) -> Result<()> {
    let separator = if nul { b'\0' } else { b'\n' };
    for target in mapped {
        writer
            .write_all(target.as_bytes())
            .and_then(|_| writer.write_all(&[separator]))
            .map_err(|error| format!("failed to write map output: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_prefix_suffix_and_nested_capture() {
        let map = PatternMap::from_rule("thumbs/%.webp: images/%.jpg").unwrap();
        assert_eq!(
            map.apply("images/foo/bar.jpg").unwrap(),
            "thumbs/foo/bar.webp"
        );
    }

    #[test]
    fn maps_empty_capture_and_preserves_order_and_duplicates() {
        let map = PatternMap::from_rule("thumbs/%: %").unwrap();
        let inputs = ["c.jpg", "a.jpg", "a.jpg"];
        let mapped = inputs
            .iter()
            .map(|input| map.apply(input).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(mapped, ["thumbs/c.jpg", "thumbs/a.jpg", "thumbs/a.jpg"]);
        assert_eq!(
            PatternMap::from_rule("out/%: foo%bar")
                .unwrap()
                .apply("foobar")
                .unwrap(),
            "out/"
        );
    }

    #[test]
    fn handles_unicode_spaces_and_literal_paths() {
        let map = PatternMap::from_rule("out/%: src/%").unwrap();
        assert_eq!(
            map.apply("src/café au lait.txt").unwrap(),
            "out/café au lait.txt"
        );
        assert_eq!(
            map.apply("src/../src/foo.txt").unwrap(),
            "out/../src/foo.txt"
        );
    }

    #[test]
    fn rejects_invalid_patterns_and_nonmatches() {
        assert_eq!(
            PatternMap::from_rule("out/%: foo").unwrap_err(),
            "map source pattern must contain exactly one '%'"
        );
        assert_eq!(
            PatternMap::from_rule("out/%: %/%").unwrap_err(),
            "only one % is supported per pattern"
        );
        assert_eq!(
            PatternMap::from_rule("out: %").unwrap_err(),
            "map destination pattern must contain exactly one '%'"
        );
        assert_eq!(
            PatternMap::from_rule("out/%/%: %").unwrap_err(),
            "only one % is supported per pattern"
        );
        assert_eq!(
            PatternMap::from_rule("out/%: %.jpg")
                .unwrap()
                .apply("a.png")
                .unwrap_err(),
            "input 'a.png' does not match pattern '%.jpg'"
        );
    }

    #[test]
    fn accepts_rule_syntax_and_rejects_other_rule_shapes() {
        assert_eq!(
            PatternMap::from_rule("out/%: in/%\n  echo no").unwrap_err(),
            "map rule must not contain a recipe or attributes"
        );
        assert_eq!(
            PatternMap::from_rule("out/% other/%: in/%").unwrap_err(),
            "map rule must contain exactly one target pattern"
        );
        assert_eq!(
            PatternMap::from_rule("out/%: in/% extra/%").unwrap_err(),
            "map rule must contain exactly one input pattern"
        );
        assert_eq!(
            PatternMap::from_rule("out/%: file(in/%)")
                .unwrap()
                .apply("in/foo")
                .unwrap(),
            "out/foo"
        );
    }

    #[test]
    fn output_is_delimited_and_validation_is_atomic() {
        let map = PatternMap::from_rule("out/%: %").unwrap();
        let mapped = ["a", "b"]
            .iter()
            .map(|input| map.apply(input).unwrap())
            .collect::<Vec<_>>();
        let mut output = Vec::new();
        write_mapped(&mut output, &mapped, false).unwrap();
        assert_eq!(output, b"out/a\nout/b\n");

        output.clear();
        write_mapped(&mut output, &mapped, true).unwrap();
        assert_eq!(output, b"out/a\0out/b\0");

        let map = PatternMap::from_rule("out/%: %.jpg").unwrap();
        let result = ["a.jpg", "b.png"]
            .iter()
            .map(|input| map.apply(input))
            .collect::<Result<Vec<_>>>();
        assert!(result.is_err());
    }
}
