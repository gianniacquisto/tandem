import SwiftUI

// PROTOTYPE — the themed menu bar popover (v2 redo).
//
// The menu bar item is the most-visible brand surface, so it wears the
// theme in all reach variants — but quietly: brand mark + wordmark,
// quick-add, up to 5 open todos, "… n more" truncation, "Open Tandem".
// One hairline per divider, flat fills, no card outline (the real popover
// provides its own chrome and soft shadow).

struct PopoverFaceView: View {
    var model: TandemModel
    var palette: Palette
    var onOpenApp: () -> Void
    @State private var draft = ""
    @FocusState private var quickAddFocus: Bool

    var body: some View {
        VStack(spacing: 0) {
            // header: bare-bike mark (the double-saddle tell) + wordmark
            HStack(spacing: 8) {
                BikeScene(palette: palette, riders: false, lineWidth: 7)
                    .frame(width: 24, height: 24 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
                Text("Tandem")
                    .font(.custom("AvenirNext-DemiBold", size: 13))
                    .tracking(0.3)
                    .foregroundStyle(palette.ink)
                Spacer()
            }
            .frame(height: 22)

            LigneQuickAdd(text: $draft, palette: palette,
                          onCommit: {
                              withAnimation(Motion.insert) { model.add(draft) }
                              draft = ""
                          },
                          focus: $quickAddFocus,
                          height: Space.popoverControl)
            .padding(.top, 10)

            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.top, 10)

            list
                .padding(.top, 6)

            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.top, 8)

            PrimaryButton(title: "Open Tandem", palette: palette, height: 30,
                          action: onOpenApp)
                .padding(.top, 8)
        }
        .padding(Space.popoverGutter)
        .frame(width: 280)
        .background(palette.paper)
    }

    @ViewBuilder
    private var list: some View {
        if model.openTodos.isEmpty {
            Text("All clear")
                .font(.system(size: 12.5))
                .foregroundStyle(palette.inkDim)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
        } else {
            VStack(spacing: 0) {
                ForEach(model.openTodos.prefix(5)) { todo in
                    PopoverRow(todo: todo, palette: palette,
                               onComplete: { withAnimation(Motion.toggle) { model.complete(todo.id) } })
                        .transition(.opacity)
                }
                if model.openTodos.count > 5 {
                    Button(action: onOpenApp) {
                        Text("… \(model.openTodos.count - 5) more")
                            .font(.system(size: 11.5, weight: .medium))
                            .foregroundStyle(palette.emberDeep)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 6)
                            .frame(height: 26)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}

/// One popover row: checkbox completes (the only action — a plain row
/// click is never destructive), title is plain text.
struct PopoverRow: View {
    var todo: Todo
    var palette: Palette
    var onComplete: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 8) {
            Button(action: onComplete) {
                LigneCheckbox(done: false, palette: palette)
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())

            Text(todo.title)
                .font(.system(size: 12.5))
                .foregroundStyle(palette.ink)
                .lineLimit(1)

            Spacer(minLength: 4)
        }
        .padding(.horizontal, 6)
        .frame(height: Space.popoverRow)
        .background(hovering ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
    }
}

/// On-screen mock of the popover (paper arrow + soft system-style shadow)
/// for judging in the window. The real NSStatusItem opens the same content
/// in a real NSPopover, which supplies the chrome.
struct PopoverMock: View {
    var model: TandemModel
    var palette: Palette
    var onOpenApp: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            PopoverArrow()
                .fill(palette.paper)
                .frame(width: 20, height: 9)
                .padding(.bottom, -1)
            PopoverFaceView(model: model, palette: palette, onOpenApp: onOpenApp)
                .background(RoundedRectangle(cornerRadius: 10).fill(palette.paper))
                .shadow(color: .black.opacity(0.22), radius: 14, y: 5)
        }
        .frame(width: 280)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(palette.paper)
    }
}

private struct PopoverArrow: Shape {
    func path(in rect: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: rect.minX, y: rect.maxY))
        p.addLine(to: CGPoint(x: rect.midX, y: rect.minY))
        p.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        return p
    }
}
