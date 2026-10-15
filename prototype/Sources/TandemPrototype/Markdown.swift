import SwiftUI
import AppKit
import Observation

// PROTOTYPE — a small line-based markdown renderer that stands in for the
// live preview (ticket 01 verdict: the real thing is CM6 in a WKWebView).
// Covers the ticket 05 render set cheaply: headings, paragraphs, nested
// lists + interactive tasks, blockquotes, fenced code, tables, inline +
// display math, whole-line images (placeholder art), and wikilinks per
// ticket 03 (resolved → dusty blue, broken → ember dotted; click a broken
// link creates the note). No syntax highlighting in code blocks.

// MARK: - blocks

enum BikeBlock: Identifiable {
    case heading(level: Int, text: String, line: Int)
    case paragraph(text: String, line: Int)
    case image(ref: String, alt: String, line: Int)
    case quote(lines: [String], line: Int)
    case items(items: [BikeItem], line: Int)
    case code(lang: String, lines: [String], line: Int)
    case table(header: [String], rows: [[String]], line: Int)
    case math(lines: [String], line: Int)

    var id: Int {
        switch self {
        case .heading(_, _, let l):  return l
        case .paragraph(_, let l):   return l
        case .image(_, _, let l):    return l
        case .quote(_, let l):       return l
        case .items(_, let l):       return l
        case .code(_, _, let l):     return l
        case .table(_, _, let l):    return l
        case .math(_, let l):        return l
        }
    }
}

struct BikeItem: Identifiable {
    enum Kind { case bullet, number, task }
    let kind: Kind
    var done: Bool?
    var number: Int? = nil
    let text: String
    let indent: Int
    var line: Int
    var id: Int { line }
}

enum BikeBlocks {
    /// Frontmatter (ticket 04): only a *leading, properly terminated* `---`
    /// block counts; everything after the closing `---` is content. Returns
    /// the first content index (0 when there is no frontmatter).
    static func contentStart(_ lines: [String]) -> Int {
        guard lines.first?.trimmingCharacters(in: .whitespaces) == "---" else { return 0 }
        guard let end = lines.dropFirst().firstIndex(where: {
            $0.trimmingCharacters(in: .whitespaces) == "---"
        }) else { return 0 }  // unterminated: the whole thing is content
        return end + 2
    }

