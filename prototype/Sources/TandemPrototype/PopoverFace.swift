import SwiftUI

// PROTOTYPE — the themed menu bar popover (v2 settled direction: a themed
// NSPopover replaces v1's AppKit NSMenu). This is the most-visible brand
// surface, so it wears the theme in all reach variants.
//
// Content: wordmark + bike mark, quick-add, up to 5 open todos (tap the
// line-drawn checkbox to complete), "… n more — open Tandem" truncation,
// "Open Tandem" action, on-theme empty state.

struct PopoverFaceView: View {
    var model: TandemModel
    var palette: Palette
    var onOpenApp: () -> Void
    @State private var draft = ""

    var body: some View {
        VStack(spacing: 10) {
            HStack(spacing: 8) {
                BikeScene(palette: palette, riders: false)
                    .frame(width: 34, height: 22)
                Wordmark(treatment: .lineLettering, palette: palette, height: 15)
                Spacer()
            }

            LigneQuickAdd(text: $draft, palette: palette) { model.add(draft) }

            Rectangle().fill(palette.sand).frame(height: Line.fine)

            if model.openTodos.isEmpty {
                VStack(spacing: 6) {
                    BikeScene(palette: palette, riders: false)
                        .frame(width: 56, height: 56 * 0.65)
                    Text("All clear")
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(palette.inkDim)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 10)
            } else {
                ForEach(model.openTodos.prefix(5)) { todo in
                    Button { model.complete(todo.id) } label: {
                        HStack(spacing: 8) {
                            LigneCheckbox(done: false, palette: palette, size: 18)
                            Text(todo.title)
                                .font(.system(size: 12.5))
                                .foregroundStyle(palette.ink)
                                .lineLimit(1)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 6)
                        .frame(height: 28)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                if model.openTodos.count > 5 {
                    Button(action: onOpenApp) {
                        Text("… \(model.openTodos.count - 5) more — open Tandem")
                            .font(.system(size: 11.5, weight: .medium))
                            .foregroundStyle(palette.emberDeep)
                    }
                    .buttonStyle(.plain)
                    .padding(.top, -2)
                }
            }

            Rectangle().fill(palette.sand).frame(height: Line.fine)

            HStack {
                Spacer()
                CelButton(title: "Open Tandem", palette: palette, action: onOpenApp)
            }
        }
        .padding(12)
        .frame(width: 280)
        .background(palette.paper)
    }
}

/// On-screen mock of the popover (card + flat arrow) for judging in the
/// window. The real NSStatusItem opens the same content in a real popover.
struct PopoverMock: View {
    var model: TandemModel
    var palette: Palette
    var onOpenApp: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            // flat arrow (open path: no base edge), paper-filled, ink-outlined;
            // its base tucks under the card's top border
            PopoverArrow()
                .fill(palette.paper)
                .overlay(PopoverArrow().stroke(palette.ink, lineWidth: Line.standard))
                .frame(width: 22, height: 10)
                .padding(.bottom, -Line.standard - 0.5)

            PopoverFaceView(model: model, palette: palette, onOpenApp: onOpenApp)
                .background(
                    RoundedRectangle(cornerRadius: 10)
                        .fill(palette.shadow)
                        .offset(x: 4, y: 4)
                )
                .background(RoundedRectangle(cornerRadius: 10).fill(palette.paper))
                .overlay(RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(palette.ink, lineWidth: Line.standard))
        }
    }
}

private struct PopoverArrow: Shape {
    func path(in rect: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: rect.minX, y: rect.maxY))
        p.addLine(to: CGPoint(x: rect.midX, y: rect.minY))
        p.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        return p // open: the base sits on the card's border
    }
}
