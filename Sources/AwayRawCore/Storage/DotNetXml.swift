import Foundation

/// A minimal XML tree plus a reader/writer that round-trips the exact shape .NET's
/// `XmlSerializer` produces. The Windows build's RAW_TEMP caches, presets.xml,
/// settings.xml and export.xml must stay readable and writable from both platforms,
/// so this deliberately reproduces .NET's conventions rather than inventing its own:
///
/// * element order follows the C# class's declaration order,
/// * a `null` reference is written by omitting the element entirely,
/// * an empty string is `<Name />`,
/// * booleans are `true` / `false`,
/// * arrays nest per-item elements named after the item type (`<double>`),
/// * numbers use .NET's round-trip form — notably `5200`, never `5200.0`.
public final class XmlNode {
    public var name: String
    public var text: String?
    public var children: [XmlNode] = []
    public var attributes: [(String, String)] = []

    public init(_ name: String, text: String? = nil) {
        self.name = name
        self.text = text
    }

    // ---- building --------------------------------------------------------

    @discardableResult
    public func add(_ child: XmlNode) -> XmlNode { children.append(child); return child }

    @discardableResult
    public func add(_ name: String, _ text: String?) -> XmlNode {
        add(XmlNode(name, text: text))
    }

    public func add(_ name: String, _ value: Double) { add(name, DotNetXml.string(value)) }
    public func add(_ name: String, _ value: Int)    { add(name, String(value)) }
    public func add(_ name: String, _ value: Bool)   { add(name, value ? "true" : "false") }

    /// A .NET `double[]`: `<Name><double>…</double>…</Name>`.
    public func addDoubleArray(_ name: String, _ values: [Double]) {
        let n = add(XmlNode(name))
        for v in values { n.add("double", DotNetXml.string(v)) }
    }

    // ---- reading ---------------------------------------------------------

    public func child(_ name: String) -> XmlNode? { children.first { $0.name == name } }
    public func childrenNamed(_ name: String) -> [XmlNode] { children.filter { $0.name == name } }

    public func string(_ name: String) -> String? { child(name)?.text }
    public func string(_ name: String, default d: String) -> String { child(name)?.text ?? d }

    public func double(_ name: String) -> Double? {
        guard let t = child(name)?.text else { return nil }
        return Double(t)
    }
    public func double(_ name: String, default d: Double) -> Double { double(name) ?? d }

    public func int(_ name: String) -> Int? {
        guard let t = child(name)?.text else { return nil }
        // Tolerate a value written as "3.0" by a future/other writer.
        return Int(t) ?? Double(t).map { Int($0) }
    }
    public func int(_ name: String, default d: Int) -> Int { int(name) ?? d }

    public func bool(_ name: String) -> Bool? {
        guard let t = child(name)?.text?.lowercased() else { return nil }
        if t == "true" || t == "1" { return true }
        if t == "false" || t == "0" { return false }
        return nil
    }
    public func bool(_ name: String, default d: Bool) -> Bool { bool(name) ?? d }

    public func doubleArray(_ name: String, count: Int) -> [Double]? {
        guard let n = child(name) else { return nil }
        let vals = n.children.compactMap { Double($0.text ?? "") }
        return vals.count == count ? vals : nil
    }

    // ---- writing ---------------------------------------------------------

    /// A complete document, matching .NET's declaration and namespace attributes.
    public func documentData() -> Data {
        var s = "<?xml version=\"1.0\"?>\n"
        // XmlSerializer always emits these two on the root element.
        let root = self
        if !root.attributes.contains(where: { $0.0 == "xmlns:xsi" }) {
            root.attributes.append(("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"))
            root.attributes.append(("xmlns:xsd", "http://www.w3.org/2001/XMLSchema"))
        }
        write(into: &s, indent: 0)
        return Data(s.utf8)
    }

    private func write(into s: inout String, indent: Int) {
        let pad = String(repeating: "  ", count: indent)
        s += pad + "<" + name
        for (k, v) in attributes { s += " \(k)=\"\(DotNetXml.escape(v))\"" }

        if children.isEmpty {
            guard let t = text, !t.isEmpty else { s += " />\n"; return }
            s += ">" + DotNetXml.escape(t) + "</" + name + ">\n"
            return
        }
        s += ">\n"
        for c in children { c.write(into: &s, indent: indent + 1) }
        s += pad + "</" + name + ">\n"
    }
}

public enum DotNetXml {

    /// .NET's round-trip double formatting: integral values carry no decimal point
    /// (`5200`, not `5200.0`), everything else uses the shortest representation that
    /// round-trips, with an uppercase exponent.
    public static func string(_ v: Double) -> String {
        if v.isNaN { return "NaN" }
        if v.isInfinite { return v > 0 ? "INF" : "-INF" }
        if v == v.rounded(), abs(v) < 1e15 {
            return String(Int64(v))
        }
        var s = "\(v)"                       // Swift already gives the shortest round-trip
        if let e = s.firstIndex(of: "e") {
            // Swift writes 1e-05; .NET writes 1E-05.
            s.replaceSubrange(e...e, with: "E")
        }
        return s
    }

    public static func escape(_ s: String) -> String {
        var out = ""
        out.reserveCapacity(s.count)
        for c in s {
            switch c {
            case "&": out += "&amp;"
            case "<": out += "&lt;"
            case ">": out += "&gt;"
            case "\"": out += "&quot;"
            default: out.append(c)
            }
        }
        return out
    }

    // ---- parsing ---------------------------------------------------------

    public static func parse(data: Data) -> XmlNode? {
        let d = Delegate()
        let p = XMLParser(data: data)
        p.delegate = d
        guard p.parse() else { return nil }
        return d.root
    }

    public static func parse(contentsOf url: URL) -> XmlNode? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return parse(data: data)
    }

    private final class Delegate: NSObject, XMLParserDelegate {
        var root: XmlNode?
        private var stack: [XmlNode] = []

        func parser(_ parser: XMLParser, didStartElement elementName: String,
                    namespaceURI: String?, qualifiedName qName: String?,
                    attributes attributeDict: [String: String]) {
            let node = XmlNode(elementName)
            node.attributes = attributeDict.map { ($0.key, $0.value) }
            if let top = stack.last { top.children.append(node) } else { root = node }
            stack.append(node)
        }

        func parser(_ parser: XMLParser, foundCharacters string: String) {
            guard let top = stack.last else { return }
            top.text = (top.text ?? "") + string
        }

        func parser(_ parser: XMLParser, didEndElement elementName: String,
                    namespaceURI: String?, qualifiedName qName: String?) {
            if let top = stack.last {
                // Whitespace between child elements is not content.
                if !top.children.isEmpty { top.text = nil }
                else { top.text = top.text?.trimmingCharacters(in: .whitespacesAndNewlines) }
            }
            stack.removeLast()
        }
    }
}
