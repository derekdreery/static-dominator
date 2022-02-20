use anyhow::{bail, format_err, Context, Error};
use ego_tree::{NodeId, NodeRef, Tree};
use scraper::{Html, Node};
use std::{
    collections::HashSet,
    env,
    ffi::OsStr,
    fmt::{self, Write as _},
    fs::{self, File},
    io::{self, Write as _},
    path::{Path, PathBuf},
};

type Result<T = (), E = Error> = std::result::Result<T, E>;

/// Processes all files in `dir` and outputs `<name>.rs.inc` in `var!(OUT_DIR)`.
///
/// Designed to be used in build.rs
pub fn process_dir(in_dir: impl AsRef<Path>, trim: bool) -> Result {
    let in_dir = in_dir.as_ref();
    let out_dir = env::var_os("OUT_DIR").context("are we not in a `build.rs` script?")?;
    for file in fs::read_dir(in_dir).context(format!("reading {}", in_dir.display()))? {
        let file = file?;
        let name_raw = file.file_name();
        let name = Path::new(&name_raw).display();

        // skip directories
        let md = file.metadata()?;
        if md.is_dir() {
            bail!("found directory {}", name);
        }

        // skip non-html files
        match Path::new(&name_raw).extension() {
            Some(s) if s == OsStr::new("html") => (),
            _ => {
                bail!("found non-html file {}", name);
            }
        }

        // get contents
        let contents = match fs::read_to_string(file.path()) {
            Ok(c) => c,
            Err(e) => {
                bail!("error reading {} ({})", name, e);
            }
        };

        // parse contents & skip if invalid
        let parsed = match StaticDom::from_str(&contents, trim) {
            Ok(v) => v,
            Err(e) => {
                bail!("error parsing {}\n{}", name, e);
            }
        };

        // write out result
        let mut name_out = PathBuf::from(name_raw);
        name_out.set_extension("rs.inc");
        let out_path = Path::new(&out_dir).join(&name_out);
        let mut out = io::BufWriter::new(File::create(out_path)?);
        write!(out, "{}", parsed.as_html())?;
    }
    Ok(())
}

pub struct StaticDom {
    inner: Tree<Node>,
    trim: bool,
}

impl StaticDom {
    /// Setting `trim` will trim whitespace from the beginning and end of all text nodes, which is
    /// not the same behavior as html.
    pub fn from_str(input: &str, trim: bool) -> Result<Self> {
        let html = Html::parse_fragment(input);
        if !html.errors.is_empty() {
            let msg = html.errors.join("\n");
            Err(format_err!("{}", msg))
        } else {
            let mut out = StaticDom {
                inner: html.tree,
                trim,
            };
            out.trim_non_el_text();
            out.trim_insignificant();
            Ok(out)
        }
    }

    /// Trim nodes that aren't an element or text
    fn trim_non_el_text(&mut self) {
        // for recursion
        fn inner(node: NodeRef<'_, Node>, ids: &mut HashSet<NodeId>) {
            for child in node.children() {
                match child.value() {
                    Node::Element(_) => inner(child, ids),
                    Node::Text(_) => (),
                    _ => {
                        ids.insert(child.id());
                    }
                }
            }
        }

        let mut ids = HashSet::new();
        inner(self.get_html(), &mut ids);
        for id in ids.iter().copied() {
            if let Some(mut node) = self.inner.get_mut(id) {
                node.detach();
            }
        }
    }

    /// Remove insignificant whitespace
    ///
    /// This function doesn't get it all (that would require deeper analysis).
    fn trim_insignificant(&mut self) {
        fn inner(node: NodeRef<'_, Node>, ids: &mut HashSet<NodeId>) {
            // trim first child if empty string or unsupported node type
            if let Some(child) = node.first_child() {
                match child.value() {
                    Node::Text(txt) => {
                        if txt.trim().is_empty() {
                            ids.insert(child.id());
                        }
                    }
                    Node::Element(_) => (),
                    _ => unreachable!(),
                };
            }
            // trim last child if empty string or unsupported node type
            if let Some(child) = node.last_child() {
                match child.value() {
                    Node::Text(txt) => {
                        if txt.trim().is_empty() {
                            ids.insert(child.id());
                        }
                    }
                    Node::Element(_) => (),
                    _ => unreachable!(),
                };
            }

            for child in node.children() {
                inner(child, ids)
            }
        }

        let mut ids = HashSet::new();
        inner(self.get_html(), &mut ids);
        for id in ids.iter().copied() {
            if let Some(mut node) = self.inner.get_mut(id) {
                node.detach();
            }
        }
    }

    /// Get the <html> node
    fn get_html(&self) -> NodeRef<'_, Node> {
        let root = self.inner.root();
        assert!(matches!(root.value(), Node::Fragment));
        let mut iter = root.children();
        let out = iter.next().unwrap();
        assert!(iter.next().is_none());
        out
    }

    pub fn as_html(&self) -> impl fmt::Display + '_ {
        struct AsHtml<'a>(&'a StaticDom);
        impl fmt::Display for AsHtml<'_> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let len = self
                    .0
                    .get_html()
                    .children()
                    .filter(|t| filter_node_text(t, self.0.trim))
                    .count();
                let mut iter = self
                    .0
                    .get_html()
                    .children()
                    .filter(|t| filter_node_text(t, self.0.trim))
                    .peekable();
                if len > 1 {
                    write!(f, "[")?;
                }
                while let Some(child) = iter.next() {
                    format_node(
                        child,
                        Indenter::new(),
                        iter.peek().is_some(),
                        self.0.trim,
                        f,
                    )?;
                }
                if len > 1 {
                    write!(f, "]")?;
                }
                Ok(())
            }
        }
        AsHtml(self)
    }
}

