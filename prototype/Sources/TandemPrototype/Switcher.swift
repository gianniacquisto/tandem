import SwiftUI

// PROTOTYPE — the floating switcher bar (v1 prototype convention).
// Obviously not part of the design being evaluated: high-contrast pill,
// bottom-centre. Cycles:
//   · search style  (tree / bar / panel) — ←/→ cycle this
//   · backlink style (bottom / side)
//   · face (Main/Popover/Assets), reach (A/B/C references),
//     appearance (Auto/Light/Dark), banner demo, wipe/seed the vault.

enum AppearanceMode: Int, CaseIterable, Identifiable {
    case system, light, dark
    var id: Int { rawValue }

    var label: String {
        switch self {
        case .system: "Auto"
        case .light:  "Light"
        case .dark:   "Dark"
        }
    }

    var icon: String {
        switch self {
        case .system: "circle.lefthalf.filled"
        case .light:  "sun.max"
        case .dark:   "moon"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light:  .light
        case .dark:   .dark
        }
    }
}

struct SwitcherBar: View {
    var model: TandemModel
    @Bindable var bike: BikeModel
    @Binding var reach: Reach
    @Binding var face: Face
    @Binding var mode: AppearanceMode
    @Binding var showBanner: Bool
    var onReset: () -> Void

    var body: some View {
        HStack(spacing: 7) {
            arrow("chevron.left") { cycleSearch(-1) }

            ForEach(BikeSearchStyle.allCases, id: \.rawValue) { s in
                chip(s.label, active: s == bike.searchStyle) { bike.searchStyle = s }
            }

            arrow("chevron.right") { cycleSearch(1) }
            divider()

            ForEach(BikeBacklinkStyle.allCases, id: \.rawValue) { b in
                chip(b.label, active: b == bike.backlinkStyle) { bike.backlinkStyle = b }
            }
            divider()

            ForEach(Face.allCases) { f in
                chip(f.rawValue, active: f == face) { face = f }
            }
            divider()

            ForEach(Reach.allCases) { r in
                chip(String(r.short.prefix(1)), active: r == reach) { reach = r }
            }

            chip(mode.label, active: false, icon: mode.icon) {
                mode = AppearanceMode(rawValue: (mode.rawValue + 1) % AppearanceMode.allCases.count)!
            }

            if face == .main {
                chip("banner", active: showBanner) { showBanner.toggle() }
                chip("wipe vault", active: false) {
                    bike.notes = []
                    bike.attachments = []
                    bike.openPath = nil
                    bike.query = ""
                }
            }
            chip("reset data", active: false) { onReset() }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .background(Capsule().fill(Color.black.opacity(0.82)))
        .foregroundStyle(.white)
        .shadow(color: .black.opacity(0.3), radius: 6, y: 2)
    }

    private func cycleSearch(_ delta: Int) {
        let all = BikeSearchStyle.allCases
        let i = (bike.searchStyle.rawValue + delta + all.count) % all.count
        bike.searchStyle = all[i]
    }

    private func arrow(_ icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.system(size: 11, weight: .bold))
                .frame(width: 16, height: 16)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func divider() -> some View {
        Rectangle().fill(.white.opacity(0.25)).frame(width: 1, height: 14)
    }

    private func chip(_ label: String, active: Bool, icon: String? = nil,
                      action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let icon { Image(systemName: icon).font(.system(size: 10)) }
                Text(label).font(.system(size: 11, weight: active ? .bold : .regular))
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(Capsule().fill(active ? .white.opacity(0.28) : .clear))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
    }
}