    static func parse(_ body: String) -> [BikeBlock] {
        let lines = body.components(separatedBy: "\n")
        var blocks: [BikeBlock] = []
        var para: [String] = []
        var paraStart = 0

        func flushPara() {
            let text = para.joined(separator: " ")
                .trimmingCharacters(in: .whitespaces)
            if !text.isEmpty { blocks.append(.paragraph(text: text, line: paraStart)) }
            para = []
        }

        var i = contentStart(lines)
        while i < lines.count {
            let raw = lines[i]
            let t = raw.trimmingCharacters(in: .whitespaces)
            if t.isEmpty { flushPara(); i += 1; continue }

            // fenced code
            if t.hasPrefix("```") {
                flushPara()
                let start = i
                let lang = String(t.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                var code: [String] = []
                i += 1
                while i < lines.count,
                      !lines[i].trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                    code.append(lines[i]); i += 1
                }
                i += 1  // closing fence (or EOF)
                blocks.append(.code(lang: lang, lines: code, line: start))
                continue
            }

            // display math: $$ … $$ (block form only, ticket 05)
            if t.hasPrefix("$$") && t.hasSuffix("$$"), t.count > 4 {
                flushPara()
                blocks.append(.math(lines: [String(t.dropFirst(2).dropLast(2))], line: i))
                i += 1
                continue
            }
            if t == "$$" {
                flushPara()
                let start = i
                var math: [String] = []
                i += 1
                while i < lines.count, lines[i].trimmingCharacters(in: .whitespaces) != "$$" {
                    math.append(lines[i]); i += 1
                }
                i += 1
                blocks.append(.math(lines: math, line: start))
                continue
            }

            // heading
            if t.hasPrefix("#") {
                flushPara()
                let level = t.prefix { $0 == "#" }.count
                if level <= 6, t.count > level, t.index(t.startIndex, offsetBy: level) != t.endIndex,
                   t[t.index(t.startIndex, offsetBy: level)] == " " {
                    blocks.append(.heading(level: level,
                                           text: String(t[t.index(t.startIndex, offsetBy: level + 1)...]),
                                           line: i))
                    i += 1
                    continue
                }
            }

            // whole-line image (ticket 05: note-relative; missing → placeholder)
            if let img = imageLine(t) {
                flushPara()
                blocks.append(.image(ref: img.ref, alt: img.alt, line: i))
                i += 1
                continue
            }

            // table: a | row followed by a |---| separator
            if t.hasPrefix("|"), i + 1 < lines.count {
                let next = lines[i + 1].trimmingCharacters(in: .whitespaces)
                if next.hasPrefix("|"), next.hasSuffix("|"), next.contains("-"),
                   !next.replacingOccurrences(of: "|", with: "").contains(where: {
                       $0 != "-" && $0 != ":" && !$0.isWhitespace
                   }) {
                    flushPara()
                    let start = i
                    func cells(_ s: String) -> [String] {
                        var t = s.trimmingCharacters(in: .whitespaces)
                        if t.hasPrefix("|") { t.removeFirst() }
                        if t.hasSuffix("|") { t.removeLast() }
                        return t.split(separator: "|", omittingEmptySubsequences: false)
                            .map { $0.trimmingCharacters(in: .whitespaces) }
                    }
                    var header = cells(t)
                    var rows: [[String]] = []
                    i += 2
                    while i < lines.count,
                          lines[i].trimmingCharacters(in: .whitespaces).hasPrefix("|") {
                        rows.append(cells(lines[i])); i += 1
                    }
                    // pad ragged rows to the header width
                    rows = rows.map { r in r + Array(repeating: "", count: max(0, header.count - r.count)) }
                    blocks.append(.table(header: header, rows: rows, line: start))
                    continue
                }
            }

            // blockquote (consecutive > lines)
            if t.hasPrefix(">") {
                flushPara()
                let start = i
                var quote: [String] = []
                while i < lines.count,
                      lines[i].trimmingCharacters(in: .whitespaces).hasPrefix(">") {
                    let l = lines[i].trimmingCharacters(in: .whitespaces)
                    quote.append(String(l.dropFirst().trimmingCharacters(in: .whitespaces)))
                    i += 1
                }
                blocks.append(.quote(lines: quote, line: start))
                continue
            }

            // list items (consecutive; 2-space indent steps)
            if parseItem(raw) != nil {
                flushPara()
                let start = i
                var items: [BikeItem] = []
                while i < lines.count, let it = parseItem(lines[i]) {
                    var it = it
                    it.line = i  // original line — task toggles rewrite this line
                    items.append(it)
                    i += 1
                }
                blocks.append(.items(items: items, line: start))
                continue
            }

            // paragraph
            if para.isEmpty { paraStart = i }
            para.append(t)
            i += 1
        }
        flushPara()
        return blocks
    }

    /// Whole-line image: `![alt](ref)` — the only image form the block parser
    /// treats as a block; the ref must close the line.
    static func imageLine(_ t: String) -> (alt: String, ref: String)? {
        guard t.hasPrefix("!["), let close = t.firstIndex(of: "]") else { return nil }
        let alt = String(t[t.index(after: t.startIndex)..<close])
        let parenOpen = t.index(after: close)
        guard parenOpen < t.endIndex, t[parenOpen] == "(",
              let paren = t.firstIndex(of: ")"),
              paren == t.index(before: t.endIndex) else { return nil }
        return (alt, String(t[t.index(after: parenOpen)..<paren]))
    }

    /// `- [x] task` / `- bullet` / `1. numbered` with 2-space indent steps.
    static func parseItem(_ raw: String) -> BikeItem? {
        let trimmed = raw.drop(while: { $0 == " " })
        let indent = (raw.count - trimmed.count) / 2
        guard let c = trimmed.first else { return nil }
        let rest = trimmed.dropFirst()
        if c == "-" || c == "*" {
            guard rest.first == " " else { return nil }
            let after = rest.dropFirst()
            if after.hasPrefix("[ ] ") {
                return BikeItem(kind: .task, done: false, text: String(after.dropFirst(4)),
                                indent: indent, line: 0)
            }
            if after.hasPrefix("[x] ") {
                return BikeItem(kind: .task, done: true, text: String(after.dropFirst(4)),
                                indent: indent, line: 0)
            }
            return BikeItem(kind: .bullet, done: nil, text: String(after),
                            indent: indent, line: 0)
        }
        if c.isNumber {
            let digits = trimmed.prefix { $0.isNumber }
            guard trimmed[digits.endIndex] == ".",
                  trimmed.count > digits.count + 1,
                  trimmed[trimmed.index(digits.endIndex, offsetBy: 1)] == " " else { return nil }
            let textIdx = trimmed.index(digits.endIndex, offsetBy: 2)
            return BikeItem(kind: .number, done: nil, number: Int(String(digits)),
                            text: String(trimmed[textIdx...]),
                            indent: indent, line: 0)
        }
        return nil
    }
}

