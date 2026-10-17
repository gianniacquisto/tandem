import Foundation
import Testing
@testable import TandemCore

/// The dev/production split (`TandemHome`, ADR 0001): only the shipped app may touch
/// the real `~/.tandem`. The rule is tested through `resolve` — which branch a given
/// binary takes is a property of how it was built, not of anything a test can flip at
/// runtime — plus a wiring check that the defaults actually ask `TandemHome`.
@Suite("Tandem home")
struct TandemHomeTests {

    private let home = "/Users/tester"

    private func resolve(_ environment: [String: String] = [:],
                         exe: String = "/Users/tester/projects/tandem/.build/debug/Tandem",
                         debug: Bool = true) -> String {
        TandemHome.resolve(environment: environment,
                           executablePath: exe,
                           home: home,
                           isDebugBuild: debug).path
    }

    @Test("a dev build gets ~/.tandem-dev — a bare release binary too")
    func devBuildIsIsolated() {
        #expect(resolve() == "/Users/tester/.tandem-dev")
        #expect(resolve(debug: false) == "/Users/tester/.tandem-dev")
    }

    @Test("the shipped app — a release build inside a .app — gets the real ~/.tandem")
    func shippedAppGetsRealHome() {
        #expect(resolve(exe: "/Applications/Tandem.app/Contents/MacOS/Tandem", debug: false)
            == "/Users/tester/.tandem")
        // The in-bundle MCP server is the same app: Bundle.main resolves to the enclosing
        // .app, so its executable path still carries the Tandem.app component.
        #expect(resolve(exe: "/Applications/Tandem.app/Contents/MacOS/tandem-mcp", debug: false)
            == "/Users/tester/.tandem")
    }

    @Test("a debug build is never the shipped app, even inside a .app bundle")
    func debugBundleIsStillDev() {
        #expect(resolve(exe: "/Applications/Tandem.app/Contents/MacOS/Tandem", debug: true)
            == "/Users/tester/.tandem-dev")
    }

    @Test("TANDEM_HOME overrides everything, and expands ~")
    func envOverrideWins() {
        #expect(resolve(["TANDEM_HOME": "/tmp/throwaway"]) == "/tmp/throwaway")
        #expect(resolve(["TANDEM_HOME": "/tmp/throwaway"], debug: false) == "/tmp/throwaway")
        #expect(resolve(["TANDEM_HOME": "/tmp/throwaway"],
                        exe: "/Applications/Tandem.app/Contents/MacOS/Tandem",
                        debug: false) == "/tmp/throwaway")
        // An empty value is not an override.
        #expect(resolve(["TANDEM_HOME": ""]) == "/Users/tester/.tandem-dev")
        // Tilde expansion uses the real home, so assert the shape, not the value.
        let expanded = resolve(["TANDEM_HOME": "~/scratch"])
        #expect(!expanded.contains("~"))
        #expect(expanded.hasSuffix("/scratch"))
    }

    @Test("the default Store and Bikeshed take their directory from TandemHome")
    func defaultsFollowTandemHome() {
        #expect(Store().directory == TandemHome.directory)
        #expect(Bikeshed().directory == TandemHome.directory.appendingPathComponent("bikeshed"))
    }
}
