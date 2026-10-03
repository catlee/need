use std::{fs, path::Path};

use crate::{
    Result,
    model::{BuildCtx, OutputKind, ProjectPath},
};

pub(crate) fn validate_directory_path(output: &ProjectPath) -> Result<()> {
    let path = output.as_str();
    if path == "."
        || path.is_empty()
        || path == ".need"
        || path.starts_with(".need/")
        || Path::new(path).components().any(|component| {
            component
                .as_os_str()
                .as_encoded_bytes()
                .starts_with(b".need-tmp-")
        })
    {
        return Err(format!(
            "unsafe directory output {output}\nhelp: choose a directory beneath the project root outside .need and the reserved .need-tmp-* namespace"
        ));
    }
    ProjectPath::output(path)?;
    Ok(())
}

pub(crate) fn check_output_overlap(
    a: &ProjectPath,
    ak: OutputKind,
    b: &ProjectPath,
    bk: OutputKind,
) -> Result<()> {
    if (ak == OutputKind::Directory || bk == OutputKind::Directory)
        && (Path::new(b.as_str()).starts_with(a.as_str())
            || Path::new(a.as_str()).starts_with(b.as_str()))
    {
        return Err(format!(
            "output ownership overlaps: {a} and {b}\nhelp: give directory output subtrees one owning rule"
        ));
    }
    Ok(())
}

pub(crate) fn has_directory_ownership(c: &BuildCtx) -> bool {
    *c.session.directory_ownership.get_or_init(|| {
        c.project
            .rules
            .iter()
            .any(|rule| rule.kind == OutputKind::Directory)
            || c.session
                .state
                .rules
                .values()
                .any(|rule| rule.kind == OutputKind::Directory)
    })
}

pub(crate) fn register_outputs(
    c: &BuildCtx,
    key: &str,
    outputs: &[ProjectPath],
    kind: OutputKind,
) -> Result<()> {
    if !has_directory_ownership(c) && kind == OutputKind::File {
        return Ok(());
    }
    let mut owners = c
        .session
        .owners
        .lock()
        .map_err(|_| "output ownership lock poisoned".to_string())?;
    for output in outputs {
        if kind == OutputKind::Directory {
            validate_destination(&c.project.root, output)?;
        }
        for rule in c
            .project
            .rules
            .iter()
            .filter(|rule| rule.kind == OutputKind::Directory && rule.pattern)
        {
            let pattern = crate::map::PercentPattern::new(rule.outputs[0].as_str()).unwrap();
            for ancestor in Path::new(output.as_str())
                .ancestors()
                .filter(|path| !path.as_os_str().is_empty())
            {
                let ancestor = ancestor.to_string_lossy();
                if pattern.capture(&ancestor).is_some()
                    && (ancestor.as_ref() != output.as_str()
                        || kind != OutputKind::Directory
                        || key != ancestor.as_ref())
                {
                    return Err(format!(
                        "output {output} overlaps directory pattern owner {ancestor} ({})\nhelp: choose disjoint output subtrees",
                        rule.source
                    ));
                }
            }
        }
        for rule in c.project.rules.iter().filter(|rule| !rule.pattern) {
            if crate::execute::group_key(&rule.outputs) != key {
                for other in &rule.outputs {
                    check_output_overlap(output, kind, other, rule.kind)
                        .map_err(|error| format!("{}: {error}", rule.source))?;
                }
            }
        }
        for (other_key, saved) in &c.session.state.rules {
            if other_key != key {
                for other in saved.outputs.keys().chain(&saved.missing) {
                    check_output_overlap(output, kind, other, saved.kind)?;
                }
            }
        }
        for (other, (other_key, other_kind)) in owners.iter() {
            if other_key != key {
                check_output_overlap(output, kind, other, *other_kind)?;
            }
        }
    }
    for output in outputs {
        owners.insert(output.clone(), (key.to_owned(), kind));
    }
    Ok(())
}

pub(crate) fn validate_destination(root: &Path, output: &ProjectPath) -> Result<()> {
    validate_directory_path(output)?;
    let path = crate::safe_removal_path(root, output)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.is_dir() => Err(format!(
            "directory output {output} is not a directory\nhelp: remove the file or symlink before building"
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "could not inspect directory output {output}: {error}\nhelp: check access to the output"
        )),
    }
}

pub(crate) fn remove_temporary(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "could not inspect {}: {error}\nhelp: check access and retry",
                path.display()
            ));
        }
    }
    .map_err(|error| {
        format!(
            "could not remove {}: {error}\nhelp: check permissions and retry",
            path.display()
        )
    })
}

pub(crate) fn publish(root: &Path, temporary: &ProjectPath, output: &ProjectPath) -> Result<()> {
    validate_destination(root, output)?;
    let staging = root.join(temporary.as_str());
    let destination = root.join(output.as_str());
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        use nix::fcntl::{RenameFlags, renameat2};
        let exists = match fs::symlink_metadata(&destination) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                return Err(format!(
                    "could not inspect directory destination {}: {error}\nhelp: check access to the output parent",
                    destination.display()
                ));
            }
        };
        let flag = if exists {
            RenameFlags::RENAME_EXCHANGE
        } else {
            RenameFlags::RENAME_NOREPLACE
        };
        renameat2(nix::fcntl::AT_FDCWD, &staging, nix::fcntl::AT_FDCWD, &destination, flag)
            .map_err(|error| format!("could not atomically publish directory {output}: {error}\nhelp: use Linux GNU and a filesystem supporting renameat2 exchange/no-replace"))?;
        if exists {
            remove_temporary(&staging)?;
        }
        Ok(())
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        let _ = (staging, destination);
        Err(format!(
            "cannot atomically publish directory {output} on this platform\nhelp: use Linux GNU and a filesystem supporting renameat2 exchange/no-replace"
        ))
    }
}
