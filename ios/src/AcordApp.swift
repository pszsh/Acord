import SwiftUI

@main
struct AcordApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
                .ignoresSafeArea(.keyboard)
        }
    }
}

struct ContentView: View {
    var body: some View {
        IcedViewportRepresentable()
            .ignoresSafeArea()
    }
}
