import Cocoa

// The delegate is stored in a static so it outlives `app.run()`. NSApplication
// holds its delegate weakly; if the compiler decides the local `delegate`
// binding isn't needed past `app.delegate = ...` it can be released early,
// tearing down state mid-run. A static keeps a concrete strong reference.
enum AcordAppMain {
    static let delegate = AppDelegate()
}

let app = NSApplication.shared
app.delegate = AcordAppMain.delegate
app.setActivationPolicy(.regular)
app.activate(ignoringOtherApps: true)
app.run()
_ = AcordAppMain.delegate  // keep alive past app.run()
