//! A minimal XML tree with a reader/writer that round-trips what .NET's `XmlSerializer`
//! produces. Port of the Swift build's `DotNetXml.swift`: RAW_TEMP caches, presets and
//! settings have to stay readable and writable by the Windows, macOS and Rust builds,
//! so this reproduces .NET's conventions instead of inventing its own:
//!
//! * element order follows the C# class's declaration order,
//! * a `null` reference is written by omitting the element,
//! * an empty string is `<Name />`,
//! * booleans are `true` / `false`,
//! * arrays nest per-item elements named after the item type (`<double>`),
//! * numbers use .NET's round-trip form — `5200`, never `5200.0`.
//!
//! The file layout follows the platform's native build so a file the Rust build rewrites
//! without changes is byte-identical to the one the C# (Windows) or Swift (macOS) build
//! wrote: see [`XmlStyle`].

use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmlNode {
    pub name: String,
    pub text: Option<String>,
    pub children: Vec<XmlNode>,
    pub attributes: Vec<(String, String)>,
}

/// How a document is laid out on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlStyle {
    /// The C# build: `encoding="utf-8"` declaration, CRLF, no newline after the root.
    DotNet,
    /// The Swift build: bare declaration, LF, a trailing newline.
    Swift,
}

impl XmlStyle {
    /// What this platform's native build writes.
    pub fn native() -> Self {
        if cfg!(windows) {
            Self::DotNet
        } else {
            Self::Swift
        }
    }
}

impl XmlNode {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), ..Default::default() }
    }

    pub fn with_text(name: &str, text: &str) -> Self {
        Self { name: name.to_string(), text: Some(text.to_string()), ..Default::default() }
    }

    // ---- building --------------------------------------------------------

    pub fn add(&mut self, child: XmlNode) -> &mut XmlNode {
        self.children.push(child);
        self.children.last_mut().unwrap()
    }

    pub fn add_str(&mut self, name: &str, v: &str) {
        self.add(XmlNode::with_text(name, v));
    }

    pub fn add_f64(&mut self, name: &str, v: f64) {
        self.add_str(name, &format_double(v));
    }

    pub fn add_i64(&mut self, name: &str, v: i64) {
        self.add_str(name, &v.to_string());
    }

    pub fn add_bool(&mut self, name: &str, v: bool) {
        self.add_str(name, if v { "true" } else { "false" });
    }

    /// A .NET `double[]`: `<Name><double>…</double>…</Name>`.
    pub fn add_f64_array(&mut self, name: &str, values: &[f64]) {
        let n = self.add(XmlNode::new(name));
        for &v in values {
            n.add_f64("double", v);
        }
    }

    // ---- reading ---------------------------------------------------------

    pub fn child(&self, name: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlNode> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        let c = self.child(name)?;
        // `<Name />` is an empty string, not a missing value.
        Some(c.text.as_deref().unwrap_or(""))
    }

    pub fn string_or(&self, name: &str, d: &str) -> String {
        self.string(name).unwrap_or(d).to_string()
    }

    pub fn f64(&self, name: &str) -> Option<f64> {
        parse_double(self.child(name)?.text.as_deref()?)
    }

    pub fn f64_or(&self, name: &str, d: f64) -> f64 {
        self.f64(name).unwrap_or(d)
    }

    pub fn i64(&self, name: &str) -> Option<i64> {
        let t = self.child(name)?.text.as_deref()?.trim();
        // Tolerate a value written as "3.0" by another writer.
        t.parse::<i64>().ok().or_else(|| parse_double(t).map(|v| v as i64))
    }

    pub fn i64_or(&self, name: &str, d: i64) -> i64 {
        self.i64(name).unwrap_or(d)
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.child(name)?.text.as_deref()?.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }

    pub fn bool_or(&self, name: &str, d: bool) -> bool {
        self.bool(name).unwrap_or(d)
    }

    /// A `double[]` of exactly `count` values.
    pub fn f64_array(&self, name: &str, count: usize) -> Option<Vec<f64>> {
        let n = self.child(name)?;
        let vals: Vec<f64> = n.children.iter().filter_map(|c| parse_double(c.text.as_deref()?)).collect();
        (vals.len() == count).then_some(vals)
    }

    // ---- writing ---------------------------------------------------------

    /// A complete document, with the declaration and the two namespace attributes
    /// `XmlSerializer` always puts on the root.
    pub fn to_document(&self, style: XmlStyle) -> String {
        let mut root = self.clone();
        if !root.attributes.iter().any(|(k, _)| k == "xmlns:xsi") {
            root.attributes.push(("xmlns:xsi".into(), "http://www.w3.org/2001/XMLSchema-instance".into()));
            root.attributes.push(("xmlns:xsd".into(), "http://www.w3.org/2001/XMLSchema".into()));
        }
        let nl = match style {
            XmlStyle::DotNet => "\r\n",
            XmlStyle::Swift => "\n",
        };
        let mut s = String::new();
        s += match style {
            XmlStyle::DotNet => "<?xml version=\"1.0\" encoding=\"utf-8\"?>",
            XmlStyle::Swift => "<?xml version=\"1.0\"?>",
        };
        s += nl;
        root.write(&mut s, 0, style, nl);
        if style == XmlStyle::DotNet {
            // XmlSerializer leaves no newline after the closing root tag.
            s.truncate(s.len() - nl.len());
        }
        s
    }

    fn write(&self, s: &mut String, indent: usize, style: XmlStyle, nl: &str) {
        let pad = "  ".repeat(indent);
        s.push_str(&pad);
        s.push('<');
        s.push_str(&self.name);
        for (k, v) in &self.attributes {
            s.push_str(&format!(" {k}=\"{}\"", escape(v, true)));
        }
        if self.children.is_empty() {
            match self.text.as_deref() {
                Some(t) if !t.is_empty() => {
                    s.push('>');
                    s.push_str(&escape(t, style == XmlStyle::Swift));
                    s.push_str("</");
                    s.push_str(&self.name);
                    s.push('>');
                }
                _ => s.push_str(" />"),
            }
            s.push_str(nl);
            return;
        }
        s.push('>');
        s.push_str(nl);
        for c in &self.children {
            c.write(s, indent + 1, style, nl);
        }
        s.push_str(&pad);
        s.push_str("</");
        s.push_str(&self.name);
        s.push('>');
        s.push_str(nl);
    }
}

