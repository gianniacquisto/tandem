import SwiftUI
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
    @State private var error: String?
    /// What the last install reported, drawn under the button. `nil` until the
    /// human presses it: nothing is written to a host without that press.
    @State private var agentReport: SetupInstall.Report?

    private var palette: Palette { Palette.forScheme(colorScheme) }

    /// The in-bundle agent CLI (ADR 0009).
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
                face(name: "Tickets",
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
            Text("Tandem ships the skills and the four agent prompts your host "
                 + "reads. One press installs them and wires OpenCode up, the same "
                 + "`tandem-cli setup install` you can run yourself.")
                .bodyText(palette)
            HStack(spacing: 10) {
                PrimaryButton(title: agentReport == nil ? "Install agent face"
                                                       : "Install again",
                              palette: palette) { installAgentFace() }
                    .frame(width: 160)
                Spacer(minLength: 0)
            }
            if let agentReport {
                AgentFaceSummary(state: agentReport.state, palette: palette)
            }
            if let error {
                LigneBanner(palette: palette,
                            systemImage: "xmark.circle.fill",
                            title: error,
                            onDismiss: { self.error = nil })
                    .fixedSize(horizontal: false, vertical: true)
            }
            Text("Your agent reaches the same list through tandem-cli, at \(cliPath).")
                .font(.system(size: 12))
                .foregroundStyle(palette.inkDim)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var finish_: some View {
        VStack(alignment: .leading, spacing: 14) {
            heading("You're set")
            Text("The list starts empty on purpose — add your first ticket "
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

    /// The Part A install, run in-process: `SetupInstall.install` is the
    /// function behind `tandem-cli setup install`, so the app adds no second
    /// writer (ADR 0034). A throw is the wizard's banner, never a silent pass.
    private func installAgentFace() {
        let environment = ProcessInfo.processInfo.environment
        do {
            agentReport = try SetupInstall.install(
                home: TandemHome.directory,
                environment: environment,
                bundle: SetupInstall.bundledFace(environment: environment),
                force: false,
                hosts: [.opencode])
            error = nil
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

/// A quiet hairline button — the wizard's secondary action (Choose…).
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

// MARK: - the agent face's state, drawn the same on both surfaces (TAN-216)

/// What `SetupInstall` reports, in the app's register: the bundled and
/// installed versions, each host's state, and every warning with the hint that
/// says what to do. The wizard's agent step and Settings' Agent section both
/// draw this view, so the two cannot say different things about one install,
/// and neither re-derives a state of its own.
struct AgentFaceSummary: View {
    var state: SetupInstall.State
    var palette: Palette

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            row("Bundled", state.bundledVersion ?? "none")
            row("Installed", state.installedVersion ?? "not installed")
            ForEach(Array(state.hosts.enumerated()), id: \.offset) { _, host in
                row(host.host, hostState(host))
            }
            ForEach(Array(state.warnings.enumerated()), id: \.offset) { _, warning in
                Text("\(warning.code.rawValue): \(warning.hint)")
                    .font(.system(size: 11.5))
                    .foregroundStyle(warning.severity == .info ? palette.inkDim
                                                              : palette.emberDeep)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 2)
            }
        }
        .padding(11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            Text(label)
                .font(.system(size: 11.5, weight: .medium))
                .foregroundStyle(palette.inkDim)
                .frame(width: 76, alignment: .leading)
            Text(value)
                .font(.system(size: 11.5))
                .foregroundStyle(palette.ink)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
    }

    /// The four states `SetupInstall.state` produces, in words a human reads
    /// without the ADR open.
    private func hostState(_ host: SetupInstall.HostState) -> String {
        switch host.state {
        case "installed":  return "installed, \(host.links) of \(host.expectedLinks) links"
        case "checkout":   return "checkout links, left alone"
        case "real-file":  return "a real file is in the way"
        default:           return "not wired"
        }
    }
}
