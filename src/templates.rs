/// Wrapper around the ``liquid`` crate to handle templating.
use liquid::ParserBuilder;

use std::{
    fmt::{self, Display, Formatter},
    fs::{create_dir_all, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::error::{Result, ResultExt};
use semver;

pub use liquid::value::{Object as Parameters, Value};

mod external {
    include!(concat!(env!("OUT_DIR"), "/_template_files.rs"));
}

pub enum Template {
    Project,
}

impl Display for Template {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Template::Project => write!(f, "project"),
        }
    }
}

const LIQUID_TEMPLATE_EXTENSION: &str = ".gdpu";

pub fn deploy(
    template: Template,
    version: &Option<String>,
    output: &Path,
    params: &Parameters,
) -> Result<()> {
    let parser = ParserBuilder::with_liquid().build().unwrap();
    let template_map = external::template_files();
    let template_versions = template_map
        .keys()
        .map(|v| semver::Version::parse(v).unwrap());
    let version: String = match version {
        Some(ref ver) => semver::Version::parse(ver)
            .chain_err(|| format!("Could not parse version {}", ver))?
            .to_string(),
        None => template_versions
            .max()
            .ok_or("No template available")?
            .to_string(),
    };

    let mut par = params.clone();
    par.insert("amethyst_version".into(), Value::scalar(version.clone()));
    let params = &par;

    let template_files = template_map
        .get::<str>(&version)
        .ok_or_else(|| format!("No template for version {}", version))?
        .get::<str>(&template.to_string())
        .unwrap();

    for &(path, content) in template_files {
        let mut path = path.to_owned();

        let is_parsed = path.ends_with(LIQUID_TEMPLATE_EXTENSION);
        if is_parsed {
            let len = path.len();
            path.truncate(len - LIQUID_TEMPLATE_EXTENSION.len());
        }

        let mut out = if is_parsed {
            parser
                .parse(content)
                .chain_err(|| format!("Could not parse liquid template at {:?}", path))?
                .render(params)
                .chain_err(|| {
                    format!(
                        "Could not render liquid template at {:?} with parameters {:?}",
                        path, params
                    )
                })?
        } else {
            content.to_owned()
        };

        #[cfg(target_os = "windows")]
        {
            use regex::Regex;

            out = Regex::new("(?P<last>[^\r])\n")
                .unwrap()
                .replace_all(&out, "$last\r\n")
                .to_string();
        }
        #[cfg(not(target_os = "windows"))]
        {
            out = out.replace("\r\n", "\n");
        }

        let remove_path_str = match template {
            Template::Project => "project",
        };
        let path: PathBuf = output
            .join(path)
            .iter()
            .enumerate()
            .filter_map(|(_, e)| if e != remove_path_str { Some(e) } else { None })
            .collect();

        create_dir_all(path.parent().expect("Path has no parent"))?;
        File::create(&path)
            .chain_err(|| format!("failed to create file {:?}", &path))?
            .write_all(out.as_bytes())
            .chain_err(|| format!("could not write contents to file {:?}", &path))?;
    }

    Ok(())
}