fn format_node(
    node: NodeRef<'_, Node>,
    indenter: Indenter,
    trail_comma: bool,
    trim: bool,
    f: &mut fmt::Formatter,
) -> fmt::Result {
    let trail_comma = if trail_comma { "," } else { "" };
    match node.value() {
        Node::Element(el) => {
            assert!(el.name.prefix.is_none());

            // escape hatch -> an element called "escape" will have its text contents copied
            // verbatim. You can use this to embed code
            if &*el.name.local == "escape" {
                let text = match node.children().next().unwrap().value() {
                    Node::Text(txt) => txt,
                    _ => panic!("<escape> should contain text"),
                };
                for line in text.lines() {
                    writeln!(f, "{}", line)?;
                }
                return Ok(());
            }

            indenter.writeln(
                f,
                format_args!(
                    "::dominator::html!(\"{}\", {{",
                    el.name.local.escape_debug()
                ),
            )?;

            // class
            match el.classes.len() {
                0 => (),
                1 => {
                    indenter.add().writeln(
                        f,
                        format_args!(
                            ".class(\"{}\")",
                            el.classes.iter().next().unwrap().escape_debug()
                        ),
                    )?;
                }
                _ => {
                    let mut classes = String::new();
                    for class in el.classes.iter() {
                        write!(&mut classes, "\"{}\", ", class.escape_debug())?;
                    }
                    indenter
                        .add()
                        .writeln(f, format_args!(".class([{}])", classes))?;
                }
            }

            // style
            if let Some((_, style)) = el.attrs.iter().find(|(name, _)| &*name.local == "style") {
                let indenter = indenter.add();
                for pair in style.split(';') {
                    let pair = pair.trim();
                    if pair.is_empty() {
                        continue;
                    }
                    let mut pair = pair.splitn(2, ":");
                    let name = pair.next().ok_or(fmt::Error)?;
                    let val = pair.next().ok_or(fmt::Error)?;
                    indenter.writeln(
                        f,
                        format_args!(
                            r#".style("{}", "{}")"#,
                            name.escape_debug(),
                            val.escape_debug()
                        ),
                    )?;
                }
            }

            // other attrs
            for (name, val) in el.attrs.iter().filter(|(name, _)| {
                let name = &*name.local;
                name != "style" && name != "class"
            }) {
                assert!(name.prefix.is_none());
                indenter.add().writeln(
                    f,
                    format_args!(
                        r#".attr("{}", "{}")"#,
                        name.local.escape_debug(),
                        val.escape_debug()
                    ),
                )?;
            }

            let child_count = node.children().count();
            if child_count == 0 {
                // do nothing
            } else if child_count == 1 && !is_escape(node.first_child().unwrap()) {
                let indenter = indenter.add();
                let child = node.children().next().unwrap();
                indenter.writeln(f, format_args!(".child("))?;
                format_node(child, indenter.add(), false, trim, f)?;
                indenter.writeln(f, format_args!(")"))?;
            } else {
                let indenter = indenter.add();
                indenter.writeln(f, format_args!(".children(&mut ["))?;
                let mut child_iter = node
                    .children()
                    .filter(|t| filter_node_text(t, trim))
                    .peekable();
                while let Some(child) = child_iter.next() {
                    let has_more = child_iter.peek().is_some();
                    format_node(child, indenter.add(), has_more, trim, f)?;
                }
                indenter.writeln(f, format_args!("])"))?;
            }
            indenter.writeln(f, format_args!("}}){}", trail_comma))
        }
        Node::Text(txt) => {
            let mut txt = &**txt;
            if trim {
                txt = txt.trim();
            }
            if !txt.is_empty() {
                indenter.writeln(
                    f,
                    format_args!(
                        "::dominator::text(\"{}\"){}",
                        txt.escape_debug(),
                        trail_comma
                    ),
                )
            } else {
                Ok(())
            }
        }
        _ => unreachable!(),
    }
}

#[derive(Copy, Clone)]
struct Indenter {
    indent: u32,
}

impl Indenter {
    fn new() -> Self {
        Indenter { indent: 0 }
    }

    fn add(self) -> Self {
        Indenter {
            indent: self.indent.checked_add(1).unwrap(),
        }
    }

    fn writeln(self, f: &mut fmt::Formatter, args: fmt::Arguments<'_>) -> fmt::Result {
        for _ in 0..self.indent {
            f.write_str("  ")?;
        }
        writeln!(f, "{}", args)
    }
}

fn filter_node_text(el: &NodeRef<'_, Node>, trim: bool) -> bool {
    match el.value() {
        Node::Element(_) => true,
        Node::Text(txt) => {
            if trim {
                !txt.trim().is_empty()
            } else {
                !txt.is_empty()
            }
        }
        other => {
            eprintln!("ignoring unexpected node {:?} in html input", other);
            false
        }
    }
}

fn is_escape(node: NodeRef<'_, Node>) -> bool {
    match node.value() {
        Node::Element(el) => &*el.name.local == "escape",
        _ => false,
    }
}
