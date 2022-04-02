use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use std::{fmt, io};

pub struct StaticMarkdown<'input> {
    parser: Parser<'input, 'input>,
}

impl<'input> StaticMarkdown<'input> {
    pub fn from_str(input: &'input str) -> Self {
        use pulldown_cmark::Options;
        StaticMarkdown {
            parser: Parser::new_ext(input, Options::all()),
        }
    }

    pub fn generate_dominator(self, writer: &'input mut impl io::Write) -> io::Result<()> {
        StaticMarkdownWriter {
            parser: self.parser,
            writer,
            indent: 0,
        }
        .fmt()
    }
}

pub struct StaticMarkdownWriter<'a, W> {
    parser: Parser<'a, 'a>,
    writer: &'a mut W,
    indent: u32,
}

impl<'a, W> StaticMarkdownWriter<'a, W>
where
    W: io::Write,
{
    fn fmt(&mut self) -> io::Result<()> {
        while let Some(event) = self.parser.next() {
            self.fmt_event(event)?;
        }
        Ok(())
    }

    fn fmt_event(&mut self, evt: Event<'a>) -> io::Result<()> {
        match evt {
            Event::Start(tag) => self.fmt_start_event(tag),
            Event::End(tag) => self.fmt_end_event(tag),
            Event::Text(text) => writeln!(
                self.writer,
                "{}.text(\"{}\")",
                indent(self.indent),
                text.escape_debug()
            ),
            Event::Code(text) => {
                writeln!(
                    self.writer,
                    "{}.child(::dominator::html!(\"code\") {{\n}})",
                    indent(self.indent)
                )?;
                writeln!(
                    self.writer,
                    "{}.text(\"{}\")",
                    indent(self.indent + 1),
                    text.escape_debug()
                )?;
                writeln!(self.writer, "{}}})", indent(self.indent))
            }
            Event::Html(_html) => Ok(()),
            Event::FootnoteReference(_tag) => Ok(()),
            Event::SoftBreak => Ok(()),
            Event::HardBreak => Ok(()),
            Event::Rule => write!(self.writer, ".child(::dominator::html!(\"hr\"))"),
            Event::TaskListMarker(yes) => {
                writeln!(self.writer, ".text!(\"{}\")", if yes { "☑" } else { "☐" })
            }
        }
    }

    fn fmt_start_event(&mut self, tag: Tag<'a>) -> io::Result<()> {
        match tag {
            Tag::Paragraph => writeln!(self.writer, ".child(::dominator::html!(\"p\"), {{"),
            Tag::Heading(level, _, _) => {
                writeln!(
                    self.writer,
                    "{}.child(::dominator::html!(\"{}\", {{",
                    indent(self.indent),
                    conv_heading(level)
                )
            }
            // TODO pre isn't really appropriate here
            Tag::BlockQuote => writeln!(self.writer, ".child(::dominator::html!(\"pre\"), {{"),
            Tag::CodeBlock(_kind) => {
                writeln!(
                    self.writer,
                    "{}.child(::dominator::html!(\"code\"), {{",
                    indent(self.indent)
                )
            }
            Tag::List(Some(num)) => {
                writeln!(
                    self.writer,
                    "{}.child(::dominator::html!(\"ul\"), {{",
                    indent(self.indent)
                )?;
                writeln!(
                    self.writer,
                    "{}.attr(\"start\", \"{}\")",
                    indent(self.indent + 1),
                    num
                )
            }
            Tag::List(None) => writeln!(
                self.writer,
                "{}.child(::dominator::html!(\"ul\"), {{",
                indent(self.indent)
            ),
            Tag::Item => writeln!(
                self.writer,
                "{}.child(::dominator::html!(\"li\"), {{",
                indent(self.indent)
            ),
            Tag::FootnoteDefinition(_text) => Ok(()),
            Tag::Table(_alignment) => Ok(()),
            Tag::TableHead => Ok(()),
            Tag::TableRow => Ok(()),
            Tag::TableCell => Ok(()),
            Tag::Emphasis => Ok(()),
            Tag::Strong => Ok(()),
            Tag::Strikethrough => Ok(()),
            Tag::Link(_ty, to, title) => {
                writeln!(
                    self.writer,
                    "{}.child(::dominator::html(\"a\"), {{",
                    indent(self.indent)
                )?;
                writeln!(
                    self.writer,
                    "{}.attr(\"html\", \"{}\"",
                    indent(self.indent + 1),
                    to.escape_debug()
                )?;
                writeln!(
                    self.writer,
                    "{}.attr(\"title\", \"{}\"",
                    indent(self.indent + 1),
                    title.escape_debug()
                )
            }
            Tag::Image(_ty, to, title) => {
                writeln!(
                    self.writer,
                    "{}.child(::dominator::html(\"img\"), {{",
                    indent(self.indent)
                )?;
                writeln!(
                    self.writer,
                    "{}.attr(\"src\", \"{}\"",
                    indent(self.indent + 1),
                    to.escape_debug()
                )?;
                writeln!(
                    self.writer,
                    "{}.attr(\"title\", \"{}\"",
                    indent(self.indent + 1),
                    title.escape_debug()
                )
            }
        }?;
        self.indent += 1;
        Ok(())
    }

    fn fmt_end_event(&mut self, tag: Tag<'a>) -> io::Result<()> {
        self.indent -= 1;
        match tag {
            Tag::Paragraph => writeln!(self.writer, "{}}})", indent(self.indent)),
            Tag::Heading(_, _, _) => writeln!(self.writer, "{}}})", indent(self.indent)),
            Tag::BlockQuote => writeln!(self.writer, "{}}})", indent(self.indent)),
            Tag::CodeBlock(_) => writeln!(self.writer, "{}}})", indent(self.indent)),
            Tag::List(_) => writeln!(self.writer, "{}}})", indent(self.indent)),
            Tag::Item => writeln!(self.writer, "{}}})", indent(self.indent)),
            Tag::FootnoteDefinition(_) => Ok(()),
            Tag::Table(_) => Ok(()),
            Tag::TableHead => Ok(()),
            Tag::TableRow => Ok(()),
            Tag::TableCell => Ok(()),
            Tag::Emphasis => Ok(()),
            Tag::Strong => Ok(()),
            Tag::Strikethrough => Ok(()),
            Tag::Link(_, _, _) => writeln!(self.writer, "{}}})", indent(self.indent)),
            Tag::Image(_, _, _) => Ok(()),
        }
    }
}

fn conv_heading(level: HeadingLevel) -> &'static str {
    use HeadingLevel::*;
    match level {
        H1 => "h1",
        H2 => "h2",
        H3 => "h3",
        H4 => "h4",
        H5 => "h5",
        H6 => "h6",
    }
}

fn indent(amt: u32) -> impl fmt::Display {
    struct Indent(u32);

    impl fmt::Display for Indent {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            for _ in 0..self.0 {
                f.write_str("  ")?;
            }
            Ok(())
        }
    }

    Indent(amt)
}
