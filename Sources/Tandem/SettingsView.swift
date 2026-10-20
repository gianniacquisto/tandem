import SwiftUI
import TandemCore

/// Whether the settings sheet is up. A tiny app-lifetime bridge (the
/// `SetupPresenter` pattern) so the "Settings…" menu command can raise the
/// sheet that `MainView` owns. Presentation state only — the flags behind it
/// live in the core (`TandemSettings`).
@Observable
final class SettingsPresenter {
    static let shared = SettingsPresenter()
    var isPresented = false
    private init() {}
}

/// The app's settings surface: one sheet, one section — the bikeshed.
///
/// The only setting it holds is the reveal flag
/// (`TandemSettings.showHiddenFiles`): off by default, because the curated
/// layer is the reading surface and the hidden layer (the capture saddlebag
/// and the consolidation archive) is a layer the human asks to see. Turning it
/// on changes what the tree draws — never what the curated graph is: hidden
/// notes stay out of search, backlinks, and wikilink resolution either way.
struct SettingsView: View {
    var model: BikeshedModel
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.dismiss) private var dismiss

    private var palette: Palette { Palette.forScheme(colorScheme) }

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 16) {
                Text("Settings")
                    .font(.custom("AvenirNext-DemiBold", size: 16))
                    .tracking(0.2)
                    .foregroundStyle(palette.ink)
                VStack(alignment: .leading, spacing: 0) {
                    Text("Bikeshed")
                        .font(.system(size: 11.5, weight: .semibold))
                        .foregroundStyle(palette.inkDim)
                    Rectangle().fill(palette.sand).frame(height: Line.hairline)
                        .padding(.vertical, 10)
                    revealRow
                    Rectangle().fill(palette.sand).frame(height: Line.hairline)
                        .padding(.top, 12)
                    Text("Reveals each project's capture saddlebag (.raw/) and the "
                         + "consolidation archive (.raw/.done/) in the tree. Hidden "
                         + "notes stay out of search, backlinks, and wikilinks, and a "
                         + "capture you delete there is the file deleted.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 10)
                }
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .padding(.horizontal, 24)
            .padding(.top, 24)
            .padding(.bottom, 18)
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            HStack {
                Spacer(minLength: 8)
                TextButton(title: "Done", palette: palette) { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 14)
        }
        .frame(width: 460)
        .background(palette.paper)
    }

    /// The reveal flag: a checkbox row (the app's own line-drawn box) that
    /// persists the choice and redraws the tree live.
    private var revealRow: some View {
        Button {
            model.setShowsHiddenFiles(!model.showsHiddenFiles)
        } label: {
            HStack(alignment: .top, spacing: 10) {
                LigneCheckbox(done: model.showsHiddenFiles, palette: palette, size: 16)
                Text("Show hidden files")
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(palette.ink)
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Reveal the capture queue and the consolidation archive in the tree.")
    }
}