/// Text escaping. .NET's writer leaves `"` alone in element text; Swift's escapes it.
fn escape(t: &str, quotes: bool) -> String {
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quotes => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// .NET's round-trip double formatting (`double.ToString("R")` on .NET Core 3+):
/// the shortest digits that read back exactly; plain notation for decimal exponents
/// −4..14 (0.0001 ≤ |v| < 1e15), otherwise `d.dddE+XX` with at least two exponent digits.
pub fn format_double(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "INF".into() } else { "-INF".into() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    // Rust's `{:e}` is already the shortest round-trip form: "-1.2345e-5".
    let sci = format!("{:e}", v);
    let (mantissa, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let (neg, mantissa) = match mantissa.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if !(-4..15).contains(&exp) {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('E');
        out.push(if exp < 0 { '-' } else { '+' });
        out.push_str(&format!("{:02}", exp.abs()));
    } else if exp < 0 {
        out.push_str("0.");
        out.push_str(&"0".repeat((-exp - 1) as usize));
        out.push_str(&digits);
    } else {
        let int_len = exp as usize + 1;
        if digits.len() <= int_len {
            out.push_str(&digits);
            out.push_str(&"0".repeat(int_len - digits.len()));
        } else {
            out.push_str(&digits[..int_len]);
            out.push('.');
            out.push_str(&digits[int_len..]);
        }
    }
    out
}

/// `XmlConvert.ToDouble`: also accepts `INF` / `-INF` / `NaN`.
pub fn parse_double(t: &str) -> Option<f64> {
    match t.trim() {
        "INF" => Some(f64::INFINITY),
        "-INF" => Some(f64::NEG_INFINITY),
        "NaN" => Some(f64::NAN),
        s => s.parse().ok(),
    }
}

/// Parse a document into its root element. None on malformed XML.
pub fn parse(src: &str) -> Option<XmlNode> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut reader = Reader::from_str(src);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;

    fn open(e: &quick_xml::events::BytesStart) -> Option<XmlNode> {
        let mut n = XmlNode::new(e.name().as_ref());
        for a in e.attributes() {
            let a = a.ok()?;
            let k = a.key.as_ref().to_string();
            let v = a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok()?.into_owned();
            n.attributes.push((k, v));
        }
        Some(n)
    }

    fn close(stack: &mut Vec<XmlNode>, root: &mut Option<XmlNode>, mut node: XmlNode) {
        // Whitespace between child elements is not content.
        if !node.children.is_empty() {
            node.text = None;
        } else if let Some(t) = &node.text {
            node.text = Some(t.trim().to_string());
        }
        match stack.last_mut() {
            Some(parent) => parent.children.push(node),
            None => *root = Some(node),
        }
    }

    loop {
        match reader.read_event().ok()? {
            Event::Start(e) => stack.push(open(&e)?),
            Event::Empty(e) => {
                let n = open(&e)?;
                close(&mut stack, &mut root, n);
            }
            Event::End(_) => {
                let n = stack.pop()?;
                close(&mut stack, &mut root, n);
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.get_or_insert_with(String::new).push_str(&t.xml10_content());
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    let s = t.to_string();
                    top.text.get_or_insert_with(String::new).push_str(&s);
                }
            }
            Event::GeneralRef(r) => {
                let ch = if r.is_char_ref() {
                    r.resolve_char_ref().ok()??.to_string()
                } else {
                    match &*r {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => return None,
                    }
                    .to_string()
                };
                if let Some(top) = stack.last_mut() {
                    top.text.get_or_insert_with(String::new).push_str(&ch);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_match_dotnet() {
        let cases = [
            (5200.0, "5200"),
            (0.0, "0"),
            (1.0, "1"),
            (0.4, "0.4"),
            (-30.0, "-30"),
            (6500.111995871754, "6500.111995871754"),
            (-15.857306958617297, "-15.857306958617297"),
            (0.0001, "0.0001"),
            (0.00001, "1E-05"),
            (1.2345e-7, "1.2345E-07"),
            (123456789012345.0, "123456789012345"),
            (1e15, "1E+15"),
            (1.5e300, "1.5E+300"),
            (-0.0020881236996501684, "-0.0020881236996501684"),
        ];
        for (v, want) in cases {
            assert_eq!(format_double(v), want, "{v:e}");
            assert_eq!(parse_double(want), Some(v));
        }
    }

    #[test]
    fn entities_round_trip() {
        let mut root = XmlNode::new("R");
        root.add_str("P", r#"C:\a & b <c> "d""#);
        for style in [XmlStyle::DotNet, XmlStyle::Swift] {
            let doc = root.to_document(style);
            let back = parse(&doc).unwrap();
            assert_eq!(back.string("P"), Some(r#"C:\a & b <c> "d""#));
        }
    }
}
