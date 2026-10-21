import SwiftUI
import TandemCore

// The themed menu bar popover (replaces v1's AppKit NSMenu — the menu bar is
// the most-visible brand surface, so it wears the theme).
//
// One hairline per divider, flat fills, no card outline: the real NSPopover
// supplies its own chrome and soft shadow. The content follows the system
// appearance through its colorScheme.

struct PopoverFaceView: View {
    var model: TandemModel
    var onOpenApp: () -> Void
    @Environment(\.colorScheme) private var colorScheme
    @State private var draft = ""
    @FocusState private var quickAddFocus: Bool

    private var palette: Palette { Palette.forScheme(colorScheme) }

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
                // The undo toast, over the list: the popover's checkbox is the
                // same one-click completion the main window's is, so it leaves the
                // same offer behind — one click back from a mis-click.
                .overlay(alignment: .bottom) {
                    Group {
                        if let offer = model.undoOffer {
                            LigneToast(offer: offer, palette: palette) {
                                withAnimation(Motion.toggle) { model.undoCompletion() }
                            }
                            .padding(.bottom, 6)
                            .transition(.move(edge: .bottom).combined(with: .opacity))
                        }
                    }
                    .animation(Motion.toggle, value: model.undoOffer)
                }

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
        if model.openTickets.isEmpty {
            Text("All clear")
                .font(.system(size: 12.5))
                .foregroundStyle(palette.inkDim)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
        } else {
            VStack(spacing: 0) {
                ForEach(model.openTickets.prefix(5)) { ticket in
                    PopoverRow(ticket: ticket, palette: palette,
                               onComplete: { withAnimation(Motion.toggle) { model.complete(ticket.id.uuidString) } })
                        .transition(.opacity)
                }
                if model.openTickets.count > 5 {
                    Button(action: onOpenApp) {
                        Text("… \(model.openTickets.count - 5) more")
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

/// One popover row: the checkbox completes (the only action — a plain row
/// click is never destructive), the title is plain text.
struct PopoverRow: View {
    var ticket: Ticket
    var palette: Palette
    var onComplete: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 8) {
            Button(action: onComplete) {
                LigneCheckbox(done: ticket.done, palette: palette)
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())

            Text(ticket.title)
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
