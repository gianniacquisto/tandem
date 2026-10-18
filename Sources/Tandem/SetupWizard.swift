import SwiftUI
import AppKit
import TandemCore

/// Whether the setup sheet is up. A tiny app-lifetime bridge (the
/// `WindowManager` pattern) so the "Setup…" menu command can raise the sheet
/// that `MainView` owns, and so first launch can raise it once when
/// `TandemSettings.setupComplete()` is false. Presentation state only — the
/// flag that decides whether to raise it lives in the core (`TandemSettings`).
@Observable
final class SetupPresenter {
    static let shared = SetupPresenter()
    var isPresented = false
    private init() {}
}

/// The first-run setup wizard — app onboarding chrome, in one sheet of four
/// steps: welcome, choose the bikeshed, connect an agent, finish.
///
/// It seeds **no** note content: the bikeshed is deliberately anti-onboarding,
/// and a quiet, empty bikeshed is the correct result. The only things it does are
/// show the app's two faces, run the app's one "Choose Bikeshed…" folder
/// picker (the `NSOpenPanel` path in `TandemApp.swift`, not a second copy), and
/// record `TandemSettings.setupComplete` when the human finishes or skips.
///
/// Thin by construction (spec: the GUI draws, `TandemCore` decides): the bikeshed
/// path and the picker are injected, and every string here is the app's plain,
/// warm register — no marketing.
struct SetupWizardView: View {
    /// The bikeshed path to show (the live choice; the model re-renders this when
    /// a switch lands) and the action that runs the app's folder picker.
    var bikeshedPath: String
    var onChooseBikeshed: () -> Void
    /// Dismiss the sheet. The flag is written here first (Finish/Skip), so a
    /// dismissal that races a crash still re-shows the wizard next launch.
    var onClose: () -> Void

    @Environment(\.colorScheme) private var colorScheme
    @State private var step: Step = .welcome
    @State private var copiedID: String?
    @State private var error: String?

    private var palette: Palette { Palette.forScheme(colorScheme) }

    /// The in-bundle agent CLI (ADR 0009) — the stable path the human copies.
    private let cliPath = "/Applications/Tandem.app/Contents/MacOS/tandem-cli"

    enum Step: Int, CaseIterable, Identifiable {
        case welcome, bikeshed, agent, finish
        var id: Int { rawValue }
    }

    /// `initialStep` defaults to `.welcome` (the app's only entry point); the
    /// parameter exists so a step can be rendered or previewed on its own.
    init(bikeshedPath: String, initialStep: Step = .welcome,
         onChooseBikeshed: @escaping () -> Void, onClose: @escaping () -> Void) {
        self.bikeshedPath = bikeshedPath
        self.onChooseBikeshed = onChooseBikeshed
        self.onClose = onClose
        _step = State(initialValue: initialStep)
    }

    var body: some View {
        VStack(spacing: 0) {
            stepBody
                .frame(maxWidth: .infinity, alignment: .topLeading)
                .padding(.horizontal, 28)
                .padding(.top, 28)
                .padding(.bottom, 22)
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            footer
                .padding(.horizontal, 20)
                .padding(.vertical, 14)
        }
        .frame(width: 560)
        .frame(minHeight: 430, alignment: .top)
        .background(palette.paper)
    }

    // MARK: - Steps

    @ViewBuilder
    private var stepBody: some View {
        switch step {
        case .welcome: welcome
        case .bikeshed:   bikeshed
        case .agent:   agent
        case .finish:  finish_
        }
    }

    private var welcome: some View {
        VStack(alignment: .leading, spacing: 16) {
            bikeMark
            heading("Welcome to Tandem")
            Text("A shared workbench for you and your AI agents.")
                .font(.system(size: 14))
                .foregroundStyle(palette.ink)
            Text("Everything lives on this Mac. No accounts, no network — "
                 + "just a list and a folder your agent can reach too.")
                .bodyText(palette)
            VStack(alignment: .leading, spacing: 0) {
                face(name: "Todos",
                     detail: "One list you and your agents both read and write.")
                Rectangle().fill(palette.sand).frame(height: Line.hairline)
                    .padding(.leading, 96)
                face(name: "Bikeshed",
                     detail: "A plain folder of markdown notes — your wiki of the work.")
            }
        }
    }

    private var bikeshed: some View {
        VStack(alignment: .leading, spacing: 14) {
            heading("Your bikeshed")
            Text("Tandem keeps notes in a plain folder of markdown files. "
                 + "Point it at a new folder, or at one you already keep notes in "
                 + "— it stays a normal folder on disk.")
                .bodyText(palette)
            VStack(alignment: .leading, spacing: 8) {
                Text("Current bikeshed")
                    .font(.system(size: 11.5, weight: .medium))
                    .foregroundStyle(palette.inkDim)
                HStack(spacing: 10) {
                    Text(bikeshedPath)
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundStyle(palette.ink)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    OutlineButton(title: "Choose…", palette: palette,
                                  action: onChooseBikeshed)
                }
                .padding(.horizontal, 11)
                .frame(height: 36)
                .background(RoundedRectangle(cornerRadius: Radius.control)
                    .fill(palette.parchment))
                .overlay(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(palette.line, lineWidth: Line.hairline))
            }
            Text("Switching takes effect right away — the notes you see in the "
                 + "Bikeshed tab are the ones in this folder.")
                .bodyText(palette)
        }
    }