// MARK: - inline

enum BikeRunKind: Equatable {
    case plain, code, math, strike
    case wikilink(resolved: Bool)
    case mdlink
}

struct BikeRun {
    var text: String
    var kind: BikeRunKind = .plain
    var bold = false
    var italic = false
    /// Wikilink target (kind == .wikilink) or link target (kind == .mdlink).
    var linkTarget: String?
}

/// Recursive-descent inline parser over ticket 03's wikilink grammar and the
/// GFM floor. Returns runs; `MarkdownView` folds them into an AttributedString.
enum BikeInline {
    static func runs(_ text: String) -> [BikeRun] {
        parse(Substring(text), bold: false, italic: false)
    }

    private static func parse(_ s: Substring, bold: Bool, italic: Bool) -> [BikeRun] {
        var out: [BikeRun] = []
        var s = s
        while !s.isEmpty {
            // Find the earliest special token.
            struct Tok { var at: Substring.Index; var kind: Int; var len: Substring.Index }
            var best: Tok?
            func consider(_ r: Range<Substring.Index>, _ kind: Int) {
                if best == nil || r.lowerBound < best!.at { best = Tok(at: r.lowerBound, kind: kind, len: r.upperBound) }
            }
            for (pat, kind) in [
                (Substring("`"), 0),
                (Substring("[["), 1),
                (Substring("**"), 2),
                (Substring("~~"), 3),
                (Substring("*"), 4),
                (Substring("_"), 5),
                (Substring("$"), 6),
                (Substring("["), 7),
            ] {
                if let r = s.range(of: pat) { consider(r, kind) }
            }
            guard let b = best else {
                out.append(BikeRun(text: String(s), bold: bold, italic: italic))
                break
            }
            if b.at != s.startIndex {
                out.append(BikeRun(text: String(s[..<b.at]), bold: bold, italic: italic))
            }
            s = s[b.at...]
            switch b.kind {
            case 0:  // inline code — no parsing inside
                if let close = s.dropFirst().range(of: "`") {
                    out.append(BikeRun(text: String(s[..<close.upperBound]), kind: .code))
                    s = s[close.upperBound...]
                } else {
                    out.append(BikeRun(text: "`", bold: bold, italic: italic))
                    s = s.dropFirst()
                }
            case 1:  // [[target[#fragment][|alias]]] — first ]] closes (ticket 03)
                if let close = s.dropFirst(2).range(of: "]]") {
                    let inner = String(s.dropFirst(2)[..<close.lowerBound])
                    var target = inner, fragment: String? = nil, alias: String? = nil
                    if let h = target.firstIndex(of: "#") {
                        fragment = String(target[..<h])
                        target = String(target[target.index(after: h)...])
                    }
                    if let p = target.firstIndex(of: "|") {
                        alias = String(target[target.index(after: p)...])
                        target = String(target[..<p])
                    }
                    target = target.trimmingCharacters(in: .whitespaces)
                    guard !target.isEmpty else {
                        out.append(BikeRun(text: "[[\(inner)]]", bold: bold, italic: italic))
                        s = s[close.upperBound...]
                        continue
                    }
                    let display = (alias ?? fragment ?? target)
                        .trimmingCharacters(in: .whitespaces)
                    // `resolved` is a placeholder — attributed(_:pal:resolve:)
                    // settles it against the current vault set.
                    out.append(BikeRun(text: display.isEmpty ? target : display,
                                       kind: .wikilink(resolved: false), linkTarget: target))
                    s = s[close.upperBound...]
                } else {
                    // Unclosed [[ at end of line/file: literal text (ticket 03).
                    out.append(BikeRun(text: String(s), bold: bold, italic: italic))
                    break
                }
            case 2:  // **bold**
                if let close = s.dropFirst(2).range(of: "**") {
                    let inner = s.dropFirst(2)[..<close.lowerBound]
                    out.append(contentsOf: parse(inner, bold: true, italic: italic))
                    s = s[close.upperBound...]
                } else {
                    out.append(BikeRun(text: "**", bold: bold, italic: italic))
                    s = s.dropFirst(2)
                }
            case 3:  // ~~strikethrough~~
                if let close = s.dropFirst(2).range(of: "~~") {
                    let inner = s.dropFirst(2)[..<close.lowerBound]
                    out.append(contentsOf: parse(inner, bold: bold, italic: italic).map {
                        var r = $0; r.kind = r.kind == .plain ? .strike : r.kind; return r
                    })
                    s = s[close.upperBound...]
                } else {
                    out.append(BikeRun(text: "~~", bold: bold, italic: italic))
                    s = s.dropFirst(2)
                }
            case 4, 5:  // *italic* / _italic_
                let marker = s.prefix(1)
                if let close = s.dropFirst().range(of: marker),
                   s.dropFirst().distance(from: s.dropFirst().startIndex, to: close.lowerBound) > 0 {
                    let inner = s.dropFirst()[..<close.lowerBound]
                    out.append(contentsOf: parse(inner, bold: bold, italic: true))
                    s = s[close.upperBound...]
                } else {
                    out.append(BikeRun(text: String(marker), bold: bold, italic: italic))
                    s = s.dropFirst()
                }
            case 6:  // $inline math$ — not $$, non-space edges (ticket 05 trigger rule)
                if s.dropFirst().first != "$", let close = s.dropFirst().range(of: "$") {
                    let inner = s.dropFirst()[..<close.lowerBound]
                    let it = String(inner).trimmingCharacters(in: .whitespaces)
                    if !inner.isEmpty, inner.first != " ", inner.last != " " {
                        out.append(BikeRun(text: it, kind: .math))
                        s = s[close.upperBound...]
                        continue
                    }
                }
                out.append(BikeRun(text: "$", bold: bold, italic: italic))
                s = s.dropFirst()
            case 7:  // [text](target)
                if let close = s.range(of: "]("),
                   let paren = s[close.upperBound...].range(of: ")") {
                    let text = String(s.dropFirst()[..<close.lowerBound])
                    let target = String(s[close.upperBound..<paren.lowerBound])
                    out.append(BikeRun(text: text.isEmpty ? target : text,
                                       kind: .mdlink, linkTarget: target))
                    s = s[paren.upperBound...]
                } else {
                    out.append(BikeRun(text: "[", bold: bold, italic: italic))
                    s = s.dropFirst()
                }
            default:
                break
            }
        }
        return out
    }

