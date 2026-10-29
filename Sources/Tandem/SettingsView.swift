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

/// The app's settings surface: one sheet, two sections — the bikeshed and
/// the roster.
///
/// The bikeshed setting is the reveal flag
/// (`TandemSettings.showHiddenFiles`): off by default, because the curated
/// layer is the reading surface and the hidden layer (the capture saddlebag
/// and the consolidation archive) is a layer the human asks to see. Turning it
/// on changes what the tree draws — never what the curated graph is: hidden
/// notes stay out of search, backlinks, and wikilink resolution either way.
///
/// The roster (ticket 4B7CD193) is the name list the ticket sheet's assignee
/// picker offers: add and remove here, and the picker follows. `human` and
/// Unassigned are the picker's own constants, so removing `human` from the
/// roster cannot take it out of the picker.
struct SettingsView: View {
    var model: BikeshedModel
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.dismiss) private var dismiss
    /// The roster as this sheet holds it, seeded from the settings file; every
    /// edit persists at once (the reveal row's write-on-act habit).
    @State private var roster: [String] = TandemSettings.roster()
    /// The name being added.
    @State private var newRole = ""
    /// The agent face's state, read from `SetupInstall.status` and never
    /// derived here; `nil` until the sheet has read it once.
    @State private var agentState: SetupInstall.State?
    @State private var agentError: String?

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
                rosterSection
                agentSection
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
        .onAppear { refreshAgentFace() }
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

    // MARK: - The roster (ticket 4B7CD193)

    /// The roster: one row per name with its remove button, and the add row
    /// under them. Every edit writes the whole list through
    /// `TandemSettings.setRoster` at once — the sheet edits a copy, then
    /// persists it, so a write never carries a half-applied edit.
    private var rosterSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Roster")
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(palette.inkDim)
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.vertical, 10)
            ForEach(roster, id: \.self) { name in
                HStack(spacing: 8) {
                    Text(name)
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(name == TicketQuery.human ? palette.emberDeep
                                                                   : palette.ink)
                    Spacer(minLength: 0)
                    TextButton(title: "Remove", palette: palette) {
                        applyRoster(roster.filter { $0 != name })
                    }
                    .help("Remove \(name) from the assignee picker")
                }
                .padding(.vertical, 4)
                if name != roster.last {
                    Rectangle().fill(palette.sand).frame(height: Line.hairline)
                }
            }
            if roster.isEmpty {
                Text("No names — the assignee picker offers Unassigned and human.")
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
                    .padding(.vertical, 4)
            }
            HStack(spacing: 8) {
                TextField("Add a name…", text: $newRole)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12))
                    .foregroundStyle(palette.ink)
                    .onSubmit(addRole)
                TextButton(title: "Add", palette: palette,
                           enabled: !trimmedRole.isEmpty) { addRole() }
            }
            .padding(.vertical, 8)
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.bottom, 12)
            Text("The names the ticket sheet's assignee picker offers. Unassigned "
                 + "and human are always offered, whatever this list holds.")
                .font(.system(size: 11.5))
                .foregroundStyle(palette.inkDim)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    // MARK: - The agent face (TAN-216)

    /// Bundled version, installed version, each host's state, and one Install
    /// or Repair button. The state is `SetupInstall`'s (`status` and `install`,
    /// the functions behind `tandem-cli setup install`), so Settings adds no
    /// second writer, and nothing is written to a host without the press.
    private var agentSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Agent")
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(palette.inkDim)
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.vertical, 10)
            if let agentState {
                AgentFaceSummary(state: agentState, palette: palette)
            } else {
                Text("Reading the agent face…")
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
            }
            HStack(spacing: 10) {
                TextButton(title: agentState?.hasTrouble == true ? "Repair" : "Install",
                           palette: palette) { installAgentFace() }
                Spacer(minLength: 0)
            }
            .padding(.top, 8)
            if let agentError {
                Text(agentError)
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.emberDeep)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 8)
            }
            Text("Runs `tandem-cli setup install`: the bundled skills, the four "
                 + "agent prompts and AGENTS.md go into Tandem's home, and the "
                 + "host links point at that copy. Nothing is written to a host "
                 + "until you press it.")
                .font(.system(size: 11.5))
                .foregroundStyle(palette.inkDim)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 8)
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.top, 12)
        }
    }

    /// The state read through `SetupInstall.status(home:environment:)`, the same
    /// function `tandem-cli setup status` prints.
    private func refreshAgentFace() {
        agentState = SetupInstall.status(
            home: TandemHome.directory,
            environment: ProcessInfo.processInfo.environment).state
    }

    /// The one write: `SetupInstall.install` at its defaults (OpenCode, no
    /// force), so a checkout link stays and a real file is never touched. The
    /// report's own state replaces the row, so the sheet shows what landed.
    private func installAgentFace() {
        let environment = ProcessInfo.processInfo.environment
        do {
            let report = try SetupInstall.install(
                home: TandemHome.directory,
                environment: environment,
                bundle: SetupInstall.bundledFace(environment: environment),
                force: false,
                hosts: [.opencode])
            agentState = report.state
            agentError = nil
        } catch {
            agentError = (error as? any TandemError)?.message ?? error.localizedDescription
            refreshAgentFace()
        }
    }

    private var trimmedRole: String {
        newRole.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Adds the field's trimmed name — a blank or already-listed name adds
    /// nothing — and clears the field.
    private func addRole() {
        let name = trimmedRole
        guard !name.isEmpty, !roster.contains(name) else { return }
        applyRoster(roster + [name])
        newRole = ""
    }

    /// Puts the edited list in the sheet's copy and persists it. A write that
    /// fails is read as silent here the way `Model.setDefaultLane`'s is: the
    /// next edit rewrites the whole list, so nothing is half-saved.
    private func applyRoster(_ names: [String]) {
        roster = names
        try? TandemSettings.setRoster(names)
    }
}