    private var agent: some View {
        VStack(alignment: .leading, spacing: 14) {
            heading("Connect your agent")
            Text("Your agent reads and writes the same list through tandem-cli, "
                 + "the command line Tandem bundles. Copy its path:")
                .bodyText(palette)
            codeField(cliPath, id: "cli")
            Text("Then teach your agent the Tandem skill.")
                .font(.system(size: 13))
                .foregroundStyle(palette.ink)
            Text("Claude Code — link the skill into your personal skills directory:")
                .bodyText(palette)
            codeField("ln -s \"$PWD/skills/tandem\" ~/.claude/skills/tandem",
                      id: "claude")
            Text("OpenCode — add this checkout's skills folder to opencode.json:")
                .bodyText(palette)
            codeField(#"{ "skills": ["/path/to/tandem/skills"] }"#, id: "opencode")
        }
    }

    private var finish_: some View {
        VStack(alignment: .leading, spacing: 14) {
            heading("You're set")
            Text("The list starts empty on purpose — add your first todo "
                 + "whenever you like, or let your agent file one.")
                .bodyText(palette)
            Text("You can reopen this setup later from the File menu.")
                .font(.system(size: 12))
                .foregroundStyle(palette.inkDim)
            if let error {
                LigneBanner(palette: palette,
                            systemImage: "xmark.circle.fill",
                            title: error,
                            onDismiss: { self.error = nil })
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: 12) {
            if step != .welcome {
                TextButton(title: "Back", palette: palette) { back() }
            }
            TextButton(title: "Skip setup", palette: palette) { complete() }
                .help("Close the wizard without finishing.")
            Spacer(minLength: 8)
            if step == .finish {
                PrimaryButton(title: "Finish", palette: palette) { complete() }
                    .frame(width: 120)
            } else {
                PrimaryButton(title: "Continue", palette: palette) { forward() }
                    .frame(width: 120)
            }
        }
    }

    // MARK: - Actions

    private func forward() {
        guard let next = Step(rawValue: step.rawValue + 1) else { return }
        withAnimation(Motion.toggle) { step = next }
    }

    private func back() {
        guard let prev = Step(rawValue: step.rawValue - 1) else { return }
        withAnimation(Motion.toggle) { step = prev }
    }

    /// Finish or Skip: record the flag, then dismiss. The write happens first
    /// and only ever sets `true`, so the bikeshed choice (applied when the human
    /// picked a folder) is already on disk and a failed write re-shows the
    /// wizard next launch rather than trapping the human now.
    private func complete() {
        do {
            try TandemSettings.setSetupComplete(true)
            onClose()
        } catch {
            self.error = (error as? any TandemError)?.message ?? error.localizedDescription
        }
    }

    // MARK: - Pieces

    private var bikeMark: some View {
        Group {
            if let photo = BrandAsset.bikePhoto {
                photo
                    .resizable()
                    .scaledToFill()
                    .frame(width: 150, height: 150 * BrandAsset.bikePhotoAspect)
                    .clipShape(RoundedRectangle(cornerRadius: Radius.control,
                                                style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: Radius.control,
                                              style: .continuous)
                        .strokeBorder(palette.sand, lineWidth: Line.hairline))
            } else {
                BikeScene(palette: palette, riders: true, ground: true)
                    .frame(width: 150, height: 150 * BikeArt.sceneSize.height
                                                 / BikeArt.sceneSize.width)
            }
        }
        .padding(.bottom, 2)
    }

    private func heading(_ text: String) -> some View {
        Text(text)
            .font(.custom("AvenirNext-DemiBold", size: 20))
            .tracking(0.2)
            .foregroundStyle(palette.ink)
    }

    /// One of the app's two faces, for the welcome step.
    private func face(name: String, detail: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            Text(name)
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(palette.ink)
                .frame(width: 96, alignment: .leading)
            Text(detail)
                .font(.system(size: 12.5))
                .foregroundStyle(palette.inkDim)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 10)
    }

    /// A paste-ready, selectable line with a Copy button (which flips to
    /// "Copied" for a moment so the click has a visible answer).
    private func codeField(_ text: String, id: String) -> some View {
        HStack(spacing: 10) {
            Text(text)
                .font(.system(size: 11.5, design: .monospaced))
                .foregroundStyle(palette.ink)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            OutlineButton(title: copiedID == id ? "Copied" : "Copy",
                          palette: palette) { copy(text, id: id) }
        }
        .padding(.horizontal, 11)
        .frame(height: 36)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }

    private func copy(_ text: String, id: String) {
        let board = NSPasteboard.general
        board.clearContents()
        board.setString(text, forType: .string)
        copiedID = id
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
            if copiedID == id { copiedID = nil }
        }
    }
}

// MARK: - shared pieces (Ligne register; no literals outside the theme)

private extension Text {
    /// The wizard's secondary body run.
    func bodyText(_ palette: Palette) -> some View {
        self.font(.system(size: 13))
            .foregroundStyle(palette.inkDim)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// A quiet hairline button — the wizard's secondary action (Choose…, Copy).
/// `TextButton` is text-only; this one carries the control box.
private struct OutlineButton: View {
    var title: String
    var palette: Palette
    var action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12.5))
                .lineLimit(1)
                .fixedSize()
                .foregroundStyle(hovering ? palette.ink : palette.inkDim)
                .padding(.horizontal, 12)
                .frame(height: 26)
                .background(RoundedRectangle(cornerRadius: Radius.control)
                    .fill(hovering ? palette.paper : palette.parchment))
                .overlay(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(palette.line, lineWidth: Line.hairline))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}