    /// Folds runs into an AttributedString in the Ligne register. `resolve`
    /// maps wikilink target → resolves-in-current-vault-set (ticket 03:
    /// derived state — a broken link starts resolving the moment the note
    /// appears).
    static func attributed(_ text: String, pal: Palette,
                           resolve: [String: Bool]) -> AttributedString {
        var out = AttributedString()
        for run in runs(text) {
            var piece = AttributedString(run.text)
            var font = Font.system(size: 13.5)
            var color = pal.ink
            var url: URL?
            switch run.kind {
            case .plain, .strike:
                break
            case .code:
                font = .system(size: 12, design: .monospaced)
                color = pal.ink
                piece.backgroundColor = pal.sand.opacity(0.45)
            case .math:
                font = .system(size: 13, design: .serif).italic()
                color = pal.dustyBlueDeep
            case .wikilink:
                // No underline attribute (the SDK's key trips the compiler);
                // Ligne restraint — color carries the affordance.
                let resolved = run.linkTarget.flatMap { resolve[$0] } ?? false
                color = resolved ? pal.dustyBlue : pal.emberDeep
                if let t = run.linkTarget,
                   let enc = t.addingPercentEncoding(
                        withAllowedCharacters: CharacterSet.alphanumerics.union(
                            CharacterSet(charactersIn: "/-._"))) {
                    url = URL(string: "tandem-wikilink://\(enc)")
                }
            case .mdlink:
                color = pal.dustyBlue
            }
            if run.bold { font = font.weight(.bold) }
            if run.italic { font = font.italic() }
            piece.font = font
            piece.foregroundColor = color
            if let url { piece.link = url }
            if run.kind == .strike { piece.strikethroughStyle = .single }
            out.append(piece)
        }
        return out
    }
}

// MARK: - view

/// The live-preview mock: renders one note's body as blocks in a prose column.
struct MarkdownView: View {
    var note: BikeNote
    var model: BikeModel
    var pal: Palette

