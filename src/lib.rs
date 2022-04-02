use anyhow::{Context, Error};
use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{self, Write as _},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

mod html;
pub use html::StaticHtml;
mod markdown;
pub use markdown::StaticMarkdown;

type Result<T = (), E = Error> = std::result::Result<T, E>;

/// Converts html and markdown files to dominator source
///
/// # Warning
///
/// This convertor will escape text on a best-effort basis, but it hasn't been audited and should
/// not be used on untrusted input.
///
/// # Examples
///
/// ```no_run
/// // in build.rs
/// # use dominator_static::Convertor;
/// Convertor::new("<path/to/static_content>").process().unwrap();
/// ```
///
/// ```ignore
/// // In your code do
/// include!(concat!(var!("OUT_DIR"), "<path/to/name>.rs.inc"));
/// // where `<path/to/name>` is the relative path from the `static_content` directory to your
/// // html or md file (without the `.html` or `.md` extension)
/// ```
pub struct Convertor {
    path: PathBuf,
    trim_html: bool,
}

impl Convertor {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
            trim_html: false,
        }
    }

    pub fn trim_html(mut self, yes: bool) -> Self {
        self.trim_html = yes;
        self
    }

    /// Processes all files in `path` and outputs `<name>.rs.inc` in `var!(OUT_DIR)`.
    ///
    /// Designed to be used in build.rs
    pub fn process(self) -> Result {
        let out_dir = env::var_os("OUT_DIR").context("are we not in a `build.rs` script?")?;
        // TODO support symlinks
        for entry in WalkDir::new(&self.path) {
            let entry =
                entry.with_context(|| format!("walking through {}", self.path.display()))?;
            let path_strip = entry
                .path()
                .strip_prefix(&self.path)
                .context("entry in WalkDir not a child of base path")
                .context("internal error, please report as issue")?;
            let mut path_out = Path::new(&out_dir).join(path_strip);
            path_out.set_extension("rs.inc");

            let ft = entry.file_type();
            if ft.is_dir() {
                fs::create_dir(&path_out).with_context(|| {
                    format!("could not create directory {}", path_out.display())
                })?;
            } else if ft.is_file() {
                // get contents
                let contents = fs::read_to_string(entry.path()).with_context(|| {
                    format!("failed to read contents of {}", entry.path().display())
                })?;
                match entry.path().extension() {
                    Some(e) if e == OsStr::new("html") || e == OsStr::new("htm") => {
                        let static_html = StaticHtml::from_str(&contents, self.trim_html)
                            .with_context(|| format!("error parsing {:?}", entry.file_name()))?;
                        let mut out = io::BufWriter::new(File::create(path_out)?);
                        write!(out, "{}", static_html.gen_dominator())?;
                    }
                    Some(e) if e == OsStr::new("md") => {
                        let md_parser = StaticMarkdown::from_str(&contents);
                        let mut out = io::BufWriter::new(File::create(path_out)?);
                        md_parser.generate_dominator(&mut out)?;
                    }
                    o => {
                        eprintln!(
                            "unexpected extension {:?} on {:?}, skipping",
                            o,
                            entry.file_name()
                        );
                        continue;
                    }
                };
            } else if ft.is_symlink() {
                eprintln!(
                    "symlinks unsupported (entry at {}), skipping",
                    entry.path().display()
                );
                continue;
            }
        }
        Ok(())
    }
}