    /// Wikilink targets in this note, resolved against the current vault set
    /// (ticket 03: derived state — a broken link starts resolving the moment
    /// the note appears). Recomputed per render; the @Observable model
    /// re-renders the view when the vault changes.
    private var resolveMap: [String: Bool] {
        var m: [String: Bool] = [:]
        for t in BikeModel.wikilinkTargets(in: note.body) {
            let target = String(t.split(whereSeparator: { $0 == "#" || $0 == "|" }).first ?? "")
            guard !target.isEmpty else { continue }
            m[target] = model.resolve(target) != nil
        }
        return m
    }

    var body: some View {
        let blocks = BikeBlocks.parse(note.body)
        let resolve = resolveMap
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(blocks) { block in
                    blockView(block)
                }
            }
            .frame(maxWidth: 720, alignment: .leading)
            .padding(.horizontal, 28)
            .padding(.vertical, 24)
            .frame(maxWidth: .infinity, alignment: .center)
        }
        .background(pal.paper)
        .environment(\.openURL, OpenURLAction { url in
            if url.scheme == "tandem-wikilink" {
                let target = String(url.absoluteString.dropFirst("tandem-wikilink://".count))
                    .removingPercentEncoding ?? ""
                model.followWikilink(target)
                return .handled
            }
            return .systemAction
        })
    }

    @ViewBuilder
    private func blockView(_ block: BikeBlock) -> some View {
        switch block {
        case .heading(let level, let text, _):
            Text(BikeInline.attributed(text, pal: pal, resolve: resolveMap))
                .font(headingFont(level))
                .foregroundStyle(pal.ink)
                .padding(.top, level <= 2 ? 6 : 2)

        case .paragraph(let text, _):
            Text(BikeInline.attributed(text, pal: pal, resolve: resolveMap))
                .font(.system(size: 13.5))
                .foregroundStyle(pal.ink)
                .lineSpacing(3)

        case .image(let ref, let alt, _):
            ImageBlock(ref: ref, alt: alt, note: note, model: model, pal: pal)

        case .quote(let lines, _):
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(lines.enumerated()), id: \.offset) { _, l in
                    Text(BikeInline.attributed(l, pal: pal, resolve: resolveMap))
                        .font(.system(size: 13))
                        .foregroundStyle(pal.ink)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(pal.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(pal.sand, lineWidth: Line.hairline))

        case .items(let items, _):
            VStack(alignment: .leading, spacing: 4) {
                ForEach(items) { item in
                    itemView(item)
                }
            }

        case .code(let lang, let lines, _):
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Spacer()
                    if !lang.isEmpty {
                        Text(lang)
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundStyle(pal.inkDim)
                    }
                }
                Text(lines.joined(separator: "\n"))
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(pal.ink)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding([.horizontal, .bottom], 10)
            }
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(pal.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(pal.sand, lineWidth: Line.hairline))

        case .table(let header, let rows, _):
            VStack(spacing: 0) {
                tableRow(header, bold: true, pal: pal)
                Rectangle().fill(pal.ink).frame(height: Line.hairline)
                ForEach(Array(rows.enumerated()), id: \.offset) { idx, row in
                    tableRow(row, bold: false, pal: pal)
                    if idx != rows.count - 1 {
                        Rectangle().fill(pal.sand).frame(height: Line.hairline)
                    }
                }
            }

        case .math(let lines, _):
            Text(lines.joined(separator: "\n"))
                .font(.system(size: 14, design: .serif).italic())
                .foregroundStyle(pal.dustyBlueDeep)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
        }
    }

    private func headingFont(_ level: Int) -> Font {
        switch level {
        case 1: return .system(size: 20, weight: .bold)
        case 2: return .system(size: 16, weight: .semibold)
        default: return .system(size: 14, weight: .semibold)
        }
    }

    private func tableRow(_ cells: [String], bold: Bool, pal: Palette) -> some View {
        HStack(spacing: 0) {
            ForEach(Array(cells.enumerated()), id: \.offset) { _, c in
                Text(BikeInline.attributed(c, pal: pal, resolve: resolveMap))
                    .font(.system(size: 12.5, weight: bold ? .semibold : .regular))
                    .foregroundStyle(pal.ink)
                    .lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
            }
        }
    }

    private func itemView(_ item: BikeItem) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            switch item.kind {
            case .bullet:
                Circle().fill(pal.dustyBlue).frame(width: 4, height: 4)
                    .padding(.top, 5)
            case .number:
                Text("\(item.number ?? 0).")
                    .font(.system(size: 13.5))
                    .foregroundStyle(pal.inkDim)
                    .monospacedDigit()
            case .task:
                Button {
                    model.toggleTask(notePath: note.path, line: item.line)
                } label: {
                    LigneCheckbox(done: item.done ?? false, palette: pal, size: 14)
                }
                .buttonStyle(.plain)
            }
            Text(BikeInline.attributed(item.text, pal: pal, resolve: resolveMap))
                .font(.system(size: 13.5))
                .foregroundStyle(item.kind == .task && item.done == true ? pal.inkDim : pal.ink)
                .lineSpacing(2)
        }
        .padding(.leading, CGFloat(item.indent) * 18)
    }
}

// MARK: - images (ticket 05: note-relative, missing → placeholder)

struct ImageBlock: View {
    var ref: String
    var alt: String
    var note: BikeNote
    var model: BikeModel
    var pal: Palette

    private var exists: Bool { model.imageExists(ref, in: note.path) }
    private var isRemote: Bool {
        ref.hasPrefix("http://") || ref.hasPrefix("https://")
    }

    var body: some View {
        if exists {
            // The prototype has no real image bytes: a flat Ligne landscape
            // stands in for the rendered photo (the real app serves the file
            // through a tandem: WKURLSchemeHandler).
            VStack(spacing: 0) {
                BikeLandscape(pal: pal)
                    .frame(width: 260, height: 150)
                    .clipShape(RoundedRectangle(cornerRadius: Radius.control))
                    .overlay(RoundedRectangle(cornerRadius: Radius.control)
                        .strokeBorder(pal.sand, lineWidth: Line.hairline))
                Text(ref)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundStyle(pal.inkDim)
                    .padding(.top, 5)
            }
            .frame(maxWidth: .infinity)
        } else {
            HStack(spacing: 8) {
                Image(systemName: "photo")
                    .font(.system(size: 13))
                    .foregroundStyle(pal.emberDeep)
                VStack(alignment: .leading, spacing: 1) {
                    Text(isRemote ? "remote images are never fetched"
                                  : "missing · \(alt.isEmpty ? ref : alt)")
                        .font(.system(size: 11.5, weight: .medium))
                        .foregroundStyle(pal.emberDeep)
                    Text(ref)
                        .font(.system(size: 10.5, design: .monospaced))
                        .foregroundStyle(pal.inkDim)
                }
                Spacer()
            }
            .padding(10)
            .frame(maxWidth: 300, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(pal.ember, style: StrokeStyle(lineWidth: Line.hairline, dash: [3, 3])))
        }
    }
}

/// Flat Ligne landscape stand-in for a rendered local image.
struct BikeLandscape: View {
    var pal: Palette

    var body: some View {
        GeometryReader { geo in
            ZStack {
                pal.sky
                Circle().fill(pal.sun)
                    .frame(width: geo.size.width * 0.16)
                    .position(x: geo.size.width * 0.76, y: geo.size.height * 0.3)
                // far hill
                Path { p in
                    p.move(to: CGPoint(x: 0, y: geo.size.height))
                    p.addLine(to: CGPoint(x: 0, y: geo.size.height * 0.72))
                    p.addQuadCurve(to: CGPoint(x: geo.size.width * 0.55, y: geo.size.height * 0.72),
                                   control: CGPoint(x: geo.size.width * 0.3, y: geo.size.height * 0.42))
                    p.addLine(to: CGPoint(x: geo.size.width, y: geo.size.height))
                    p.closeSubpath()
                }
                .fill(pal.hillFar)
                // near hill
                Path { p in
                    p.move(to: CGPoint(x: geo.size.width * 0.35, y: geo.size.height))
                    p.addQuadCurve(to: CGPoint(x: geo.size.width, y: geo.size.height * 0.8),
                                   control: CGPoint(x: geo.size.width * 0.78, y: geo.size.height * 0.48))
                    p.addLine(to: CGPoint(x: geo.size.width, y: geo.size.height))
                    p.closeSubpath()
                }
                .fill(pal.hill)
                // ground hairline
                Rectangle().fill(pal.sand).frame(height: Line.hairline)
                    .frame(maxHeight: .infinity, alignment: .bottom)
            }
        }
    }
}
